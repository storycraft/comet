use comet_layout::{container::ContainerContext, style::geometry::AutoValue};
use taffy::{
    CoreStyle, FlexboxContainerStyle, FlexboxItemStyle, LayoutContainingBlock,
    LayoutFlexboxContainer, LayoutPartialTree, OofItemStyle, TraversePartialTree,
};

use crate::styles;

const ROOT: taffy::NodeId = taffy::NodeId::new(0);

pub(crate) fn layout(
    context: &mut ContainerContext<'_, '_>,
    input: taffy::LayoutInput,
) -> taffy::LayoutOutput {
    let mut styles = Vec::with_capacity(context.child_count() + 1);
    styles.push(Style::container(context));

    for index in 0..context.child_count() {
        styles.push(Style::child(context, index));
    }

    let mut adapter = Adapter { context, styles };
    let mut output = taffy::compute_flexbox_layout(&mut adapter, ROOT, input);
    taffy::compute_oof_layout(&mut adapter, ROOT, &mut output);

    output
}

/// Local IDs describe only the container (0) and its direct children (1..).
struct Adapter<'a, 'engine, 'ui> {
    context: &'a mut ContainerContext<'engine, 'ui>,
    styles: Vec<Style>,
}

impl TraversePartialTree for Adapter<'_, '_, '_> {
    type ChildIter<'a>
        = std::iter::Map<std::ops::Range<usize>, fn(usize) -> taffy::NodeId>
    where
        Self: 'a;

    fn child_ids(&self, id: taffy::NodeId) -> Self::ChildIter<'_> {
        (1..self.child_count(id) + 1).map(taffy::NodeId::from)
    }

    fn child_count(&self, id: taffy::NodeId) -> usize {
        if id == ROOT {
            self.context.child_count()
        } else {
            0
        }
    }

    fn get_child_id(&self, id: taffy::NodeId, index: usize) -> taffy::NodeId {
        assert!(index < self.child_count(id));

        (index + 1).into()
    }
}

impl LayoutPartialTree for Adapter<'_, '_, '_> {
    type CustomIdent = String;
    type CoreContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_core_container_style(&self, id: taffy::NodeId) -> &Style {
        &self.styles[usize::from(id)]
    }

    fn set_unrounded_layout(&mut self, id: taffy::NodeId, layout: &taffy::Layout) {
        self.context.set_child_layout(usize::from(id) - 1, *layout);
    }

    fn compute_child_layout(
        &mut self,
        id: taffy::NodeId,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        self.context.measure_child(usize::from(id) - 1, input)
    }
}

impl LayoutFlexboxContainer for Adapter<'_, '_, '_> {
    type FlexboxContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type FlexboxItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_flexbox_container_style(&self, id: taffy::NodeId) -> &Style {
        self.get_core_container_style(id)
    }

    fn get_flexbox_child_style(&self, id: taffy::NodeId) -> &Style {
        self.get_core_container_style(id)
    }
}

impl LayoutContainingBlock for Adapter<'_, '_, '_> {
    type OofItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_oof_item_style(&self, id: taffy::NodeId) -> &Style {
        self.get_core_container_style(id)
    }

    // Comet owns the output tree; Taffy's optional rounding traversal isn't used.
    fn clear_hoisted_children(&mut self, _: taffy::NodeId) {}

    fn add_hoisted_children(&mut self, _: taffy::NodeId, _: &[taffy::NodeId]) {}
}

struct Style {
    core: Option<comet_layout::taffy::style::Style>,
    flex: taffy::Style<String>,
}

impl Style {
    fn container(context: &ContainerContext<'_, '_>) -> Self {
        macro_rules! prop {
            ($name:ident) => {
                context
                    .prop::<styles::$name>()
                    .map(|v| v.0)
                    .unwrap_or_else(|| styles::$name::default().0)
            };
        }

        let gap = prop!(Gap);

        Self {
            // The enclosing leaf already applies the container's box styles.
            core: None,
            flex: taffy::Style {
                position: taffy::Position::Relative,
                flex_direction: prop!(FlexDirection),
                flex_wrap: prop!(FlexWrap),
                flex_line_count: prop!(FlexLineCount),
                gap: taffy::Size {
                    width: context.resolve_length(gap.width),
                    height: context.resolve_length(gap.height),
                },
                align_items: prop!(AlignItems),
                align_content: prop!(AlignContent),
                justify_content: prop!(JustifyContent),
                ..Default::default()
            },
        }
    }

    fn child(context: &ContainerContext<'_, '_>, index: usize) -> Self {
        macro_rules! prop {
            ($name:ident) => {
                context
                    .child_prop::<styles::$name>(index)
                    .map(|v| v.0)
                    .unwrap_or_else(|| styles::$name::default().0)
            };
        }

        let basis = match prop!(FlexBasis) {
            AutoValue::Auto => taffy::Dimension::auto(),
            AutoValue::Value(value) => context.resolve_child_length(index, value).into(),
        };

        Self {
            core: Some(context.child_style(index).clone()),
            flex: taffy::Style {
                flex_basis: basis,
                flex_grow: prop!(FlexGrow),
                flex_shrink: prop!(FlexShrink),
                align_self: prop!(AlignSelf),
                ..Default::default()
            },
        }
    }
}

macro_rules! core_style {
    ($($name:ident: $ty:ty),* $(,)?) => {$(
        fn $name(&self) -> $ty {
            match &self.core {
                Some(core) => core.$name(),
                None => self.flex.$name(),
            }
        }
    )*};
}

impl CoreStyle for Style {
    type CustomIdent = String;

    core_style!(
        is_block: bool,
        is_replaced: bool,
        is_compressible_replaced: bool,
        position: taffy::Position,
        box_sizing: taffy::BoxSizing,
        overflow: taffy::Point<taffy::Overflow>,
        inset: taffy::Rect<taffy::LengthPercentageAuto>,
        size: taffy::Size<taffy::Dimension>,
        min_size: taffy::Size<taffy::LengthPercentageAuto>,
        max_size: taffy::Size<taffy::LengthPercentageAuto>,
        aspect_ratio: Option<f32>,
        margin: taffy::Rect<taffy::LengthPercentageAuto>,
        padding: taffy::Rect<taffy::LengthPercentage>,
        border: taffy::Rect<taffy::LengthPercentage>,
    );
}

impl FlexboxContainerStyle for Style {
    fn flex_direction(&self) -> taffy::FlexDirection {
        self.flex.flex_direction
    }

    fn flex_wrap(&self) -> taffy::FlexWrap {
        self.flex.flex_wrap
    }

    fn flex_line_count(&self) -> u16 {
        self.flex.flex_line_count
    }

    fn gap(&self) -> taffy::Size<taffy::LengthPercentage> {
        self.flex.gap
    }

    fn align_items(&self) -> taffy::AlignItems {
        self.flex.align_items
    }

    fn align_content(&self) -> taffy::AlignContent {
        self.flex.align_content
    }

    fn justify_content(&self) -> taffy::JustifyContent {
        self.flex.justify_content
    }
}

impl FlexboxItemStyle for Style {
    fn flex_basis(&self) -> taffy::Dimension {
        self.flex.flex_basis
    }

    fn flex_grow(&self) -> f32 {
        self.flex.flex_grow
    }

    fn flex_shrink(&self) -> f32 {
        self.flex.flex_shrink
    }

    fn align_self(&self) -> Option<taffy::AlignSelf> {
        self.flex.align_self
    }
}

impl OofItemStyle for Style {}
