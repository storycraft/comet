use comet_layout::{
    style::{
        LayoutConstraint, bx,
        geometry::{AutoValue, Rect, Size, StyleUnit},
    },
    taffy::style::resolve,
};
use comet_tree::{node::Node, ui::Ui};
use taffy::style::CoreStyle;

#[test]
fn display_semantics_do_not_depend_on_taffy_display_defaults() {
    let mut ui = Ui::new();
    let cx = LayoutConstraint::new_root(Size::new(800.0, 600.0), 16.0);

    for (inner, block, replaced) in [
        (bx::DisplayInner::Flow, true, false),
        (bx::DisplayInner::FlowRoot, false, false),
        (bx::DisplayInner::Content, true, true),
    ] {
        let node = ui.create(Node::Box, (inner,));
        let style = resolve(&ui, Some(node), &cx);

        assert_eq!(style.is_block(), block);
        assert_eq!(style.is_replaced(), replaced);
        assert_eq!(style.is_compressible_replaced(), replaced);
    }
}

#[test]
fn anonymous_boxes_use_comet_defaults() {
    let ui = Ui::new();
    let cx = LayoutConstraint::new_root(Size::new(800.0, 600.0), 16.0);
    let style = resolve(&ui, None, &cx);

    assert!(style.is_block());
    assert!(!style.is_replaced());
    assert_eq!(style.position(), taffy::Position::Relative);
    assert_eq!(style.size().width, taffy::Dimension::auto());
    assert_eq!(style.margin().left, taffy::LengthPercentageAuto::length(0.0));
    assert_eq!(style.padding().left, taffy::LengthPercentage::length(0.0));
}

#[test]
fn adapter_resolves_constraint_units_and_preserves_percentages() {
    let mut ui = Ui::new();
    let node = ui.create(
        Node::Box,
        (
            bx::Size(Size::new(
                AutoValue::Value(StyleUnit::Em(2.0)),
                AutoValue::Value(StyleUnit::Percent(0.5)),
            )),
            bx::Padding(Rect::new_invidiual(
                StyleUnit::Rem(2.0),
                StyleUnit::Vw(0.1),
                StyleUnit::Vh(0.1),
                StyleUnit::Percent(0.25),
            )),
        ),
    );
    let mut cx = LayoutConstraint::new_root(Size::new(800.0, 600.0), 16.0);
    cx.font_size = 24.0;

    let style = resolve(&ui, Some(node), &cx);

    assert_eq!(style.size().width, taffy::Dimension::length(48.0));
    assert_eq!(style.size().height, taffy::Dimension::percent(0.5));
    assert_eq!(style.padding().top, taffy::LengthPercentage::length(32.0));
    assert_eq!(style.padding().right, taffy::LengthPercentage::length(80.0));
    assert_eq!(style.padding().bottom, taffy::LengthPercentage::length(60.0));
    assert_eq!(style.padding().left, taffy::LengthPercentage::percent(0.25));

    cx.font_size = 12.0;

    let updated = resolve(&ui, Some(node), &cx);

    assert_ne!(style, updated);
    assert_eq!(updated.size().width, taffy::Dimension::length(24.0));
}
