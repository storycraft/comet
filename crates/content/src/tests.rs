use anyrender::Scene;
use comet_tree::{
    node::{Node, NodeKey},
    ui::{Props, Ui},
};

use crate::{
    Content, DrawInput, MeasureInput, fit_transform, manager::ContentManager, natural_measure,
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

/// A content of a fixed natural size, drawing nothing.
struct Fixed(kurbo::Size);

impl Content for Fixed {
    fn measure(&self, input: &MeasureInput<'_>) -> taffy::Size<f32> {
        natural_measure(self.0, input)
    }

    fn draw(&self, _: &DrawInput<'_>, _: &mut Scene) {}
}

fn fixed(width: f64, height: f64) -> Fixed {
    Fixed(kurbo::Size::new(width, height))
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
    let a = contents.insert(fixed(1.0, 1.0));
    let b = contents.insert(fixed(2.0, 2.0));
    assert_ne!(a, b);

    assert!(contents.remove(a).is_some());
    let c = contents.insert(fixed(3.0, 3.0));
    assert!(![a, b].contains(&c));
    assert!(contents.get(a).is_none());
    assert_eq!(contents.len(), 2);
}

#[test]
fn replacing_keeps_the_id() {
    let fx = Fixture::new();
    let mut contents = ContentManager::new();
    let id = contents.insert(fixed(1.0, 1.0));

    assert!(contents.replace(id, fixed(4.0, 2.0)).is_some());
    let measured = contents.get(id).unwrap().measure(&fx.input(None, None));
    assert_eq!(measured, size(4.0, 2.0));

    contents.remove(id);
    assert!(contents.replace(id, fixed(1.0, 1.0)).is_none());
    assert!(contents.is_empty());
}

#[test]
fn fit_transforms_scale_the_source_onto_the_box() {
    let natural = kurbo::Size::new(20.0, 10.0);

    assert_eq!(
        fit_transform(natural, kurbo::Size::new(40.0, 40.0)),
        kurbo::Affine::scale_non_uniform(2.0, 4.0)
    );
    assert_eq!(
        fit_transform(kurbo::Size::ZERO, kurbo::Size::new(40.0, 40.0)),
        kurbo::Affine::IDENTITY
    );
}
