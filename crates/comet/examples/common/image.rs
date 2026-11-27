use comet::prelude::*;
use comet_content::ContentId;
use comet_content_image::ImageContent;
use comet_geometry::AutoValue;
use comet_layout::style::bx::{DisplayInner, DisplayOuter};
use comet_paint::style::Paint;
use comet_reactivity::cx::NodeBuild;
use comet_tree::node::NodeKey;
use peniko::{Blob, Color, ImageAlphaType, ImageData, ImageFormat};
use taffy::AlignItems;

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
                cx.div()
                    .flex()
                    .align_items(AlignItems::START)
                    .gap(size(px(12), px(12)))
                    .children(|cx| {
                        [
                            // A width alone keeps the image square.
                            image(cx, cat).size(size(px(120), auto())).build(),
                            // Padding and border wrap around the image.
                            image(cx, cat)
                                .size(size(px(72), auto()))
                                .padding(all(px(6)))
                                .border(all(px(2)))
                                .border_fill(solid(Color::from_rgb8(52, 84, 209)))
                                .background(solid(Color::WHITE))
                                .build(),
                            // Both sizes stretch it.
                            image(cx, cat).size(size(px(120), px(56))).build(),
                        ]
                    }),
                caption(
                    cx,
                    "Background / stretched over the border box, under its text",
                ),
                cx.div()
                    .background(solid(Color::from_rgb8(255, 214, 170)))
                    .border_radius(all(px(16)))
                    .size(size(px(240), px(120)))
                    .children(|cx| {
                        cx.div()
                            .background(Paint::Content(cat))
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

/// A box displaying `content`.
fn image(cx: &UiCx, content: ContentId) -> NodeBuild<'_> {
    cx.div().display_inner(DisplayInner::Content(content))
}

fn caption(cx: &UiCx, label: &str) -> NodeKey {
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
