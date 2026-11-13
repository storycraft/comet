use anyrender::Paint;
use comet_layout::{
    fragment::{FragmentItem, FragmentKey, tree::FragmentTree},
    input::{LayoutInputKey, tree::LayoutInputTree},
    style::{
        bx::Overflow,
        geometry::{Point as StylePoint, StyleUnit, UnitResolveContext},
        taffy::Overflow as OverflowMode,
    },
};
use comet_tree::ui::{Props, Ui};
use kurbo::{Affine, Point, Rect, RoundedRectRadii, Size, Vec2};

use crate::{
    pre_paint::{FragmentData, LayoutInputData, tree::PrePaintTree},
    style::{BorderRadius, TextColor, Transform, TransformOrigin},
};

/// Resolves a [`PrePaintTree`] for a fragment tree. Keep it to reuse its
/// buffers across resolves.
#[derive(Default)]
pub struct PrePaintTreeResolver {
    /// Fragments to position, with the absolute position of their parents.
    fragments: Vec<(FragmentKey, Point)>,
    /// Positioned fragments, to resolve their other data after their items.
    positioned: Vec<FragmentKey>,
    /// Layout input items to resolve, after their parents.
    items: Vec<LayoutInputKey>,
}

/// What resolving an item reads from.
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

    /// Resolve the fragments under `key` into `tree` again, then the layout
    /// input items under its source. Unresolved ancestors are resolved along
    /// with them, and resolved ancestors stay as they are.
    ///
    /// Resolve again the subtrees whose layout or paint styles changed, or the
    /// whole tree after a fragment build or when the root font size or
    /// viewport changes. Resolving from the root also drops the data of
    /// deleted fragments and items.
    ///
    /// `inputs` is the input tree `fragments` were built from, and `tree`
    /// belongs to them. `units` resolves border radii and transform origins,
    /// with each item's font size.
    pub fn resolve(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        fragments: &FragmentTree,
        units: &UnitResolveContext,
        tree: &mut PrePaintTree,
        key: FragmentKey,
    ) {
        let sources = Sources {
            ui,
            inputs,
            fragments,
            units,
        };

        // Items read the positions of their fragments, and fragments the
        // stitched sizes of their items.
        self.resolve_positions(sources, tree, key);
        self.resolve_items(sources, tree, fragments.items[key].source);

        for &key in &self.positioned {
            let data = sources.resolve_fragment(tree, key);

            tree.fragment_mapping[key] = data;
        }
    }

    /// Resolve the positions of the fragments under `key`, each after the
    /// parent it is positioned in. Their other data is left unresolved.
    fn resolve_positions(
        &mut self,
        sources: Sources<'_>,
        tree: &mut PrePaintTree,
        key: FragmentKey,
    ) {
        let fragments = sources.fragments;

        // Start below the nearest positioned ancestor.
        let mut start = key;
        let mut origin = Point::ORIGIN;

        while let Some(parent) = fragments.items.parent(start) {
            if let Some(parent) = tree.fragment(parent) {
                origin = parent.position;
                break;
            }

            start = parent;
        }

        let pending = &mut self.fragments;
        pending.clear();
        pending.push((start, origin));
        self.positioned.clear();

        while let Some((key, origin)) = pending.pop() {
            let location = fragments.items[key].layout.location;
            let position = origin + Vec2::new(location.x, location.y);

            tree.fragment_mapping.insert(
                key,
                FragmentData {
                    position,
                    border_radius: None,
                    clip: (false, false),
                },
            );
            self.positioned.push(key);
            pending.extend(
                fragments
                    .items
                    .children(key)
                    .rev()
                    .map(|child| (child, position)),
            );
        }

        // Every fragment of the tree was positioned.
        if fragments.items.parent(start).is_none() {
            tree.retain_fragments(fragments);
        }
    }

    /// Resolve the data of the layout input items under `key`, each after the
    /// parent it inherits from.
    fn resolve_items(
        &mut self,
        sources: Sources<'_>,
        tree: &mut PrePaintTree,
        key: LayoutInputKey,
    ) {
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
            let data = sources.resolve_item(tree, key);

            tree.layout_mapping.insert(key, data);
            pending.extend(items.children(key).rev());
        }

        // Every item of the tree was resolved.
        if items.parent(start).is_none() {
            tree.retain_items(sources.inputs);
        }
    }
}

impl Sources<'_> {
    /// Resolve the data of one positioned fragment from the UI and the data of
    /// its item.
    fn resolve_fragment(&self, tree: &PrePaintTree, key: FragmentKey) -> FragmentData {
        let fragment = &self.fragments.items[key];
        let props = self.ui.props(self.inputs.items[fragment.source].node);

        FragmentData {
            position: tree[key].position,
            border_radius: props.and_then(|props| self.border_radius(tree, key, props)),
            // Overflow clips only apply to boxes.
            clip: props
                .filter(|_| fragment.item.is_box())
                .map_or((false, false), lookup_clip),
        }
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

        let radius = props.get::<BorderRadius>()?.0;

        let mut units = self.units.clone();
        units.font_size = self.inputs.items[fragment.source].text_style.span.font_size as f64;

        let size = tree[fragment.source].stitched_size;
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

    /// Resolve the data of one item from the UI, its fragments and its
    /// parent's data.
    fn resolve_item(&self, tree: &PrePaintTree, key: LayoutInputKey) -> LayoutInputData {
        let item = &self.inputs.items[key];
        let props = self.ui.props(item.node);
        // Transforms only apply to boxes.
        let box_props = props.filter(|_| self.fragments.box_fragment(key).is_some());

        let (inherited_transform, inherited_color) =
            match self.inputs.items.parent(key).map(|parent| &tree[parent]) {
                Some(parent) => (parent.transform, parent.text_color.clone()),
                None => (Affine::IDENTITY, TextColor::default().0),
            };

        let bounding_box = self.bounding_box(tree, key);
        let transform_origin = self.transform_origin(
            key,
            box_props.map_or(TransformOrigin::default().0, lookup_transform_origin),
            bounding_box,
        );
        let transform = resolve_transform(
            inherited_transform,
            box_props.map_or(Transform::default().0, lookup_transform),
            transform_origin,
        );

        LayoutInputData {
            bounding_box,
            stitched_size: self.stitched_size(key),
            transform,
            inverse_transform: invert(transform),
            transform_origin,
            text_color: props.and_then(lookup_text_color).unwrap_or(inherited_color),
        }
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

    /// The absolute bounding box of the fragments an item generates and links
    /// together. Items without fragments have an empty box at the origin.
    fn bounding_box(&self, tree: &PrePaintTree, key: LayoutInputKey) -> Rect {
        self.fragments
            .fragments(key)
            .map(|fragment| {
                let size = self.fragments.items[fragment].layout.size;

                Rect::from_origin_size(tree[fragment].position, (size.width, size.height))
            })
            .reduce(|bounds, rect| bounds.union(rect))
            .unwrap_or(Rect::ZERO)
    }

    /// The absolute point of `bounds` an item's own transform is applied
    /// around, resolving `origin` with the item's font size.
    fn transform_origin(
        &self,
        key: LayoutInputKey,
        origin: StylePoint<StyleUnit>,
        bounds: Rect,
    ) -> Point {
        let mut units = self.units.clone();
        units.font_size = self.inputs.items[key].text_style.span.font_size as f64;

        let origin = origin.resolve(&units, StylePoint::new(bounds.width(), bounds.height()));
        bounds.origin() + Vec2::new(origin.x, origin.y)
    }
}

/// The text color an item sets, or `None` to inherit it.
fn lookup_text_color(props: Props<'_>) -> Option<Paint> {
    props.get::<TextColor>().map(|color| color.0.clone())
}

/// The transform of a box, or identity.
fn lookup_transform(props: Props<'_>) -> Affine {
    props
        .get::<Transform>()
        .map_or(Transform::default().0, |transform| transform.0)
}

/// The point of a box its transform is applied around, or its centre.
fn lookup_transform_origin(props: Props<'_>) -> StylePoint<StyleUnit> {
    props
        .get::<TransformOrigin>()
        .map_or(TransformOrigin::default().0, |origin| origin.0)
}

/// The axes a box clips its overflow on, or none.
fn lookup_clip(props: Props<'_>) -> (bool, bool) {
    let overflow = props
        .get::<Overflow>()
        .map_or(Overflow::default().0, |overflow| overflow.0);

    (
        overflow.x != OverflowMode::Visible,
        overflow.y != OverflowMode::Visible,
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

/// Apply an item's own `transform` around its absolute `origin`, after the
/// transforms it inherits.
fn resolve_transform(inherited: Affine, transform: Affine, origin: Point) -> Affine {
    if transform == Affine::IDENTITY {
        return inherited;
    }

    let origin = origin.to_vec2();

    inherited * Affine::translate(origin) * transform * Affine::translate(-origin)
}
