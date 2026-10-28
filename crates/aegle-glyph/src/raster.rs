use crate::{CacheLimits, FontSource, GlyphError, Image, RasterOptions};

/// The backend number of this target's platform rasterizer.
#[cfg(all(
    not(feature = "swash"),
    any(target_os = "windows", target_os = "macos")
))]
fn platform_backend() -> u8 {
    #[cfg(target_os = "windows")]
    {
        crate::SystemFont::DWRITE
    }
    #[cfg(target_os = "macos")]
    {
        crate::SystemFont::CORETEXT
    }
}

pub(crate) struct Rasterizer {
    #[cfg(feature = "swash")]
    swash: Option<crate::swash_raster::SwashRasterizer>,
    #[cfg(all(not(feature = "swash"), target_os = "linux"))]
    ft: Option<crate::freetype::FreeTypeRasterizer>,
}

impl Rasterizer {
    pub fn new() -> Self {
        // Exactly one arm is compiled per configuration, and each is the
        // function's tail expression.
        #[cfg(feature = "swash")]
        {
            Self {
                swash: Some(crate::swash_raster::SwashRasterizer::new()),
            }
        }
        #[cfg(all(not(feature = "swash"), target_os = "linux"))]
        {
            Self {
                ft: crate::freetype::FreeTypeRasterizer::new(),
            }
        }
        // Windows and macOS rasterize through the registered platform
        // rasterizer, so the Rust rasterizer keeps no state of its own.
        #[cfg(all(
            not(feature = "swash"),
            any(target_os = "windows", target_os = "macos")
        ))]
        {
            Self {}
        }
    }

    pub fn rasterize(
        &mut self,
        font: &FontSource,
        glyph: u16,
        options: RasterOptions<'_>,
        limits: CacheLimits,
        #[cfg_attr(all(not(feature = "swash"), target_os = "linux"), allow(unused_mut))]
        mut reserve: impl FnMut(usize) -> Result<(), GlyphError>,
    ) -> Result<Image, GlyphError> {
        // COLRv1 and OpenType-SVG glyphs have no outline the platform backends
        // can return, so they are decoded here first. A glyph that carries one
        // of those representations never falls back to its monochrome outline.
        #[cfg(any(feature = "swash", feature = "colrv1", feature = "svg"))]
        if let FontSource::Bytes(data) = font
            && let Some(rendered) = crate::color::render(data, glyph, options, limits, &mut reserve)
        {
            return rendered;
        }
        // Platform backends rasterize through the registered system rasterizer,
        // for handles and for shared bytes alike: on Windows and macOS the
        // outline comes from GDI/CoreGraphics, not from the Rust rasterizer.
        #[cfg(all(
            not(feature = "swash"),
            any(target_os = "windows", target_os = "macos")
        ))]
        {
            let handle = match font {
                FontSource::System(system) => *system,
                FontSource::Bytes(data) => crate::system::shared_face(platform_backend(), data),
            };
            crate::system::rasterize(handle, glyph, options, limits, &mut reserve)
        }
        #[cfg(any(feature = "swash", target_os = "linux"))]
        {
            let FontSource::Bytes(data) = font else {
                let system = font.system().expect("system font source");
                return crate::system::rasterize(system, glyph, options, limits, &mut reserve);
            };
            #[cfg(all(not(feature = "swash"), target_os = "linux"))]
            {
                let ft = self.ft.as_mut().ok_or(GlyphError::UnsupportedGlyph)?;
                ft.rasterize(data, glyph, options, limits, reserve)
            }
            #[cfg(feature = "swash")]
            {
                let rasterizer = self.swash.as_mut().ok_or(GlyphError::UnsupportedGlyph)?;
                rasterizer.rasterize(data, glyph, options, limits, reserve)
            }
        }
    }

    pub fn release_scratch(&mut self) {
        #[cfg(feature = "swash")]
        {
            if let Some(swash) = &mut self.swash {
                swash.clear();
            }
        }
        #[cfg(all(not(feature = "swash"), target_os = "linux"))]
        {
            if let Some(ft) = &mut self.ft {
                ft.clear();
            }
        }
    }
}
