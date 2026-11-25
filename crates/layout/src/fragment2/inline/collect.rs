//! Collecting and shaping the inline content of an inline formatting
//! context into its [`InlineLayout`].
//!
//! Only the text side is built here. The boxes inside the inline content are
//! returned in flow order for the fragment tree builder, which reconciles
//! them with the child boxes of the owner in place.
//!
//! Block level boxes in flow split the inline content into runs, see
//! [`split_runs`]. Each run is built on its own, and the inline elements
//! split by a block are open in the runs on both sides of it.

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
    /// Inline elements open at the current position: their offset entry, the
    /// number of inline boxes when they were pushed, and whether they were
    /// opened again at the start of the run.
    open: Vec<(usize, usize, bool)>,
    /// Inline boxes of the text being built.
    boxes: Vec<parley::InlineBox>,
    /// Inline elements open again at the start of the run being built.
    reopened: Vec<LayoutInputKey>,
    /// Style table index of each item's style.
    styles: FxHashMap<LayoutInputKey, u16>,
    /// Boxes inside the inline content of the latest build, in flow order,
    /// with how they take part in it: atomic inline and out-of-flow boxes.
    pub children: Vec<(LayoutInputKey, BoxFragmentType)>,
}

impl InlineContentBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the content of `inline` with a run of the inline content of
    /// `root`, which establishes an inline formatting context, shaped but not
    /// laid out: the run after the block level box `start`, or from the
    /// start, up to the next block level box in flow. Its allocations are
    /// kept. The boxes inside are left in [`Self::children`], and
    /// [`InlineLayout::atomics`] is left empty.
    ///
    /// Text is taken as is, without white space processing. An out-of-flow
    /// box takes no character: its Parley inline box only gives its static
    /// position.
    pub fn build(
        &mut self,
        cx: &InlineBuildContext<'_>,
        fonts: &mut parley::FontContext,
        root: LayoutInputKey,
        start: Option<LayoutInputKey>,
        inline: &mut InlineLayout,
    ) {
        inline.clear();
        inline.run_start = start;

        let mut children = std::mem::take(&mut self.children);
        children.clear();
        self.collect(cx, root, start, inline, &mut children);
        self.children = children;

        self.shape(cx, fonts, root, inline);
    }

    /// Concatenate a run of the inline content into the text, recording
    /// style runs, inline boxes, offsets and the boxes inside it.
    fn collect(
        &mut self,
        cx: &InlineBuildContext<'_>,
        root: LayoutInputKey,
        start: Option<LayoutInputKey>,
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
        };
        // Item whose style unstyled text uses: the innermost open inline
        // element, or the root.
        let mut parents = vec![root];

        let prelayout = match start {
            Some(block) => {
                // The inline elements split by the block are open again.
                let mut reopened = std::mem::take(&mut self.reopened);
                InlinePrelayout::open_around(tree, root, block, &mut reopened);

                for &key in &reopened {
                    let (_, bits) = cx.resolve_style(key);
                    inline.content_bits |= bits & NodeLayoutBits::PROPAGATED;
                    self.open
                        .push((inline.offset_mapping.len(), self.boxes.len(), true));
                    inline.offset_mapping.push(OffsetEntry {
                        source: key,
                        range: 0..0,
                        text: false,
                    });
                    parents.push(key);
                }

                self.reopened = reopened;
                InlinePrelayout::after(tree, root, block)
            }
            None => InlinePrelayout::new(tree, root),
        };

        for item in prelayout {
            match item {
                InputItem::Push(key) => {
                    let (_, bits) = cx.resolve_style(key);
                    inline.content_bits |= bits & NodeLayoutBits::PROPAGATED;

                    self.open
                        .push((inline.offset_mapping.len(), self.boxes.len(), false));
                    inline.offset_mapping.push(OffsetEntry {
                        source: key,
                        range: text.len()..text.len(),
                        text: false,
                    });
                    parents.push(key);
                }
                InputItem::Pop => {
                    let Some((entry, boxes, reopened)) = self.open.pop() else {
                        continue;
                    };
                    let key = parents.pop().unwrap_or(root);
                    let start = inline.offset_mapping[entry].range.start;

                    // An empty inline element still takes a position on a
                    // line, as a zero sized box. The rest of one split by a
                    // block does not.
                    if !reopened && text.len() == start && self.boxes.len() == boxes {
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
                    let out_of_flow = is_out_of_flow(cx, key);

                    // A block level box in flow ends the run, splitting the
                    // inline elements open around it.
                    if !out_of_flow && matches!(item, InputItem::Block(_)) {
                        break;
                    }

                    let parent = parents.last().copied().unwrap_or(root);
                    if out_of_flow {
                        self.boxes.push(inline_box(
                            key,
                            text.len(),
                            parley::InlineBoxKind::OutOfFlow,
                        ));
                        children.push((key, BoxFragmentType::OutOfFlow));
                    } else {
                        let index = text.push_char(parent, OBJECT_REPLACEMENT);
                        self.boxes
                            .push(inline_box(key, index, parley::InlineBoxKind::InFlow));
                        children.push((key, BoxFragmentType::AtomicInline));
                    }
                }
            }
        }

        // Inline elements split by the block ending the run end with it.
        for (entry, _, _) in self.open.drain(..) {
            inline.offset_mapping[entry].range.end = text.len();
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
}

impl Collector<'_> {
    fn len(&self) -> usize {
        self.text.len()
    }

    /// Append `value` in the style of `key`.
    fn push_str(&mut self, key: LayoutInputKey, value: &str) {
        self.text.push_str(value);

        match self.runs.last_mut() {
            Some((last, end)) if *last == key => *end = self.text.len(),
            _ => self.runs.push((key, self.text.len())),
        }
    }

    /// Append `c` in the style of `key`, returning its offset.
    fn push_char(&mut self, key: LayoutInputKey, c: char) -> usize {
        self.push_str(key, c.encode_utf8(&mut [0; 4]));
        self.text.len() - c.len_utf8()
    }
}

/// An item of the flow of an inline formatting context split by block level
/// boxes, returned by [`split_runs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowItem {
    /// A run of inline content, after the block level box it holds, or from
    /// the start.
    Run(Option<LayoutInputKey>),
    /// A block level box in flow.
    Block(LayoutInputKey),
}

/// Whether the inline content of `root` holds a block level box in flow,
/// which splits it into runs.
pub fn has_block_in_flow(cx: &InlineBuildContext<'_>, root: LayoutInputKey) -> bool {
    InlinePrelayout::new(cx.tree, root)
        .any(|item| matches!(item, InputItem::Block(key) if !is_out_of_flow(cx, key)))
}

/// Split the inline content of `root` into the block level boxes in flow and
/// the runs of inline content between them, in flow order, into `out`.
///
/// Runs without content are left out, so adjacent block level boxes meet.
/// Content is text, an atomic inline or out-of-flow box, or an empty inline
/// element; the rest of an inline element split by a block is not.
pub fn split_runs(cx: &InlineBuildContext<'_>, root: LayoutInputKey, out: &mut Vec<FlowItem>) {
    out.clear();

    let mut start = None;
    let mut content = false;
    // Open inline elements: whether each was opened again in this run, and
    // whether it has content.
    let mut open: Vec<(bool, bool)> = Vec::new();

    for item in InlinePrelayout::new(cx.tree, root) {
        let has_content = match item {
            InputItem::Push(_) => {
                open.push((false, false));
                false
            }
            InputItem::Pop => match open.pop() {
                Some((reopened, has_content)) => !reopened && !has_content,
                None => false,
            },
            InputItem::Text(key) => cx
                .ui
                .prop::<Text>(cx.tree.items[key].node)
                .is_some_and(|value| !value.0.is_empty()),
            InputItem::Atomic(_) => true,
            InputItem::Block(key) if is_out_of_flow(cx, key) => true,
            InputItem::Block(key) => {
                if content {
                    out.push(FlowItem::Run(start));
                }
                out.push(FlowItem::Block(key));

                start = Some(key);
                content = false;
                open.fill((true, false));
                false
            }
        };

        if has_content {
            content = true;
            for element in &mut open {
                element.1 = true;
            }
        }
    }

    if content {
        out.push(FlowItem::Run(start));
    }
}

/// Whether the box of `key` is out of flow.
fn is_out_of_flow(cx: &InlineBuildContext<'_>, key: LayoutInputKey) -> bool {
    cx.resolve_style(key).0.position() == taffy::Position::Absolute
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
