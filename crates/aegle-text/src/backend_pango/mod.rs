//! The Linux system text stack: Pango for layout, Fontconfig for fonts and
//! FreeType (through PangoFT2) for glyphs, all loaded from the distribution's
//! shared objects at run time.
//!
//! Shape, itemize, break lines, bidi-reorder and fall back here exactly as the
//! desktop's own toolkit does; Aegle keeps only the retained paragraph, the
//! caret geometry and the coverage masks it paints.

mod layout;
mod metrics;
#[cfg(feature = "scene")]
mod paint;
mod sys;
mod system;

pub(crate) use layout::PangoLayout;
pub(crate) use system::PangoSystem;
