//! Inputs for testing contents.

use comet_content::{Content, DrawInput, MeasureInput};
use comet_tree::{
    node::{Node, NodeKey},
    ui::{Props, Ui},
};

/// A box node without props, for the props of the inputs.
pub struct Fixture {
    ui: Ui,
    node: NodeKey,
}

impl Fixture {
    pub fn new() -> Self {
        let mut ui = Ui::new();
        let node = ui.create(Node::Box, ());

        Self { ui, node }
    }

    /// A box node fitting its content as `fit` says.
    pub fn fitted(fit: comet_content::style::ObjectFit) -> Self {
        let mut ui = Ui::new();
        let node = ui.create(Node::Box, (fit,));

        Self { ui, node }
    }

    fn props(&self) -> Props<'_> {
        self.ui.props(self.node).unwrap()
    }

    pub fn input(&self, width: Option<f32>, height: Option<f32>) -> MeasureInput<'_> {
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

    /// The only command `content` records in a `width` x `height` box of
    /// this node.
    pub fn draw_one(
        &self,
        content: &dyn Content,
        width: f64,
        height: f64,
    ) -> anyrender::recording::RenderCommand {
        let mut scene = anyrender::Scene::new();
        content.draw(&self.draw(width, height), &mut scene);

        assert_eq!(scene.commands.len(), 1);
        scene.commands.pop().unwrap()
    }

    pub fn draw(&self, width: f64, height: f64) -> DrawInput<'_> {
        DrawInput {
            props: self.props(),
            size: kurbo::Size::new(width, height),
            scale_factor: 2.0,
            units: comet_geometry::UnitResolveContext::default(),
        }
    }
}

pub fn size(width: f32, height: f32) -> taffy::Size<f32> {
    taffy::Size { width, height }
}
