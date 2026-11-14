use anyrender::Paint;
use comet_layout::style::geometry::{AutoValue, Point, Rect, StyleUnit};
use comet_tree::style::define_style_props;
use kurbo::Affine;
use peniko::Color;

define_style_props!(
    // Box draw styles
    #[derive(Debug, Clone, PartialEq)]
    pub Fill: Paint;
    #[derive(Debug, Clone, PartialEq)]
    pub BorderFill: Paint;
    /// Circular corner radii, clockwise from top-left, using Rect's
    /// top/right/bottom/left slots. Percentages use the shorter box dimension.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub BorderRadius: Rect<StyleUnit> = Rect::default();

    /// Transform of a box and its contents, applied around
    /// [`TransformOrigin`].
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub Transform: Affine = Affine::IDENTITY;
    /// Point of the border box the [`Transform`] is applied around.
    /// Percentages use the border box width and height. Defaults to the centre.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub TransformOrigin: Point<StyleUnit> = Point::new(StyleUnit::Percent(0.5), StyleUnit::Percent(0.5));

    // Text draw styles
    #[derive(Debug, Clone, PartialEq)]
    pub TextColor: Paint = Paint::Solid(Color::BLACK);

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub ZIndex: AutoValue<i32> = AutoValue::Auto;

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub Opacity: f64 = 1.0;
);
