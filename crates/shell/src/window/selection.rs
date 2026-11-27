//! Selecting text with a pointer.

use core::cmp::Ordering;

use comet_layout::fragment::{BoxFragmentKey, FragmentRef, inline::InlineLayout};
use comet_tree::node::Text;
use kurbo::Point;
use parley::Affinity::{Downstream, Upstream};
use reactive_graph::traits::GetUntracked;

use super::{Window, state::update_rw};
use crate::{
    selection::{Selection, TextPosition},
    style::Selectable,
};

/// The text selection.
#[derive(Default)]
pub(super) struct SelectionState {
    pub current: Option<Selection>,
    /// Whether a pointer drag extends the selection.
    pub selecting: bool,
}

impl Window {
    /// Set the selection, following it with
    /// [`WindowState::selection`](super::WindowState::selection).
    pub(super) fn set_selection(&mut self, selection: Option<Selection>) {
        self.selection.current = selection;
        update_rw(self.state.selection, selection);

        if selection.is_none() {
            self.selection.selecting = false;
        }
    }

    /// Apply a selection change made through the window state, and clear a
    /// selection in text removed from the tree.
    pub(super) fn sync_selection(&mut self) {
        let wanted = self.state.selection.get_untracked();
        if wanted != self.selection.current {
            self.set_selection(wanted);
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

        let selection = self.with_inline_layout(owner, |inline| {
            let layout = &inline.layout;
            match current {
                Some(current) => Some(Selection::new(
                    current.anchor,
                    self.position_at(inline, parley::Cursor::from_point(layout, x, y), None)?,
                )),
                None => {
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

        if let Some(selection) = selection.flatten() {
            self.set_selection(Some(selection));
            self.selection.selecting = true;
        }
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
        if let Some(extent) = extent.flatten() {
            self.set_selection(Some(Selection::new(current.anchor, extent)));
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
        let item = inline.item_at(cursor.index(), side.unwrap_or(cursor.affinity()))?;
        let node = self.pipeline.trees.node(item)?;
        let tree = self.ui.tree().borrow();
        let text = tree.ui.prop::<Text>(node)?;
        let index = inline.source_index(item, &text.0, cursor.index())?;

        Some(TextPosition::new(node, index, cursor.affinity().into()))
    }

    /// The box owning the inline layout holding `position`, and the index of
    /// `position` in its text, from the last layout. Text in the runs of a
    /// box split by blocks is held by its run box.
    pub(super) fn layout_position(
        &self,
        position: TextPosition,
    ) -> Option<(BoxFragmentKey, usize)> {
        let trees = &self.pipeline.trees;
        let item = trees.inputs.item_for_node(position.node)?;
        let owner = trees.fragments.inline_owner(&trees.inputs, item)?;
        let tree = self.ui.tree().borrow();
        let text = tree.ui.prop::<Text>(position.node)?;
        let index =
            trees
                .fragments
                .inline_layout(owner)?
                .layout_index(item, &text.0, position.index)?;

        Some((owner, index))
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
    /// [`Selectable`] other than `Auto` from
    /// the node hit up the tree decides whether it can be selected there,
    /// so an unselectable span in selectable text is not selected. Text is
    /// selectable when no node decides.
    fn text_at(&self) -> Option<(BoxFragmentKey, Point)> {
        let leaf = self.pointer.path.last().map(|hit| hit.node);
        if !self
            .inherited::<Selectable, _>(leaf, |prop| prop.0)
            .unwrap_or(true)
        {
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
                    if !self
                        .inherited::<Selectable, _>(Some(node), |prop| prop.0)
                        .unwrap_or(true)
                    {
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
