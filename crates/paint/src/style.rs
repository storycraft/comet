use anyrender::{Paint, filters::FilterEffect};
use comet_layout::style::geometry::{AutoValue, Point, Rect, StyleUnit};
use comet_tree::style::{StyleProp, define_style_props};
use kurbo::Affine;
use peniko::BlendMode;

define_style_props!(
    // Box draw styles
    #[derive(Debug, Clone, PartialEq)]
    pub Fill: Paint;
    #[derive(Debug, Clone, PartialEq)]
    pub BorderFill: Paint;
    #[derive(Debug, Clone, PartialEq)]
    pub OutlineFill: Paint;

    /// Width of the outline. Percentages resolve against the parent's
    /// outline width.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub OutlineWidth: StyleUnit;

    /// Distance between the outline and the border box. Percentages resolve
    /// against the parent's outline offset.
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

    /// Horizontal scroll position of a scroll container, in logical pixels
    /// from the left edge of its content. Only boxes whose
    /// [`Overflow`](comet_layout::style::bx::Overflow) scrolls or hides
    /// their overflow on an axis scroll; others ignore it. Clamped to the
    /// scrollable range when used.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub ScrollLeft: f64 = 0.0;
    /// Vertical scroll position of a scroll container, in logical pixels
    /// from the top edge of its content. See [`ScrollLeft`].
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub ScrollTop: f64 = 0.0;

    // Text selection
    /// How the text of the node and its descendants can be selected, or
    /// [`AutoValue::Auto`] to select as the parent does. Text is selectable
    /// at the root. Resolved into
    /// [`LayoutInputData::selectable`](crate::prepaint::LayoutInputData::selectable)
    /// and
    /// [`LayoutInputData::selection_container`](crate::prepaint::LayoutInputData::selection_container).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub Selectable: AutoValue<UserSelect> = AutoValue::Auto;

    // Text draw styles
    /// Color of the text of a node and its descendants. Auto follows the
    /// parent's text color, black at the root.
    #[derive(Debug, Clone, PartialEq)]
    pub TextColor: AutoValue<Paint> = AutoValue::Auto;

    /// Color of the selected text of a node and its descendants. Auto
    /// follows the parent's selection color, a deep navy at the root.
    #[derive(Debug, Clone, PartialEq)]
    pub SelectionColor: AutoValue<Paint> = AutoValue::Auto;

    /// Fill behind the selected text of a node and its descendants. Auto
    /// follows the parent's selection fill, a semi transparent blue at the
    /// root.
    #[derive(Debug, Clone, PartialEq)]
    pub SelectionFill: AutoValue<Paint> = AutoValue::Auto;

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

    /// Blend mode a box and its contents are composited with onto what is
    /// painted behind them.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub MixBlend: BlendMode;
);

/// How the text of a node and its descendants can be selected, set with
/// [`Selectable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UserSelect {
    /// The text cannot be selected at all: pressing it selects nothing.
    None,
    /// The text can be selected.
    Text,
    /// The text can be selected, like [`Self::Text`], but a selection
    /// starting in the node's subtree ends in it too, and, like an editable
    /// element on the web, a selection from outside selects it whole or not
    /// at all.
    Contain,
}

/// How the background, border and border radius of a box split into
/// several fragments, such as an inline element broken across lines, are
/// painted on each fragment. Not inherited. Resolved into
/// [`LayoutInputData::box_decoration_break`](crate::prepaint::LayoutInputData::box_decoration_break).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoxDecorationBreak {
    /// The fragments are painted as pieces of one box: the corners joining
    /// another fragment are square.
    #[default]
    Break,
    /// Each fragment is painted as a box of its own, closed on every side:
    /// all four corners keep the border radius.
    Clone,
}

impl StyleProp for BoxDecorationBreak {}
