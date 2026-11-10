//! Building the Parley layout of an inline formatting context.

use comet_tree::node::{NodeKey, Text};
use slotmap::Key;

use super::{
    super::Engine,
    events::{InlineEvents, Op},
};
use crate::{
    fragment::InlineLayout,
    input::{LayoutInputKey, tree::LayoutInputTree},
    parley::TextStyle,
};

impl Engine<'_> {
    /// Build the Parley layout of an inline formatting context.
    pub(super) fn shape(&mut self, key: LayoutInputKey) {
        let tree = self.tree;
        let ui = self.ui;

        let root_style = span_style(tree, key);
        let mut content = Content {
            builder: self
                .parley
                .tree_builder(self.fonts, 1.0, false, &root_style),
            line_open: false,
            after_block: false,
        };

        for op in InlineEvents::new(tree, key) {
            match op {
                Op::Push(child) => content.builder.push_style_span(span_style(tree, child)),
                Op::Pop => content.builder.pop_style_span(),
                Op::Text(child) => {
                    if let Some(text) = ui.prop::<Text>(tree.items[child].node) {
                        content.text(span_style(tree, child), &text.0);
                    }
                }
                Op::Box(child) => content.atomic(child),
                Op::Empty(child) => content.empty(child),
                Op::Block(child) => content.block(child),
            }
        }

        let builder = content.builder;

        if let Some(inline) = self.fragments.inline_layouts.get_mut(key) {
            inline.text = builder.build_into(&mut inline.layout);
            inline.shaped = true;
        } else {
            let (layout, text) = builder.build();

            self.fragments.inline_layouts.insert(
                key,
                InlineLayout {
                    text,
                    layout,
                    shaped: true,
                    first_baseline: None,
                    last_baseline: None,
                },
            );
        }
    }
}

/// The item's inherited text style, branded with its node so glyph runs can
/// be traced back to their source.
fn span_style(tree: &LayoutInputTree, key: LayoutInputKey) -> TextStyle {
    let item = &tree.items[key];
    let mut style = item.text_style.as_ref().clone();
    style.brush = Some(item.node);

    style
}

/// Inline content pushed into a Parley tree builder. Block children are
/// separated from surrounding inline content by line breaks, inserted only
/// where inline content needs them.
struct Content<'a> {
    builder: parley::TreeBuilder<'a, Option<NodeKey>>,
    /// Inline content on the current line since the last forced break.
    line_open: bool,
    /// A block was placed and inline content following it needs a new line.
    after_block: bool,
}

impl Content<'_> {
    fn text(&mut self, style: TextStyle, text: &str) {
        if text.is_empty() {
            return;
        }

        self.break_after_block();
        self.builder.push_style_span(style);
        self.builder.push_text(text);
        self.builder.pop_style_span();
        self.line_open = true;
    }

    /// An atomic inline box. Its metrics are set after laying out the box.
    fn atomic(&mut self, key: LayoutInputKey) {
        self.break_after_block();
        self.inline_box(key, parley::InlineBoxKind::InFlow);
        self.line_open = true;
    }

    /// An inline element without children, placed to anchor its fragment.
    fn empty(&mut self, key: LayoutInputKey) {
        self.inline_box(key, parley::InlineBoxKind::InFlow);
    }

    /// A block box. The line breaker yields at it so it can be placed between lines.
    fn block(&mut self, key: LayoutInputKey) {
        if self.line_open && !self.builder.text().ends_with('\n') {
            self.builder.push_text("\n");
        }

        self.inline_box(key, parley::InlineBoxKind::CustomOutOfFlow);
        self.line_open = false;
        self.after_block = true;
    }

    fn break_after_block(&mut self) {
        if std::mem::take(&mut self.after_block) {
            self.builder.push_text("\n");
        }
    }

    fn inline_box(&mut self, key: LayoutInputKey, kind: parley::InlineBoxKind) {
        self.builder.push_inline_box(parley::InlineBox {
            id: key.data().as_ffi(),
            index: 0,
            width: 0.0,
            height: 0.0,
            kind,
            baseline: None,
            vertical_align: Default::default(),
        });
    }
}
