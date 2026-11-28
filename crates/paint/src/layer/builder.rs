use std::sync::Arc;

use anyrender::filters::FilterEffect;
use comet_geometry::{AutoValue, Point as StylePoint, StyleUnit, UnitResolveContext};
use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{BoxFragment, BoxFragmentKey, FragmentGeometry, FragmentRef, tree::FragmentTree},
    input::{LayoutInputKey, tree::LayoutInputTree},
    style::bx::Overflow,
};
use comet_tree::ui::{Props, Ui};
use kurbo::{Affine, Point, Size, Vec2};

use crate::{
    layer::{
        PaintLayerItem, PaintLayerKey,
        tree::{PaintLayerTree, layer_owner},
    },
    prepaint::fragment::{
        BoxRole, for_each_paint_child, paint_parent, paint_props, part_of_piece, piece_of_part,
    },
    style::{
        BackdropFilter, Filter, Layer, MixBlend, Opacity, ScrollLeft, ScrollTop, Transform,
        TransformOrigin, ZIndex,
    },
};

/// Builds and updates a [`PaintLayerTree`] for a fragment tree, from the
/// layer styles of its items. Keep it to reuse its buffers: once the trees
/// settle, building again allocates nothing but the filters it resolves.
#[derive(Default)]
pub struct PaintLayerBuilder {
    /// Number of the current build, stamped on the layers it reaches.
    pass: u32,
    /// Fragments to place, with the layer they paint in.
    pending: Vec<Visit>,
    /// Layout input items to check for layer changes.
    items: Vec<LayoutInputKey>,
    /// Layers to place the child layers of again, with their depths.
    dirty: Vec<(usize, PaintLayerKey)>,
    /// Child layers detached from the layers being rebuilt. Those not
    /// reached again are deleted.
    detached: Vec<PaintLayerKey>,
    /// Layers to delete, children after their parents.
    deleting: Vec<PaintLayerKey>,
    /// Layers to resolve again after the viewport changed.
    viewport: Vec<PaintLayerKey>,
}

/// What building reads from.
#[derive(Clone, Copy)]
struct Sources<'a> {
    ui: &'a Ui,
    inputs: &'a LayoutInputTree,
    fragments: &'a FragmentTree,
    units: &'a UnitResolveContext,
}

#[derive(Clone, Copy)]
struct Visit {
    fragment: FragmentRef,
    layer: PaintLayerKey,
}

impl PaintLayerBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build every layer under `root`, deleting the layers of other boxes,
    /// and return the root layer.
    fn build_root(
        &mut self,
        sources: Sources<'_>,
        tree: &mut PaintLayerTree,
        root: BoxFragmentKey,
    ) -> PaintLayerKey {
        let root_layer = match tree.layer_of(root) {
            Some(layer) => {
                tree.layers.remove_parent(layer);
                layer
            }
            None => self.insert(tree, root, 0),
        };
        let item = &mut tree.layers[root_layer];
        item.z_index = 0;
        sources.resolve(item);

        self.rebuild(sources, tree, root_layer, true);

        // Layers of boxes outside `root`, such as previous roots, were not
        // reached.
        self.deleting.clear();
        self.deleting
            .extend(tree.by_owner.values().copied().filter(|&layer| {
                tree.layers
                    .get(layer)
                    .is_some_and(|item| item.pass != self.pass)
            }));
        self.delete_pending(tree);

        root_layer
    }

    /// Resolve the data of the layers whose owner's transform origin uses
    /// `vw` or `vh` again.
    fn resolve_viewport(&mut self, sources: Sources<'_>, tree: &mut PaintLayerTree) {
        self.viewport.clear();
        self.viewport
            .extend(tree.by_owner.iter().filter_map(|(owner, &layer)| {
                let props = sources.props(sources.fragments.get(owner)?.source)?;
                let TransformOrigin(origin) = props.get_cloned()?;
                let viewport = origin.x.is_viewport_relative() || origin.y.is_viewport_relative();

                viewport.then_some(layer)
            }));

        for &layer in &self.viewport {
            if let Some(item) = tree.layers.get_mut(layer) {
                sources.resolve(item);
            }
        }
    }

    /// Resolve the data of the layers owned by marked items again, and place
    /// the child layers of the layers with layer changes again.
    fn update(
        &mut self,
        sources: Sources<'_>,
        dirty: &DirtyMap,
        tree: &mut PaintLayerTree,
        root: LayoutInputKey,
        root_layer: PaintLayerKey,
    ) {
        let Sources {
            inputs, fragments, ..
        } = sources;
        let root_fragment = tree.layers[root_layer].owner;
        self.dirty.clear();

        // Layers whose owners were deleted, such as the inline part boxes of
        // an inline layout emitted again, are not reached from the
        // fragments, so the layers they paint in are placed again.
        for (owner, &layer) in &tree.by_owner {
            if fragments.get(owner).is_none()
                && let Some(parent) = tree.layers.parent(layer)
            {
                self.dirty.push((0, parent));
            }
        }

        // Layers are found from the layers the ancestors started before this
        // build.
        self.items.clear();
        self.items.push(root);

        while let Some(key) = self.items.pop() {
            if inputs.items.get(key).is_none() {
                continue;
            }
            let entry = dirty.entry(key);

            if entry.flags.contains(DirtyFlags::NEED_LAYER)
                && let Some(owner) = sources.owner_of_item(key)
            {
                let current = tree.layer_of(owner);

                // Layer data follows the owner's styles and its layout.
                if let Some(item) = current.and_then(|layer| tree.layers.get_mut(layer)) {
                    sources.resolve(item);
                }

                // `NEED_CHILDREN` includes `NEED_LAYER`, so the stacking of
                // the owner is compared with the layer it starts. The root
                // layer has z index 0, whatever the root's styles.
                let stacking = sources.stacking(owner);
                let current = current.map(|layer| tree.layers[layer].z_index);

                // An owner gaining, losing or reordering its layer changes the
                // child layers of the layer it paints in.
                if owner != root_fragment
                    && stacking != current
                    && let Some(layer) = paint_parent(fragments, FragmentRef::Box(owner))
                        .and_then(|parent| tree.enclosing(fragments, parent))
                {
                    self.dirty.push((0, layer));
                }
            }

            // Children inserted, removed or reordered can reorder the layers
            // they start, which are child layers of the layer enclosing the
            // item's fragments: its own layer if it starts one.
            if entry.flags.contains(DirtyFlags::NEED_CHILDREN)
                && let Some(layer) = fragments
                    .fragments(inputs, key)
                    .next()
                    .and_then(|fragment| tree.enclosing(fragments, fragment))
            {
                self.dirty.push((0, layer));
            }

            if entry.child_bits.contains(DirtyFlags::NEED_LAYER) {
                self.items.extend(inputs.items.children(key));
            }
        }

        for (depth, layer) in &mut self.dirty {
            *depth = std::iter::successors(tree.layers.parent(*layer), |&layer| {
                tree.layers.parent(layer)
            })
            .count();
        }

        // Deeper layers first: placing the child layers of a layer again
        // can take grandchildren from a child layer losing its owner, which
        // placing that child layer later would take back.
        self.dirty
            .sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        self.dirty.dedup();

        for index in 0..self.dirty.len() {
            let layer = self.dirty[index].1;
            let Some(item) = tree.layers.get(layer) else {
                continue;
            };

            // A layer whose owner was deleted or no longer starts one is
            // deleted by the layer it paints in, which is placed again.
            let owner = item.owner;
            let starts = layer == root_layer || sources.stacking(owner).is_some();

            if fragments.get(owner).is_none() || !starts {
                continue;
            }

            self.rebuild(sources, tree, layer, false);
        }
    }

    /// Place the child layers of `layer` again from the descendants of its
    /// owner in paint order. Layers new in this build are built the same
    /// way, and so is every other child layer when `deep`; other child layers
    /// keep their children. Layers built resolve their data. Detached child
    /// layers are deleted at the end of the build unless they are reached
    /// again.
    fn rebuild(
        &mut self,
        sources: Sources<'_>,
        tree: &mut PaintLayerTree,
        layer: PaintLayerKey,
        deep: bool,
    ) {
        let fragments = sources.fragments;
        let owner = tree.layers[layer].owner;

        tree.layers[layer].pass = self.pass;
        self.pending.clear();
        self.detach_children(tree, layer);
        self.push_layer_children(fragments, owner, layer);

        while let Some(Visit { fragment, layer }) = self.pending.pop() {
            let starting = layer_owner(fragments, fragment)
                .and_then(|owner| Some((owner, sources.stacking(owner)?)));
            let Some((owner, z_index)) = starting else {
                self.push_children(fragments, fragment, layer);
                continue;
            };

            // The pieces of an inline element paint in the layer its first
            // piece starts, which covers them all.
            let own = match fragment {
                FragmentRef::Box(key) => Some(key),
                FragmentRef::Inline { .. } => part_of_piece(fragments, fragment),
            };
            if own != Some(owner) {
                continue;
            }

            let (child, created) = match tree.layer_of(owner) {
                Some(child) => (child, false),
                None => (self.insert(tree, owner, z_index), true),
            };
            let build = deep || created;
            let item = &mut tree.layers[child];
            item.z_index = z_index;
            item.pass = self.pass;

            if build {
                sources.resolve(item);
            }

            insert_sorted(tree, layer, child);

            if build {
                self.detach_children(tree, child);
                self.push_layer_children(fragments, owner, child);
            }
        }
    }

    /// Insert a parentless layer owned by `owner`, with its data unresolved.
    fn insert(
        &self,
        tree: &mut PaintLayerTree,
        owner: BoxFragmentKey,
        z_index: i32,
    ) -> PaintLayerKey {
        let layer = tree.layers.insert(PaintLayerItem {
            owner,
            transform: Affine::IDENTITY,
            inverse_transform: Some(Affine::IDENTITY),
            z_index,
            opacity: 1.0,
            filter: None,
            backdrop_filter: None,
            blend_mode: None,
            scroll: Vec2::ZERO,
            pass: self.pass,
        });
        tree.by_owner.insert(owner, layer);

        layer
    }

    /// Detach the child layers of `layer`, keeping them for reuse.
    fn detach_children(&mut self, tree: &mut PaintLayerTree, layer: PaintLayerKey) {
        while let Some(child) = tree.layers.first_child(layer) {
            tree.layers.remove_parent(child);
            self.detached.push(child);
        }
    }

    /// Queue the children of `fragment` in paint order, painting in `layer`.
    fn push_children(
        &mut self,
        fragments: &FragmentTree,
        fragment: FragmentRef,
        layer: PaintLayerKey,
    ) {
        let start = self.pending.len();

        for_each_paint_child(fragments, fragment, |child| {
            self.pending.push(Visit {
                fragment: child,
                layer,
            });
        });

        // Popped from the last.
        self.pending[start..].reverse();
    }

    /// Queue the fragments painting in the layer `owner` starts, in paint
    /// order: the children of a principal box, or those of every piece of an
    /// inline element, following its linked inline part boxes.
    fn push_layer_children(
        &mut self,
        fragments: &FragmentTree,
        owner: BoxFragmentKey,
        layer: PaintLayerKey,
    ) {
        let start = self.pending.len();
        let mut push = |fragment| {
            for_each_paint_child(fragments, fragment, |child| {
                self.pending.push(Visit {
                    fragment: child,
                    layer,
                });
            });
        };

        if piece_of_part(fragments, owner).is_some() {
            for part in fragments.split_boxes(owner) {
                if let Some(piece) = piece_of_part(fragments, part) {
                    push(piece);
                }
            }
        } else {
            push(FragmentRef::Box(owner));
        }

        // Popped from the last.
        self.pending[start..].reverse();
    }

    /// Delete the detached layers this build did not reach again.
    fn delete_detached(&mut self, tree: &mut PaintLayerTree) {
        self.deleting.clear();
        for &layer in &self.detached {
            if tree
                .layers
                .get(layer)
                .is_some_and(|item| item.pass != self.pass)
            {
                self.deleting.push(layer);
            }
        }
        self.delete_pending(tree);
    }

    /// Delete the queued layers and the layers left under them. Layers are
    /// deleted as leaves, children first, so deleting allocates nothing.
    fn delete_pending(&mut self, tree: &mut PaintLayerTree) {
        while let Some(&layer) = self.deleting.last() {
            if tree.layers.get(layer).is_none() {
                self.deleting.pop();
                continue;
            }

            if let Some(child) = tree.layers.first_child(layer) {
                self.deleting.push(child);
                continue;
            }

            self.deleting.pop();

            if let Some(item) = tree.layers.delete_node(layer)
                && tree.by_owner.get(item.owner) == Some(&layer)
            {
                tree.by_owner.remove(item.owner);
            }
        }
    }
}

#[bon::bon]
impl PaintLayerBuilder {
    /// Build the layers of the fragments generated under the layout input
    /// item `root` into `tree`, and return the root layer, owned by the box
    /// fragment of `root`. The root box always starts the root layer, with
    /// z index 0, whatever its styles.
    ///
    /// A principal box starts a layer when its item sets [`Layer`] to true,
    /// a [`Transform`], an [`Opacity`], a [`Filter`], a [`BackdropFilter`]
    /// or a [`MixBlend`], whatever its value, a [`ZIndex`] other than auto,
    /// or an [`Overflow`]
    /// scrolling or hiding its overflow on an axis, making it a scroll
    /// container. So does an inline element setting one of them other than a
    /// [`Transform`] or an [`Overflow`], which inline elements ignore: its
    /// first inline part box owns the layer, covering every piece of the
    /// element. An auto z index counts as 0. Each layer resolves its
    /// transform, opacity, filters, blend mode and scroll offset from its
    /// owner's item.
    ///
    /// When `tree` already holds the root layer, only the layers affected by
    /// changes marked in `dirty` are updated. Layers whose owner still starts
    /// one keep their keys. Items with [`DirtyFlags::NEED_LAYER`], which
    /// [`DirtyFlags::NEED_CHILDREN`] includes, are checked for boxes gaining,
    /// losing or reordering a layer, and the layers they own resolve their
    /// data again. It is marked when a layer style changes, and by the
    /// fragment builder for the items whose fragments it changed, including
    /// every item of an inline layout it emitted again.
    ///
    /// `units` resolves transform origins, with the font size of each owner.
    /// With `viewport_changed`, every layer whose owner's transform origin
    /// uses `vw` or `vh` resolves its data again. Pass it when the viewport
    /// in `units` differs from the last build. The root font size is not
    /// tracked: build into a new or [cleared](PaintLayerTree::clear) tree
    /// when it changes.
    ///
    /// `inputs` is the input tree `fragments` were built from. Build before
    /// clearing `dirty`. Returns `None` if `root` has no box fragment in
    /// `fragments`.
    #[builder(finish_fn = build)]
    pub fn prepare(
        &mut self,
        #[builder(start_fn)] tree: &mut PaintLayerTree,
        ui: &Ui,
        inputs: &LayoutInputTree,
        dirty: &DirtyMap,
        fragments: &FragmentTree,
        units: &UnitResolveContext,
        root: LayoutInputKey,
        viewport_changed: bool,
    ) -> Option<PaintLayerKey> {
        let sources = Sources {
            ui,
            inputs,
            fragments,
            units,
        };
        let root_fragment = fragments.box_fragment(root)?;
        self.pass = self.pass.wrapping_add(1);
        self.detached.clear();

        let root_layer = match tree.layer_of(root_fragment) {
            Some(layer) if tree.layers.parent(layer).is_none() => {
                if viewport_changed {
                    self.resolve_viewport(sources, tree);
                }
                self.update(sources, dirty, tree, root, layer);
                layer
            }
            _ => self.build_root(sources, tree, root_fragment),
        };

        self.delete_detached(tree);

        Some(root_layer)
    }
}

impl Sources<'_> {
    /// The props a layout input item paints with.
    fn props(&self, key: LayoutInputKey) -> Option<Props<'_>> {
        paint_props(self.ui, self.inputs, key)
    }

    /// The box that would own the layer an item starts: its box, or the
    /// first inline part box of an inline element.
    fn owner_of_item(&self, key: LayoutInputKey) -> Option<BoxFragmentKey> {
        self.fragments.box_fragment(key).or_else(|| {
            let part = self.fragments.inline_boxes(self.inputs, key).next()?;
            layer_owner(self.fragments, FragmentRef::Box(part))
        })
    }

    /// The z index of the layer a box starts, or `None` if it paints in the
    /// layer of its paint parent. Only principal boxes and the inline part
    /// boxes of inline elements start layers.
    fn stacking(&self, key: BoxFragmentKey) -> Option<i32> {
        let fragment = self.fragments.get(key)?;
        let principal = match BoxRole::of(fragment) {
            BoxRole::Principal => true,
            BoxRole::InlinePart => false,
            BoxRole::Anonymous => return None,
        };

        stacking(self.props(fragment.source)?, principal)
    }

    /// Resolve the transform, opacity, filters, blend mode and scroll offset
    /// of a layer from its owner's item.
    fn resolve(&self, item: &mut PaintLayerItem) {
        let fragment = self.fragments.get(item.owner);
        let props = fragment.and_then(|fragment| self.props(fragment.source));

        // Only principal boxes are transformed.
        item.transform = fragment
            .zip(props)
            .filter(|(fragment, _)| BoxRole::of(fragment).is_principal())
            .map_or(Affine::IDENTITY, |(fragment, props)| {
                self.transform(fragment, props)
            });
        item.inverse_transform = invert(item.transform);

        item.opacity = props.map_or(1.0, |props| {
            let Opacity(opacity) = props.get_or_default();
            opacity.clamp(0.0, 1.0) as f32
        });
        item.filter = props.and_then(|props| {
            let effects = props.get::<Filter>()?;
            let bounds = self.bounds(item.owner, |geometry| geometry.overflow)?;
            Some(filter(&effects.0, bounds))
        });
        item.backdrop_filter = props.and_then(|props| {
            let effects = props.get::<BackdropFilter>()?;
            let bounds = self.bounds(item.owner, FragmentGeometry::border_box)?;
            Some(filter(&effects.0, bounds))
        });
        item.blend_mode = props
            .and_then(|props| props.get_cloned())
            .map(|MixBlend(mode)| mode);

        // Only principal scroll containers scroll, through their single box
        // fragment. Both axes scroll once either does, as both clip.
        let principal = fragment.is_some_and(|fragment| BoxRole::of(fragment).is_principal());
        let props = props.filter(|&props| principal && scrolls(props));

        item.scroll = match props.zip(self.fragments.scroll_range(item.owner)) {
            Some((props, max)) => {
                let ScrollLeft(left) = props.get_or_default();
                let ScrollTop(top) = props.get_or_default();

                // `max` first maps NaN to 0, which `min` would map to the
                // range instead. The range is at least 0.
                Vec2::new(left.max(0.0).min(max.x), top.max(0.0).min(max.y))
            }
            _ => Vec2::ZERO,
        };
    }

    /// The own transform of a principal box with `props`: its [`Transform`]
    /// applied around its [`TransformOrigin`], resolved with its item's font
    /// size against its border box.
    fn transform(&self, fragment: &BoxFragment, props: Props<'_>) -> Affine {
        let Transform(transform) = props.get_or_default();

        if transform == Affine::IDENTITY {
            return Affine::IDENTITY;
        }

        let TransformOrigin(origin) = props.get_or_default();
        let mut units = self.units.clone();
        units.font_size = self.inputs.items[fragment.source].text_style.span.font_size as f64;

        let size = fragment.layout.geometry.size;
        let origin = resolve_origin(origin, &units, Size::new(size.width, size.height));

        Affine::translate(origin) * transform * Affine::translate(-origin)
    }

    /// The `area` of the box `owner`, in its local coordinates, joined with
    /// those of the other pieces of its inline element when it is an inline
    /// part box.
    fn bounds(
        &self,
        owner: BoxFragmentKey,
        area: impl Fn(&FragmentGeometry) -> kurbo::Rect,
    ) -> Option<kurbo::Rect> {
        let fragment = self.fragments.get(owner)?;
        let mut bounds = area(&fragment.layout.geometry);

        if BoxRole::of(fragment) != BoxRole::InlinePart {
            return Some(bounds);
        }

        let origin = self.content_offset(owner)?;

        for part in self.fragments.split_boxes(owner).skip(1) {
            if let Some(offset) = self.content_offset(part) {
                let geometry = &self.fragments.boxes[part].layout.geometry;
                bounds = bounds.union(area(geometry) + (offset - origin));
            }
        }

        Some(bounds)
    }

    /// The location of an inline part box in the box holding the inline
    /// content of its element: the owner of its inline layout, or the box
    /// whose run that is. Run boxes are only placed, so the pieces of an
    /// element share these coordinates.
    fn content_offset(&self, part: BoxFragmentKey) -> Option<Vec2> {
        let location = |key| {
            let location = self.fragments.get(key)?.layout.geometry.location;
            Some(Vec2::new(location.x, location.y))
        };
        let parent = self.fragments.boxes.parent(part)?;
        let mut offset = location(part)?;

        if BoxRole::of(self.fragments.get(parent)?) == BoxRole::Anonymous {
            offset += location(parent)?;
        }

        Some(offset)
    }
}

/// The z index of the layer a fragment with `props` starts, or `None` if it
/// paints in the layer of its paint parent. A true [`Layer`] starts a
/// layer, and so does setting another layer style, whatever its value,
/// though a [`Transform`] only for `principal` boxes. So does being a scroll container, for `principal` boxes only. An
/// auto z index counts as 0.
fn stacking(props: Props<'_>, principal: bool) -> Option<i32> {
    if let AutoValue::Value(z_index) = props.get_or_default::<ZIndex>().0 {
        return Some(z_index);
    }

    let starts = principal && (props.get::<Transform>().is_some() || scrolls(props))
        || props.get_or_default::<Layer>().0
        || props.get::<Opacity>().is_some()
        || props.get::<Filter>().is_some()
        || props.get::<BackdropFilter>().is_some()
        || props.get::<MixBlend>().is_some();

    starts.then_some(0)
}

/// The point of a border box of `size` that `origin` resolves to, as a
/// vector from its top-left corner. Percentages use its width and height.
fn resolve_origin(origin: StylePoint<StyleUnit>, units: &UnitResolveContext, size: Size) -> Vec2 {
    let origin = origin.resolve(units, StylePoint::new(size.width, size.height));

    Point::new(origin.x, origin.y).to_vec2()
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

/// Whether an item with `props` is a scroll container, scrolling or hiding
/// its overflow on an axis.
fn scrolls(props: Props<'_>) -> bool {
    let Overflow(overflow) = props.get_or_default();

    overflow.x.is_scroll_container() || overflow.y.is_scroll_container()
}

/// A filter applying `effects` in order to content within `bounds`, its
/// region grown by the expansion of each effect.
fn filter(effects: &[FilterEffect], bounds: kurbo::Rect) -> Arc<anyrender::Filter> {
    Arc::new(anyrender::Filter::linear_list(
        effects.iter().cloned(),
        bounds,
    ))
}

/// Attach `child` to `parent` after the last child layer with a z index at
/// most its own. Children attached in paint order then stay in paint order
/// among equal z indices.
fn insert_sorted(tree: &mut PaintLayerTree, parent: PaintLayerKey, child: PaintLayerKey) {
    let z_index = tree.layers[child].z_index;
    let after = tree
        .layers
        .children(parent)
        .rev()
        .find(|&sibling| sibling != child && tree.layers[sibling].z_index <= z_index);

    match after {
        Some(sibling) => tree.layers.after(sibling, child),
        None => tree.layers.prepend(parent, child),
    };
}
