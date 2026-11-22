use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{Fragment, FragmentItem, FragmentKey, tree::FragmentTree},
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
    prepaint::{FragmentData, LayoutInputData, tree::PrepaintTree},
    style::{
        BorderRadius, OutlineOffset, OutlineWidth, ScrollLeft, ScrollTop, Transform,
        TransformOrigin,
    },
};

/// Builds a [`PrepaintTree`] for a fragment tree. Keep it to reuse its
/// buffers across resolves.
#[derive(Default)]
pub struct PrepaintTreeBuilder {
    /// Fragments to visit.
    pending: Vec<FragmentKey>,
    /// Layout input items to resolve.
    items: Vec<LayoutInputKey>,
    /// Fragments to resolve.
    changed: Vec<FragmentKey>,
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

    /// Builds into `tree` the fragments that need it, and their layout input
    /// items:
    /// - fragments under `root` of items whose [`DirtyEntry::flags`](comet_layout::dirty_map::DirtyEntry::flags) in `dirty` have
    ///   [`DirtyFlags::NEED_PRE_PAINT`]: those the fragment builder created,
    ///   moved or resized, those whose font size the input builder resolved
    ///   to a new value, and those marked with it, such as after their paint
    ///   styles changed. Other layout changes,
    ///   [`DirtyFlags::NEED_FULL_LAYOUT`] and
    ///   [`DirtyFlags::NEED_BOX_LAYOUT`], reach pre-paint only through the
    ///   fragments they create, move or resize,
    /// - fragments under `root` without data in `tree`.
    ///
    /// Fragments are visited from `root`, going into the children of a
    /// fragment only when its item's
    /// [`DirtyEntry::flags`](comet_layout::dirty_map::DirtyEntry::flags) or
    /// [`DirtyEntry::child_bits`](comet_layout::dirty_map::DirtyEntry::child_bits)
    /// have [`DirtyFlags::NEED_PRE_PAINT`], or when it has no data in `tree`.
    /// The former reaches the lines and anonymous blocks of a laid out box.
    /// A [cleared](PrepaintTree::clear) `tree` resolves every fragment.
    ///
    /// Data only depends on the fragment or item itself, so ancestors are not
    /// resolved. `dirty` is not cleared; the layer build reads it too, so
    /// call [`DirtyMap::clear`] after building layers.
    ///
    /// Items one of whose fragments gets new data, including fragments
    /// without data before, are marked [`DirtyFlags::NEED_LAYER`] in `dirty`,
    /// for the layer build.
    ///
    /// Changes builds cannot see, such as paint styles, must be marked.
    ///
    /// With `viewport_changed`, every item whose paint values, or those of
    /// its fragments, use `vw` or `vh` is resolved again with all of its
    /// fragments. Pass it when the viewport in `units` differs from the
    /// last successful call. The root font size is not tracked: resolve into a new
    /// or [cleared](PrepaintTree::clear) tree when it changes.
    ///
    /// `inputs` is the input tree `fragments` were built from, and `tree`
    /// belongs to them. `units` resolves border radii and transform origins,
    /// with each item's font size.
    ///
    /// Returns `None` if a fragment or item to resolve no longer matches the
    /// UI, such as when its source node was deleted after `inputs` and
    /// `fragments` were built. Rebuild them, then resolve again. Data resolved
    /// before the failure stays in `tree`, so `tree` may be partly updated,
    /// but the next call resolves the same fragments again.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        dirty: &mut DirtyMap,
        fragments: &FragmentTree,
        units: &UnitResolveContext,
        tree: &mut PrepaintTree,
        root: FragmentKey,
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
            let fragment = fragments.items.get(key)?;
            inputs.items.get(fragment.source)?;
            let entry = dirty.entry(fragment.source);
            let missing = !tree.fragment_mapping.contains_key(key);

            if entry.flags.contains(DirtyFlags::NEED_PRE_PAINT) || missing {
                changed.push(key);
                items.push(fragment.source);
            }

            // Lines and anonymous blocks carry the item of the box owning
            // them, so the children of a flagged item are visited too.
            if (entry.flags | entry.child_bits).contains(DirtyFlags::NEED_PRE_PAINT) || missing {
                pending.extend(fragments.items.children(key));
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
                changed.extend(fragments.fragments(key));
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
            let props = ui.props(inputs.items[key].node)?;
            if depends_viewport(props) {
                tree.viewport_dependents.insert(key, ());
            } else {
                tree.viewport_dependents.remove(key);
            }
        }

        for &key in changed.iter() {
            let data = sources.resolve_fragment(key)?;

            // Layers read the data of their fragments, so changed data
            // updates the layers too.
            if tree.fragment_mapping.get(key) != Some(&data) {
                tree.fragment_mapping.insert(key, data);
                dirty.mark(inputs, fragments.items[key].source, DirtyFlags::NEED_LAYER);
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
    /// Resolve the data of one fragment from the UI and the data of its item.
    fn resolve_fragment(&self, key: FragmentKey) -> Option<FragmentData> {
        let fragment = self.fragments.items.get(key)?;
        let props = self.ui.props(self.inputs.items[fragment.source].node)?;
        // Transforms only apply to boxes.
        let box_props = if fragment.item.is_box() {
            Some(props)
        } else {
            None
        };

        let TransformOrigin(origin) = box_props.map(Props::get_or_default).unwrap_or_default();
        let Transform(own_transform) = box_props.map(Props::get_or_default).unwrap_or_default();

        let layout = &fragment.layout;
        let transform_origin = self.transform_origin(
            key,
            origin,
            Size::new(layout.size.width, layout.size.height),
        );
        let transform = resolve_transform(
            Point::new(layout.location.x, layout.location.y),
            own_transform,
            transform_origin,
        );

        Some(FragmentData {
            transform,
            inverse_transform: invert(transform),
            border_radius: self.border_radius(fragment, props),
        })
    }

    /// The point a fragment's own transform is applied around, relative to
    /// its border box of `size`, resolving `origin` with its item's font size.
    fn transform_origin(
        &self,
        key: FragmentKey,
        origin: StylePoint<StyleUnit>,
        size: Size,
    ) -> Point {
        let source = self.fragments.items[key].source;
        let mut units = self.units.clone();
        units.font_size = self.inputs.items[source].text_style.span.font_size as f64;

        let origin = origin.resolve(&units, StylePoint::new(size.width, size.height));
        Point::new(origin.x, origin.y)
    }

    /// Corner radii of a fragment's border box, from its item's `props`,
    /// resolved against the fragment's size, all four corners rounded.
    /// `None` for fragments painting no box and items setting no radius.
    fn border_radius(&self, fragment: &Fragment, props: Props<'_>) -> Option<RoundedRectRadii> {
        if matches!(
            fragment.item,
            FragmentItem::Line { .. } | FragmentItem::AnonymousBlock { .. }
        ) {
            return None;
        }

        let BorderRadius(radius) = props.get_cloned()?;
        let mut units = self.units.clone();
        units.font_size = self.inputs.items[fragment.source].text_style.span.font_size as f64;

        Some(border_radius(
            radius,
            &units,
            Size::new(fragment.layout.size.width, fragment.layout.size.height),
        ))
    }

    /// Resolve the data of one item from the UI.
    fn resolve_item(&self, key: LayoutInputKey) -> Option<LayoutInputData> {
        let item = &self.inputs.items[key];
        let props = self.ui.props(item.node)?;
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
        let scroll_offset = match self.fragments.box_fragment(key) {
            Some(fragment) if scroll => {
                let ScrollLeft(left) = props.get_or_default();
                let ScrollTop(top) = props.get_or_default();
                let max = scroll_range(&self.fragments.items[fragment]);

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

/// The largest scroll offset of a scroll container's box `fragment`: how far
/// its content overflows its padding box towards the right and bottom.
///
/// Its overflow includes its border box, so only content passing the border
/// box edge scrolls, by that excess and the border width.
fn scroll_range(fragment: &Fragment) -> Vec2 {
    let layout = &fragment.layout;
    let range = |excess: f64, border: f64| {
        if excess > 0.0 { excess + border } else { 0.0 }
    };

    Vec2::new(
        range(
            layout.overflow.x1 - layout.size.width,
            layout.border.right_bottom.x,
        ),
        range(
            layout.overflow.y1 - layout.size.height,
            layout.border.right_bottom.y,
        ),
    )
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

/// Apply a fragment's own `transform` around `origin`, relative to its border
/// box, then place it at its `location` in its parent fragment.
fn resolve_transform(location: Point, transform: Affine, origin: Point) -> Affine {
    let location = location.to_vec2();

    if transform == Affine::IDENTITY {
        return Affine::translate(location);
    }

    let origin = origin.to_vec2();

    Affine::translate(location + origin) * transform * Affine::translate(-origin)
}
