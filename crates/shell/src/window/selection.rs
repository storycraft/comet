//! Selecting text with a pointer.

use comet_layout::{fragment::FragmentKey, input::LayoutInputKey};
use comet_tree::node::NodeKey;
use kurbo::Point;
use reactive_graph::traits::GetUntracked;

use super::{Window, state::update_rw};
use crate::{
    selection::{InlineText, Selection, TextPosition},
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
            [selection.anchor, selection.extent]
                .iter()
                .flat_map(|position| [Some(position.node), position.run])
                .flatten()
                .any(|node| !self.attached(node))
        }) {
            self.set_selection(None);
        }
    }

    /// Start selecting text at `position`, pressed `count` times in a row:
    /// a caret once, a word twice, a line three times or more. With
    /// `extend`, move the extent of the selection there instead. A press
    /// outside selectable text clears the selection, unless extending.
    pub(super) fn start_selection(&mut self, count: u8, extend: bool) {
        let Some((text, point)) = self.text_at() else {
            if !extend {
                self.set_selection(None);
            }
            return;
        };
        let (x, y) = (point.x as f32, point.y as f32);
        let current = self.selection.current.filter(|_| extend);

        let selection = self.with_inline_layout(text, |layout| match current {
            Some(current) => Selection::new(
                current.anchor,
                TextPosition::from_cursor(text, parley::Cursor::from_point(layout, x, y)),
            ),
            None => Selection::from_parley(
                text,
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
        let target = self.text_at().or_else(|| {
            let text = current.extent.text();
            Some((text, self.inline_point(text, position)?))
        });
        let Some((text, point)) = target else {
            return;
        };

        let extent = self.with_inline_layout(text, |layout| {
            let cursor = parley::Cursor::from_point(layout, point.x as f32, point.y as f32);
            TextPosition::from_cursor(text, cursor)
        });
        if let Some(extent) = extent {
            self.set_selection(Some(Selection::new(current.anchor, extent)));
        }
    }

    /// The selectable text under the pointer, with the pointer in the
    /// coordinates of its inline layout.
    ///
    /// The innermost box or anonymous block owning an inline layout on the
    /// hit path of the pointer holds the text. A box hit itself, with its
    /// inline content in anonymous blocks, holds the text of the anonymous
    /// block under the pointer: anonymous blocks are only on the path to
    /// the text they hold. The first [`Selectable`] other than `Auto` from
    /// the node hit up the tree decides whether it can be selected there,
    /// so an unselectable span in selectable text is not selected. Text is
    /// selectable when no node decides.
    fn text_at(&self) -> Option<(InlineText, Point)> {
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
            let Some(inline) = fragments.get_inline(hit.fragment) else {
                continue;
            };
            // An anonymous block belongs to the node of its owner.
            let run = if fragments.items.get(hit.fragment)?.item.is_anonymous_block() {
                trees.run_of(hit.fragment)?
            } else {
                None
            };

            text = Some((
                InlineText {
                    node: hit.node,
                    run,
                },
                hit.position - inline.origin(fragments)?,
            ));
        }

        let leaf = self.pointer.path.last()?;
        if fragments.items.get(leaf.fragment)?.item.is_box()
            && fragments.get_inline(leaf.fragment).is_none()
            && let Some(run) = self.run_at(leaf.node, leaf.fragment)
        {
            return Some(run);
        }

        text
    }

    /// The text of the anonymous block of `owner`, the box fragment of
    /// `node`, under the pointer, with the pointer in the coordinates of
    /// its inline layout.
    fn run_at(&self, node: NodeKey, owner: FragmentKey) -> Option<(InlineText, Point)> {
        let trees = &self.pipeline.trees;
        let cursor = self.pointer.cursor?;

        trees.fragments.items.children(owner).find_map(|child| {
            let run = trees.run_of(child)?;
            let to_scene = trees.fragment_to_scene(child)?;
            if to_scene.determinant() == 0.0 {
                return None;
            }

            let local = to_scene.inverse() * cursor;
            let layout = &trees.fragments.items.get(child)?.layout;
            if !(0.0..layout.size.height).contains(&local.y) {
                return None;
            }

            let origin = trees
                .fragments
                .get_inline(child)?
                .origin(&trees.fragments)?;
            Some((InlineText { node, run }, local - origin))
        })
    }

    /// `position` in the coordinates of the inline layout of `text`.
    fn inline_point(&self, text: InlineText, position: Point) -> Option<Point> {
        let trees = &self.pipeline.trees;
        let to_scene = trees.inline_to_scene(trees.inline_run(text.node, text.run)?)?;
        if to_scene.determinant() == 0.0 {
            return None;
        }

        Some(to_scene.inverse() * position)
    }

    /// Call `f` with the inline layout of `text`, from the last layout.
    fn with_inline_layout<R>(
        &self,
        text: InlineText,
        f: impl FnOnce(&parley::Layout<Option<LayoutInputKey>>) -> R,
    ) -> Option<R> {
        let trees = &self.pipeline.trees;
        let inline = trees
            .fragments
            .get_inline(trees.inline_run(text.node, text.run)?)?;

        Some(f(&inline.layout))
    }
}
