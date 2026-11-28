use comet::prelude::*;
use comet_content::{ContentId, style::ObjectFit};
use comet_content_image::ImageContent;
use comet_geometry::{AutoValue, Size};
use comet_layout::style::bx::{DisplayInner, DisplayOuter};
use comet_paint::style::Paint;
use comet_reactivity::cx::NodeBuild;
use comet_tree::node::NodeKey;
use peniko::{Blob, Color, ImageAlphaType, ImageData, ImageFormat};
use taffy::{AlignItems, FlexWrap};

use super::{MUTED, panel, solid};

/// The sample image, a black cat 512px square.
const BLACK_CAT: &[u8] = include_bytes!("images/black_cat.png");

pub(super) fn showcase(cx: &UiCx) -> NodeKey {
    // Every box below shows the same content, sized and drawn for each.
    let cat = cx
        .tree()
        .borrow_mut()
        .contents
        .insert(ImageContent::new(decode(BLACK_CAT)));

    panel(
        cx,
        "11 / Image content",
        Color::from_rgb8(246, 241, 236),
        |cx| {
            [
                caption(cx, "Content box / sized by the image, keeping its ratio"),
                row(cx, |cx| {
                    [
                        // A width alone keeps the image square.
                        image(cx, cat)
                            .object_fit(ObjectFit::Fill)
                            .size(size(px(96), auto()))
                            .build(),
                        // Padding and border wrap around the image.
                        image(cx, cat)
                            .object_fit(ObjectFit::Fill)
                            .size(size(px(72), auto()))
                            .padding(all(px(6)))
                            .border(all(px(2)))
                            .border_fill(solid(Color::from_rgb8(52, 84, 209)))
                            .background(solid(Color::WHITE))
                            .build(),
                        // A custom ratio sizes the box and stretches the image.
                        image(cx, cat)
                            .object_fit(ObjectFit::Fill)
                            .aspect_ratio(AutoValue::Value(Size::new(2.0, 1.0)))
                            .size(size(px(144), auto()))
                            .build(),
                    ]
                }),
                caption(cx, "Object fit / the same image in a 96 x 56 box"),
                row(cx, |cx| {
                    [
                        fit(cx, cat, ObjectFit::None, "None"),
                        fit(cx, cat, ObjectFit::Fill, "Fill"),
                        fit(cx, cat, ObjectFit::Contain, "Contain"),
                        fit(cx, cat, ObjectFit::Cover, "Cover"),
                    ]
                }),
                caption(
                    cx,
                    "Anchor + position / which point of the image sits where",
                ),
                row(cx, |cx| {
                    [
                        // The top of the covering image at the top of the box.
                        tile(
                            cx,
                            image(cx, cat)
                                .object_fit(ObjectFit::Cover)
                                .object_anchor(point(percent(50), px(0)))
                                .object_position(point(percent(50), px(0))),
                            "Cover / top",
                        ),
                        // The left of the contained image 8px into the box.
                        tile(
                            cx,
                            image(cx, cat)
                                .object_fit(ObjectFit::Contain)
                                .object_anchor(point(px(0), percent(50)))
                                .object_position(point(px(8), percent(50))),
                            "Contain / left + 8px",
                        ),
                    ]
                }),
                caption(
                    cx,
                    "Background / contained in the border box, under its text",
                ),
                cx.div()
                    .background(solid(Color::from_rgb8(255, 214, 170)))
                    .border_radius(all(px(16)))
                    .size(size(px(240), px(120)))
                    .children(|cx| {
                        cx.div()
                            .background(Paint::Content(cat))
                            .object_fit(ObjectFit::Contain)
                            .object_anchor(point(percent(100), percent(50)))
                            .object_position(point(percent(100), percent(50)))
                            .size(size(percent(100), percent(100)))
                            .padding(all(px(12)))
                            .color(AutoValue::Value(solid(Color::from_rgb8(127, 74, 32))))
                            .font_weight(parley::FontWeight::SEMI_BOLD)
                            .children(|cx| {
                                cx.div()
                                    .display_outer(DisplayOuter::Inline)
                                    .background(solid(Color::from_rgba8(255, 255, 255, 210)))
                                    .border_radius(all(px(6)))
                                    .padding(axis(px(2), px(6)))
                                    .children(|cx| cx.text("Text over an image background").build())
                            })
                    }),
            ]
        },
    )
}

/// A row of boxes aligned to its top, so they keep their own heights.
pub(super) fn row<V: IntoView>(cx: &UiCx, children: impl FnOnce(&UiCx) -> V) -> NodeKey {
    cx.div()
        .flex()
        .flex_wrap(FlexWrap::Wrap)
        .align_items(AlignItems::START)
        .gap(size(px(12), px(12)))
        .children(children)
}

/// `content` in a 96 x 56 box fitting it as `object_fit` says, labelled.
fn fit(cx: &UiCx, content: ContentId, object_fit: ObjectFit, label: &str) -> NodeKey {
    tile(cx, image(cx, content).object_fit(object_fit), label)
}

/// `image` in a white box, 96 x 56 unless it sets a size, over `label`.
pub(super) fn tile(cx: &UiCx, image: NodeBuild<'_>, label: &str) -> NodeKey {
    let image = image
        .size(size(px(96), px(56)))
        .background(solid(Color::WHITE))
        .border_radius(all(px(6)))
        .build();

    cx.div().children(|cx| [image, caption(cx, label)])
}

/// A box displaying `content`.
pub(super) fn image(cx: &UiCx, content: ContentId) -> NodeBuild<'_> {
    cx.div().display_inner(DisplayInner::Content(content))
}

pub(super) fn caption(cx: &UiCx, label: &str) -> NodeKey {
    cx.div()
        .font_size(px(12))
        .font_weight(parley::FontWeight::SEMI_BOLD)
        .color(AutoValue::Value(solid(MUTED)))
        .padding(sides(px(10), px(0), px(6), px(0)))
        .children(|cx| cx.text(label).build())
}

/// Decode a PNG into straight alpha RGBA pixels.
fn decode(png: &[u8]) -> ImageData {
    let image = image::load_from_memory(png)
        .expect("the sample image decodes")
        .into_rgba8();
    let (width, height) = image.dimensions();

    ImageData {
        data: Blob::from(image.into_raw()),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    }
}
