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
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub FlexDirection: taffy::FlexDirection = taffy::FlexDirection::Row;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub FlexWrap: taffy::FlexWrap = taffy::FlexWrap::NoWrap;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub FlexLineCount: u16 = 1;
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub Gap: Size<StyleUnit> = Size::new(StyleUnit::ZERO, StyleUnit::ZERO);

    // Container alignment.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub AlignItems: taffy::AlignItems = taffy::AlignItems::NORMAL;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub AlignContent: taffy::AlignContent = taffy::AlignContent::NORMAL;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub JustifyContent: taffy::JustifyContent = taffy::JustifyContent::NORMAL;

    // Item sizing and alignment.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub FlexGrow: f32 = 0.0;
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub FlexShrink: f32 = 1.0;
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub FlexBasis: AutoValue<StyleUnit> = AutoValue::Auto;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub AlignSelf: Option<taffy::AlignSelf> = None;
);
