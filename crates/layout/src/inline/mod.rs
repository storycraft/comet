//! Navigating the inline content of a [`FragmentTree`](crate::fragment::tree::FragmentTree).
//!
//! Inline layouts are owned by the box of an inline formatting context, or by
//! an anonymous block when block boxes interrupt the inline content. Each line
//! item of an inline layout is one leaf fragment, a run: a glyph run, an
//! atomic inline box, or an empty inline element. [`InlineCursor`] walks these
//! runs, addressed by an [`InlinePosition`] in the layout's text.

mod cursor;

pub use cursor::InlineCursor;

use crate::fragment::FragmentKey;

/// A position in the text of one inline layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InlinePosition {
    /// Fragment owning the inline layout: the box of an inline formatting
    /// context, or an anonymous block.
    pub owner: FragmentKey,
    /// Byte offset into
    /// [`InlineLayout::text`](crate::fragment::InlineLayout::text): the text
    /// after white space processing, where each inline box is one U+FFFC
    /// character. In `0..=text.len()`, on a char boundary.
    pub index: usize,
}

impl InlinePosition {
    pub const fn new(owner: FragmentKey, index: usize) -> Self {
        Self { owner, index }
    }
}
