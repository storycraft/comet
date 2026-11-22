use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{FragmentItem, FragmentKey, tree::FragmentTree},
    input::{LayoutInputKey, LayoutInputKind, tree::LayoutInputTree},
    style::bx::Overflow,
};
use comet_paint::prepaint::tree::PrepaintTree;
use comet_tree::{
    node::{Node, NodeKey},
    ui::{Props, Ui},
};
use kurbo::{Affine, Vec2};
use parley::PositionedLayoutItem;

use crate::{AxFlags, AxKey, AxNode, AxNodeKind, node_id, style::Role, tree::AxTree};

/// Layout flags of an item whose node is resolved again:
/// - [`DirtyFlags::NEED_LAYER`]: a paint property of the item changed.
/// - [`DirtyFlags::NEED_CHILDREN`]: its children changed. It includes
///   [`DirtyFlags::NEED_LAYER`].
/// - [`DirtyFlags::NEED_PRE_PAINT`]: its fragments were laid out, moved or
///   resized, or their transforms changed, so its bounds, transform or text
///   runs may have changed.
const RESOLVE: DirtyFlags = DirtyFlags::NEED_LAYER.union(DirtyFlags::NEED_PRE_PAINT);

/// Builds and updates an [`AxTree`] for a layout. Keep it to reuse its
/// buffers across builds.
#[derive(Default)]
pub struct AxTreeBuilder {
    /// Items to visit.
    pending: Vec<Visit>,
    /// Children of the node being resolved.
    children: Vec<AxKey>,
    /// Existing text run nodes of the item being resolved, reused in order.
    runs: Vec<AxKey>,
}

#[derive(Clone, Copy)]
struct Visit {
    item: LayoutInputKey,
    /// Fragment whose space the parent node is in: the box of the nearest
    /// ancestor item with one. `None` for the root.
    space: Option<FragmentKey>,
    /// Whether the parent node was resolved in this build.
    parent_resolved: bool,
    /// Whether the scroll offset of `space` changed in this build, moving
    /// every node in its space.
    space_scrolled: bool,
}

/// What resolving reads from.
#[derive(Clone, Copy)]
struct Sources<'a> {
    ui: &'a Ui,
    inputs: &'a LayoutInputTree,
    fragments: &'a FragmentTree,
    prepaint: &'a PrepaintTree,
}

impl AxTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build into `tree` the nodes of the items under the layout input item
    /// `root` that need it, adding the nodes that changed to the tree's
    /// [changed](AxTree::changed) list, sent and cleared by
    /// [`AxTree::take_update`].
    ///
    /// The tree follows the layout input tree, in the order of the UI: each
    /// item has one node, whose children are the nodes of its child items. A
    /// text item's node also holds a [`TextRun`](accesskit::Role::TextRun)
    /// node for each of its glyph runs, before them, carrying their text and
    /// the metrics of their characters. Platform adapters expose text runs
    /// only through the text's node, a [`Label`](accesskit::Role::Label)
    /// unless the text has another role.
    ///
    /// An item with a box fragment has the box's border box as bounds, and
    /// the transform mapping the box into its parent node, from the
    /// pre-paint transforms. Other items, such as inline elements and texts
    /// in an inline flow, are in the space of their parent node, with the
    /// area covering their fragments as bounds. Text runs have their glyph
    /// run's box as bounds and the transform mapping it into the text's
    /// node.
    ///
    /// Transforms include the scroll offsets of scroll containers, so nodes
    /// inside follow the scroll. A box clipping its overflow
    /// [clips its children](accesskit::Node::clips_children), and a scroll
    /// container also has its scroll offset and range on both axes.
    ///
    /// The node of an item is resolved:
    /// - when its flags in `dirty` have [`DirtyFlags::NEED_LAYER`], which
    ///   [`DirtyFlags::NEED_CHILDREN`] includes, or
    ///   [`DirtyFlags::NEED_PRE_PAINT`],
    /// - when it is [`AxFlags::DIRTY`] in `tree`,
    /// - when it is new, or has a child item without a node,
    /// - when its parent node was resolved and its box is not inside the
    ///   parent's space in the fragment tree, as its transform depends on
    ///   both,
    /// - when the scroll offset of the box whose space it is in changed,
    ///   along with the items in that space without a box of their own.
    ///
    /// Items are visited from the root, going into the children of an item
    /// only when its flags or child bits in `dirty` have one of those layout
    /// flags, when it is [`AxFlags::CHILD_DIRTY`], or when its node is new.
    /// Resolved nodes equal to their previous node are not added to the
    /// changed list. Nodes of items no longer listed by their parent are
    /// deleted with it.
    ///
    /// When the root item has no resolved node in `tree`, such as on the
    /// first build or after a new root, the tree is cleared and every node is
    /// resolved, so the next update sends the whole tree.
    ///
    /// Build after resolving `prepaint` for `fragments`, and before clearing
    /// `dirty`. Returns the root node, the node of `root`, or `None` if
    /// `root` has no box fragment, or if an item
    /// no longer matches the UI or a fragment has no pre-paint data. Nodes
    /// resolved before the failure stay in `tree`, without their flags
    /// cleared, so the next build resolves them again; build from a
    /// [cleared](AxTree::clear) tree to send a complete tree again.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        dirty: &DirtyMap,
        fragments: &FragmentTree,
        prepaint: &PrepaintTree,
        tree: &mut AxTree,
        root: LayoutInputKey,
    ) -> Option<AxKey> {
        fragments.box_fragment(root)?;
        let sources = Sources {
            ui,
            inputs,
            fragments,
            prepaint,
        };
        let Self {
            pending,
            children,
            runs,
        } = self;
        pending.clear();

        // A root without a resolved node, new or never built, resolves every
        // node.
        let root_resolved = tree
            .item_node(root)
            .and_then(|key| tree.get(key))
            .is_some_and(|node| node.resolved && tree.nodes.parent(tree.items[root]).is_none());
        if !root_resolved {
            tree.clear();
        }
        let root_key = tree.item_node_or_insert(root);

        pending.push(Visit {
            item: root,
            space: None,
            parent_resolved: true,
            space_scrolled: false,
        });

        while let Some(Visit {
            item,
            space,
            parent_resolved,
            space_scrolled,
        }) = pending.pop()
        {
            inputs.items.get(item)?;
            let key = tree.item_node_or_insert(item);
            let entry = dirty.entry(item);
            let flags = tree.flags(item);
            let missing = !tree.nodes[key].resolved;
            let own_box = fragments.box_fragment(item);

            // Items without a box are in the space of their parent node, so
            // their children are in the scrolled space too.
            let descend = missing
                || flags.contains(AxFlags::CHILD_DIRTY)
                || (entry.flags | entry.child_bits).intersects(RESOLVE)
                || (space_scrolled && own_box.is_none());

            let resolve = missing
                || space_scrolled
                || flags.contains(AxFlags::DIRTY)
                || entry.flags.intersects(RESOLVE)
                || (descend
                    && inputs
                        .items
                        .children(item)
                        .any(|child| tree.item_node(child).is_none()))
                || (parent_resolved
                    && own_box
                        .zip(space)
                        .is_some_and(|(own, space)| !sources.contains(space, own)));

            let previous_scroll = scroll_of(&tree.nodes[key].node);
            if resolve {
                sources.resolve(tree, key, item, own_box, space, children, runs)?;
            }

            // The children of a box are in its space, moved by its scroll.
            let space_scrolled = match own_box {
                Some(_) => resolve && scroll_of(&tree.nodes[key].node) != previous_scroll,
                None => space_scrolled,
            };
            let descend = descend || space_scrolled;

            if descend {
                let space = own_box.or(space);
                // Popped in order.
                let start = pending.len();
                pending.extend(inputs.items.children(item).map(|item| Visit {
                    item,
                    space,
                    parent_resolved: resolve,
                    space_scrolled,
                }));
                pending[start..].reverse();
            }

            tree.flags.remove(item);
        }

        Some(root_key)
    }
}

impl Sources<'_> {
    /// Resolve the node of `item` at `key`, with its text run nodes, and
    /// list the nodes of its child items, inserting the missing ones. `own`
    /// is its box, and `space` the fragment whose space its parent node is
    /// in.
    #[allow(clippy::too_many_arguments)]
    fn resolve(
        &self,
        tree: &mut AxTree,
        key: AxKey,
        item: LayoutInputKey,
        own: Option<FragmentKey>,
        space: Option<FragmentKey>,
        children: &mut Vec<AxKey>,
        runs: &mut Vec<AxKey>,
    ) -> Option<()> {
        let input = self.inputs.items.get(item)?;
        let props = self.ui.props(input.node)?;

        let role = match props.get_cloned() {
            Some(Role(role)) => role,
            None => self.default_role(input.node, own),
        };
        let mut node = accesskit::Node::new(role);

        match (own, space) {
            (Some(own), space) => {
                let size = self.fragments.items.get(own)?.layout.size;
                node.set_bounds(accesskit::Rect::new(0.0, 0.0, size.width, size.height));
                let transform = match space {
                    Some(space) => self.relative_transform(own, space)?,
                    // The root has no parent node.
                    None => self.prepaint.fragment(own)?.transform,
                };
                if transform != Affine::IDENTITY {
                    node.set_transform(accesskit::Affine::new(transform.as_coeffs()));
                }
                self.set_scroll(&mut node, item, own, props)?;
            }
            // In the space of the parent node, covering its fragments.
            (None, Some(space)) => {
                let mut bounds: Option<kurbo::Rect> = None;
                for fragment in self.fragments.fragments(item) {
                    let size = self.fragments.items.get(fragment)?.layout.size;
                    let rect = self
                        .relative_transform(fragment, space)?
                        .transform_rect_bbox(kurbo::Rect::from_origin_size(
                            (0.0, 0.0),
                            (size.width, size.height),
                        ));
                    bounds = Some(bounds.map_or(rect, |bounds| bounds.union(rect)));
                }
                if let Some(bounds) = bounds {
                    node.set_bounds(accesskit::Rect::new(
                        bounds.x0, bounds.y0, bounds.x1, bounds.y1,
                    ));
                }
            }
            (None, None) => return None,
        }

        // Text runs first, reusing the existing ones in order.
        children.clear();
        runs.clear();
        runs.extend(
            tree.nodes
                .children(key)
                .filter(|&child| tree.nodes[child].kind == AxNodeKind::TextRun),
        );
        if input.kind == LayoutInputKind::Text {
            let run_space = own.or(space)?;
            let glyph_runs: Vec<_> = self.glyph_runs(item, own).collect();
            for index in 0..glyph_runs.len() {
                let run_key = runs.get(index).copied().unwrap_or_else(|| {
                    tree.nodes.insert(AxNode {
                        node: accesskit::Node::default(),
                        item,
                        kind: AxNodeKind::TextRun,
                        resolved: false,
                    })
                });
                children.push(run_key);
            }
            for (index, &run) in glyph_runs.iter().enumerate() {
                let previous = index
                    .checked_sub(1)
                    .map(|index| (glyph_runs[index], children[index]));
                let next = glyph_runs
                    .get(index + 1)
                    .map(|&run| (run, children[index + 1]));
                let run_node = self.text_run(run, run_space, previous, next)?;
                tree.set_node(children[index], run_node);
            }
        }

        for child in self.inputs.items.children(item) {
            children.push(tree.item_node_or_insert(child));
        }

        node.set_children(children.iter().copied().map(node_id).collect::<Vec<_>>());
        tree.set_children(key, children);
        tree.set_node(key, node);

        Some(())
    }

    /// Set the clip and scroll of the node of `item`, generating the box
    /// `own`, from its pre-paint data and `props`. Scroll containers,
    /// scrolling or hiding their overflow on an axis, get their scroll
    /// offset and range on both axes.
    fn set_scroll(
        &self,
        node: &mut accesskit::Node,
        item: LayoutInputKey,
        own: FragmentKey,
        props: Props<'_>,
    ) -> Option<()> {
        let Some(data) = self.prepaint.get(item) else {
            return Some(());
        };

        if data.clip.0 || data.clip.1 {
            node.set_clips_children();
        }

        let Overflow(overflow) = props.get_or_default();
        if overflow.x.is_scroll_container() || overflow.y.is_scroll_container() {
            let range = self.fragments.scroll_range(own)?;
            node.set_scroll_x(data.scroll.x);
            node.set_scroll_x_min(0.0);
            node.set_scroll_x_max(range.x);
            node.set_scroll_y(data.scroll.y);
            node.set_scroll_y_min(0.0);
            node.set_scroll_y_max(range.y);
        }

        Some(())
    }

    /// The role of a node without one, generating `own` if it is a box:
    /// texts are labels, boxes laying out one inline formatting context
    /// groups, and other boxes generic containers. A box split by blocks
    /// owns no inline layout: its anonymous blocks do.
    fn default_role(&self, node: NodeKey, own: Option<FragmentKey>) -> accesskit::Role {
        if self.ui.kind(node) == Some(Node::Text) {
            accesskit::Role::Label
        } else if own.is_some_and(|own| self.fragments.get_inline(own).is_some()) {
            accesskit::Role::Group
        } else {
            accesskit::Role::GenericContainer
        }
    }

    /// The glyph runs of a text item in order: its linked fragments in an
    /// inline flow, or the runs inside its own box, which are not linked
    /// with the box.
    fn glyph_runs(
        &self,
        item: LayoutInputKey,
        own: Option<FragmentKey>,
    ) -> impl Iterator<Item = FragmentKey> {
        let items = &self.fragments.items;
        let in_box = own.map(|own| items.descendants(own)).into_iter().flatten();
        let linked = own
            .is_none()
            .then(|| self.fragments.fragments(item))
            .into_iter()
            .flatten();

        in_box.chain(linked).filter(move |&key| {
            matches!(items[key].item, FragmentItem::GlyphRun { .. }) && items[key].source == item
        })
    }

    /// Whether `ancestor` is `key` or one of its ancestors.
    fn contains(&self, ancestor: FragmentKey, key: FragmentKey) -> bool {
        let items = &self.fragments.items;
        core::iter::successors(Some(key), |&key| items.parent(key)).any(|key| key == ancestor)
    }

    /// The transform mapping `key` into `space`, moved by the scroll offsets
    /// of the fragments it is inside of, `space` included. Without `space`
    /// among the ancestors of `key`, it goes through the root fragment.
    fn relative_transform(&self, key: FragmentKey, space: FragmentKey) -> Option<Affine> {
        let items = &self.fragments.items;
        let mut transform = Affine::IDENTITY;
        let mut current = Some(key);

        // Usually an ancestor: no inverse is needed.
        while let Some(key) = current {
            if key == space {
                return Some(transform);
            }
            transform = self.prepaint.fragment(key)?.transform * transform;
            current = items.parent(key);
            if let Some(parent) = current {
                transform = Affine::translate(-self.scroll(parent)) * transform;
            }
        }

        let space = self.root_transform(space)? * Affine::translate(-self.scroll(space));
        Some(space.inverse() * transform)
    }

    /// The transform mapping `key` into the space of the root fragment's
    /// parent, moved by the scroll offsets of its ancestors.
    fn root_transform(&self, key: FragmentKey) -> Option<Affine> {
        let items = &self.fragments.items;
        let mut transform = Affine::IDENTITY;
        let mut current = Some(key);

        while let Some(key) = current {
            transform = self.prepaint.fragment(key)?.transform * transform;
            current = items.parent(key);
            if let Some(parent) = current {
                transform = Affine::translate(-self.scroll(parent)) * transform;
            }
        }

        Some(transform)
    }

    /// The scroll offset a fragment applies to its descendants.
    fn scroll(&self, key: FragmentKey) -> Vec2 {
        self.fragments
            .items
            .get(key)
            .map_or(Vec2::ZERO, |fragment| self.prepaint.scroll_offset(fragment))
    }

    /// The line box containing a glyph run.
    fn line(&self, key: FragmentKey) -> Option<FragmentKey> {
        let items = &self.fragments.items;
        core::iter::successors(items.parent(key), |&key| items.parent(key))
            .find(|&key| matches!(items[key].item, FragmentItem::Line { .. }))
    }

    /// The [`TextRun`](accesskit::Role::TextRun) node of a glyph run, in the
    /// space of `space`, from the inline layout of the line containing it,
    /// as `accesskit` expects: its value, the length of each character,
    /// their positions and widths in the text direction, the characters
    /// starting words, and the runs of the same text before and after it on
    /// the same line, given with their node keys.
    ///
    /// A character is a cluster: the smallest unit of text the layout
    /// places.
    fn text_run(
        &self,
        key: FragmentKey,
        space: FragmentKey,
        previous: Option<(FragmentKey, AxKey)>,
        next: Option<(FragmentKey, AxKey)>,
    ) -> Option<accesskit::Node> {
        let fragment = self.fragments.items.get(key)?;
        let FragmentItem::GlyphRun { line, item } = fragment.item else {
            return None;
        };

        let mut node = accesskit::Node::new(accesskit::Role::TextRun);
        let size = fragment.layout.size;
        node.set_bounds(accesskit::Rect::new(0.0, 0.0, size.width, size.height));
        let transform = self.relative_transform(key, space)?;
        if transform != Affine::IDENTITY {
            node.set_transform(accesskit::Affine::new(transform.as_coeffs()));
        }

        let line_key = self.line(key)?;
        let inline = self.fragments.inline_layout(line_key)?;
        let Some(PositionedLayoutItem::GlyphRun(run)) = inline
            .layout
            .get(line)
            .and_then(|line| line.items().nth(item))
        else {
            return Some(node);
        };

        // The shaping run may hold the text of other items, whose styles
        // differ: only the clusters of the glyph run's style are its text,
        // in logical order.
        let style = run.style_index();
        let mut range: Option<core::ops::Range<usize>> = None;
        let mut lengths = Vec::new();
        let mut positions = Vec::new();
        let mut widths = Vec::new();
        let mut word_starts = Vec::new();
        let mut position = 0.0;
        for cluster in run
            .run()
            .clusters()
            .filter(|cluster| cluster.style_index() == style)
        {
            let text = cluster.text_range();
            // Word starts are bytes in `accesskit`: later ones are left out.
            if cluster.is_word_boundary()
                && let Ok(index) = u8::try_from(lengths.len())
            {
                word_starts.push(index);
            }
            // A run has one direction, so the first character in logical
            // order is the first in the text direction.
            lengths.push(u8::try_from(text.len()).unwrap_or(u8::MAX));
            positions.push(position);
            widths.push(cluster.advance());
            position += cluster.advance();
            range = Some(match range {
                Some(range) => range.start.min(text.start)..range.end.max(text.end),
                None => text,
            });
        }
        let Some(value) = range.and_then(|range| inline.text.get(range)) else {
            return Some(node);
        };

        node.set_value(value);
        node.set_character_lengths(lengths);
        node.set_character_positions(positions);
        node.set_character_widths(widths);
        node.set_word_starts(word_starts);
        node.set_text_direction(if run.run().is_rtl() {
            accesskit::TextDirection::RightToLeft
        } else {
            accesskit::TextDirection::LeftToRight
        });

        // Lines are only followed within one text, whose node holds them.
        if let Some((run, run_key)) = previous
            && self.line(run) == Some(line_key)
        {
            node.set_previous_on_line(node_id(run_key));
        }
        if let Some((run, run_key)) = next
            && self.line(run) == Some(line_key)
        {
            node.set_next_on_line(node_id(run_key));
        }

        Some(node)
    }
}

/// The scroll offset set on a node, to find nodes whose scroll changed.
fn scroll_of(node: &accesskit::Node) -> (Option<f64>, Option<f64>) {
    (node.scroll_x(), node.scroll_y())
}
