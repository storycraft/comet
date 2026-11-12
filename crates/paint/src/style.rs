use anyrender::Paint;
use comet_layout::style::geometry::{Point, Rect, StyleUnit};
use comet_tree::style::define_style_props;
use kurbo::Affine;
use peniko::Color;

define_style_props!(
    // Box draw styles
    pub Fill: Paint;
    pub BorderFill: Paint;
    /// Circular corner radii, clockwise from top-left, using Rect's
    /// top/right/bottom/left slots. Percentages use the shorter box dimension.
    pub BorderRadius: Rect<StyleUnit> = Rect::default();

    /// Transform of a box and its contents, applied around
    /// [`TransformOrigin`].
    pub Transform: Affine = Affine::default();
    /// Point of the border box the [`Transform`] is applied around.
    /// Percentages use the border box width and height. Defaults to the centre.
    pub TransformOrigin: Point<StyleUnit> = Point::new(StyleUnit::Percent(0.5), StyleUnit::Percent(0.5));

    // Text draw styles
    pub TextColor: Paint = Paint::Solid(Color::BLACK);
);
