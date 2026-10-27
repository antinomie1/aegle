//! Word boundaries from the operating system's own text segmenter.
//!
//! Word-wise cursor movement and double-click selection need to know where
//! words begin, which Unicode's rule-based boundary detection cannot answer for
//! scripts written without spaces (Chinese, Japanese, Thai, Lao, Khmer,
//! Burmese). The dictionary data that answers it is large, so this crate does
//! not carry it: it asks the operating system.
//!
//! | Target | Segmenter | Data |
//! |---|---|---|
//! | Linux, Windows | the system ICU (`libicui18n`, `icu.dll`/`icuin.dll`), loaded at run time | installed with the OS or distribution |
//! | macOS | `CFStringTokenizer` with the best language for the text | installed with the OS |
//! | others | none | — |
//!
//! Nothing is linked statically and no data is bundled, so [`available`] can be
//! false: a Linux system without libicu, or a target with neither ICU nor
//! CoreFoundation. This crate then reports no boundaries, and there is no
//! second, approximate segmenter to fall back to: word operations stay off
//! until the platform can answer. Loading the library is a one-time cost per
//! process (about a millisecond); a missing library is remembered, not
//! retried. Setting `AEGLE_SEGMENT=none` in the environment turns the
//! segmenter off deliberately, which is how the disabled path is tested on a
//! host that does have the library.

mod boundaries;
#[cfg(target_os = "macos")]
mod core_foundation;
#[cfg(any(target_os = "linux", windows))]
mod icu;
#[cfg(any(target_os = "linux", windows))]
mod language;
mod units;

pub use boundaries::Boundaries;

use std::sync::OnceLock;

/// The segmenter of one platform: the byte offsets where words start.
type Segments = fn(&str) -> Option<Vec<usize>>;

/// The process's segmenter, or `None` when this platform has none or its
/// library is missing.
fn segmenter() -> Option<Segments> {
    static SEGMENTER: OnceLock<Option<Segments>> = OnceLock::new();
    *SEGMENTER.get_or_init(|| {
        // `AEGLE_SEGMENT=none` answers "what does the fallback do here" for
        // debugging and for hosts without the library; it is read once.
        if std::env::var_os("AEGLE_SEGMENT").is_some_and(|value| value == "none") {
            return None;
        }
        #[cfg(target_os = "macos")]
        {
            core_foundation::segments()
        }
        #[cfg(any(target_os = "linux", windows))]
        {
            icu::segments()
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
        {
            None
        }
    })
}

/// Whether this process has a word segmenter.
///
/// False on a Linux system without libicu and on targets that have neither ICU
/// nor CoreFoundation; word-based editing is then unavailable rather than
/// approximated.
pub fn available() -> bool {
    segmenter().is_some()
}

/// The word boundaries of `text`, in ascending UTF-8 byte offsets, always
/// including the first and the last offset.
///
/// `None` when no segmenter is available, so callers fall back to their own
/// boundaries. Offsets are at character boundaries and never split a grapheme
/// cluster the segmenter reported.
pub fn boundaries(text: &str) -> Option<Boundaries> {
    let starts = segmenter()?(text)?;
    Some(Boundaries::new(text, starts))
}
