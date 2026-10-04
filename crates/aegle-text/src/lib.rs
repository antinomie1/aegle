//! Retained Unicode paragraphs, independent of windows and drawing backends.
//!
//! Share one [`TextSystem`] across the UI thread. Each [`Paragraph`] retains its
//! text and shaping result; resizing only breaks and aligns the existing glyphs.
//! Register application fonts explicitly, or enable `system-fonts` and construct
//! `TextSystem::system` to discover platform fonts. No font files are embedded.
//!
//! `text-dictionary` enables Parley's dictionary word segmentation. Without it,
//! basic CJK display and line wrapping remain supported, but dictionary-based
//! word navigation and some Southeast Asian segmentation are unavailable.
//! `text-a11y` enables upstream AccessKit layout support without an OS adapter.
//! `scene` adds paragraph painting into Aegle's retained scene records; it rejects
//! synthetic bold/oblique faces until those rasterization operations are supported.
//!
//! This module currently supplies display paragraphs; it is not an editor or
//! native IME implementation. See [`Paragraph::diagnostics`] for missing fonts.

#[cfg(feature = "scene")]
mod paint;
mod paragraph;
mod style;
mod system;

#[cfg(feature = "scene")]
pub use paint::PaintError;
pub use paragraph::{Paragraph, TextDiagnostics};
pub use style::{TextError, TextStyle};
pub use system::TextSystem;

pub use parley::fontique::{
    Blob, Collection, FallbackKey, FamilyId, FontInfo, GenericFamily, Language, Script,
};
pub use parley::{
    Alignment, ContentWidths, FontData, FontStyle, FontWeight, FontWidth, GlyphRun, Layout,
    LineHeight,
};
