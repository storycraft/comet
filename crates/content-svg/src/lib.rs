//! SVG contents for comet: vector images drawn by replaced boxes, see
//! [`comet_content`]. SVG data is parsed into a [`usvg::Tree`] and drawn
//! with [`anyrender_svg`].

pub use usvg;

use anyrender::Scene;
use kurbo::Affine;

use comet_content::{Content, DrawInput, MeasureInput, fit_transform, style::ObjectStyle};
use comet_layout::style::bx::AspectRatio;
use comet_tree::ui::Props;

/// A parsed SVG, sized and placed in its box by the
/// [content styles](comet_content::style) of the box's node. Its natural
/// size is the size of the SVG in logical units: SVG user units are CSS
/// pixels, so it is drawn as large on every display, scaled rather than
/// rasterized. `<text>` is not drawn yet.
#[derive(Debug, Clone)]
pub struct SvgContent {
    pub tree: usvg::Tree,
}

impl SvgContent {
    pub fn new(tree: usvg::Tree) -> Self {
        Self { tree }
    }

    /// Size of the SVG in user units, logical units in the box.
    fn size(&self) -> kurbo::Size {
        let size = self.tree.size();

        kurbo::Size::new(f64::from(size.width()), f64::from(size.height()))
    }

    /// Size of the SVG in physical pixels on a display of `scale_factor`,
    /// as the [content styles](ObjectStyle) measure it.
    fn physical_size(&self, scale_factor: f64) -> kurbo::Size {
        let size = self.size();

        // Kept logical where the styles keep physical sizes as they are.
        if scale_factor > 0.0 && scale_factor.is_finite() {
            size * scale_factor
        } else {
            size
        }
    }
}

impl Content for SvgContent {
    fn measure(&self, input: &MeasureInput<'_>) -> taffy::Size<f32> {
        object_style(input.props).measure(self.physical_size(input.scale_factor), input)
    }

    fn draw(&self, input: &DrawInput<'_>, scene: &mut Scene) {
        let rect = object_style(input.props).place(self.physical_size(input.scale_factor), input);
        let transform =
            Affine::translate(rect.origin().to_vec2()) * fit_transform(self.size(), rect.size());

        anyrender_svg::render_svg_tree(scene, &self.tree, transform);
    }
}

/// The content styles of a box of `props`, with its aspect ratio.
fn object_style(props: Props<'_>) -> ObjectStyle {
    ObjectStyle::of(props, props.get_or_default::<AspectRatio>().0)
}
