use anyrender::{Scene, recording::RenderCommand};
use comet_content::{Content, DrawInput, MeasureInput, style::ObjectFit};
use comet_tree::{
    node::{Node, NodeKey},
    style::StyleProps,
    ui::{Props, Ui},
};
use kurbo::Affine;

use crate::SvgContent;

/// A box node with props, on a display of a scale factor.
struct Fixture {
    ui: Ui,
    node: NodeKey,
    scale_factor: f64,
}

impl Fixture {
    fn new(props: impl StyleProps, scale_factor: f64) -> Self {
        let mut ui = Ui::new();
        let node = ui.create(Node::Box, props);

        Self {
            ui,
            node,
            scale_factor,
        }
    }

    fn props(&self) -> Props<'_> {
        self.ui.props(self.node).unwrap()
    }

    fn measure(&self, content: &dyn Content) -> taffy::Size<f32> {
        content.measure(&MeasureInput {
            props: self.props(),
            known_dimensions: taffy::Size::NONE,
            available_space: taffy::Size {
                width: taffy::AvailableSpace::Definite(500.0),
                height: taffy::AvailableSpace::MaxContent,
            },
            parent_size: taffy::Size::NONE,
            scale_factor: self.scale_factor,
        })
    }

    /// Transforms of the fills `content` records in a `width` x `height`
    /// box of this node.
    fn fill_transforms(&self, content: &dyn Content, width: f64, height: f64) -> Vec<Affine> {
        let mut scene = Scene::new();
        let input = DrawInput {
            props: self.props(),
            size: kurbo::Size::new(width, height),
            scale_factor: self.scale_factor,
            units: comet_geometry::UnitResolveContext::default(),
        };
        content.draw(&input, &mut scene);

        scene
            .commands
            .iter()
            .filter_map(|command| match command {
                RenderCommand::Fill(fill) => Some(fill.transform),
                _ => None,
            })
            .collect()
    }
}

/// A 40 x 20 SVG of one rectangle.
fn rect() -> SvgContent {
    SvgContent::from_str(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20">
            <rect width="40" height="20" fill="red"/>
        </svg>"#,
        &usvg::Options::default(),
    )
    .unwrap()
}

fn size(width: f32, height: f32) -> taffy::Size<f32> {
    taffy::Size { width, height }
}

#[test]
fn svgs_measure_their_size_in_logical_units_on_any_display() {
    let content = rect();

    assert_eq!(Fixture::new((), 1.0).measure(&content), size(40.0, 20.0));
    assert_eq!(Fixture::new((), 2.0).measure(&content), size(40.0, 20.0));
}

#[test]
fn svgs_are_drawn_where_they_are_placed() {
    let content = rect();

    // Centred at their size by default, on any display.
    for scale_factor in [1.0, 2.0] {
        assert_eq!(
            Fixture::new((), scale_factor).fill_transforms(&content, 100.0, 100.0),
            [Affine::translate((30.0, 40.0))]
        );
    }

    // Scaled onto the box when filling it.
    assert_eq!(
        Fixture::new((ObjectFit::Fill,), 2.0).fill_transforms(&content, 80.0, 80.0),
        [Affine::scale_non_uniform(2.0, 4.0)]
    );
}

#[test]
fn invalid_data_is_an_error() {
    assert!(SvgContent::from_data(b"not an svg", &usvg::Options::default()).is_err());
}
