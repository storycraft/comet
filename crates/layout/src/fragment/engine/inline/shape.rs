//! Building the Parley layout of a fragment owning lines.

use core::ops::Range;

use comet_tree::node::Text;
use rustc_hash::FxHashMap;

use super::{
    super::{Engine, FragmentKey, Mode},
    events::{InlineEvents, Op},
};
use crate::{
    fragment::InlineLayout,
    input::{LayoutInputKey, LayoutInputKind, SpanStyle, tree::LayoutInputTree},
};

impl Engine<'_> {
    /// Build the Parley layout of an inline formatting context's box, or of
    /// one run of an anonymous block. A run starts after its preceding block
    /// box and ends at the next one.
    pub(super) fn shape(&mut self, fragment: FragmentKey) {
        let tree = self.tree;
        let ui = self.ui;
        let owner = self.fragments.items[fragment].source;
        let after = match self.fragments.get_state(fragment).map(|state| state.mode) {
            Some(Mode::Run { after }) => after,
            Some(_) => None,
            None => return,
        };

        let mut boxes = self
            .fragments
            .get_inline_mut(fragment)
            .map(|inline| std::mem::take(&mut inline.boxes))
            .unwrap_or_default();
        boxes.clear();
        let mut offsets = self
            .fragments
            .get_inline_mut(fragment)
            .map(|inline| std::mem::take(&mut inline.offset_mapping))
            .unwrap_or_default();
        offsets.clear();
        // The text item the pending white space of the builder follows.
        let mut last_text = None;

        let root_style = span_style(tree, owner);
        let mut builder = self
            .parley
            .tree_builder(self.fonts, 1.0, false, &root_style);
        // Style spans open in the builder.
        let mut open = 0;

        let events = match after {
            Some(block) => {
                // Inline elements containing the block continue in this run.
                let ancestors = &mut self.scratch.inline.ancestors;
                ancestors.clear();
                ancestors.extend(
                    std::iter::successors(tree.items.parent(block), |&key| tree.items.parent(key))
                        .take_while(|&key| key != owner)
                        .filter(|&key| tree.items[key].kind == LayoutInputKind::Inline),
                );

                for &key in ancestors.iter().rev() {
                    builder.push_style_span(span_style(tree, key));
                    open += 1;
                }

                InlineEvents::after(tree, owner, block)
            }
            None => InlineEvents::new(tree, owner),
        };

        for op in events {
            match op {
                Op::Push(key) => {
                    builder.push_style_span(span_style(tree, key));
                    open += 1;
                }
                Op::Pop => {
                    builder.pop_style_span();
                    open -= 1;
                }
                Op::Text(key) => {
                    let Some(text) = ui.prop::<Text>(tree.items[key].node) else {
                        continue;
                    };

                    let pending = builder.has_pending_whitespace();
                    let before = builder.text().len();
                    if !text.0.is_empty() {
                        builder.push_style_span(span_style(tree, key));
                        builder.push_text(&text.0);
                        builder.pop_style_span();
                    }

                    // A space flushed for white space pending before the
                    // text belongs to the text it follows, unless this text
                    // starts with white space collapsing into it.
                    let start = if text.0.starts_with(char::is_whitespace) {
                        before
                    } else {
                        before + flushed_space(&mut offsets, last_text, pending, &builder, before)
                    };
                    offsets.insert(key, start..builder.text().len().max(start));
                    last_text = Some(key);
                }
                // Atomic inline metrics are set after laying out the box.
                Op::Box(key) | Op::Empty(key) => {
                    let pending = builder.has_pending_whitespace();
                    let before = builder.text().len();
                    builder.push_inline_box(parley::InlineBox {
                        id: boxes.len() as u64,
                        index: 0,
                        width: 0.0,
                        height: 0.0,
                        kind: parley::InlineBoxKind::InFlow,
                        baseline: None,
                        vertical_align: Default::default(),
                    });
                    boxes.push(key);
                    flushed_space(&mut offsets, last_text, pending, &builder, before);
                    last_text = None;
                }
                // The run ends at the next block box.
                Op::Block(_) => break,
            }
        }

        for _ in 0..open {
            builder.pop_style_span();
        }

        let Some(state) = self.fragments.get_state_mut(fragment) else {
            return;
        };

        if let Some(inline) = state.inline.as_deref_mut() {
            inline.text = builder.build_into(&mut inline.layout);
            inline.boxes = boxes;
            inline.offset_mapping = offsets;
            inline.shaped = true;
            // The lines refer to the previous text until emitted again.
            inline.lines.clear();
        } else {
            let (layout, text) = builder.build();

            state.inline = Some(Box::new(InlineLayout {
                text,
                layout,
                offset_mapping: offsets,
                boxes,
                shaped: true,
                first_baseline: None,
                last_baseline: None,
                lines: Vec::new(),
            }));
        }
    }
}

/// Give the space the builder flushed at `before` for white space pending
/// before a push to the text item `last` it follows, and return its length:
/// zero when no space was flushed.
fn flushed_space(
    offsets: &mut FxHashMap<LayoutInputKey, Range<usize>>,
    last: Option<LayoutInputKey>,
    pending: bool,
    builder: &parley::TreeBuilder<'_, Option<LayoutInputKey>>,
    before: usize,
) -> usize {
    let len = builder.text()[before..]
        .chars()
        .next()
        .filter(|&c| pending && c != '\u{FFFC}' && c.is_whitespace())
        .map_or(0, char::len_utf8);

    if len > 0
        && let Some(range) = last.and_then(|last| offsets.get_mut(&last))
        && range.end == before
    {
        range.end += len;
    }

    len
}

/// The item's inherited text style, branded with the item so glyph runs can
/// be traced back to their source.
fn span_style(tree: &LayoutInputTree, key: LayoutInputKey) -> SpanStyle {
    let mut style = tree.items[key].text_style.span.clone();
    style.brush = Some(key);

    style
}
