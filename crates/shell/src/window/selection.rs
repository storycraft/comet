//! Selecting text with a pointer.

use comet_layout::input::LayoutInputKey;
use comet_tree::node::NodeKey;
use kurbo::Point;
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
    /// outside selectable text clears the selection, unless extending.
    pub(super) fn start_selection(&mut self, position: Point, count: u8, extend: bool) {
        let Some((node, point)) = self.text_at(position) else {
            if !extend {
                self.set_selection(None);
            }
            return;
        };
        let (x, y) = (point.x as f32, point.y as f32);
        let current = self.selection.current.filter(|_| extend);

        let selection = self.with_inline_layout(node, |layout| match current {
            Some(current) => Selection::new(
                current.anchor,
                TextPosition::from_cursor(node, parley::Cursor::from_point(layout, x, y)),
            ),
            None => Selection::from_parley(
                node,
                match count {
                    0 | 1 => parley::Selection::from_point(layout, x, y),
                    2 => parley::Selection::word_from_point(layout, x, y),
                    _ => parley::Selection::line_from_point(layout, x, y),
                },
            ),
        });

        if let Some(selection) = selection {
            self.set_selection(Some(selection));
            self.selection.selecting = true;
        }
    }

    /// Move the extent of the selection to `position`: into the selectable
    /// text under it, or to the closest position of the text it is in.
    pub(super) fn extend_selection(&mut self, position: Point) {
        let Some(current) = self.selection.current else {
            return;
        };
        let target = self.text_at(position).or_else(|| {
            let node = current.extent.node;
            Some((node, self.inline_point(node, position)?))
        });
        let Some((node, point)) = target else {
            return;
        };

        let extent = self.with_inline_layout(node, |layout| {
            let cursor = parley::Cursor::from_point(layout, point.x as f32, point.y as f32);
            TextPosition::from_cursor(node, cursor)
        });
        if let Some(extent) = extent {
            self.set_selection(Some(Selection::new(current.anchor, extent)));
        }
    }

    /// The node owning the selectable text under `position`, with the
    /// position in the coordinates of its inline layout, from the last
    /// layout.
    ///
    /// Walks the path to the fragment under `position` from the root down:
    /// the innermost box owning an inline layout on it holds the text, and
    /// the [`Selectable`] prop nearest to the fragment hit decides whether
    /// it can be selected there, so an unselectable span in selectable text
    /// is not selected.
    fn text_at(&mut self, position: Point) -> Option<(NodeKey, Point)> {
        let tree = self.ui.tree().borrow();
        let (mut cursor, trees) = self.pipeline.hit_cursor(position)?;
        let mut selectable = true;
        let mut text = None;

        while let Some(hit) = cursor.hit_next(&trees.layers, &trees.fragments, &trees.prepaint) {
            let fragment = trees.fragments.items.get(hit.fragment)?;
            let node = trees.node(fragment.source);

            if let Some(prop) = node.and_then(|node| tree.ui.prop::<Selectable>(node)) {
                selectable = prop.0;
            }

            if fragment.item.is_anonymous_block() {
                // Its inline layout has no node to keep positions in.
                text = None;
            } else if fragment.item.is_box()
                && let Some(inline) = trees.fragments.get_inline(hit.fragment)
            {
                let origin = inline.origin(&trees.fragments)?;
                text = node.map(|node| (node, hit.position - origin));
            }
        }

        text.filter(|_| selectable)
    }

    /// `position` in the coordinates of the inline layout of `node`.
    fn inline_point(&self, node: NodeKey, position: Point) -> Option<Point> {
        let trees = &self.pipeline.trees;
        let to_scene = trees.inline_to_scene(trees.inline_owner(node)?)?;
        if to_scene.determinant() == 0.0 {
            return None;
        }

        Some(to_scene.inverse() * position)
    }

    /// Call `f` with the inline layout of `node`, from the last layout.
    fn with_inline_layout<R>(
        &self,
        node: NodeKey,
        f: impl FnOnce(&parley::Layout<Option<LayoutInputKey>>) -> R,
    ) -> Option<R> {
        let trees = &self.pipeline.trees;
        let inline = trees.fragments.get_inline(trees.inline_owner(node)?)?;

        Some(f(&inline.layout))
    }
}
