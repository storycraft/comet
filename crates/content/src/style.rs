//! Style props of content boxes: how a content is sized in its box and
//! where it is drawn, read by contents from the props of the box's node.
//!
//! [`ObjectStyle`] gathers them and holds the helpers contents measure and
//! draw with.

use comet_geometry::{AutoValue, Point, Size, StyleUnit, UnitResolveContext};
use comet_tree::{
    style::{StyleProp, define_style_props},
    ui::Props,
};

use crate::{DrawInput, MeasureInput};

/// How a content is sized in a box whose size is known. Default to
/// [`None`](Self::None).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectFit {
    /// Drawn at its own size, following its
    /// [aspect ratio](ObjectStyle::aspect_ratio), whatever the box size.
    #[default]
    None,
    /// Scaled to cover the whole box, keeping its aspect ratio. Parts past
    /// the box are clipped.
    Cover,
    /// Scaled to fit inside the box, keeping its aspect ratio, so all of it
    /// shows.
    Contain,
    /// Stretched to fill the box on both axes.
    Fill,
}

impl StyleProp for ObjectFit {}

define_style_props!(
    /// Point of the drawn content placed at the [`ObjectPosition`] of the
    /// box. Percentages resolve against the drawn content size. Defaults to
    /// the centre.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub ObjectAnchor: Point<StyleUnit> = Point::new(StyleUnit::Percent(0.5), StyleUnit::Percent(0.5));

    /// Point of the content box the [`ObjectAnchor`] of the drawn content is
    /// placed at. Percentages resolve against the content box size. Defaults
    /// to the centre.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub ObjectPosition: Point<StyleUnit> = Point::new(StyleUnit::Percent(0.5), StyleUnit::Percent(0.5));
);

/// The content style props of a node, measuring and placing a content of a
/// physical size in its box, in logical units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectStyle {
    pub fit: ObjectFit,
    pub anchor: Point<StyleUnit>,
    pub position: Point<StyleUnit>,
    /// Aspect ratio the content is measured and drawn with, as a width and a
    /// height, or [`AutoValue::Auto`] to follow the content's own: the
    /// aspect ratio of the box, a layout style prop.
    pub aspect_ratio: AutoValue<Size<f64>>,
}

impl Default for ObjectStyle {
    fn default() -> Self {
        Self {
            fit: ObjectFit::default(),
            anchor: ObjectAnchor::default().0,
            position: ObjectPosition::default().0,
            aspect_ratio: AutoValue::Auto,
        }
    }
}

impl ObjectStyle {
    /// The content style props set on a node, with the defaults of the
    /// others, and the node's `aspect_ratio`, which the layout style props
    /// hold.
    pub fn of(props: Props<'_>, aspect_ratio: AutoValue<Size<f64>>) -> Self {
        Self {
            fit: props.get_or_default(),
            anchor: props.get_or_default::<ObjectAnchor>().0,
            position: props.get_or_default::<ObjectPosition>().0,
            aspect_ratio,
        }
    }

    /// The size before fitting, in logical units, of a content `physical` in
    /// size on a display of `scale_factor`: its
    /// [logical size](logical_size), with the height following the
    /// [aspect ratio](Self::aspect_ratio) when it sets one.
    pub fn intrinsic_size(&self, physical: kurbo::Size, scale_factor: f64) -> kurbo::Size {
        let natural = logical_size(physical, scale_factor);

        match self.ratio(natural) {
            Some(ratio) if self.aspect_ratio != AutoValue::Auto => {
                kurbo::Size::new(natural.width, natural.width / ratio)
            }
            _ => natural,
        }
    }

    /// The content box size, in logical units, of a box displaying a content
    /// `physical` in size, under `input` and on its display, as a Taffy leaf
    /// measure function returns it:
    /// - the known sizes, with a missing axis following the aspect ratio,
    /// - else the [intrinsic size](Self::intrinsic_size), scaled down to fit
    ///   the definite available space when the fit is
    ///   [`Contain`](ObjectFit::Contain).
    pub fn measure(&self, physical: kurbo::Size, input: &MeasureInput<'_>) -> taffy::Size<f32> {
        let known = input.known_dimensions;
        let intrinsic = self.intrinsic_size(physical, input.scale_factor);
        let ratio = self.ratio(logical_size(physical, input.scale_factor));
        let from_ratio = |length: f32, ratio: Option<f64>, fallback: f64| {
            ratio.map_or(fallback as f32, |ratio| (f64::from(length) * ratio) as f32)
        };

        let size = match (known.width, known.height) {
            (Some(width), Some(height)) => taffy::Size { width, height },
            (Some(width), None) => taffy::Size {
                width,
                height: from_ratio(width, ratio.map(f64::recip), intrinsic.height),
            },
            (None, Some(height)) => taffy::Size {
                width: from_ratio(height, ratio, intrinsic.width),
                height,
            },
            (None, None) => {
                let size = match self.fit {
                    ObjectFit::Contain => shrink_into(intrinsic, input.available_space),
                    _ => intrinsic,
                };

                taffy::Size {
                    width: size.width as f32,
                    height: size.height as f32,
                }
            }
        };

        taffy::Size {
            width: size.width.max(0.0),
            height: size.height.max(0.0),
        }
    }

    /// The size, in logical units, a content `physical` in size on a display
    /// of `scale_factor` is drawn at in a content box `box_size` in size, by
    /// the [fit](ObjectFit).
    pub fn fitted_size(
        &self,
        physical: kurbo::Size,
        scale_factor: f64,
        box_size: kurbo::Size,
    ) -> kurbo::Size {
        let intrinsic = self.intrinsic_size(physical, scale_factor);
        let scale = |pick: fn(f64, f64) -> f64| {
            if intrinsic.width <= 0.0 || intrinsic.height <= 0.0 {
                return box_size;
            }

            intrinsic
                * pick(
                    box_size.width / intrinsic.width,
                    box_size.height / intrinsic.height,
                )
        };

        match self.fit {
            ObjectFit::None => intrinsic,
            ObjectFit::Fill => box_size,
            ObjectFit::Contain => scale(f64::min),
            ObjectFit::Cover => scale(f64::max),
        }
    }

    /// The rectangle a content of `size`, as [fitted](Self::fitted_size), is
    /// drawn in, in a content box `box_size` in size: its
    /// [anchor](ObjectAnchor), resolved against `size`, placed at the
    /// [position](ObjectPosition) of the box, resolved against `box_size`,
    /// both with `units`. In the coordinates of the content box, so it may
    /// reach past it.
    pub fn position(
        &self,
        size: kurbo::Size,
        box_size: kurbo::Size,
        units: &UnitResolveContext,
    ) -> kurbo::Rect {
        let anchor = kurbo::Vec2::new(
            self.anchor.x.resolve(units, size.width),
            self.anchor.y.resolve(units, size.height),
        );
        let position = kurbo::Point::new(
            self.position.x.resolve(units, box_size.width),
            self.position.y.resolve(units, box_size.height),
        );

        kurbo::Rect::from_origin_size(position - anchor, size)
    }

    /// The rectangle, in logical units, a content `physical` in size is
    /// drawn in, in the content box of `input` and on its display:
    /// [fitted](Self::fitted_size) to it and [positioned](Self::position)
    /// in it.
    pub fn place(&self, physical: kurbo::Size, input: &DrawInput<'_>) -> kurbo::Rect {
        let size = self.fitted_size(physical, input.scale_factor, input.size);

        self.position(size, input.size, &input.units)
    }

    /// Width over height of a content `natural` in size: the
    /// [aspect ratio](Self::aspect_ratio), or its natural one when auto. `None` when either is
    /// empty.
    fn ratio(&self, natural: kurbo::Size) -> Option<f64> {
        let (width, height) = match self.aspect_ratio {
            AutoValue::Value(ratio) => (ratio.width, ratio.height),
            AutoValue::Auto => (natural.width, natural.height),
        };

        (width > 0.0 && height > 0.0).then(|| width / height)
    }
}

/// The logical size of a content `physical` in size on a display of
/// `scale_factor` physical pixels per logical unit: one physical unit of the
/// content per physical pixel. The physical size itself for a scale factor
/// that is not positive and finite.
pub fn logical_size(physical: kurbo::Size, scale_factor: f64) -> kurbo::Size {
    if scale_factor > 0.0 && scale_factor.is_finite() {
        physical / scale_factor
    } else {
        physical
    }
}

/// `size` scaled down, keeping its aspect ratio, to fit the definite axes of
/// `available`. Not scaled up.
fn shrink_into(size: kurbo::Size, available: taffy::Size<taffy::AvailableSpace>) -> kurbo::Size {
    let limit = |length: f64, space: taffy::AvailableSpace| match space {
        taffy::AvailableSpace::Definite(space) if length > 0.0 => f64::from(space) / length,
        _ => f64::INFINITY,
    };
    let scale = limit(size.width, available.width)
        .min(limit(size.height, available.height))
        .min(1.0);

    size * scale.max(0.0)
}
