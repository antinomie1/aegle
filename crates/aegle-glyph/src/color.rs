//! Color-glyph representations the Rust stack decodes: COLRv1 paint graphs and
//! OpenType-SVG documents.
//!
//! The platform rasterizers render monochrome coverage and cannot expose either
//! representation (FreeType's mask, DirectWrite's alpha texture and an
//! alpha-only CoreGraphics context all drop color). These two features therefore do not replace the platform
//! rasterizer; instead the rasterizer consults this module first, and only the
//! glyphs that actually carry a color representation take this path.
//!
//! Both decoders are bounded: the caller passes the image byte limit and the
//! renderers ask for their buffer before allocating it.

use skrifa::MetadataProvider;
use skrifa::raw::TableProvider;

use crate::{FontData, GlyphError, Image, RasterOptions};

/// The color representation a glyph carries.
// Each variant is only constructed by its own feature's branch, so the enum is
// sparse in single-feature builds by design.
#[allow(dead_code)]
enum Representation {
    /// A COLRv1 paint graph, decodable when the `colrv1` feature is on.
    ColrV1,
    /// An OpenType-SVG document, decodable when the `svg` feature is on.
    Svg,
}

/// Renders a color glyph.
///
/// Returns `None` when the glyph carries no color representation at all, so the
/// caller draws its outline as usual. Returns `Some(Err(UnsupportedGlyph))` when
/// the glyph *does* carry a color representation this build was not asked to
/// decode: falling back to the outline would draw the wrong pixels, so the gap
/// is reported instead.
pub(crate) fn render(
    data: &FontData,
    glyph: u16,
    options: RasterOptions<'_>,
    limits: crate::CacheLimits,
    reserve: &mut dyn FnMut(usize) -> Result<(), GlyphError>,
) -> Option<Result<Image, GlyphError>> {
    // With `swash` alone neither representation is decodable, so the renderer
    // only classifies glyphs; the parameters stay for the other builds.
    #[cfg(not(any(feature = "colrv1", feature = "svg")))]
    let _ = (&options, &limits, &mut *reserve);
    let parsed = skrifa::FontRef::from_index(data.data.data(), data.index).ok()?;
    let units_per_em = parsed.head().ok()?.units_per_em();
    if units_per_em == 0 {
        return None;
    }
    match representation(&parsed, glyph)? {
        #[cfg(feature = "colrv1")]
        Representation::ColrV1 => Some(crate::colrv1::render(
            crate::colrv1::Request {
                glyph: parsed.color_glyphs().get(glyph.into())?,
                outlines: parsed.outline_glyphs(),
                coords: options.normalized_coords,
                units_per_em: f32::from(units_per_em),
                size: options.size,
                offset: options.offset,
                palette: palette(&parsed),
                foreground: options.foreground,
                max_bytes: limits.image_bytes,
            },
            reserve,
        )),
        #[cfg(feature = "svg")]
        Representation::Svg => {
            let Some(document) = parsed
                .svg()
                .ok()
                .and_then(|svg| svg.glyph_data(glyph.into()).ok().flatten())
            else {
                return Some(Err(GlyphError::UnsupportedGlyph));
            };
            Some(crate::svg::render(
                crate::svg::Request {
                    data: document,
                    glyph,
                    units_per_em: f32::from(units_per_em),
                    size: options.size,
                    offset: options.offset,
                    max_bytes: limits.image_bytes,
                },
                reserve,
            ))
        }
        #[cfg(not(feature = "colrv1"))]
        Representation::ColrV1 => Some(Err(GlyphError::UnsupportedGlyph)),
        #[cfg(not(feature = "svg"))]
        Representation::Svg => Some(Err(GlyphError::UnsupportedGlyph)),
    }
}

/// Which color representation `glyph` carries, whether or not this build
/// decodes it.
///
/// Reporting the representation even when the corresponding feature is off is
/// what lets the caller answer `UnsupportedGlyph`; returning `None` instead
/// would let the outline path draw a glyph whose pixels are wrong.
fn representation(parsed: &skrifa::FontRef<'_>, glyph: u16) -> Option<Representation> {
    let color = parsed.color_glyphs().get(glyph.into());
    if color.is_some_and(|color| matches!(color.format(), skrifa::color::ColorGlyphFormat::ColrV1))
    {
        return Some(Representation::ColrV1);
    }
    let svg = parsed
        .svg()
        .ok()
        .and_then(|svg| svg.glyph_data(glyph.into()).ok().flatten());
    if svg.is_some() {
        return Some(Representation::Svg);
    }
    None
}

/// The default palette of `parsed`, as straight sRGB entries.
#[cfg(feature = "colrv1")]
fn palette(parsed: &skrifa::FontRef<'_>) -> Vec<[u8; 4]> {
    parsed
        .color_palettes()
        .get(0)
        .map(|palette| {
            palette
                .colors()
                .iter()
                .map(|color| [color.red(), color.green(), color.blue(), color.alpha()])
                .collect()
        })
        .unwrap_or_default()
}

/// Keeps the `TableProvider` import meaningful for the SVG table lookup.
const _: Option<fn(&skrifa::FontRef<'_>) -> bool> = None;
