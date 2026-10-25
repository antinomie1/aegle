use std::fmt;

/// A platform result.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The result of a host drawing callback.
pub type DrawResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Window, input or presentation failure, the same on every platform.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A native connection, protocol or OS operation failed.
    Backend(String),
    /// The platform lacks a capability the request needs, such as layer
    /// surfaces; the string names it.
    Unsupported(&'static str),
    /// An extent is empty or cannot be represented natively.
    InvalidSize,
    /// The window was removed or belongs to another connection.
    InvalidWindow,
    /// A title or application identifier contains NUL or exceeds 4000 UTF-8 bytes.
    InvalidString,
    /// No native composition is available.
    ImeUnavailable,
    /// Invalid surrounding text, selection or candidate geometry.
    InvalidIme(&'static str),
    /// Native text input was malformed.
    InvalidText,
    /// Presentation storage would exceed the window's budget.
    BufferBudget {
        /// Required bytes.
        required: usize,
        /// Configured allowance.
        budget: usize,
    },
    /// The host's drawing callback failed; nothing was presented.
    Draw(Box<dyn std::error::Error>),
}

impl Error {
    /// A [`Self::Backend`] error describing `error`.
    pub fn backend(error: impl fmt::Display) -> Self {
        Self::Backend(error.to_string())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => f.write_str(error),
            Self::Unsupported(what) => write!(f, "{what} is unsupported on this platform"),
            Self::InvalidSize => f.write_str("invalid window size"),
            Self::InvalidWindow => f.write_str("unknown window"),
            Self::InvalidString => {
                f.write_str("window strings must be NUL-free and at most 4000 UTF-8 bytes")
            }
            Self::ImeUnavailable => f.write_str("native text input is unavailable"),
            Self::InvalidIme(reason) => write!(f, "invalid IME state: {reason}"),
            Self::InvalidText => f.write_str("native text input is malformed"),
            Self::BufferBudget { required, budget } => {
                write!(
                    f,
                    "presentation requires {required} bytes, budget is {budget}"
                )
            }
            Self::Draw(error) => write!(f, "drawing failed: {error}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Draw(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}
