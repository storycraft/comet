//! Building the box fragment of an inline formatting context and shaping its
//! inline content.

use comet_tree::{node::Text, ui::Ui};
use rustc_hash::FxHashMap;
use taffy::style::CoreStyle;

use super::{
    InlineLayout, OffsetEntry, inline_box_id,
    input::{InlinePrelayout, InputItem},
};
use crate::{
    fragment::state::NodeLayoutBits,
    fragment2::{BoxFragment, BoxFragmentKey, BoxFragmentType, ChildLayoutType, FragmentTree},
    input::{LayoutInputKey, LayoutInputKind, SpanStyle, tree::LayoutInputTree},
    style::LayoutConstraint,
    taffy::style::{self, Style},
};

/// Object replacement character standing for an in-flow inline box.
const OBJECT_REPLACEMENT: char = '\u{FFFC}';

/// What [`InlineLayoutBuilder::build`] reads from.
pub struct InlineBuildContext<'a> {
    pub ui: &'a Ui,
    pub tree: &'a LayoutInputTree,
    pub fonts: &'a mut parley::FontContext,
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

/// A box inside inline content to build: an atomic inline, inline part or
/// out-of-flow box. Returned by [`InlineFragmentainer::next`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildBlockData {
    /// Item whose box needs to be built.
    pub item: LayoutInputKey,
    /// How the box takes part in the inline layout.
    pub box_type: BoxFragmentType,
    /// Box fragment establishing the inline formatting context, which the
    /// built box is a child of.
    pub parent: BoxFragmentKey,
    /// The child of [`Self::parent`] to attach the built box after, which may
    /// be anonymous: the box attached for the previous child. `None` attaches
    /// it as the first child.
    pub after: Option<BoxFragmentKey>,
}

impl ChildBlockData {
    /// Attach the built box `child` where it belongs.
    pub fn attach(&self, fragments: &mut FragmentTree, child: BoxFragmentKey) {
        match self.after {
            Some(after) => fragments.boxes.after(after, child),
            None => fragments.boxes.prepend(self.parent, child),
        };
    }
}

/// Yields the boxes inside the inline content of a box built by
/// [`InlineLayoutBuilder::build`], in flow order, for the caller to build and
/// attach. Building them may build nested inline formatting contexts with
/// the same builder.
#[must_use = "boxes inside the inline content are only built by draining it"]
#[derive(Debug)]
pub struct InlineFragmentainer {
    parent: BoxFragmentKey,
    children: std::vec::IntoIter<(LayoutInputKey, BoxFragmentType)>,
    /// Child of `parent` the previous box was asked to be attached after.
    after: Option<BoxFragmentKey>,
    /// Whether a box was yielded, so the next call looks for it after
    /// `after`.
    yielded: bool,
}

impl InlineFragmentainer {
    /// The next box to build, or `None` once every box inside the inline
    /// content was yielded.
    ///
    /// Attach the box built for the previous [`ChildBlockData`] with
    /// [`ChildBlockData::attach`] before calling it again: the box following
    /// [`ChildBlockData::after`] is taken as that box, and the next one is
    /// attached after it. Nothing is skipped when it was not attached.
    pub fn next(&mut self, fragments: &mut FragmentTree) -> Option<ChildBlockData> {
        if std::mem::take(&mut self.yielded) {
            let attached = match self.after {
                Some(after) => fragments.boxes.next_sibling(after),
                None => fragments.boxes.first_child(self.parent),
            };

            if attached.is_some() {
                self.after = attached;
            }
        }

        let (item, box_type) = self.children.next()?;
        self.yielded = true;

        Some(ChildBlockData {
            item,
            box_type,
            parent: self.parent,
            after: self.after,
        })
    }

    /// Number of boxes left to yield.
    pub fn len(&self) -> usize {
        self.children.len()
    }

    pub fn is_empty(&self) -> bool {
        self.children.len() == 0
    }
}

/// Builds the box fragment of an inline formatting context with its
/// [`InlineLayout`], shaped but not laid out. Keep it to reuse its buffers.
#[derive(Default)]
pub struct InlineLayoutBuilder {
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
    /// Inline layouts taken from deleted box fragments, reused by later
    /// builds until [`InlineLayoutBuilder::release`]. Boxed as owned by
    /// [`ChildLayoutType::Inline`], so moving them keeps their allocations.
    #[allow(clippy::vec_box)]
    pool: Vec<Box<InlineLayout>>,
    /// Box fragments of a subtree being deleted.
    stale: Vec<BoxFragmentKey>,
}

impl InlineLayoutBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the detached box fragment of `root`, which establishes an
    /// inline formatting context.
    ///
    /// The root box is a `box_type` box owning the [`InlineLayout`] of its
    /// inline content, shaped but not laid out: its items are empty. The
    /// returned [`InlineFragmentainer`] yields the boxes inside the inline
    /// content, in flow order, for the caller to build and attach to the
    /// root box.
    ///
    /// `old` is a box fragment built for `root` before, such as by a previous
    /// build. It is deleted from `fragments` with its subtree, and the inline
    /// layouts in that subtree are reused by this build and by later ones,
    /// such as those of nested inline formatting contexts, keeping their text
    /// and Parley layout allocations. Box fragment keys are not kept.
    ///
    /// Text is taken as is, without white space processing. A block level box
    /// inside the inline content is an inline part box on its own line,
    /// broken from the content around it by line feeds.
    #[must_use = "boxes inside the inline content are only built by draining the fragmentainer"]
    pub fn build(
        &mut self,
        cx: &mut InlineBuildContext<'_>,
        fragments: &mut FragmentTree,
        root: LayoutInputKey,
        box_type: BoxFragmentType,
        old: Option<BoxFragmentKey>,
    ) -> (BoxFragmentKey, InlineFragmentainer) {
        if let Some(old) = old {
            self.reclaim(fragments, root, old);
        }

        let mut inline = self.pool.pop().unwrap_or_default();
        inline.clear();

        let mut children = Vec::new();
        self.collect(cx, root, &mut inline, &mut children);
        self.shape(cx, root, &mut inline);

        let (style, bits) = cx.resolve_style(root);
        let mut fragment = BoxFragment::new(root, box_type, ChildLayoutType::Inline(inline), style);
        fragment.bits = bits;
        let key = fragments.insert(fragment);

        let fragmentainer = InlineFragmentainer {
            parent: key,
            children: children.into_iter(),
            after: None,
            yielded: false,
        };

        (key, fragmentainer)
    }

    /// Drop the inline layouts kept for reuse.
    pub fn release(&mut self) {
        self.pool.clear();
    }

    /// Delete the box fragment `old` built for `root` and its subtree,
    /// keeping their inline layouts for reuse. Boxes of other items, or
    /// missing from `fragments`, are left as they are.
    fn reclaim(&mut self, fragments: &mut FragmentTree, root: LayoutInputKey, old: BoxFragmentKey) {
        if fragments
            .get(old)
            .is_none_or(|fragment| fragment.source != root)
        {
            return;
        }

        self.stale.clear();
        self.stale.extend(fragments.boxes.descendants(old));

        for &key in &self.stale {
            let fragment = &mut fragments.boxes[key];

            if let ChildLayoutType::Inline(inline) =
                std::mem::replace(&mut fragment.child_layout, ChildLayoutType::Content)
            {
                self.pool.push(inline);
            }
        }

        fragments.delete(old);
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
                InputItem::Atomic(key) => {
                    let parent = parents.last().copied().unwrap_or(root);
                    let box_type =
                        if cx.resolve_style(key).0.position() == taffy::Position::Absolute {
                            BoxFragmentType::OutOfFlow
                        } else if tree.items[key].kind == LayoutInputKind::Block {
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
        cx: &mut InlineBuildContext<'_>,
        root: LayoutInputKey,
        inline: &mut InlineLayout,
    ) {
        let tree = cx.tree;
        let InlineLayout { text, layout, .. } = inline;
        let mut builder = self.lcx.style_run_builder(cx.fonts, text, 1.0, false);
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
    fn clear(&mut self) {
        self.text.clear();
        self.offset_mapping.clear();
        self.items.clear();
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
