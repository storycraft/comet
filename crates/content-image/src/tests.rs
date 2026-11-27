use anyrender::{Paint, recording::RenderCommand};
use comet_content::{Content, ContentKind, manager::ContentManager};
use kurbo::{Affine, Shape};
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

use crate::{
    ImageContent,
    fixture::{Fixture, draw_one, size},
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
fn images_stretch_over_the_content_box() {
    let fx = Fixture::new();
    let content = ImageContent::new(image(20, 10)).with_natural_size(kurbo::Size::new(10.0, 5.0));
    assert_eq!(content.kind(), ContentKind::Scene);
    assert_eq!(content.measure(&fx.input(None, None)), size(10.0, 5.0));

    let RenderCommand::Fill(fill) = draw_one(&content, 40.0, 40.0) else {
        panic!("images draw a fill");
    };
    assert!(matches!(fill.brush, Paint::Image(_)));
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
