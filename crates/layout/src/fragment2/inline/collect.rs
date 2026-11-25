//! Collecting and shaping the inline content of an inline formatting
//! context into its [`InlineLayout`].
//!
//! Only the text side is built here. The boxes inside the inline content are
//! returned in flow order for the fragment tree builder, which reconciles
//! them with the child boxes of the owner in place.

use comet_tree::{node::Text, ui::Ui};
use rustc_hash::FxHashMap;
use taffy::style::CoreStyle;

use super::{
    InlineLayout, OffsetEntry, inline_box_id,
    input::{InlinePrelayout, InputItem},
};
use crate::{
    fragment::state::NodeLayoutBits,
    fragment2::BoxFragmentType,
    input::{LayoutInputKey, SpanStyle, tree::LayoutInputTree},
    style::LayoutConstraint,
    taffy::style::{self, Style},
};

/// Object replacement character standing for an in-flow inline box.
const OBJECT_REPLACEMENT: char = '\u{FFFC}';

/// What box styles and inline content are resolved from.
#[derive(Clone, Copy)]
pub struct InlineBuildContext<'a> {
    pub ui: &'a Ui,
    pub tree: &'a LayoutInputTree,
    /// Viewport and root font size resolving box styles. Its font size is
    /// replaced by the font size of each item.
    pub constraint: &'a LayoutConstraint,
}

impl InlineBuildContext<'_> {
    /// Resolve the box style of an item, with what its layout depends on.
    pub fn resolve_style(&self, key: LayoutInputKey) -> (Style, NodeLayoutBits) {
        let item = &self.tree.items[key];
        let mut cx = self.constraint.clone();
        cx.units.font_size = f64::from(item.text_style.span.font_size);

        let mut bits = NodeLayoutBits::empty();
        let style = style::resolve_tracked(self.ui.props(item.node), &cx, &mut |unit| {
            bits |= NodeLayoutBits::of_unit(unit)
        });

        (style, bits)
    }
}

/// Collects and shapes inline content. Keep it to reuse its buffers.
#[derive(Default)]
pub struct InlineContentBuilder {
    lcx: parley::LayoutContext<LayoutInputKey>,
    /// Style runs of the text being built, by the item whose style they use.
    runs: Vec<(LayoutInputKey, usize)>,
    /// Inline elements open at the current position: their offset entry and
    /// the number of inline boxes when they were pushed.
    open: Vec<(usize, usize)>,
    /// Inline boxes of the text being built.
    boxes: Vec<parley::InlineBox>,
    /// Style table index of each item's style.
    styles: FxHashMap<LayoutInputKey, u16>,
    /// Boxes inside the inline content of the latest build, in flow order,
    /// with how they take part in it: atomic inline, inline part and
    /// out-of-flow boxes.
    pub children: Vec<(LayoutInputKey, BoxFragmentType)>,
}

impl InlineContentBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the content of `inline` with the inline content of `root`,
    /// which establishes an inline formatting context, shaped but not laid
    /// out. Its allocations are kept. The boxes inside are left in
    /// [`Self::children`], and [`InlineLayout::atomics`] is left empty.
    ///
    /// Text is taken as is, without white space processing. A block level box
    /// inside the inline content is an inline part box on its own line,
    /// broken from the content around it by line feeds. An out-of-flow box
    /// takes no character: its Parley inline box only gives its static
    /// position.
    pub fn build(
        &mut self,
        cx: &InlineBuildContext<'_>,
        fonts: &mut parley::FontContext,
        root: LayoutInputKey,
        inline: &mut InlineLayout,
    ) {
        inline.clear();

        let mut children = std::mem::take(&mut self.children);
        children.clear();
        self.collect(cx, root, inline, &mut children);
        self.children = children;

        self.shape(cx, fonts, root, inline);
    }

    /// Concatenate the inline content into the text, recording style runs,
    /// inline boxes, offsets and the boxes inside it.
    fn collect(
        &mut self,
        cx: &InlineBuildContext<'_>,
        root: LayoutInputKey,
        inline: &mut InlineLayout,
        children: &mut Vec<(LayoutInputKey, BoxFragmentType)>,
    ) {
        let tree = cx.tree;
        self.runs.clear();
        self.open.clear();
        self.boxes.clear();

        let mut text = Collector {
            text: &mut inline.text,
            runs: &mut self.runs,
            break_pending: false,
        };
        // Item whose style unstyled text uses: the innermost open inline
        // element, or the root.
        let mut parents = vec![root];

        for item in InlinePrelayout::new(tree, root) {
            match item {
                InputItem::Push(key) => {
                    let (_, bits) = cx.resolve_style(key);
                    inline.content_bits |= bits & NodeLayoutBits::PROPAGATED;

                    self.open
                        .push((inline.offset_mapping.len(), self.boxes.len()));
                    inline.offset_mapping.push(OffsetEntry {
                        source: key,
                        range: text.len()..text.len(),
                        text: false,
                    });
                    parents.push(key);
                }
                InputItem::Pop => {
                    let Some((entry, boxes)) = self.open.pop() else {
                        continue;
                    };
                    let key = parents.pop().unwrap_or(root);
                    let start = inline.offset_mapping[entry].range.start;

                    // An empty inline element still takes a position on a
                    // line, as a zero sized box.
                    if text.len() == start && self.boxes.len() == boxes {
                        let index = text.push_char(key, OBJECT_REPLACEMENT);
                        self.boxes
                            .push(inline_box(key, index, parley::InlineBoxKind::InFlow));
                    }

                    inline.offset_mapping[entry].range.end = text.len();
                }
                InputItem::Text(key) => {
                    let start = text.len();

                    if let Some(value) = cx.ui.prop::<Text>(tree.items[key].node)
                        && !value.0.is_empty()
                    {
                        text.push_str(key, &value.0);
                    }

                    inline.offset_mapping.push(OffsetEntry {
                        source: key,
                        range: start..text.len(),
                        text: true,
                    });
                }
                InputItem::Atomic(key) | InputItem::Block(key) => {
                    let parent = parents.last().copied().unwrap_or(root);
                    let box_type =
                        if cx.resolve_style(key).0.position() == taffy::Position::Absolute {
                            BoxFragmentType::OutOfFlow
                        } else if matches!(item, InputItem::Block(_)) {
                            BoxFragmentType::InlinePart
                        } else {
                            BoxFragmentType::AtomicInline
                        };

                    match box_type {
                        BoxFragmentType::OutOfFlow => {
                            self.boxes.push(inline_box(
                                key,
                                text.len(),
                                parley::InlineBoxKind::OutOfFlow,
                            ));
                        }
                        BoxFragmentType::InlinePart => {
                            text.break_line(parent);
                            let index = text.push_char(parent, OBJECT_REPLACEMENT);
                            self.boxes
                                .push(inline_box(key, index, parley::InlineBoxKind::InFlow));
                            // The content after it starts a new line.
                            text.break_pending = true;
                        }
                        _ => {
                            let index = text.push_char(parent, OBJECT_REPLACEMENT);
                            self.boxes
                                .push(inline_box(key, index, parley::InlineBoxKind::InFlow));
                        }
                    }

                    children.push((key, box_type));
                }
            }
        }
    }

    /// Build the Parley layout of the collected text, reusing the layout of
    /// `inline`.
    fn shape(
        &mut self,
        cx: &InlineBuildContext<'_>,
        fonts: &mut parley::FontContext,
        root: LayoutInputKey,
        inline: &mut InlineLayout,
    ) {
        let tree = cx.tree;
        let InlineLayout { text, layout, .. } = inline;
        let mut builder = self.lcx.style_run_builder(fonts, text, 1.0, false);
        self.styles.clear();

        // The root style comes first, so empty text uses it.
        let mut style_of = |builder: &mut parley::StyleRunBuilder<'_, LayoutInputKey>,
                            key: LayoutInputKey| {
            *self
                .styles
                .entry(key)
                .or_insert_with(|| builder.push_style(span_style(tree, key)))
        };
        style_of(&mut builder, root);

        let mut start = 0;
        for &(key, end) in &self.runs {
            let index = style_of(&mut builder, key);
            builder.push_style_run(index, start..end);
            start = end;
        }

        for inline_box in &self.boxes {
            builder.push_inline_box(inline_box.clone());
        }

        builder.build_into(layout, &*text);
    }
}

impl InlineLayout {
    /// Drop the content, keeping the allocations.
    pub(crate) fn clear(&mut self) {
        self.text.clear();
        self.offset_mapping.clear();
        self.items.clear();
        self.atomics.clear();
        self.content_bits = NodeLayoutBits::empty();
        self.first_baseline = None;
        self.last_baseline = None;
        self.origin = kurbo::Vec2::ZERO;
    }
}

/// Appends text with its style runs.
struct Collector<'a> {
    text: &'a mut String,
    /// Style runs, each ending at its offset.
    runs: &'a mut Vec<(LayoutInputKey, usize)>,
    /// Whether the next content starts a new line, after an inline part box.
    break_pending: bool,
}

impl Collector<'_> {
    fn len(&self) -> usize {
        self.text.len()
    }

    /// Append `value` in the style of `key`.
    fn push_str(&mut self, key: LayoutInputKey, value: &str) {
        if self.break_pending {
            self.break_pending = false;
            self.push_raw(key, "\n");
        }

        self.push_raw(key, value);
    }

    /// Append `c` in the style of `key`, returning its offset.
    fn push_char(&mut self, key: LayoutInputKey, c: char) -> usize {
        self.push_str(key, c.encode_utf8(&mut [0; 4]));
        self.text.len() - c.len_utf8()
    }

    /// End the current line unless it is empty.
    fn break_line(&mut self, key: LayoutInputKey) {
        self.break_pending = false;

        if !self.text.is_empty() && !self.text.ends_with('\n') {
            self.push_raw(key, "\n");
        }
    }

    fn push_raw(&mut self, key: LayoutInputKey, value: &str) {
        self.text.push_str(value);

        match self.runs.last_mut() {
            Some((last, end)) if *last == key => *end = self.text.len(),
            _ => self.runs.push((key, self.text.len())),
        }
    }
}

fn inline_box(key: LayoutInputKey, index: usize, kind: parley::InlineBoxKind) -> parley::InlineBox {
    parley::InlineBox {
        id: inline_box_id(key),
        index,
        width: 0.0,
        height: 0.0,
        kind,
        baseline: None,
        vertical_align: Default::default(),
    }
}

/// The item's inherited text style, branded with the item so glyph runs can
/// be traced back to their source.
/// TODO:: REMOVE
fn span_style(
    tree: &LayoutInputTree,
    key: LayoutInputKey,
) -> parley::TextStyle<'static, 'static, LayoutInputKey> {
    // Destructured exhaustively, so new style fields are not missed.
    let SpanStyle {
        font_family,
        font_size,
        font_width,
        font_style,
        font_weight,
        font_variations,
        font_features,
        locale,
        brush: _,
        has_underline,
        underline_offset,
        underline_size,
        underline_brush: _,
        has_strikethrough,
        strikethrough_offset,
        strikethrough_size,
        strikethrough_brush: _,
        line_height,
        vertical_align,
        word_spacing,
        letter_spacing,
        word_break,
        line_break,
        overflow_wrap,
        text_wrap_mode,
        white_space_collapse,
        collapsed_space,
    } = tree.items[key].text_style.span.clone();

    parley::TextStyle {
        font_family,
        font_size,
        font_width,
        font_style,
        font_weight,
        font_variations,
        font_features,
        locale,
        brush: key,
        has_underline,
        underline_offset,
        underline_size,
        underline_brush: None,
        has_strikethrough,
        strikethrough_offset,
        strikethrough_size,
        strikethrough_brush: None,
        line_height,
        vertical_align,
        word_spacing,
        letter_spacing,
        word_break,
        line_break,
        overflow_wrap,
        text_wrap_mode,
        white_space_collapse,
        collapsed_space,
    }
}
