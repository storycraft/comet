use comet_layout::{
    fragment::{FragmentItem, FragmentKey, tree::FragmentTree},
    input::{LayoutInputKey, tree::LayoutInputTree},
    style::{
        bx::Overflow,
        geometry::{AutoValue, Point as StylePoint, StyleUnit, UnitResolveContext},
        taffy::Overflow as OverflowMode,
    },
};
use comet_tree::ui::{Props, Ui};
use kurbo::{Affine, Point, RoundedRectRadii, Size};

use crate::{
    pre_paint::{FragmentData, LayoutInputData, tree::PrePaintTree},
    style::{BorderRadius, Opacity, TextColor, Transform, TransformOrigin, ZIndex},
};

/// Resolves a [`PrePaintTree`] for a fragment tree. Keep it to reuse its
/// buffers across resolves.
#[derive(Default)]
pub struct PrePaintTreeResolver {
    /// Fragments to resolve.
    fragments: Vec<FragmentKey>,
    /// Layout input items to resolve, after their parents.
    items: Vec<LayoutInputKey>,
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

    /// Resolve the layout input items under the source of `key` into `tree`
    /// again, then the fragments under `key`. Fragment data is relative to
    /// the parent fragment, so ancestor fragments are neither read nor
    /// resolved. Items inherit from their parents: unresolved ancestor items
    /// are resolved along with them, and resolved ones stay as they are.
    ///
    /// Resolve again the subtrees whose layout or paint styles changed, or the
    /// whole tree after a fragment build or when the root font size or
    /// viewport changes.
    ///
    /// `inputs` is the input tree `fragments` were built from, and `tree`
    /// belongs to them. `units` resolves border radii and transform origins,
    /// with each item's font size.
    ///
    /// Returns `None` if a fragment or item to resolve no longer matches the
    /// UI, such as when its source node was deleted after `inputs` and
    /// `fragments` were built. Rebuild them, then resolve again. Data resolved
    /// before the failure stays in `tree`, so `tree` may be partly updated.
    ///
    /// # Panics
    ///
    /// Panics if `key` is not in `fragments`.
    pub fn resolve(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        fragments: &FragmentTree,
        units: &UnitResolveContext,
        tree: &mut PrePaintTree,
        key: FragmentKey,
    ) -> Option<()> {
        let sources = Sources {
            ui,
            inputs,
            fragments,
            units,
        };

        // Fragments read the stitched sizes of their items.
        self.resolve_items(sources, tree, fragments.items[key].source)?;
        self.resolve_fragments(sources, tree, key)?;
        Some(())
    }

    /// Resolve the data of the fragments under `key`.
    fn resolve_fragments(
        &mut self,
        sources: Sources<'_>,
        tree: &mut PrePaintTree,
        key: FragmentKey,
    ) -> Option<()> {
        let fragments = sources.fragments;

        let pending = &mut self.fragments;
        pending.clear();
        pending.push(key);

        while let Some(key) = pending.pop() {
            let data = sources.resolve_fragment(tree, key)?;

            tree.fragment_mapping.insert(key, data);
            pending.extend(fragments.items.children(key));
        }
        Some(())
    }

    /// Resolve the data of the layout input items under `key`, each after the
    /// parent it inherits from.
    fn resolve_items(
        &mut self,
        sources: Sources<'_>,
        tree: &mut PrePaintTree,
        key: LayoutInputKey,
    ) -> Option<()> {
        let items = &sources.inputs.items;

        // Start below the nearest resolved ancestor.
        let mut start = key;

        while let Some(parent) = items.parent(start)
            && tree.get(parent).is_none()
        {
            start = parent;
        }

        let pending = &mut self.items;
        pending.clear();
        pending.push(start);

        while let Some(key) = pending.pop() {
            let data = sources.resolve_item(tree, key)?;

            tree.layout_mapping.insert(key, data);
            pending.extend(items.children(key).rev());
        }
        Some(())
    }
}

impl Sources<'_> {
    /// Resolve the data of one fragment from the UI and the data of its item.
    fn resolve_fragment(&self, tree: &PrePaintTree, key: FragmentKey) -> Option<FragmentData> {
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
            border_radius: self.border_radius(tree, key, props),
        })
    }

    /// The corner radii of a fragment's border box, or `None` for fragments
    /// painting no box or items setting no radius. Radii resolve against the
    /// stitched size of the item, and corners joining a linked fragment are
    /// square.
    fn border_radius(
        &self,
        tree: &PrePaintTree,
        key: FragmentKey,
        props: Props<'_>,
    ) -> Option<RoundedRectRadii> {
        let fragment = &self.fragments.items[key];

        if matches!(
            fragment.item,
            FragmentItem::Line { .. } | FragmentItem::AnonymousBlock { .. }
        ) {
            return None;
        }

        let BorderRadius(radius) = props.get_cloned()?;

        let mut units = self.units.clone();
        units.font_size = self.inputs.items[fragment.source].text_style.span.font_size as f64;

        let size = tree[fragment.source].stitched_size;
        // TODO:: horizontal, vertical curve value for round rect.
        let basis = size.width.min(size.height);
        let resolve = |value: StyleUnit| value.resolve(&units, basis).max(0.0);

        let mut radii = RoundedRectRadii::new(
            resolve(radius.left_top.y),
            resolve(radius.right_bottom.x),
            resolve(radius.right_bottom.y),
            resolve(radius.left_top.x),
        );

        // Box fragments continue below their previous fragment, inline and
        // glyph run fragments to its right.
        let vertical = fragment.item.is_box();

        if fragment.link.prev.is_some() {
            radii.top_left = 0.0;

            if vertical {
                radii.top_right = 0.0;
            } else {
                radii.bottom_left = 0.0;
            }
        }

        if fragment.link.next.is_some() {
            radii.bottom_right = 0.0;

            if vertical {
                radii.bottom_left = 0.0;
            } else {
                radii.top_right = 0.0;
            }
        }

        Some(radii)
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

    /// Resolve the data of one item from the UI, its fragments and its
    /// parent's data.
    fn resolve_item(&self, tree: &PrePaintTree, key: LayoutInputKey) -> Option<LayoutInputData> {
        let props = self.ui.props(self.inputs.items[key].node)?;

        // Text colors inherit from the parent item unless the item sets one.
        let TextColor(text_color) =
            props
                .get_cloned()
                .unwrap_or_else(|| match self.inputs.items.parent(key) {
                    Some(parent) => TextColor(tree[parent].text_color.clone()),
                    None => TextColor::default(),
                });
        let Overflow(overflow) = props.get_or_default();
        let Opacity(opacity) = props.get_or_default();
        let ZIndex(z_index) = props.get_or_default();
        let z_index = match z_index {
            AutoValue::Auto => None,
            AutoValue::Value(z_index) => Some(z_index),
        };

        Some(LayoutInputData {
            stitched_size: self.stitched_size(key),
            text_color,
            clip: (
                overflow.x != OverflowMode::Visible,
                overflow.y != OverflowMode::Visible,
            ),
            z_index,
            opacity: opacity.clamp(0.0, 1.0),
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
