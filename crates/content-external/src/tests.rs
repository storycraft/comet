use anyrender::{Paint, ResourceId, recording::RenderCommand};
use comet_content::Content;
use kurbo::Affine;

use crate::{
    TextureContent,
    fixture::{Fixture, draw_one, size},
};

#[test]
fn textures_draw_their_resource() {
    let fx = Fixture::new();
    let resource = ResourceId::new();
    let content = TextureContent::new(resource, 64, 32);
    assert_eq!(
        content.measure(&fx.input(Some(32.0), None)),
        size(32.0, 16.0)
    );

    let RenderCommand::Fill(fill) = draw_one(&content, 32.0, 16.0) else {
        panic!("textures draw a fill");
    };
    let Paint::Resource(brush) = fill.brush else {
        panic!("textures fill with their resource");
    };
    assert_eq!(brush.image, resource);
    assert_eq!(fill.brush_transform, Some(Affine::scale(0.5)));
}
