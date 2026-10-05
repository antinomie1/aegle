use crate::Span;
use std::fmt;

/// A parsing or schema diagnostic tied to the original source bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// Source range responsible for the error.
    pub span: Span,
    /// Human-readable reason.
    pub message: String,
}

impl Error {
    /// Creates a diagnostic, also usable by a host's component validator.
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }

    /// Renders file, one-based line and Unicode scalar column, and source text.
    ///
    /// Columns count Unicode scalar values rather than UTF-8 bytes. Invalid
    /// caller-provided spans are clamped to the source for diagnostic display.
    pub fn render(&self, source: &str, file: &str) -> String {
        let mut offset = self.span.start.min(source.len());
        while !source.is_char_boundary(offset) {
            offset -= 1;
        }
        let before = &source[..offset];
        let line = before.bytes().filter(|&byte| byte == b'\n').count() + 1;
        let start = before.rfind('\n').map_or(0, |index| index + 1);
        let column = source[start..offset].chars().count() + 1;
        let end = source[offset..]
            .find(['\n', '\r'])
            .map_or(source.len(), |index| offset + index);
        format!(
            "{file}:{line}:{column}: {}\n{}\n{}^",
            self.message,
            &source[start..end],
            " ".repeat(column - 1)
        )
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.span.start)
    }
}

impl std::error::Error for Error {}
