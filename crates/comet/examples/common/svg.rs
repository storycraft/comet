use comet::prelude::*;
use comet_content::style::ObjectFit;
use comet_content_svg::{SvgContent, usvg};
use comet_paint::style::Paint;
use comet_tree::node::NodeKey;
use peniko::Color;

use super::{
    image::{caption, image, row, tile},
    panel, solid,
};

/// The sample SVG, Ferris the crab, 1200 x 800 user units.
const FERRIS: &[u8] = include_bytes!("images/ferris.svg");

pub(super) fn showcase(cx: &UiCx) -> NodeKey {
    let tree =
        usvg::Tree::from_data(FERRIS, &usvg::Options::default()).expect("the sample SVG parses");
    let ferris = cx
        .tree()
        .borrow_mut()
        .contents
        .insert(SvgContent::new(tree));

    panel(
        cx,
        "12 / SVG content",
        Color::from_rgb8(238, 243, 247),
        |cx| {
            [
                caption(cx, "Scaled / drawn as vectors, sharp at any size"),
                row(cx, |cx| {
                    [48, 120, 240].map(|width| {
                        image(cx, ferris)
                            .object_fit(ObjectFit::Fill)
                            .size(size(px(width), auto()))
                            .build()
                    })
                }),
                caption(cx, "Object fit / the same SVG in a 96 x 56 box"),
                row(cx, |cx| {
                    [
                        (ObjectFit::None, "None"),
                        (ObjectFit::Fill, "Fill"),
                        (ObjectFit::Contain, "Contain"),
                        (ObjectFit::Cover, "Cover"),
                    ]
                    .map(|(fit, label)| tile(cx, image(cx, ferris).object_fit(fit), label))
                }),
                caption(cx, "Background / covering the border box"),
                cx.div()
                    .background(solid(Color::from_rgb8(255, 236, 214)))
                    .border_radius(all(px(16)))
                    .size(size(px(240), px(96)))
                    .children(|cx| {
                        cx.div()
                            .background(Paint::Content(ferris))
                            .object_fit(ObjectFit::Cover)
                            .size(size(percent(100), percent(100)))
                            .build()
                    }),
            ]
        },
    )
}
