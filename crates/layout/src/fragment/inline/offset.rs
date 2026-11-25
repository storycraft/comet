//! Index mapping between the text of a text item and its white space
//! processed text in an inline layout.

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
