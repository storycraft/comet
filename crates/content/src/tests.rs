use anyrender::Scene;
use comet_tree::{
    node::{Node, NodeKey},
    ui::{Props, Ui},
};

use comet_geometry::{AutoValue, Point, Size, StyleUnit, UnitResolveContext};

use crate::{
    Content, DrawInput, MeasureInput, fit_transform,
    manager::ContentManager,
    style::{ObjectFit, ObjectStyle},
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
        self.input_in(width, height, 500.0)
    }

    /// Measure input with `available` width.
    fn input_in(
        &self,
        width: Option<f32>,
        height: Option<f32>,
        available: f32,
    ) -> MeasureInput<'_> {
        MeasureInput {
            props: self.props(),
            known_dimensions: taffy::Size { width, height },
            available_space: taffy::Size {
                width: taffy::AvailableSpace::Definite(available),
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
        ObjectStyle::default().measure(self.0, input)
    }

    fn draw(&self, _: &DrawInput<'_>, _: &mut Scene) {}
}

fn fixed(width: f64, height: f64) -> Fixed {
    Fixed(kurbo::Size::new(width, height))
}

fn size(width: f32, height: f32) -> taffy::Size<f32> {
    taffy::Size { width, height }
}

fn kurbo_size(width: f64, height: f64) -> kurbo::Size {
    kurbo::Size::new(width, height)
}

fn style(fit: ObjectFit) -> ObjectStyle {
    ObjectStyle {
        fit,
        ..ObjectStyle::default()
    }
}

#[test]
fn measured_sizes_follow_the_aspect_ratio() {
    let fx = Fixture::new();
    let natural = kurbo_size(200.0, 100.0);
    let measure =
        |style: ObjectStyle, width, height| style.measure(natural, &fx.input(width, height));
    let auto = ObjectStyle::default();

    assert_eq!(measure(auto, None, None), size(200.0, 100.0));
    assert_eq!(measure(auto, Some(50.0), None), size(50.0, 25.0));
    assert_eq!(measure(auto, None, Some(50.0)), size(100.0, 50.0));
    assert_eq!(measure(auto, Some(10.0), Some(10.0)), size(10.0, 10.0));

    // A custom ratio replaces the content's own.
    let square = ObjectStyle {
        aspect_ratio: AutoValue::Value(Size::new(1.0, 1.0)),
        ..auto
    };
    assert_eq!(measure(square, None, None), size(200.0, 200.0));
    assert_eq!(measure(square, Some(50.0), None), size(50.0, 50.0));

    // An empty natural axis gives no ratio to follow.
    assert_eq!(
        auto.measure(kurbo_size(0.0, 100.0), &fx.input(Some(50.0), None)),
        size(50.0, 100.0)
    );
}

#[test]
fn contained_contents_shrink_into_the_available_space() {
    let fx = Fixture::new();
    let natural = kurbo_size(200.0, 100.0);

    assert_eq!(
        style(ObjectFit::Contain).measure(natural, &fx.input_in(None, None, 50.0)),
        size(50.0, 25.0)
    );
    // Not grown into more space, nor shrunk by other fits.
    assert_eq!(
        style(ObjectFit::Contain).measure(natural, &fx.input_in(None, None, 800.0)),
        size(200.0, 100.0)
    );
    assert_eq!(
        style(ObjectFit::Cover).measure(natural, &fx.input_in(None, None, 50.0)),
        size(200.0, 100.0)
    );
}

#[test]
fn fits_size_the_drawn_content() {
    let natural = kurbo_size(200.0, 100.0);
    let box_size = kurbo_size(100.0, 100.0);

    assert_eq!(
        style(ObjectFit::None).fitted_size(natural, box_size),
        natural
    );
    assert_eq!(
        style(ObjectFit::Fill).fitted_size(natural, box_size),
        box_size
    );
    assert_eq!(
        style(ObjectFit::Contain).fitted_size(natural, box_size),
        kurbo_size(100.0, 50.0)
    );
    assert_eq!(
        style(ObjectFit::Cover).fitted_size(natural, box_size),
        kurbo_size(200.0, 100.0)
    );
}

#[test]
fn anchors_are_placed_at_the_position() {
    let units = UnitResolveContext {
        root_font_size: 16.0,
        font_size: 10.0,
        viewport: Size::new(800.0, 600.0),
    };
    let size = kurbo_size(40.0, 20.0);
    let box_size = kurbo_size(100.0, 100.0);

    // Centred by default.
    assert_eq!(
        ObjectStyle::default().position(size, box_size, &units),
        kurbo::Rect::new(30.0, 40.0, 70.0, 60.0)
    );

    // The bottom right corner at 1em from the top left.
    let corner = ObjectStyle {
        anchor: Point::new(StyleUnit::Percent(1.0), StyleUnit::Percent(1.0)),
        position: Point::new(StyleUnit::Em(1.0), StyleUnit::Px(10.0)),
        ..ObjectStyle::default()
    };
    assert_eq!(
        corner.position(size, box_size, &units),
        kurbo::Rect::new(-30.0, -10.0, 10.0, 10.0)
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
