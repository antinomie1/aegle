/// Desktop appearance preferences; `None` where the system reports nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Preferences {
    /// Whether the user prefers a dark color scheme.
    pub dark: Option<bool>,
    /// Whether high-contrast presentation is requested.
    pub high_contrast: Option<bool>,
    /// Whether non-essential motion should be reduced.
    pub reduced_motion: Option<bool>,
    /// Text size as a percentage of the default (100), within 50–400.
    pub text_scale: Option<u16>,
}

/// Stage of one finger's contact with a touch screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    /// The finger touched the surface.
    Down,
    /// The finger moved while touching.
    Move,
    /// The finger lifted.
    Up,
    /// The system took the contact away: end it without activating anything.
    Cancel,
}
