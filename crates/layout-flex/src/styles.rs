//! Flex container and item properties. Properties do not inherit.
//!
//! `Gap.width` is the column gap and `Gap.height` is the row gap.
//! Percentages use fractions (`StyleUnit::Percent(0.1)` means 10%).
//! Lengths in gaps and flex bases accept all Comet units.

pub mod taffy;

use comet_layout::style::geometry::{AutoValue, Size, StyleUnit};
use comet_tree::style::define_style_props;

define_style_props!(
    // Container layout.
    pub FlexDirection: taffy::FlexDirection = taffy::FlexDirection::Row;
    pub FlexWrap: taffy::FlexWrap = taffy::FlexWrap::NoWrap;
    pub FlexLineCount: u16 = 1;
    pub Gap: Size<StyleUnit> = Size::new(StyleUnit::ZERO, StyleUnit::ZERO);

    // Container alignment.
    pub AlignItems: taffy::AlignItems = taffy::AlignItems::NORMAL;
    pub AlignContent: taffy::AlignContent = taffy::AlignContent::NORMAL;
    pub JustifyContent: taffy::JustifyContent = taffy::JustifyContent::NORMAL;

    // Item sizing and alignment.
    pub FlexGrow: f32 = 0.0;
    pub FlexShrink: f32 = 1.0;
    pub FlexBasis: AutoValue<StyleUnit> = AutoValue::Auto;
    pub AlignSelf: Option<taffy::AlignSelf> = None;
);
