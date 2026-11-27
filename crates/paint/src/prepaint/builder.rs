use anyrender::Paint;
use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{BoxFragment, BoxFragmentKey, tree::FragmentTree},
    input::{LayoutInputKey, tree::LayoutInputTree},
    style::{
        bx::Overflow,
        geometry::{AutoValue, Rect, StyleUnit, UnitResolveContext},
    },
};
use comet_tree::ui::{Props, Ui};
use kurbo::{RoundedRectRadii, Size};
use peniko::Color;
use taffy::Overflow as OverflowMode;

use crate::{
    prepaint::{
        DEFAULT_SELECTION_BACKGROUND, DEFAULT_SELECTION_COLOR, FragmentData, LayoutInputData,
        fragment::{BoxRole, boxes_of, paint_props},
        tree::PrepaintTree,
    },
    style::{
        BorderRadius, OutlineOffset, OutlineWidth, Selectable, SelectionColor, SelectionFill,
        TextColor, UserSelect,
    },
};

/// Builds a [`PrepaintTree`] for a fragment tree. Keep it to reuse its
/// buffers across resolves.
#[derive(Default)]
pub struct PrepaintTreeBuilder {
    /// Boxes to visit.
    pending: Vec<BoxFragmentKey>,
    /// Layout input items to visit, and whether to resolve them whatever
    /// their flags, as their parent's inherited values changed.
    items: Vec<(LayoutInputKey, bool)>,
    /// Items whose paint values use `vw` or `vh`, with their depth.
    dependents: Vec<(usize, LayoutInputKey)>,
    /// Boxes to resolve.
    changed: Vec<BoxFragmentKey>,
}

/// What resolving reads from.
#[derive(Clone, Copy)]
struct Sources<'a> {
    ui: &'a Ui,
    inputs: &'a LayoutInputTree,
    fragments: &'a FragmentTree,
    units: &'a UnitResolveContext,
}

impl PrepaintTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds into `tree` the layout input items and box fragments that need
    /// it.
    ///
    /// Items are visited along the input tree, from the item of `root`. An
    /// item is resolved when:
    /// - its [`DirtyEntry::flags`](comet_layout::dirty_map::DirtyEntry::flags)
    ///   in `dirty` have [`DirtyFlags::NEED_PRE_PAINT`]: those whose font size
    ///   the input builder resolved to a new value, those whose boxes the
    ///   fragment builder created, moved or resized, and those marked with it,
    ///   such as after their paint styles changed,
    /// - it has no data in `tree`,
    /// - its parent was resolved to new inherited values, such as a new
    ///   [text color](LayoutInputData::text_color), or its parent's child
    ///   list changed ([`DirtyFlags::NEED_CHILDREN`]).
    ///
    /// The children of an item are visited when it is resolved to new
    /// inherited values, or when its flags or
    /// [`DirtyEntry::child_bits`](comet_layout::dirty_map::DirtyEntry::child_bits)
    /// have [`DirtyFlags::NEED_PRE_PAINT`] or [`DirtyFlags::NEED_CHILDREN`].
    /// Text items, which place no box, are resolved too, for their text color.
    ///
    /// Box fragments are resolved when their item is flagged
    /// [`DirtyFlags::NEED_PRE_PAINT`] or when they have no data in `tree`.
    /// Boxes are visited from `root`, going into the children of a box only
    /// when its item's flags or child bits have
    /// [`DirtyFlags::NEED_PRE_PAINT`], or when it has no data in `tree`.
    /// Anonymous boxes carry the item of the box owning them, and inline part
    /// boxes the item of their inline element, inside it, so the former
    /// reaches them. Box data only depends on the box and its item's props,
    /// so changes of a parent never resolve its child boxes. Inline items
    /// placing no box, such as lines and glyph runs, get no box data. A
    /// [cleared](PrepaintTree::clear) `tree` resolves every item and box.
    ///
    /// `dirty` is only read: the layer build reads it too, so call
    /// [`DirtyMap::clear`] after building layers.
    ///
    /// Changes builds cannot see, such as paint styles, must be marked.
    ///
    /// With `viewport_changed`, every item whose paint values, or those of
    /// its boxes, use `vw` or `vh` is resolved again with all of its boxes,
    /// and its descendants when its inherited values change.
    /// Pass it when the viewport in `units` differs from the last successful
    /// call. The root font size is not tracked: resolve into a new or
    /// [cleared](PrepaintTree::clear) tree when it changes.
    ///
    /// `inputs` is the input tree `fragments` were built from, and `tree`
    /// belongs to them. `units` resolves border radii and outlines, with each
    /// item's font size.
    ///
    /// Returns `None` if a box or item to resolve no longer matches the UI,
    /// such as when its source node was deleted after `inputs` and
    /// `fragments` were built. Rebuild them, then resolve again. Data resolved
    /// before the failure stays in `tree`, so `tree` may be partly updated,
    /// but the next call resolves the same items and boxes again.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        dirty: &DirtyMap,
        fragments: &FragmentTree,
        units: &UnitResolveContext,
        tree: &mut PrepaintTree,
        root: BoxFragmentKey,
        viewport_changed: bool,
    ) -> Option<()> {
        let sources = Sources {
            ui,
            inputs,
            fragments,
            units,
        };
        let Self {
            pending,
            items,
            dependents,
            changed,
        } = self;
        pending.clear();
        items.clear();
        dependents.clear();
        changed.clear();

        let root_item = fragments.get(root)?.source;
        inputs.items.get(root_item)?;

        // Paint values using `vw` or `vh`, resolved first, ancestors before
        // their descendants, so the walk below inherits their new values.
        // Their keys are kept, so a failed call resolves them again;
        // resolving an item removes it once it no longer uses them.
        if viewport_changed {
            tree.viewport_dependents
                .retain(|key, _| inputs.items.get(key).is_some());

            dependents.extend(tree.viewport_dependents.keys().map(|key| {
                let depth = std::iter::successors(inputs.items.parent(key), |&key| {
                    inputs.items.parent(key)
                })
                .count();

                (depth, key)
            }));
            dependents.sort_unstable();

            for &(_, key) in dependents.iter() {
                items.push((key, true));
                sources.resolve_items(dirty, tree, items)?;
                changed.extend(boxes_of(fragments, inputs, key));
            }
        }

        items.push((root_item, false));
        sources.resolve_items(dirty, tree, items)?;

        pending.push(root);

        while let Some(key) = pending.pop() {
            let fragment = fragments.get(key)?;
            inputs.items.get(fragment.source)?;
            let entry = dirty.entry(fragment.source);
            let missing = !tree.fragment_mapping.contains_key(key);

            if entry.flags.contains(DirtyFlags::NEED_PRE_PAINT) || missing {
                changed.push(key);
            }

            // Anonymous boxes carry the item of the box owning them, so the
            // children of a flagged item are visited too.
            if (entry.flags | entry.child_bits).contains(DirtyFlags::NEED_PRE_PAINT) || missing {
                pending.extend(fragments.boxes.children(key));
            }
        }

        changed.sort_unstable();
        changed.dedup();

        for &key in changed.iter() {
            let data = sources.resolve_fragment(key)?;
            tree.fragment_mapping.insert(key, data);
        }

        Some(())
    }
}

/// Whether a paint value resolved from `props`, for an item or its
/// fragments, uses `vw` or `vh`.
fn depends_viewport(props: Props<'_>) -> bool {
    let viewport = StyleUnit::is_viewport_relative;
    let rect = |rect: Rect<StyleUnit>| {
        [
            rect.left_top.x,
            rect.left_top.y,
            rect.right_bottom.x,
            rect.right_bottom.y,
        ]
        .into_iter()
        .any(viewport)
    };

    props
        .get_cloned()
        .is_some_and(|BorderRadius(radius)| rect(radius))
        || props
            .get_cloned()
            .is_some_and(|OutlineWidth(width)| viewport(width))
        || props
            .get_cloned()
            .is_some_and(|OutlineOffset(offset)| viewport(offset))
}

impl Sources<'_> {
    /// Resolve the data of one box from the UI and the data of its item.
    fn resolve_fragment(&self, key: BoxFragmentKey) -> Option<FragmentData> {
        let fragment = self.fragments.get(key)?;
        let props = paint_props(self.ui, self.inputs, fragment.source)?;

        Some(FragmentData {
            border_radius: self.border_radius(fragment, props),
        })
    }

    /// Corner radii of a box's border box, from its item's `props`, resolved
    /// against the box's size, all four corners rounded. `None` for boxes
    /// painting nothing, text items and items setting no radius.
    fn border_radius(&self, fragment: &BoxFragment, props: Props<'_>) -> Option<RoundedRectRadii> {
        if !BoxRole::of(fragment).decorates() {
            return None;
        }

        let BorderRadius(radius) = props.get_cloned()?;
        let mut units = self.units.clone();
        units.font_size = self.inputs.items[fragment.source].text_style.span.font_size as f64;

        let size = fragment.layout.geometry.size;

        Some(border_radius(
            radius,
            &units,
            Size::new(size.width, size.height),
        ))
    }

    /// Visit the items in `pending`, resolving those that need it into
    /// `tree`, then their children as [`PrepaintTreeBuilder::build`]
    /// describes. Parents are resolved before their children. Empties
    /// `pending`.
    fn resolve_items(
        &self,
        dirty: &DirtyMap,
        tree: &mut PrepaintTree,
        pending: &mut Vec<(LayoutInputKey, bool)>,
    ) -> Option<()> {
        let visit = DirtyFlags::NEED_PRE_PAINT | DirtyFlags::NEED_CHILDREN;

        while let Some((key, force)) = pending.pop() {
            let entry = dirty.entry(key);
            let old = tree.layout_mapping.get(key);
            let mut inherited_changed = false;

            if force || old.is_none() || entry.flags.contains(DirtyFlags::NEED_PRE_PAINT) {
                let parent = self
                    .inputs
                    .items
                    .parent(key)
                    .and_then(|parent| tree.layout_mapping.get(parent));
                let data = self.resolve_item(key, parent)?;
                inherited_changed = old.is_none_or(|old| !old.inherits_same(&data));

                tree.layout_mapping.insert(key, data);

                // Props alone decide it, and resolving an item reads them
                // again.
                let props = paint_props(self.ui, self.inputs, key)?;
                if depends_viewport(props) {
                    tree.viewport_dependents.insert(key, ());
                } else {
                    tree.viewport_dependents.remove(key);
                }
            }

            // Children moved in may have inherited from another parent.
            let force_children =
                inherited_changed || entry.flags.contains(DirtyFlags::NEED_CHILDREN);

            if force_children || (entry.flags | entry.child_bits).intersects(visit) {
                pending.extend(
                    self.inputs
                        .items
                        .children(key)
                        .map(|child| (child, force_children)),
                );
            }
        }

        Some(())
    }

    /// Resolve the data of one item from the UI, inheriting from `parent`,
    /// the data of its parent item, if any.
    fn resolve_item(
        &self,
        key: LayoutInputKey,
        parent: Option<&LayoutInputData>,
    ) -> Option<LayoutInputData> {
        let item = self.inputs.items.get(key)?;
        let props = paint_props(self.ui, self.inputs, key)?;
        let Overflow(overflow) = props.get_or_default();

        let mut units = self.units.clone();
        units.font_size = item.text_style.span.font_size as f64;

        // Outline percentages resolve against the parent's resolved value.
        let outline = |value: Option<StyleUnit>, basis: f64| {
            value.map_or(0.0, |value| value.resolve(&units, basis))
        };
        let (parent_width, parent_offset) = parent.map_or((0.0, 0.0), |parent| {
            (parent.outline_width, parent.outline_offset)
        });

        let TextColor(text_color) = props.get_cloned().unwrap_or_default();
        let text_color = match text_color {
            AutoValue::Value(color) => color,
            AutoValue::Auto => parent.map_or(Paint::Solid(Color::BLACK), |parent| {
                parent.text_color.clone()
            }),
        };

        let SelectionColor(selection_color) = props.get_cloned().unwrap_or_default();
        let selection_color = match selection_color {
            AutoValue::Value(color) => color,
            AutoValue::Auto => parent.map_or(Paint::Solid(DEFAULT_SELECTION_COLOR), |parent| {
                parent.selection_color.clone()
            }),
        };
        // A container holds its descendants, whether they are selectable.
        let container = parent.and_then(|parent| parent.selection_container);
        let Selectable(select) = props.get_cloned().unwrap_or_default();
        let (selectable, selection_container) = match select {
            AutoValue::Auto => (parent.is_none_or(|parent| parent.selectable), container),
            AutoValue::Value(UserSelect::None) => (false, container),
            AutoValue::Value(UserSelect::Text) => (true, container),
            AutoValue::Value(UserSelect::Contain) => (true, Some(key)),
        };
        let SelectionFill(selection_background) = props.get_cloned().unwrap_or_default();
        let selection_background = match selection_background {
            AutoValue::Value(fill) => fill,
            AutoValue::Auto => parent
                .map_or(Paint::Solid(DEFAULT_SELECTION_BACKGROUND), |parent| {
                    parent.selection_background.clone()
                }),
        };

        // As in CSS and Taffy, a box scrolling or hiding its overflow on one
        // axis clips both, while `clip` only clips its own axis. Layout
        // overflow stops at the same clips.
        let scroll = overflow.x.is_scroll_container() || overflow.y.is_scroll_container();

        Some(LayoutInputData {
            clip: (
                scroll || overflow.x != OverflowMode::Visible,
                scroll || overflow.y != OverflowMode::Visible,
            ),
            outline_width: outline(
                props.get_cloned().map(|OutlineWidth(width)| width),
                parent_width,
            )
            .max(0.0),
            outline_offset: outline(
                props.get_cloned().map(|OutlineOffset(offset)| offset),
                parent_offset,
            ),
            text_color,
            selectable,
            selection_container,
            selection_color,
            selection_background,
        })
    }
}

/// The corner radii of a border box of `size`. Percentages resolve against
/// its smaller side.
fn border_radius(
    radius: Rect<StyleUnit>,
    units: &UnitResolveContext,
    size: Size,
) -> RoundedRectRadii {
    // TODO:: horizontal, vertical curve value for round rect, with
    // percentages of the width and height.
    let basis = size.width.min(size.height);
    let resolve = |value: StyleUnit| value.resolve(units, basis).max(0.0);

    RoundedRectRadii::new(
        resolve(radius.left_top.y),
        resolve(radius.right_bottom.x),
        resolve(radius.right_bottom.y),
        resolve(radius.left_top.x),
    )
}
