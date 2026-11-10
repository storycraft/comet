//! Building the Parley layout of a fragment owning lines.

use comet_tree::node::Text;

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
        let after = match self.fragments.states[fragment].mode {
            Mode::Run { after } => after,
            _ => None,
        };

        let mut boxes = self
            .fragments
            .inline_layouts
            .get_mut(fragment)
            .map(|inline| std::mem::take(&mut inline.boxes))
            .unwrap_or_default();
        boxes.clear();

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

                    if !text.0.is_empty() {
                        builder.push_style_span(span_style(tree, key));
                        builder.push_text(&text.0);
                        builder.pop_style_span();
                    }
                }
                // Atomic inline metrics are set after laying out the box.
                Op::Box(key) | Op::Empty(key) => {
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
                }
                // The run ends at the next block box.
                Op::Block(_) => break,
            }
        }

        for _ in 0..open {
            builder.pop_style_span();
        }

        if let Some(inline) = self.fragments.inline_layouts.get_mut(fragment) {
            inline.text = builder.build_into(&mut inline.layout);
            inline.boxes = boxes;
            inline.shaped = true;
        } else {
            let (layout, text) = builder.build();

            self.fragments.inline_layouts.insert(
                fragment,
                InlineLayout {
                    text,
                    layout,
                    boxes,
                    shaped: true,
                    first_baseline: None,
                    last_baseline: None,
                },
            );
        }
    }
}

/// The item's inherited text style, branded with the item so glyph runs can
/// be traced back to their source.
fn span_style(tree: &LayoutInputTree, key: LayoutInputKey) -> SpanStyle {
    let mut style = tree.items[key].text_style.span.clone();
    style.brush = Some(key);

    style
}
