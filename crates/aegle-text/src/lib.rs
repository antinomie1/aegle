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
//! `text-a11y` adds AccessKit text runs and selection actions through the same
//! editor/layout, without an OS adapter or a second text buffer.
//! `scene` adds paragraph painting into Aegle's retained scene records; it rejects
//! synthetic bold/oblique faces until those rasterization operations are supported.
//!
//! [`Editor`] retains plain text, selection, IME composition and bounded delta
//! history using the same fonts and drawing path. Platform IME and clipboard
//! protocols remain the host's responsibility.

#[cfg(feature = "text-a11y")]
mod accessibility;
mod compose;
mod edit;
mod editor;
#[cfg(feature = "scene")]
mod editor_paint;
mod history;
mod ime;
mod surrounding;

#[cfg(feature = "scene")]
mod paint;
mod paragraph;
mod style;
mod system;

pub use edit::{EditorDriver, HitSelection, Movement};
pub use editor::{EditChanges, Editor, EditorOptions, HistoryStats, Selection, TextValue};
#[cfg(feature = "scene")]
pub use editor_paint::EditorPaint;
pub use ime::ImeEdit;
pub use paragraph::{Paragraph, TextDiagnostics};
pub use style::{TextError, TextStyle};
pub use surrounding::Surrounding;
pub use system::TextSystem;

pub use parley::fontique::{
    Blob, Collection, FallbackKey, FamilyId, FontInfo, GenericFamily, Language, Script,
};
pub use parley::{
    Alignment, ContentWidths, FontData, FontStyle, FontWeight, FontWidth, GlyphRun, Layout,
    LineHeight,
};
