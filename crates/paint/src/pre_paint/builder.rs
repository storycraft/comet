use comet_layout::{
    fragment::{Fragment, FragmentItem, FragmentKey, tree::FragmentTree},
    input::{InputFlags, LayoutInputKey, tree::LayoutInputTree},
    style::{
        bx::Overflow,
        geometry::{AutoValue, Point as StylePoint, Rect, StyleUnit, UnitResolveContext},
        taffy::Overflow as OverflowMode,
    },
};
use comet_tree::ui::{Props, Ui};
use kurbo::{Affine, Point, RoundedRectRadii, Size};

use crate::{
    pre_paint::{FragmentData, LayoutInputData, tree::PrePaintTree},
    style::{
        BackdropFilter, BorderRadius, Filter, Opacity, OutlineOffset, OutlineWidth, Transform,
        TransformOrigin, ZIndex,
    },
};

/// Builds a [`PrePaintTree`] for a fragment tree. Keep it to reuse its
/// buffers across resolves.
#[derive(Default)]
pub struct PrePaintTreeBuilder {
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

impl PrePaintTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds into `tree` the fragments under `root` that need it, and
    /// their layout input items:
    /// - fragments of items whose [`LayoutInputItem::flags`](comet_layout::input::LayoutInputItem::flags) have
    ///   [`InputFlags::NEED_PRE_PAINT`], which layout changes include, or
    ///   which were marked with it, such as after their paint styles changed,
    /// - fragments that fragment builds created, moved or resized since the
    ///   last successful call, found with their
    ///   [generations](comet_layout::fragment::Fragment::geometry_generation),
    /// - fragments without data in `tree`.
    ///
    /// Fragments are visited from `root`, going into the children of a
    /// fragment only when its item's
    /// [`LayoutInputItem::child_flags`](comet_layout::input::LayoutInputItem::child_flags)
    /// have [`InputFlags::NEED_PRE_PAINT`], or when its
    /// [subtree generation](comet_layout::fragment::Fragment::subtree_generation)
    /// is newer than the last successful call. A [cleared](PrePaintTree::clear)
    /// `tree` resolves every fragment.
    ///
    /// Data only depends on the fragment or item itself, so ancestors are not
    /// resolved. Input flags are not cleared; the layer build reads them too,
    /// so call [`LayoutInputTree::clear_flags`] after building layers.
    ///
    /// Changes builds cannot see, such as paint styles, must be marked. The
    /// root font size and viewport units data resolves with are not tracked:
    /// resolve into a new or [cleared](PrePaintTree::clear) tree when they
    /// change.
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
    pub fn build(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        fragments: &FragmentTree,
        units: &UnitResolveContext,
        tree: &mut PrePaintTree,
        root: FragmentKey,
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
        let resolved = tree.fragment_generation;
        pending.clear();
        items.clear();
        changed.clear();
        pending.push(root);

        while let Some(key) = pending.pop() {
            let fragment = fragments.items.get(key)?;
            let item = inputs.items.get(fragment.source)?;

            if item.flags.contains(InputFlags::NEED_PRE_PAINT)
                || !tree.fragment_mapping.contains_key(key)
            {
                changed.push(key);
                items.push(fragment.source);
            } else if fragment.geometry_generation > resolved {
                // Item data does not depend on the layout.
                changed.push(key);
            }

            if item.child_flags.contains(InputFlags::NEED_PRE_PAINT)
                || fragment.subtree_generation > resolved
            {
                pending.extend(fragments.items.children(key));
            }
        }

        // Items with several fragments to resolve are resolved once.
        items.sort_unstable();
        items.dedup();

        for &key in items.iter() {
            tree.layout_mapping.insert(key, sources.resolve_item(key)?);
        }

        for &key in changed.iter() {
            let data = sources.resolve_fragment(key)?;
            let old = tree.fragment_mapping.get(key);

            if old != Some(&data) {
                tree.fragment_mapping.insert(key, data);
            }
        }

        tree.fragment_generation = fragments.generation();
        Some(())
    }
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
            transform_origin,
            stacking: box_props.and_then(stacking),
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

        Some(LayoutInputData {
            clip: (
                overflow.x != OverflowMode::Visible,
                overflow.y != OverflowMode::Visible,
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

/// The z index of the paint layer a box starts, or `None` if it paints in
/// the layer of its parent. Setting a layer style starts a layer, whatever
/// its value. An auto z index counts as 0.
fn stacking(props: Props<'_>) -> Option<i32> {
    if let AutoValue::Value(z_index) = props.get_or_default::<ZIndex>().0 {
        return Some(z_index);
    }

    let starts = props.get::<Transform>().is_some()
        || props.get::<Opacity>().is_some()
        || props.get::<Filter>().is_some()
        || props.get::<BackdropFilter>().is_some();

    starts.then_some(0)
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
