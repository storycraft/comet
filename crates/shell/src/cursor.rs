//! Mouse cursors.

/// A mouse cursor, by the keywords of the CSS `cursor` property. `auto` is
/// [`AutoValue::Auto`](comet_geometry::AutoValue::Auto) of the
/// [`Cursor`](crate::style::Cursor) prop, and `none` is no icon.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CursorIcon {
    /// The platform's default cursor, often an arrow.
    #[default]
    Default,
    /// A context menu is available.
    ContextMenu,
    /// Help is available.
    Help,
    /// A link, or something else that can be activated.
    Pointer,
    /// The program is busy in the background, but can be interacted with.
    Progress,
    /// The program is busy and cannot be interacted with.
    Wait,
    /// A cell or a set of cells can be selected.
    Cell,
    /// A precise selection, often a cross.
    Crosshair,
    /// Text can be selected, often an I-beam.
    Text,
    /// Vertical text can be selected.
    VerticalText,
    /// An alias or a shortcut is to be created.
    Alias,
    /// Something is to be copied.
    Copy,
    /// Something is to be moved.
    Move,
    /// What is dragged cannot be dropped here.
    NoDrop,
    /// The requested action will not be carried out.
    NotAllowed,
    /// Something can be grabbed and dragged.
    Grab,
    /// Something is being grabbed.
    Grabbing,
    /// Something can be scrolled in any direction.
    AllScroll,
    /// A column can be resized horizontally.
    ColResize,
    /// A row can be resized vertically.
    RowResize,
    /// An edge is to be moved north.
    NResize,
    /// An edge is to be moved east.
    EResize,
    /// An edge is to be moved south.
    SResize,
    /// An edge is to be moved west.
    WResize,
    /// An edge is to be moved north-east.
    NeResize,
    /// An edge is to be moved north-west.
    NwResize,
    /// An edge is to be moved south-east.
    SeResize,
    /// An edge is to be moved south-west.
    SwResize,
    /// An edge is to be resized east or west.
    EwResize,
    /// An edge is to be resized north or south.
    NsResize,
    /// An edge is to be resized north-east or south-west.
    NeswResize,
    /// An edge is to be resized north-west or south-east.
    NwseResize,
    /// Something can be zoomed in.
    ZoomIn,
    /// Something can be zoomed out.
    ZoomOut,
}
