//! Positions in the text of text items and in the text of the inline layouts
//! holding them.
//!
//! White space processing collapses or removes white space, so a text item
//! is mapped to its range of [`InlineLayout::text`] by
//! [`InlineLayout::offset_mapping`], and indices inside the range are found
//! by lining the item's text up with it.

use super::{FragmentKey, InlineLayout, tree::FragmentTree};
use crate::input::{LayoutInputKey, tree::LayoutInputTree};

impl InlineLayout {
    /// The index in [`Self::text`] of `index` in `source`, the text of the
    /// text item `item`. An index in white space removed by white space
    /// processing maps to where it was removed, and one past the end of
    /// `source` to the end of its range. `None` if the layout does not hold
    /// the item.
    pub fn layout_index(&self, item: LayoutInputKey, source: &str, index: usize) -> Option<usize> {
        let range = self.offset_mapping.get(&item)?;
        let shaped = self.text.get(range.clone())?;

        Some(range.start + to_shaped(source, shaped, index))
    }

    /// The index in `source`, the text of the text item `item`, of `index`
    /// in [`Self::text`], clamped to the range of the item. Of the indices
    /// mapping to the same place, the first. `None` if the layout does not
    /// hold the item.
    pub fn source_index(&self, item: LayoutInputKey, source: &str, index: usize) -> Option<usize> {
        let range = self.offset_mapping.get(&item)?;
        let shaped = self.text.get(range.clone())?;
        let index = index.clamp(range.start, range.end) - range.start;

        Some(to_source(source, shaped, index))
    }

    /// The text item holding `index` in [`Self::text`]. At the boundary of
    /// two items, `affinity` picks the one before or after it. Outside of
    /// every item, such as at an inline box, the closest item before it,
    /// else the first after it. `None` without text items.
    pub fn item_at(&self, index: usize, affinity: parley::Affinity) -> Option<LayoutInputKey> {
        let upstream = affinity == parley::Affinity::Upstream;
        let mut found = None;
        let mut before = None;
        let mut after = None;

        for (&item, range) in &self.offset_mapping {
            if range.contains(&index) || range.is_empty() && range.start == index {
                found = Some(item);
            } else if range.end == index {
                if upstream {
                    return Some(item);
                }
                before = Some((range.end, item));
            } else if range.end < index {
                before = before.max(Some((range.end, item)));
            } else if after.is_none_or(|(start, _)| range.start < start) {
                after = Some((range.start, item));
            }
        }

        found
            .or(before.map(|(_, item)| item))
            .or(after.map(|(_, item)| item))
    }
}

impl FragmentTree {
    /// The fragment owning the inline layout holding the text item `item`
    /// of `tree`: the box of its inline formatting context, or the
    /// anonymous block of its run. `None` if it is not laid out.
    pub fn inline_layout_of(
        &self,
        tree: &LayoutInputTree,
        item: LayoutInputKey,
    ) -> Option<FragmentKey> {
        let holds = |fragment: FragmentKey| {
            self.get_inline(fragment)
                .is_some_and(|inline| inline.offset_mapping.contains_key(&item))
        };

        // Inline elements generate no box, so the first box above owns it.
        let owner = std::iter::successors(tree.items.parent(item), |&key| tree.items.parent(key))
            .find_map(|key| self.box_fragment(key))?;
        if holds(owner) {
            return Some(owner);
        }

        self.items.children(owner).find(|&child| holds(child))
    }
}

/// Visit the indices of `source` at each of its character boundaries, with
/// the index of `shaped`, its white space processed text, they line up
/// with, until `f` returns true.
///
/// Characters of `source` matching the next character of `shaped` advance
/// both. White space matches white space, as white space processing may
/// replace it with a space; other characters were removed.
fn align(source: &str, shaped: &str, mut f: impl FnMut(usize, usize) -> bool) {
    let mut j = 0;

    for (i, c) in source.char_indices() {
        if f(i, j) {
            return;
        }

        if let Some(d) = shaped[j..].chars().next()
            && (c == d || c.is_whitespace() && d.is_whitespace())
        {
            j += d.len_utf8();
        }
    }

    f(source.len(), j);
}

/// The index of `shaped` lining up with `index` of `source`.
pub(crate) fn to_shaped(source: &str, shaped: &str, index: usize) -> usize {
    let mut found = shaped.len();
    align(source, shaped, |i, j| {
        found = j;
        i >= index
    });

    found
}

/// The first index of `source` lining up with `index` of `shaped`, or its
/// end.
pub(crate) fn to_source(source: &str, shaped: &str, index: usize) -> usize {
    let mut found = source.len();
    align(source, shaped, |i, j| {
        found = i;
        j >= index
    });

    found
}

#[cfg(test)]
mod tests {
    use super::{to_shaped, to_source};

    #[test]
    fn indices_line_up_through_collapsed_white_space() {
        // "a  \n b" collapses to "a b".
        let (source, shaped) = ("a  \n b", "a b");
        let shaped_of: Vec<_> = (0..=source.len())
            .map(|index| to_shaped(source, shaped, index))
            .collect();
        assert_eq!(shaped_of, [0, 1, 2, 2, 2, 2, 3]);

        let source_of: Vec<_> = (0..=shaped.len())
            .map(|index| to_source(source, shaped, index))
            .collect();
        assert_eq!(source_of, [0, 1, 2, 6]);
    }

    #[test]
    fn removed_leading_white_space_maps_to_the_start() {
        let (source, shaped) = ("  hi", "hi");
        assert_eq!(to_shaped(source, shaped, 1), 0);
        assert_eq!(to_shaped(source, shaped, 3), 1);
        assert_eq!(to_source(source, shaped, 1), 3);
        assert_eq!(to_shaped(source, shaped, 99), 2, "past the end");
    }
}
