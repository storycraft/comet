use comet_layout::{
    fragment::{FragmentKey, tree::FragmentTree},
    input::{LayoutInputKey, tree::LayoutInputTree},
    style::{
        bx::Overflow,
        geometry::{AutoValue, Point as StylePoint, Rect, StyleUnit, UnitResolveContext},
        taffy::Overflow as OverflowMode,
    },
};
use comet_tree::ui::{Props, Ui};
use kurbo::{Affine, Point, RoundedRectRadii, Size};

use crate::{
    pre_paint::{FragmentData, LayoutInputData, PrePaintFlags, tree::PrePaintTree},
    style::{
        BackdropFilter, BorderRadius, Filter, Opacity, OutlineOffset, OutlineWidth, Transform,
        TransformOrigin, ZIndex,
    },
};

/// Resolves a [`PrePaintTree`] for a fragment tree. Keep it to reuse its
/// buffers across resolves.
#[derive(Default)]
pub struct PrePaintTreeResolver {
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

impl PrePaintTreeResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve into `tree` the fragments under `root` that need it, and
    /// their layout input items:
    /// - fragments that fragment builds created, moved or resized since the
    ///   last successful call, found with the
    ///   [generations](FragmentTree::generation) of `fragments`,
    /// - fragments [marked](PrePaintTree::mark)
    ///   [`NEED_PRE_PAINT`](PrePaintFlags::NEED_PRE_PAINT), such as after
    ///   their paint styles changed,
    /// - fragments without data in `tree`.
    ///
    /// Every call starts a new [generation](PrePaintTree::generation).
    /// Fragment data created or changed by the call is stamped with it, and
    /// so is the [subtree generation](FragmentData::subtree_generation) of
    /// its ancestors. Data resolved to equal values keeps its generation.
    ///
    /// Subtrees without such fragments are skipped, so the first call, or a
    /// call on an empty `tree`, resolves everything. Data only depends on the
    /// fragment or item itself, so ancestors are not resolved. Flags are not
    /// cleared; call [`PrePaintTree::clear_flags`] after resolving.
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
    pub fn resolve(
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
        tree.generation += 1;
        let generation = tree.generation;

        pending.clear();
        items.clear();
        changed.clear();
        pending.push(root);

        while let Some(key) = pending.pop() {
            let fragment = fragments.items.get(key)?;
            let flags = tree.flags(key);
            let stale = fragment.subtree_generation > resolved;

            if !stale && flags.is_empty() {
                continue;
            }

            if fragment.geometry_generation > resolved
                || flags.contains(PrePaintFlags::NEED_PRE_PAINT)
                || !tree.fragment_mapping.contains_key(key)
            {
                changed.push(key);
                items.push(fragment.source);
            }

            if stale || flags.contains(PrePaintFlags::CHILD_NEED_PRE_PAINT) {
                pending.extend(fragments.items.children(key));
            }
        }

        // Items read the sizes of all their fragments, and fragments read
        // their items.
        items.sort_unstable();
        items.dedup();

        for &key in items.iter() {
            tree.layout_mapping.insert(key, sources.resolve_item(key)?);
        }

        for &key in changed.iter() {
            let mut data = sources.resolve_fragment(key)?;

            match tree.fragment_mapping.get(key) {
                Some(old) if old.same_values(&data) => continue,
                _ => {
                    data.generation = generation;
                    data.subtree_generation = generation;
                }
            }

            tree.fragment_mapping.insert(key, data);

            // Stamp the ancestors at once: a later failure keeps the stamp,
            // and resolving again to equal data would not stamp them.
            let mut ancestor = fragments.items.parent(key);

            while let Some(key) = ancestor {
                let Some(data) = tree.fragment_mapping.get_mut(key) else {
                    break;
                };

                // Ancestors of a stamped fragment are stamped already.
                if data.subtree_generation == generation {
                    break;
                }

                data.subtree_generation = generation;
                ancestor = fragments.items.parent(key);
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
            generation: 0,
            subtree_generation: 0,
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

    /// Resolve the data of one item from the UI and its fragments.
    fn resolve_item(&self, key: LayoutInputKey) -> Option<LayoutInputData> {
        let item = &self.inputs.items[key];
        let props = self.ui.props(item.node)?;
        let Overflow(overflow) = props.get_or_default();

        let mut units = self.units.clone();
        units.font_size = item.text_style.span.font_size as f64;

        let stitched_size = self.stitched_size(key);
        // Outline lengths have no percentage basis.
        let outline =
            |value: Option<StyleUnit>| value.map_or(0.0, |value| value.resolve(&units, 0.0));

        Some(LayoutInputData {
            stitched_size,
            clip: (
                overflow.x != OverflowMode::Visible,
                overflow.y != OverflowMode::Visible,
            ),
            border_radius: props
                .get_cloned()
                .map(|BorderRadius(radius)| border_radius(radius, &units)),
            outline_width: outline(props.get_cloned().map(|OutlineWidth(width)| width)).max(0.0),
            outline_offset: outline(props.get_cloned().map(|OutlineOffset(offset)| offset)),
        })
    }

    /// The size of the fragments an item generates and links together, placed
    /// end to end: box fragments below each other, inline and glyph run
    /// fragments side by side. Items without fragments have an empty size.
    fn stitched_size(&self, key: LayoutInputKey) -> Size {
        self.fragments
            .fragments(key)
            .fold(Size::ZERO, |stitched, fragment| {
                let fragment = &self.fragments.items[fragment];
                let size = fragment.layout.size;

                if fragment.item.is_box() {
                    Size::new(
                        stitched.width.max(size.width),
                        stitched.height + size.height,
                    )
                } else {
                    Size::new(
                        stitched.width + size.width,
                        stitched.height.max(size.height),
                    )
                }
            })
    }
}

/// The corner radii of an item's border box. Percentages have no basis yet
/// and resolve to 0.
fn border_radius(radius: Rect<StyleUnit>, units: &UnitResolveContext) -> RoundedRectRadii {
    // TODO:: horizontal, vertical curve value for round rect, with
    // percentages of the box size.
    let resolve = |value: StyleUnit| value.resolve(units, 0.0).max(0.0);

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
