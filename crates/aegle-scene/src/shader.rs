//! Custom fragment programs: WGSL that fills a shape on GPU renderers.

use alloc::{boxed::Box, sync::Arc};

/// A WGSL fragment program filling a shape, shared by its clones.
///
/// The source defines `fn shade(uv: vec2f, size: vec2f) -> vec4f`, returning
/// an unpremultiplied sRGB color for the point at `uv` (0 to 1 across the
/// shape, from its top-left corner) of a shape `size` logical pixels large.
/// It may read two globals: `time`, seconds since the program started, and
/// `values`, an `array<f32, 8>` the program is drawn with. Renderers clip the
/// result to the shape's rounded corners and composite it like a fill.
///
/// A program that never names `time` is drawn once; one that does is drawn
/// every frame it is visible ([`Self::is_animated`]). Only GPU renderers run
/// programs, and they compile one on first use and fail the frame if it
/// does not compile, so check sources first (`aegle_gpu::check_shader`).
#[derive(Clone)]
pub struct Shader(Arc<ShaderData>);

struct ShaderData {
    key: u64,
    source: Box<str>,
    animated: bool,
}

impl Shader {
    /// Number of `values` a program reads.
    pub const VALUES: usize = 8;

    /// Takes WGSL source defining `shade`; nothing is compiled yet.
    pub fn new(source: &str) -> Self {
        Self(Arc::new(ShaderData {
            key: fnv1a(source.as_bytes()),
            source: source.into(),
            animated: names_time(source),
        }))
    }

    /// The program's source.
    pub fn source(&self) -> &str {
        &self.0.source
    }

    /// A hash of the source: equal sources share compiled programs.
    pub fn key(&self) -> u64 {
        self.0.key
    }

    /// Whether the source names `time`, outside comments, so the program
    /// changes from frame to frame.
    pub fn is_animated(&self) -> bool {
        self.0.animated
    }
}

impl core::fmt::Debug for Shader {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Shader#{:016x}", self.key())
    }
}

impl PartialEq for Shader {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key() && self.source() == other.source()
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Whether `source` has the identifier `time` outside `//` and (nested)
/// `/* */` comments.
fn names_time(source: &str) -> bool {
    let bytes = source.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let (mut at, mut depth) = (0, 0usize);
    while at < bytes.len() {
        let rest = &bytes[at..];
        if rest.starts_with(b"/*") {
            depth += 1;
            at += 2;
        } else if depth > 0 && rest.starts_with(b"*/") {
            depth -= 1;
            at += 2;
        } else if depth > 0 {
            at += 1;
        } else if rest.starts_with(b"//") {
            at += rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
        } else if word(bytes[at]) {
            let end = at + rest.iter().position(|&b| !word(b)).unwrap_or(rest.len());
            if &bytes[at..end] == b"time" {
                return true;
            }
            at = end;
        } else {
            at += 1;
        }
    }
    false
}
