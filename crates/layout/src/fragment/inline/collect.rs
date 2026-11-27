//! Collecting and shaping the inline content of an inline formatting
//! context into its [`InlineLayout`].
//!
//! Only the text side is built here, with Parley's tree builder, which also
//! processes white space. The boxes inside the inline content are returned
//! in flow order for the fragment tree builder, which reconciles them with
//! the child boxes of the owner in place.
//!
//! Block level boxes in flow split the inline content into runs, see
//! [`split_runs`]. Each run is built on its own, and the inline elements
//! split by a block are open in the runs on both sides of it.

use std::sync::Arc;

use comet_tree::{node::Text, ui::Ui};
use parley::{StyleProperty, WhiteSpaceCollapse};

use super::{
    InlineLayout, OffsetEntry, inline_box_id,
    input::{InlinePrelayout, InputItem},
};
use crate::{
    fragment::{BoxFragmentType, NodeLayoutBits, builder::classify},
    input::{LayoutInputKey, LayoutInputKind, text::SpanStyle, tree::LayoutInputTree},
    style::LayoutConstraint,
    taffy::style::{self, Style},
};

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
    /// Text has no box properties: its box, if any, has the
    /// [anonymous](style::anonymous) style.
    pub fn resolve_style(&self, key: LayoutInputKey) -> (Style, NodeLayoutBits) {
        let item = &self.tree.items[key];
        if item.kind == LayoutInputKind::Text {
            return (style::anonymous(self.constraint), NodeLayoutBits::empty());
        }

        let mut cx = self.constraint.clone();
        cx.units.font_size = f64::from(item.text_style.span.font_size);

        let mut bits = NodeLayoutBits::empty();
        let style = style::resolve_tracked(self.ui.props(item.node), &cx, &mut |unit| {
            bits |= NodeLayoutBits::of_unit(unit)
        });

        (style, bits)
    }
}

/// Collects and shapes inline content with Parley's tree builder. Keep it to
/// reuse its buffers.
#[derive(Default)]
pub struct InlineContentBuilder {
    lcx: parley::LayoutContext<Option<LayoutInputKey>>,
    /// Inline elements open at the current position: their offset entry, the
    /// number of inline boxes pushed before them, and whether they were
    /// opened again at the start of the run.
    open: Vec<(usize, usize, bool)>,
    /// Inline elements open again at the start of the run being built.
    reopened: Vec<LayoutInputKey>,
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
    /// start, up to the next block level box in flow. The Parley layout and
    /// the other buffers of `inline` are reused. The boxes inside are left in
    /// [`Self::children`], and [`InlineLayout::atomics`] is left empty.
    ///
    /// White space is processed by Parley, following the white space style
    /// of each item: collapsible white space at the start and end of the run
    /// is removed. A space collapsed from the white space ending a text item
    /// belongs to that item. An out-of-flow box takes no character: its
    /// Parley inline box only gives its static position.
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

        let tree = cx.tree;
        let Self {
            lcx,
            open,
            reopened,
            children,
        } = self;
        open.clear();
        children.clear();

        let mut builder = Builder {
            tb: lcx.tree_builder(fonts, 1.0, false, &span_of(tree, root)),
            tree,
            offsets: &mut inline.offset_mapping,
            open,
            parents: vec![root],
            boxes: 0,
            last_text: None,
        };

        let prelayout = match start {
            Some(block) => {
                // The inline elements split by the block are open again.
                InlinePrelayout::open_around(tree, root, block, reopened);

                for &key in reopened.iter() {
                    inline.content_bits |= content_bits(cx, key);
                    builder.push_element(key, true);
                }

                InlinePrelayout::after(tree, root, block)
            }
            None => InlinePrelayout::new(tree, root),
        };

        for item in prelayout {
            match item {
                InputItem::Push(key) => {
                    inline.content_bits |= content_bits(cx, key);
                    builder.push_element(key, false);
                }
                InputItem::Pop => builder.pop_element(),
                InputItem::Text(key) => {
                    let value = cx.ui.prop::<Text>(tree.items[key].node);
                    builder.push_text(key, value.as_deref().map_or("", |text| &text.0));
                }
                InputItem::Atomic(key) | InputItem::Block(key) => {
                    let out_of_flow = is_out_of_flow(cx, key);

                    // A block level box in flow ends the run, splitting the
                    // inline elements open around it.
                    if !out_of_flow && matches!(item, InputItem::Block(_)) {
                        break;
                    }

                    if out_of_flow {
                        builder.push_box(key, parley::InlineBoxKind::OutOfFlow);
                        children.push((key, BoxFragmentType::OutOfFlow));
                    } else {
                        builder.push_box(key, parley::InlineBoxKind::InFlow);
                        children.push((key, BoxFragmentType::AtomicInline));
                    }
                }
            }
        }

        builder.finish(&mut inline.text, &mut inline.layout);
    }
}

/// Builds the text of a run with its offsets, through Parley's tree builder.
struct Builder<'a, 'b> {
    tb: parley::TreeBuilder<'b, Option<LayoutInputKey>>,
    tree: &'a LayoutInputTree,
    offsets: &'a mut Vec<OffsetEntry>,
    open: &'a mut Vec<(usize, usize, bool)>,
    /// The item whose text style the content uses: the innermost open
    /// inline element, or the root.
    parents: Vec<LayoutInputKey>,
    /// Number of inline boxes pushed.
    boxes: usize,
    /// Offset entry of the latest text item, which owns the space its
    /// trailing white space may collapse into.
    last_text: Option<usize>,
}

impl Builder<'_, '_> {
    fn len(&self) -> usize {
        self.tb.text().len()
    }

    /// Push the style span of `key`. Items sharing the style of their parent
    /// only brand their content with their key.
    fn push_span(&mut self, key: LayoutInputKey) {
        let parent = self.parents.last().copied().unwrap_or(key);
        let items = &self.tree.items;

        if parent != key && Arc::ptr_eq(&items[key].text_style, &items[parent].text_style) {
            self.tb
                .push_style_modification_span(&[StyleProperty::Brush(Some(key))]);
        } else {
            self.tb.push_style_span(span_of(self.tree, key));
        }
    }

    /// Run `push`, which commits content, giving a space flushed for pending
    /// white space to the text item it was collapsed from. Returns where the
    /// content starts.
    fn commit(&mut self, push: impl FnOnce(&mut Self)) -> usize {
        let pending = self.tb.has_pending_whitespace();
        let start = self.len();
        push(self);

        let flushed = match self.tb.text().get(start..) {
            Some(rest) if pending => rest.chars().next().filter(|c| c.is_whitespace()),
            _ => None,
        };

        match (flushed, self.last_text) {
            (Some(space), Some(entry)) => {
                let len = space.len_utf8();
                self.offsets[entry].range.end += len;

                // Inline elements opened since then start after it too.
                for &(open, _, _) in self.open.iter() {
                    let range = &mut self.offsets[open].range;
                    if range.start == start {
                        range.start += len;
                        range.end = range.start;
                    }
                }

                start + len
            }
            _ => start,
        }
    }

    /// Open the inline element `key`, opened again for a run when
    /// `reopened`.
    fn push_element(&mut self, key: LayoutInputKey, reopened: bool) {
        let start = self.len();
        self.open.push((self.offsets.len(), self.boxes, reopened));
        self.offsets.push(OffsetEntry {
            source: key,
            range: start..start,
            text: false,
        });
        self.push_span(key);
        self.parents.push(key);
    }

    /// Close the innermost inline element. An empty one still takes a
    /// position on a line, as a zero sized box. The rest of one split by a
    /// block does not.
    fn pop_element(&mut self) {
        let Some((entry, boxes, reopened)) = self.open.pop() else {
            return;
        };
        let key = self.parents.pop().unwrap_or(self.offsets[entry].source);

        if !reopened && self.len() == self.offsets[entry].range.start && self.boxes == boxes {
            self.push_box(key, parley::InlineBoxKind::InFlow);
        }

        self.tb.pop_style_span();
        self.offsets[entry].range.end = self.len();
    }

    /// Push the text item `key` with its text.
    fn push_text(&mut self, key: LayoutInputKey, value: &str) {
        let start = self.commit(|builder| {
            builder.push_span(key);
            builder.tb.push_text(value);
            builder.tb.pop_style_span();
        });

        self.last_text = Some(self.offsets.len());
        self.offsets.push(OffsetEntry {
            source: key,
            range: start..self.len().max(start),
            text: true,
        });
    }

    /// Push the inline box of `key`.
    fn push_box(&mut self, key: LayoutInputKey, kind: parley::InlineBoxKind) {
        self.commit(|builder| {
            builder.tb.push_inline_box(parley::InlineBox {
                id: inline_box_id(key),
                index: 0,
                width: 0.0,
                height: 0.0,
                kind,
                baseline: None,
                vertical_align: Default::default(),
            });
        });
        self.boxes += 1;
    }

    /// Close the inline elements split by the block ending the run, and
    /// build the Parley layout into `layout` and its text into `text`.
    fn finish(self, text: &mut String, layout: &mut parley::Layout<Option<LayoutInputKey>>) {
        let end = self.len();
        for (entry, _, _) in self.open.drain(..) {
            self.offsets[entry].range.end = end;
        }

        *text = self.tb.build_into(layout);
    }
}

/// The propagated bits of the box style of the inline element `key`.
fn content_bits(cx: &InlineBuildContext<'_>, key: LayoutInputKey) -> NodeLayoutBits {
    cx.resolve_style(key).1 & NodeLayoutBits::PROPAGATED
}

/// The text style of an item, branded with the item so glyph runs can be
/// traced back to their source.
fn span_of(tree: &LayoutInputTree, key: LayoutInputKey) -> SpanStyle {
    SpanStyle {
        brush: Some(key),
        ..tree.items[key].text_style.span.clone()
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
/// Content is text left after white space processing, an atomic inline or
/// out-of-flow box, or an empty inline element; the rest of an inline element
/// split by a block is not.
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
            InputItem::Text(key) => has_text(cx, key),
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
    classify::is_out_of_flow(&cx.resolve_style(key).0)
}

/// Whether the text item `key` has text left after white space processing.
fn has_text(cx: &InlineBuildContext<'_>, key: LayoutInputKey) -> bool {
    let item = &cx.tree.items[key];
    let Some(value) = cx.ui.prop::<Text>(item.node) else {
        return false;
    };

    match item.text_style.span.white_space_collapse {
        WhiteSpaceCollapse::Collapse => value.0.chars().any(|c| !c.is_ascii_whitespace()),
        WhiteSpaceCollapse::PreserveBreaks => value.0.chars().any(|c| !matches!(c, ' ' | '\t')),
        WhiteSpaceCollapse::Preserve | WhiteSpaceCollapse::BreakSpaces => !value.0.is_empty(),
    }
}
