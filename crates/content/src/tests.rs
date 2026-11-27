use anyrender::{Paint, ResourceId, Scene, recording::RenderCommand};
use comet_tree::{
    node::{Node, NodeKey},
    ui::{Props, Ui},
};
use kurbo::{Affine, Shape};
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

use crate::{
    Content, ContentKind, DrawInput, MeasureInput, image::ImageContent, manager::ContentManager,
    natural_measure, texture::TextureContent,
};

/// A box node without props, for the props of the inputs.
struct Fixture {
    ui: Ui,
    node: NodeKey,
}

impl Fixture {
    fn new() -> Self {
        let mut ui = Ui::new();
        let node = ui.create(Node::Box, ());

        Self { ui, node }
    }

    fn props(&self) -> Props<'_> {
        self.ui.props(self.node).unwrap()
    }

    fn input(&self, width: Option<f32>, height: Option<f32>) -> MeasureInput<'_> {
        MeasureInput {
            props: self.props(),
            known_dimensions: taffy::Size { width, height },
            available_space: taffy::Size {
                width: taffy::AvailableSpace::Definite(500.0),
                height: taffy::AvailableSpace::MaxContent,
            },
            parent_size: taffy::Size::NONE,
        }
    }
}

fn image(width: u32, height: u32) -> ImageData {
    ImageData {
        data: Blob::from(vec![0; (width * height * 4) as usize]),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    }
}

fn size(width: f32, height: f32) -> taffy::Size<f32> {
    taffy::Size { width, height }
}

#[test]
fn natural_sizes_follow_their_aspect_ratio() {
    let fx = Fixture::new();
    let natural = kurbo::Size::new(200.0, 100.0);

    assert_eq!(
        natural_measure(natural, &fx.input(None, None)),
        size(200.0, 100.0)
    );
    assert_eq!(
        natural_measure(natural, &fx.input(Some(50.0), None)),
        size(50.0, 25.0)
    );
    assert_eq!(
        natural_measure(natural, &fx.input(None, Some(50.0))),
        size(100.0, 50.0)
    );
    assert_eq!(
        natural_measure(natural, &fx.input(Some(10.0), Some(10.0))),
        size(10.0, 10.0)
    );

    // An empty natural axis gives no ratio to follow.
    let empty = kurbo::Size::new(0.0, 100.0);
    assert_eq!(
        natural_measure(empty, &fx.input(Some(50.0), None)),
        size(50.0, 100.0)
    );
}

#[test]
fn ids_are_never_reused() {
    let mut contents = ContentManager::new();
    let a = contents.insert(ImageContent::new(image(1, 1)));
    let b = contents.insert(ImageContent::new(image(2, 2)));
    assert_ne!(a, b);

    assert!(contents.remove(a).is_some());
    let c = contents.insert(ImageContent::new(image(3, 3)));
    assert!(![a, b].contains(&c));
    assert!(contents.get(a).is_none());
    assert_eq!(contents.len(), 2);
}

#[test]
fn replacing_keeps_the_id() {
    let fx = Fixture::new();
    let mut contents = ContentManager::new();
    let id = contents.insert(ImageContent::new(image(1, 1)));

    assert!(
        contents
            .replace(id, ImageContent::new(image(4, 2)))
            .is_some()
    );
    let measured = contents.get(id).unwrap().measure(&fx.input(None, None));
    assert_eq!(measured, size(4.0, 2.0));

    contents.remove(id);
    assert!(
        contents
            .replace(id, ImageContent::new(image(1, 1)))
            .is_none()
    );
    assert!(contents.is_empty());
}

/// The only command `content` records in a `width` x `height` box.
fn draw_one(content: &dyn Content, width: f64, height: f64) -> RenderCommand {
    let mut scene = Scene::new();
    let fx = Fixture::new();
    content.draw(
        &DrawInput {
            props: fx.props(),
            size: kurbo::Size::new(width, height),
            scale_factor: 2.0,
        },
        &mut scene,
    );

    assert_eq!(scene.commands.len(), 1);
    scene.commands.pop().unwrap()
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
fn textures_draw_their_resource() {
    let fx = Fixture::new();
    let resource = ResourceId::new();
    let content = TextureContent::new(resource, 64, 32);
    assert_eq!(content.kind(), ContentKind::Texture { resource });
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
