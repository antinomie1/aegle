/// What a primary pointer press does on part of a window's content: the
/// vocabulary controls and window platforms share for custom title bars and
/// resize grips.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WindowArea {
    /// Ordinary content: the press is reported as a pointer event.
    #[default]
    Client,
    /// Dragging moves the window, as a title bar does.
    Move,
    /// Dragging resizes the window by an edge or corner, as a border does.
    Resize(ResizeEdge),
}

/// The edge or corner of a window a resize drags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResizeEdge {
    /// The top edge.
    Top,
    /// The bottom edge.
    Bottom,
    /// The left edge.
    Left,
    /// The right edge.
    Right,
    /// The top-left corner.
    TopLeft,
    /// The top-right corner.
    TopRight,
    /// The bottom-left corner.
    BottomLeft,
    /// The bottom-right corner.
    BottomRight,
}
