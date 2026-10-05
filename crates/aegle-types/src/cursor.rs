/// Mouse pointer shape requested while the pointer is over a window.
///
/// Platforms map each variant to the closest system cursor: Wayland cursor-icon
/// names, Win32 `IDC_*` cursors. Shapes a system lacks use its nearest match, for
/// example Win32 shows its hand for [`Cursor::Grab`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Cursor {
    /// The ordinary arrow.
    #[default]
    Default,
    /// An I-beam, over editable or selectable text.
    Text,
    /// A hand, over links and other single-click targets.
    Pointer,
    /// A crosshair, for precise picking.
    Crosshair,
    /// Four arrows, for moving an object.
    Move,
    /// An open hand, over something that can be dragged.
    Grab,
    /// A closed hand, while dragging.
    Grabbing,
    /// A slashed circle, over something that cannot be used.
    NotAllowed,
    /// Left-right arrows, for horizontal resizing.
    ResizeHorizontal,
    /// Up-down arrows, for vertical resizing.
    ResizeVertical,
}
