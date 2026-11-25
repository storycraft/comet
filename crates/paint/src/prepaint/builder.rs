use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{BoxFragment, BoxFragmentKey, FragmentTree},
    input::{LayoutInputKey, tree::LayoutInputTree},
    style::{
        bx::Overflow,
        geometry::{Point as StylePoint, Rect, StyleUnit, UnitResolveContext},
        taffy::Overflow as OverflowMode,
    },
};
use comet_tree::ui::{Props, Ui};
use kurbo::{Affine, Point, RoundedRectRadii, Size, Vec2};

use crate::{
    prepaint::{
        FragmentData, LayoutInputData,
        fragment::{BoxRole, boxes_of, paint_props},
        tree::PrepaintTree,
    },
    style::{
        BorderRadius, OutlineOffset, OutlineWidth, ScrollLeft, ScrollTop, Transform,
        TransformOrigin,
    },
};

/// Builds a [`PrepaintTree`] for a fragment tree. Keep it to reuse its
/// buffers across resolves.
#[derive(Default)]
pub struct PrepaintTreeBuilder {
    /// Boxes to visit.
    pending: Vec<BoxFragmentKey>,
    /// Layout input items to resolve.
    items: Vec<LayoutInputKey>,
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

    /// Builds into `tree` the box fragments that need it, and their layout
    /// input items:
    /// - boxes under `root` of items whose [`DirtyEntry::flags`](comet_layout::dirty_map::DirtyEntry::flags) in `dirty` have
    ///   [`DirtyFlags::NEED_PRE_PAINT`]: those the fragment builder created,
    ///   moved or resized, those whose font size the input builder resolved
    ///   to a new value, and those marked with it, such as after their paint
    ///   styles changed. Other layout changes,
    ///   [`DirtyFlags::NEED_FULL_LAYOUT`] and
    ///   [`DirtyFlags::NEED_BOX_LAYOUT`], reach pre-paint only through the
    ///   boxes they create, move or resize,
    /// - boxes under `root` without data in `tree`.
    ///
    /// Inline items placing no box, such as lines and glyph runs, get no
    /// data, and are skipped.
    ///
    /// Boxes are visited from `root`, going into the children of a box only
    /// when its item's
    /// [`DirtyEntry::flags`](comet_layout::dirty_map::DirtyEntry::flags) or
    /// [`DirtyEntry::child_bits`](comet_layout::dirty_map::DirtyEntry::child_bits)
    /// have [`DirtyFlags::NEED_PRE_PAINT`], or when it has no data in `tree`.
    /// Anonymous boxes carry the item of the box owning them, and inline part
    /// boxes the item of their inline element, inside it, so the former
    /// reaches them. A [cleared](PrepaintTree::clear) `tree` resolves every
    /// box.
    ///
    /// Data only depends on the box or item itself, so ancestors are not
    /// resolved. `dirty` is not cleared; the layer build reads it too, so
    /// call [`DirtyMap::clear`] after building layers.
    ///
    /// Items one of whose boxes gets new data, including boxes without data
    /// before, are marked [`DirtyFlags::NEED_LAYER`] in `dirty`, for the
    /// layer build.
    ///
    /// Changes builds cannot see, such as paint styles, must be marked.
    ///
    /// With `viewport_changed`, every item whose paint values, or those of
    /// its boxes, use `vw` or `vh` is resolved again with all of its boxes.
    /// Pass it when the viewport in `units` differs from the last successful
    /// call. The root font size is not tracked: resolve into a new or
    /// [cleared](PrepaintTree::clear) tree when it changes.
    ///
    /// `inputs` is the input tree `fragments` were built from, and `tree`
    /// belongs to them. `units` resolves border radii and transform origins,
    /// with each item's font size.
    ///
    /// Returns `None` if a box or item to resolve no longer matches the UI,
    /// such as when its source node was deleted after `inputs` and
    /// `fragments` were built. Rebuild them, then resolve again. Data resolved
    /// before the failure stays in `tree`, so `tree` may be partly updated,
    /// but the next call resolves the same boxes again.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        dirty: &mut DirtyMap,
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
            changed,
        } = self;
        pending.clear();
        items.clear();
        changed.clear();
        pending.push(root);

        while let Some(key) = pending.pop() {
            let fragment = fragments.get(key)?;
            inputs.items.get(fragment.source)?;
            let entry = dirty.entry(fragment.source);
            let missing = !tree.fragment_mapping.contains_key(key);

            if entry.flags.contains(DirtyFlags::NEED_PRE_PAINT) || missing {
                changed.push(key);
                items.push(fragment.source);
            }

            // Anonymous boxes carry the item of the box owning them, so the
            // children of a flagged item are visited too.
            if (entry.flags | entry.child_bits).contains(DirtyFlags::NEED_PRE_PAINT) || missing {
                pending.extend(fragments.boxes.children(key));
            }
        }

        // Paint values using `vw` or `vh`. Their keys are kept, so a failed
        // call resolves them again; resolving an item removes it once it no
        // longer uses them.
        if viewport_changed {
            tree.viewport_dependents
                .retain(|key, _| inputs.items.get(key).is_some());

            for key in tree.viewport_dependents.keys() {
                items.push(key);
                changed.extend(boxes_of(fragments, inputs, key));
            }
        }

        changed.sort_unstable();
        changed.dedup();

        // Items with several fragments to resolve are resolved once.
        items.sort_unstable();
        items.dedup();

        for &key in items.iter() {
            let data = sources.resolve_item(key)?;
            tree.layout_mapping.insert(key, data);

            // Props alone decide it, and resolving an item reads them again.
            let props = paint_props(ui, inputs, key)?;
            if depends_viewport(props) {
                tree.viewport_dependents.insert(key, ());
            } else {
                tree.viewport_dependents.remove(key);
            }
        }

        for &key in changed.iter() {
            let data = sources.resolve_fragment(key)?;

            // Layers read the data of their boxes, so changed data updates
            // the layers too.
            if tree.fragment_mapping.get(key) != Some(&data) {
                tree.fragment_mapping.insert(key, data);
                dirty.mark(inputs, fragments.boxes[key].source, DirtyFlags::NEED_LAYER);
            }
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
            .is_some_and(|TransformOrigin(origin)| viewport(origin.x) || viewport(origin.y))
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
        // Transforms only apply to principal boxes.
        let box_props = BoxRole::of(fragment).is_principal().then_some(props);

        let TransformOrigin(origin) = box_props.map(Props::get_or_default).unwrap_or_default();
        let Transform(own_transform) = box_props.map(Props::get_or_default).unwrap_or_default();

        let geometry = &fragment.layout.geometry;
        let transform_origin = self.transform_origin(
            fragment,
            origin,
            Size::new(geometry.size.width, geometry.size.height),
        );
        let transform = resolve_transform(
            Point::new(geometry.location.x, geometry.location.y),
            own_transform,
            transform_origin,
        );

        Some(FragmentData {
            transform,
            inverse_transform: invert(transform),
            border_radius: self.border_radius(fragment, props),
        })
    }

    /// The point a box's own transform is applied around, relative to its
    /// border box of `size`, resolving `origin` with its item's font size.
    fn transform_origin(
        &self,
        fragment: &BoxFragment,
        origin: StylePoint<StyleUnit>,
        size: Size,
    ) -> Point {
        let mut units = self.units.clone();
        units.font_size = self.inputs.items[fragment.source].text_style.span.font_size as f64;

        let origin = origin.resolve(&units, StylePoint::new(size.width, size.height));
        Point::new(origin.x, origin.y)
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

    /// Resolve the data of one item from the UI.
    fn resolve_item(&self, key: LayoutInputKey) -> Option<LayoutInputData> {
        let item = &self.inputs.items[key];
        let props = paint_props(self.ui, self.inputs, key)?;
        let Overflow(overflow) = props.get_or_default();

        let mut units = self.units.clone();
        units.font_size = item.text_style.span.font_size as f64;

        // Outline lengths have no percentage basis.
        let outline =
            |value: Option<StyleUnit>| value.map_or(0.0, |value| value.resolve(&units, 0.0));

        // As in CSS and Taffy, a box scrolling or hiding its overflow on one
        // axis clips both, while `clip` only clips its own axis. Layout
        // overflow stops at the same clips.
        let scroll = overflow.x.is_scroll_container() || overflow.y.is_scroll_container();

        // Only scroll containers scroll, through their single box fragment.
        // Both axes scroll once either does, as both clip.
        let range = self
            .fragments
            .box_fragment(key)
            .and_then(|fragment| self.fragments.scroll_range(fragment));
        let scroll_offset = match range {
            Some(max) if scroll => {
                let ScrollLeft(left) = props.get_or_default();
                let ScrollTop(top) = props.get_or_default();

                // `max` first maps NaN to 0, which `min` would map to the
                // range instead. The range is at least 0.
                Vec2::new(left.max(0.0).min(max.x), top.max(0.0).min(max.y))
            }
            _ => Vec2::ZERO,
        };

        Some(LayoutInputData {
            scroll: scroll_offset,
            clip: (
                scroll || overflow.x != OverflowMode::Visible,
                scroll || overflow.y != OverflowMode::Visible,
            ),
            outline_width: outline(props.get_cloned().map(|OutlineWidth(width)| width)).max(0.0),
            outline_offset: outline(props.get_cloned().map(|OutlineOffset(offset)| offset)),
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

/// The inverse of `transform`, or `None` if it collapses an axis and has no
/// inverse.
fn invert(transform: Affine) -> Option<Affine> {
    if transform == Affine::IDENTITY {
        return Some(Affine::IDENTITY);
    }

    let determinant = transform.determinant();

    if determinant == 0.0 || !determinant.is_finite() {
        return None;
    }

    Some(transform.inverse()).filter(Affine::is_finite)
}

/// Apply a box's own `transform` around `origin`, relative to its border box,
/// then place it at its `location` in its parent box.
fn resolve_transform(location: Point, transform: Affine, origin: Point) -> Affine {
    let location = location.to_vec2();

    if transform == Affine::IDENTITY {
        return Affine::translate(location);
    }

    let origin = origin.to_vec2();

    Affine::translate(location + origin) * transform * Affine::translate(-origin)
}
