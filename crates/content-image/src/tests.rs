use anyrender::{Paint, recording::RenderCommand};
use comet_content::{Content, manager::ContentManager, style::ObjectFit};
use kurbo::{Affine, Shape};
use peniko::{Blob, Extend, ImageAlphaType, ImageData, ImageFormat, ImageQuality};

use crate::{
    ImageContent,
    fixture::{Fixture, size},
    style::{ImageExtend, ImageRendering},
};

fn image(width: u32, height: u32) -> ImageData {
    ImageData {
        data: Blob::from(vec![0; (width * height * 4) as usize]),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    }
}

#[test]
fn filled_images_stretch_over_the_content_box() {
    let fx = Fixture::with((ObjectFit::Fill,));
    let content = ImageContent::new(image(20, 10));

    let RenderCommand::Fill(fill) = fx.draw_one(&content, 40.0, 40.0) else {
        panic!("images draw a fill");
    };
    assert_eq!(
        fill.brush_transform,
        Some(Affine::scale_non_uniform(2.0, 4.0))
    );
    assert_eq!(
        fill.shape.bounding_box(),
        kurbo::Rect::new(0.0, 0.0, 40.0, 40.0)
    );
}

#[test]
fn images_are_shared_by_id() {
    let fx = Fixture::new();
    let mut contents = ContentManager::new();
    let id = contents.insert(ImageContent::new(image(4, 2)));

    let measured = contents
        .get(id)
        .unwrap()
        .measure(&fx.input(Some(8.0), None));
    assert_eq!(measured, size(8.0, 4.0));
}

#[test]
fn images_draw_once_at_their_pixel_size_by_default() {
    let fx = Fixture::new();
    let content = ImageContent::new(image(20, 10));
    assert_eq!(content.measure(&fx.input(None, None)), size(20.0, 10.0));

    let RenderCommand::Fill(fill) = fx.draw_one(&content, 40.0, 40.0) else {
        panic!("images draw a fill");
    };
    let Paint::Image(brush) = &fill.brush else {
        panic!("images fill with their image");
    };
    assert_eq!(brush.sampler.quality, ImageQuality::Medium);
    assert_eq!(
        (brush.sampler.x_extend, brush.sampler.y_extend),
        (Extend::Pad, Extend::Pad)
    );
    assert_eq!(brush.sampler.alpha, 1.0);
    // Only where the image is placed, centred at its own size.
    assert_eq!(
        fill.shape.bounding_box(),
        kurbo::Rect::new(10.0, 15.0, 30.0, 25.0)
    );
    assert_eq!(fill.brush_transform, Some(Affine::translate((10.0, 15.0))));
}

#[test]
fn image_styles_set_the_sampler_and_tile_the_box() {
    let fx = Fixture::with((
        ImageRendering(ImageQuality::Low),
        ImageExtend {
            x: Extend::Repeat,
            y: Extend::Pad,
        },
    ));
    let content = ImageContent::new(image(20, 10));

    let RenderCommand::Fill(fill) = fx.draw_one(&content, 40.0, 40.0) else {
        panic!("images draw a fill");
    };
    let Paint::Image(brush) = &fill.brush else {
        panic!("images fill with their image");
    };
    assert_eq!(brush.sampler.quality, ImageQuality::Low);
    assert_eq!(
        (brush.sampler.x_extend, brush.sampler.y_extend),
        (Extend::Repeat, Extend::Pad)
    );
    // Tiled across the box horizontally, once vertically.
    assert_eq!(
        fill.shape.bounding_box(),
        kurbo::Rect::new(0.0, 15.0, 40.0, 25.0)
    );
    // Tiles start where the image is placed.
    assert_eq!(fill.brush_transform, Some(Affine::translate((10.0, 15.0))));
}

#[cfg(feature = "reactivity")]
#[test]
fn image_style_changes_only_paint_again() {
    use comet_reactivity::prop::{PropHint, ReactiveProp};

    assert_eq!(
        ImageRendering(ImageQuality::Low).hint(None),
        PropHint::RENDER
    );
    assert_eq!(
        ImageExtend::both(Extend::Repeat).hint(Some(&ImageExtend::default())),
        PropHint::RENDER
    );
}
