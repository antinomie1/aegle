//! Retained Unicode paragraphs, independent of windows and drawing backends.
//!
//! Share one [`TextSystem`] across the UI thread. Each [`Paragraph`] retains its
//! text and shaping result; resizing only breaks and aligns the existing glyphs.
//! Register application fonts explicitly, or enable `system-fonts` and construct
//! `TextSystem::system` to discover platform fonts. No font files are embedded.
//!
//! Word-wise cursor movement and double-click selection ask the operating
//! system for word boundaries (see [`segment`]); no dictionary data is bundled.
//! When the platform has no segmenter, those two operations are off instead of
//! approximating words.
//! `text-a11y` adds AccessKit text runs and selection actions through the same
//! editor/layout, without an OS adapter or a second text buffer.
//! `scene` adds paragraph painting into Aegle's retained scene records; it rejects
//! synthetic bold/oblique faces until those rasterization operations are supported.
//!
//! [`Editor`] retains plain text, selection, IME composition and bounded delta
//! history using the same fonts and drawing path, as one layout per hard line:
//! an edit shapes only the lines it touches. Platform IME and clipboard
//! protocols remain the host's responsibility.

#[cfg(feature = "text-a11y")]
mod accessibility;
#[cfg(all(target_os = "linux", not(feature = "text-parley")))]
mod backend_pango;
#[cfg(feature = "text-parley")]
mod backend_parley;
mod compose;
mod document;
mod edit;
mod editor;
#[cfg(feature = "scene")]
mod editor_paint;
mod history;
mod ime;
mod layout;
mod nav;
mod surrounding;

#[cfg(feature = "scene")]
mod paint;
mod paragraph;
mod style;
mod system;
mod values;

pub use aegle_segment as segment;
pub use edit::{EditorDriver, HitSelection, Movement};
pub use editor::{EditChanges, Editor, EditorOptions, HistoryStats, Selection, TextValue};
#[cfg(feature = "scene")]
pub use editor_paint::EditorPaint;
pub use ime::ImeEdit;
pub use paragraph::{Paragraph, TextDiagnostics};
pub use style::{TextError, TextStyle};
pub use surrounding::Surrounding;
pub use system::TextSystem;

pub use values::{Alignment, ContentWidths, FontFamily, GenericFamily, LineHeight, Slant};
