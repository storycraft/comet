use std::sync::Arc;

use anyrender::{Filter, filters::FilterEffect};
use comet::prelude::*;
use comet_layout::style::{geometry::AutoValue, parley, taffy::Overflow};
use comet_reactivity::NodeBuild;
use comet_tree::node::NodeKey;
use kurbo::{Affine, Vec2};
use peniko::Color;

use super::{MUTED, panel, solid};

pub(super) fn showcase(cx: &UiCx) {
    panel(
        cx,
        "06 / Transforms",
        Color::from_rgb8(250, 240, 246),
        |cx| {
            transform_origins(cx);
            nested_transforms(cx);
        },
    );

    panel(
        cx,
        "07 / Opacity, z-index + filters",
        Color::from_rgb8(240, 244, 250),
        |cx| {
            opacity(cx);
            z_index(cx);
            filters(cx);
        },
    );
}

/// The same rotation around different transform origins. Transforms do not
/// move the layout: the cards keep their row slots.
fn transform_origins(cx: &UiCx) {
    caption(cx, "Transform origin");
    row(cx).children(|cx| {
        for (label, x, y, color) in [
            ("top left", 0, 0, Color::from_rgb8(255, 214, 231)),
            ("centre", 50, 50, Color::from_rgb8(224, 233, 255)),
            ("bottom right", 100, 100, Color::from_rgb8(222, 242, 233)),
        ] {
            card(cx, label, color)
                .transform(Affine::rotate(10f64.to_radians()))
                .transform_origin(point(percent(x), percent(y)))
                .build();
        }

        // Origins also take lengths, and combine with any transform.
        card(cx, "scale · 12px", Color::from_rgb8(255, 236, 200))
            .transform(Affine::scale(0.8))
            .transform_origin(point(px(12), px(12)))
            .build();
    });
}

/// Opacity fades a box together with its contents, as one group.
fn opacity(cx: &UiCx) {
    caption(cx, "Opacity");
    row(cx).children(|cx| {
        for opacity in [1.0, 0.7, 0.4] {
            card(
                cx,
                &format!("opacity {opacity}"),
                Color::from_rgb8(212, 198, 250),
            )
            .opacity(opacity)
            .build();
        }

        // The overlapping children fade as one: they do not show through each
        // other.
        card(cx, "group 0.5", Color::from_rgb8(255, 222, 161))
            .opacity(0.5)
            .children(|cx| {
                chip(cx, Color::from_rgb8(229, 72, 77))
                    .margin(axis(px(6), px(0)))
                    .build();
                chip(cx, Color::from_rgb8(31, 111, 87))
                    .margin(sides(px(-14), px(0), px(0), px(16)))
                    .build();
            });
    });
}

/// Overlapping cards paint by z-index. Auto z-index cards paint in tree
/// order, after negative layers and before positive ones.
fn z_index(cx: &UiCx) {
    caption(cx, "Z-index");
    // The row starts its own layer, so negative layers paint above its
    // background instead of below the panel's.
    row(cx)
        .gap(size(px(0), px(0)))
        .z_index(AutoValue::Value(0))
        .fill(solid(Color::WHITE))
        .border_radius(all(px(10)))
        .children(|cx| {
            for (index, (label, z_index, color)) in [
                ("z 2", AutoValue::Value(2), Color::from_rgb8(255, 214, 231)),
                (
                    "z -1",
                    AutoValue::Value(-1),
                    Color::from_rgb8(224, 233, 255),
                ),
                ("auto", AutoValue::Auto, Color::from_rgb8(222, 242, 233)),
                ("z 1", AutoValue::Value(1), Color::from_rgb8(255, 236, 200)),
            ]
            .into_iter()
            .enumerate()
            {
                // Each card overlaps the previous one and sits a little lower.
                let left = if index == 0 { 0 } else { -40 };
                card(cx, label, color)
                    .z_index(z_index)
                    .margin(sides(px(index as f64 * 8.0), px(0), px(0), px(left)))
                    .build();
            }
        });
}

/// Filters apply to a box and its contents as one group, in its local
/// coordinates.
fn filters(cx: &UiCx) {
    caption(cx, "Filters");
    row(cx).children(|cx| {
        card(cx, "blur 1.5", Color::from_rgb8(212, 198, 250))
            .filter(filter(FilterEffect::blur(1.5)))
            .build();

        card(cx, "drop shadow", Color::WHITE)
            .filter(filter(FilterEffect::drop_shadow(
                3.0,
                4.0,
                3.0,
                Color::from_rgba8(36, 46, 63, 90),
            )))
            .build();

        // The filter sees the box and its children as one image: the shadow
        // follows the chip poking out of the card too.
        card(cx, "shadow · group", Color::from_rgb8(255, 214, 231))
            .filter(filter(FilterEffect::drop_shadow(
                0.0,
                6.0,
                2.0,
                Color::from_rgba8(229, 72, 77, 120),
            )))
            .children(|cx| {
                chip(cx, Color::from_rgb8(229, 72, 77))
                    .margin(sides(px(6), px(0), px(0), px(96)))
                    .build();
            });

        // Offsets are in the box's local coordinates, so they turn with it.
        card(cx, "offset · rotated", Color::from_rgb8(222, 242, 233))
            .filter(filter(FilterEffect::Offset(Vec2::new(12.0, 0.0))))
            .transform(Affine::rotate(20f64.to_radians()))
            .build();
    });
}

fn filter(effect: FilterEffect) -> Arc<Filter> {
    Arc::new(Filter::single(
        effect,
        // TODO:: move out filter to painter
        kurbo::Rect::ZERO.inflate(f64::MAX, f64::MAX),
    ))
}

/// Nested transforms multiply, and overflow clips transformed contents.
fn nested_transforms(cx: &UiCx) {
    caption(cx, "Nesting + clipping");

    // Text colors are inherited through transforms.
    card(cx, "rotated parent", Color::from_rgb8(236, 228, 255))
        .size(size(px(260), auto()))
        .margin(axis(px(18), px(16)))
        .text_color(solid(Color::from_rgb8(96, 65, 177)))
        .transform(Affine::rotate((-4f64).to_radians()))
        .children(|cx| {
            card(cx, "translated child", Color::from_rgb8(212, 198, 250))
                .size(size(px(170), auto()))
                .margin(sides(px(8), px(0), px(0), px(0)))
                .transform(Affine::translate((60.0, 0.0)))
                .build();
        });

    // The clip also applies to the translucent strip, painting in its own
    // layer.
    cx.div()
        .size(size(auto(), px(64)))
        .overflow(point(Overflow::Hidden, Overflow::Hidden))
        .padding(all(px(10)))
        .fill(solid(Color::WHITE))
        .border_radius(all(px(10)))
        .children(|cx| {
            card(
                cx,
                "overflow clips this rotated, translucent strip at the stage edges",
                Color::from_rgb8(255, 222, 161),
            )
            .size(size(px(640), auto()))
            .opacity(0.8)
            .transform(Affine::rotate((-5f64).to_radians()))
            .transform_origin(point(px(200), percent(50)))
            .build();
        });
}

fn caption(cx: &UiCx, label: &str) -> NodeKey {
    cx.div()
        .font_size(px(12))
        .font_weight(parley::FontWeight::SEMI_BOLD)
        .text_color(solid(MUTED))
        .padding(sides(px(8), px(0), px(4), px(0)))
        .children(|cx| cx.text(label).build())
}

fn row(cx: &UiCx) -> NodeBuild<'_> {
    cx.div()
        .flex()
        .gap(size(px(16), px(16)))
        .padding(axis(px(10), px(8)))
}

/// A labelled card, left open for more styles and children. The label is
/// built before them.
fn card<'a>(cx: &'a UiCx, label: &str, color: Color) -> Card<'a> {
    Card {
        build: cx
            .div()
            .size(size(px(110), auto()))
            .padding(axis(px(10), px(12)))
            .fill(solid(color))
            .border_radius(all(px(8)))
            .font_size(px(14))
            .font_weight(parley::FontWeight::SEMI_BOLD),
        label: label.to_owned(),
    }
}

/// A card being built: [`Styled`] like a node, with its label added first
/// when it is finished.
struct Card<'a> {
    build: NodeBuild<'a>,
    label: String,
}

impl Card<'_> {
    fn build(self) -> NodeKey {
        self.children(|_| {})
    }

    fn children(self, children: impl FnOnce(&UiCx)) -> NodeKey {
        let Self { build, label } = self;
        build.children(|cx| {
            cx.text(label).build();
            children(cx);
        })
    }
}

impl Styled for Card<'_> {
    fn bind<P, T>(self, value: impl Into<Signal<T>>) -> Self
    where
        P: ReactiveProp + From<T>,
        T: Clone + Send + Sync + 'static,
    {
        Self {
            build: self.build.bind::<P, T>(value),
            label: self.label,
        }
    }
}

/// A small solid square.
fn chip(cx: &UiCx, color: Color) -> NodeBuild<'_> {
    cx.div()
        .size(size(px(28), px(20)))
        .fill(solid(color))
        .border_radius(all(px(4)))
}
