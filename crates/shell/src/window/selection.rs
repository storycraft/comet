//! Selecting text with a pointer.

use core::{cmp::Ordering, iter::successors};

use comet_layout::{
    fragment::{BoxFragmentKey, FragmentRef, inline::InlineLayout},
    input::{LayoutInputKey, tree::LayoutInputTree},
};
use comet_paint::selection::{self, Affinity, Selection, TextPosition};
use comet_tree::node::{NodeKey, Text};
use kurbo::Point;
use parley::Affinity::{Downstream, Upstream};
use reactive_graph::traits::GetUntracked;

use super::{Window, state::update_rw};

/// The text selection.
#[derive(Default)]
pub(super) struct SelectionState {
    pub current: Option<Selection>,
    /// Whether a pointer drag extends the selection.
    pub selecting: bool,
    /// The node the selection must stay in: the
    /// [selection container](comet_paint::prepaint::LayoutInputData::selection_container)
    /// of the text it started in.
    pub container: Option<NodeKey>,
}

impl Window {
    /// Set the selection, following it with
    /// [`WindowState::selection`](super::WindowState::selection).
    pub(super) fn set_selection(&mut self, selection: Option<Selection>) {
        // The selection is painted with the layout.
        self.redraw |= self.selection.current != selection;
        self.selection.current = selection;
        self.pipeline.set_selection(selection);
        update_rw(self.state.selection, selection);

        if selection.is_none() {
            self.selection.selecting = false;
            self.selection.container = None;
        }
    }

    /// Apply a selection change made through the window state, and clear a
    /// selection in text removed from the tree.
    pub(super) fn sync_selection(&mut self) {
        let wanted = self.state.selection.get_untracked();
        if wanted != self.selection.current {
            self.set_selection(wanted);
            // Set from outside: it stays in the container of its anchor.
            self.selection.container =
                wanted.and_then(|selection| self.container_of(selection.anchor.node));
        }

        if self.selection.current.is_some_and(|selection| {
            !self.attached(selection.anchor.node) || !self.attached(selection.extent.node)
        }) {
            self.set_selection(None);
        }
    }

    /// Start selecting text at `position`, pressed `count` times in a row:
    /// a caret once, a word twice, a line three times or more. With
    /// `extend`, move the extent of the selection there instead. A press
    /// outside of text, on a selectable box, selects in the nearest text.
    /// A press on nothing selectable clears the selection, unless
    /// extending.
    pub(super) fn start_selection(&mut self, count: u8, extend: bool) {
        let Some((owner, point)) = self.text_at() else {
            if !extend {
                self.set_selection(None);
            }
            return;
        };
        let (x, y) = (point.x as f32, point.y as f32);
        let current = self.selection.current.filter(|_| extend);

        // The position pressed, whose container a new selection stays in.
        let mut pressed = None;
        let selection = self.with_inline_layout(owner, |inline| {
            let layout = &inline.layout;
            match current {
                Some(current) => Some(Selection::new(
                    current.anchor,
                    self.position_at(inline, parley::Cursor::from_point(layout, x, y), None)?,
                )),
                None => {
                    pressed =
                        self.position_at(inline, parley::Cursor::from_point(layout, x, y), None);
                    let selection = match count {
                        0 | 1 => parley::Selection::from_point(layout, x, y),
                        2 => parley::Selection::word_from_point(layout, x, y),
                        _ => parley::Selection::line_from_point(layout, x, y),
                    };
                    let (anchor, focus) = (selection.anchor(), selection.focus());
                    // The start of a range is in the text after it, and its
                    // end in the text before it, whatever their affinity.
                    let (anchor_side, focus_side) = match anchor.index().cmp(&focus.index()) {
                        Ordering::Less => (Some(Downstream), Some(Upstream)),
                        Ordering::Greater => (Some(Upstream), Some(Downstream)),
                        Ordering::Equal => (None, None),
                    };
                    Some(Selection::new(
                        self.position_at(inline, anchor, anchor_side)?,
                        self.position_at(inline, focus, focus_side)?,
                    ))
                }
            }
        });

        let Some(selection) = selection.flatten() else {
            return;
        };
        let container = match current {
            Some(_) => self.selection.container,
            None => pressed.and_then(|position| self.container_of(position.node)),
        };
        // A word or line selected around the pressed text may reach past its
        // container, or into another one: both ends are kept from the
        // pressed text.
        let from = match current {
            Some(current) => current.anchor,
            None => pressed.unwrap_or(selection.anchor),
        };
        let (Some(anchor), Some(extent)) = (
            self.contain(selection.anchor, container, from),
            self.contain(selection.extent, container, from),
        ) else {
            return;
        };

        self.set_selection(Some(Selection::new(anchor, extent)));
        self.selection.selecting = true;
        self.selection.container = container;
    }

    /// Move the extent of the selection to `position`: into the selectable
    /// text under it or nearest to it, or, over nothing selectable, to the
    /// closest position of the text it is in.
    pub(super) fn extend_selection(&mut self, position: Point) {
        let Some(current) = self.selection.current else {
            return;
        };
        let target = self.text_at().or_else(|| {
            let (owner, _) = self.layout_position(current.extent)?;
            Some((owner, self.inline_point(owner, position)?))
        });
        let Some((owner, point)) = target else {
            return;
        };

        let extent = self.with_inline_layout(owner, |inline| {
            let cursor = parley::Cursor::from_point(&inline.layout, point.x as f32, point.y as f32);
            self.position_at(inline, cursor, None)
        });
        if let Some(extent) = extent
            .flatten()
            .and_then(|extent| self.contain(extent, self.selection.container, current.anchor))
        {
            self.set_selection(Some(Selection::new(current.anchor, extent)));
        }
    }

    /// The node a selection starting in the text of `node` must stay in, as
    /// resolved by the last layout. `None` if it may go anywhere.
    fn container_of(&self, node: NodeKey) -> Option<NodeKey> {
        let trees = &self.pipeline.trees;
        let container = trees
            .prepaint
            .selection_container(trees.inputs.item_for_node(node)?)?;

        Some(trees.inputs.items.get(container)?.node)
    }

    /// `position`, an end of a selection held by `container` and running
    /// from `from`, moved where the selection may end:
    /// - out of `container`, to its edge: the end of its last selectable
    ///   text when after it, the start of its first when before;
    /// - into another container inside it, one set to
    ///   [`UserSelect::Contain`](comet_paint::style::UserSelect::Contain),
    ///   past the outermost one: to the end of its last selectable text when
    ///   the selection runs forward from `from`, the start of its first when
    ///   backward. As with an editable element on the web, a container is
    ///   selected whole from outside of it, or not at all.
    ///
    /// `None` if `position`, `from` or a container is not laid out, or the
    /// container moved to holds no selectable text.
    fn contain(
        &self,
        position: TextPosition,
        container: Option<NodeKey>,
        from: TextPosition,
    ) -> Option<TextPosition> {
        let trees = &self.pipeline.trees;
        let inputs = &trees.inputs;
        let root = match container {
            Some(container) => Some(inputs.item_for_node(container)?),
            None => None,
        };
        let item = inputs.item_for_node(position.node)?;

        if let Some(root) = root
            && !successors(Some(item), |&key| inputs.items.parent(key)).any(|key| key == root)
        {
            return self.text_edge(root, follows(inputs, item, root)?);
        }

        // The outermost container holding `position` inside the selection's.
        let mut outer = None;
        let mut next = trees.prepaint.selection_container(item);
        while let Some(key) = next.filter(|&key| Some(key) != root) {
            outer = Some(key);
            next = inputs
                .items
                .parent(key)
                .and_then(|parent| trees.prepaint.selection_container(parent));
        }
        let Some(outer) = outer else {
            return Some(position);
        };

        let from_item = inputs.item_for_node(from.node)?;
        let forward = tree_order(inputs, item, from_item)?
            .then(position.index.cmp(&from.index))
            .is_gt();

        self.text_edge(outer, forward)
    }

    /// The start of the first selectable text under the item `root`, or the
    /// end of its last with `end`, in tree order.
    fn text_edge(&self, root: LayoutInputKey, end: bool) -> Option<TextPosition> {
        let trees = &self.pipeline.trees;
        let inputs = &trees.inputs;
        let tree = self.ui.tree().borrow();
        let mut texts = inputs
            .items
            .descendants(root)
            .filter(|&key| trees.prepaint.is_selectable(key))
            .filter_map(|key| {
                let node = inputs.items[key].node;
                Some((node, tree.ui.prop::<Text>(node)?.0.len()))
            });

        if end {
            let (node, len) = texts.last()?;
            Some(TextPosition::new(node, len, Affinity::Upstream))
        } else {
            let (node, _) = texts.next()?;
            Some(TextPosition::new(node, 0, Affinity::Downstream))
        }
    }

    /// The position of the text node at `cursor` in `inline`. At the
    /// boundary of two text nodes, `side` picks the one before or after it,
    /// or by default the affinity of `cursor`.
    fn position_at(
        &self,
        inline: &InlineLayout,
        cursor: parley::Cursor,
        side: Option<parley::Affinity>,
    ) -> Option<TextPosition> {
        let tree = self.ui.tree().borrow();

        TextPosition::from_cursor(&tree.ui, &self.pipeline.trees.inputs, inline, cursor, side)
    }

    /// The box owning the inline layout holding `position`, and the index of
    /// `position` in its text, from the last layout. Text in the runs of a
    /// box split by blocks is held by its run box.
    pub(super) fn layout_position(
        &self,
        position: TextPosition,
    ) -> Option<(BoxFragmentKey, usize)> {
        let trees = &self.pipeline.trees;
        let tree = self.ui.tree().borrow();

        selection::layout_position(&tree.ui, &trees.inputs, &trees.fragments, position)
    }

    /// The box owning the selectable text under the pointer, with the
    /// pointer in the coordinates of its inline layout.
    ///
    /// The innermost box or anonymous run box owning an inline layout on the
    /// hit path of the pointer holds the text. A box hit itself, with its
    /// inline content split into runs by blocks, holds the text of the run
    /// under the pointer: run boxes are only on the path to the text they
    /// hold. Outside of any text, the
    /// [nearest text](Self::nearest_text) holds it. The first
    /// [`Selectable`](comet_paint::style::Selectable) other than `Auto` from
    /// the node hit up the tree decides whether it can be selected there,
    /// so an unselectable span in selectable text is not selected. Text is
    /// selectable when no node decides.
    fn text_at(&self) -> Option<(BoxFragmentKey, Point)> {
        let leaf = self.pointer.path.last().map(|hit| hit.node);
        if leaf.is_some_and(|leaf| !self.selectable(leaf)) {
            return None;
        }

        let trees = &self.pipeline.trees;
        let fragments = &trees.fragments;
        let mut text = None;

        for hit in &self.pointer.path {
            // Inline items are placed in the scrolled contents of the box.
            if let FragmentRef::Box(key) = hit.fragment
                && let Some(inline) = fragments.inline_layout(key)
            {
                text = Some((key, hit.position + trees.scroll(key) - inline.origin()));
            }
        }

        let leaf = self.pointer.path.last()?;
        if let FragmentRef::Box(key) = leaf.fragment
            && trees.is_principal(key)
            && fragments.inline_layout(key).is_none()
            && let Some(run) = self.run_at(key)
        {
            return Some(run);
        }

        text.or_else(|| self.nearest_text())
    }

    /// The selectable text nearest to the pointer, with the pointer in the
    /// coordinates of its inline layout: the inline layout under the
    /// innermost box on the hit path that has any, whose owning box or
    /// anonymous run box is closest to the pointer.
    fn nearest_text(&self) -> Option<(BoxFragmentKey, Point)> {
        let trees = &self.pipeline.trees;
        let fragments = &trees.fragments;
        let cursor = self.pointer.cursor?;

        self.pointer.path.iter().rev().find_map(|hit| {
            let FragmentRef::Box(key) = hit.fragment else {
                return None;
            };
            trees.is_principal(key).then_some(())?;

            fragments
                .boxes
                .descendants(key)
                .filter_map(|owner| {
                    let inline = fragments.inline_layout(owner)?;
                    let node = trees.node(fragments.get(owner)?.source)?;
                    if !self.selectable(node) {
                        return None;
                    }

                    // Measured to the box holding the lines, as wide as they
                    // can be, rather than to the glyphs.
                    let to_scene = trees.fragment_to_scene(owner)?;
                    if to_scene.determinant() == 0.0 {
                        return None;
                    }
                    let local = to_scene.inverse() * cursor;
                    let bounds = fragments.get(owner)?.layout.geometry.border_box();
                    let dx = (bounds.x0 - local.x).max(local.x - bounds.x1).max(0.0);
                    let dy = (bounds.y0 - local.y).max(local.y - bounds.y1).max(0.0);
                    let point = local + trees.scroll(owner) - inline.origin();
                    Some((dx.hypot(dy), owner, point))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, owner, point)| (owner, point))
        })
    }

    /// The run box of `owner` under the pointer, if its inline content is
    /// split by blocks, with the pointer in the coordinates of its inline
    /// layout.
    fn run_at(&self, owner: BoxFragmentKey) -> Option<(BoxFragmentKey, Point)> {
        let trees = &self.pipeline.trees;
        let cursor = self.pointer.cursor?;

        trees.fragments.runs(owner).find_map(|run| {
            let fragment = trees.fragments.get(run)?;
            let to_scene = trees.fragment_to_scene(run)?;
            if to_scene.determinant() == 0.0 {
                return None;
            }

            let local = to_scene.inverse() * cursor;
            if !(0.0..fragment.layout.geometry.size.height).contains(&local.y) {
                return None;
            }

            let origin = trees.fragments.inline_layout(run)?.origin();
            Some((run, local + trees.scroll(run) - origin))
        })
    }

    /// Whether the text of `node` can be selected, as resolved by the last
    /// layout. Nodes not laid out cannot be.
    fn selectable(&self, node: NodeKey) -> bool {
        let trees = &self.pipeline.trees;

        trees
            .inputs
            .item_for_node(node)
            .is_some_and(|item| trees.prepaint.is_selectable(item))
    }

    /// `position` in the coordinates of the inline layout owned by `owner`.
    fn inline_point(&self, owner: BoxFragmentKey, position: Point) -> Option<Point> {
        let to_scene = self.pipeline.trees.inline_to_scene(owner)?;
        if to_scene.determinant() == 0.0 {
            return None;
        }

        Some(to_scene.inverse() * position)
    }

    /// Call `f` with the inline layout owned by `owner`, from the last
    /// layout.
    fn with_inline_layout<R>(
        &self,
        owner: BoxFragmentKey,
        f: impl FnOnce(&InlineLayout) -> R,
    ) -> Option<R> {
        Some(f(self.pipeline.trees.fragments.inline_layout(owner)?))
    }
}

/// Whether the item `item` of `inputs` comes after the subtree of `root` in
/// tree order, `item` being outside of it. `None` if they are not in the
/// same tree.
fn follows(inputs: &LayoutInputTree, item: LayoutInputKey, root: LayoutInputKey) -> Option<bool> {
    let items = &inputs.items;
    let root_path: Vec<_> = successors(Some(root), |&key| items.parent(key)).collect();
    let mut item_child = None;

    for key in successors(Some(item), |&key| items.parent(key)) {
        if let Some(index) = root_path.iter().position(|&ancestor| ancestor == key) {
            // `item` holds the root, so starts before it.
            let Some(item_child) = item_child else {
                return Some(false);
            };
            let root_child = root_path[..index].last().copied()?;
            let first = items
                .children(key)
                .find(|&child| child == item_child || child == root_child)?;

            return Some(first == root_child);
        }
        item_child = Some(key);
    }

    None
}

/// The order of the items `a` and `b` of `inputs` in the tree, in preorder:
/// an item comes before its descendants. `None` if they are not in the same
/// tree.
fn tree_order(inputs: &LayoutInputTree, a: LayoutInputKey, b: LayoutInputKey) -> Option<Ordering> {
    let items = &inputs.items;
    let b_path: Vec<_> = successors(Some(b), |&key| items.parent(key)).collect();
    let mut a_child = None;

    for key in successors(Some(a), |&key| items.parent(key)) {
        if let Some(index) = b_path.iter().position(|&ancestor| ancestor == key) {
            let b_child = b_path[..index].last().copied();

            return Some(match (a_child, b_child) {
                (None, None) => Ordering::Equal,
                // An ancestor comes before its descendants.
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
                (Some(a_child), Some(b_child)) => {
                    let first = items
                        .children(key)
                        .find(|&child| child == a_child || child == b_child)?;

                    if first == a_child {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    }
                }
            });
        }
        a_child = Some(key);
    }

    None
}
