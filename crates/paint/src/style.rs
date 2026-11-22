use anyrender::{Paint, filters::FilterEffect};
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
    #[derive(Debug, Clone, PartialEq)]
    pub OutlineFill: Paint;

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub OutlineWidth: StyleUnit;

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub OutlineOffset: StyleUnit;

    /// Circular corner radii, clockwise from top-left, using Rect's
    /// top/right/bottom/left slots. Percentages resolve to 0 for now.
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

    /// Filter effects applied in order to a box and its contents, as a list
    /// of CSS filter functions.
    #[derive(Debug, Clone, PartialEq)]
    pub Filter: Vec<FilterEffect>;

    /// Filter effects applied in order to what is painted behind a box.
    #[derive(Debug, Clone, PartialEq)]
    pub BackdropFilter: Vec<FilterEffect>;
);
