use crate::{FontData, GlyphError, RasterOptions};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Identity {
    blob: u64,
    index: u32,
    glyph: u16,
    size: u32,
    offset: [u32; 2],
    hint: bool,
    foreground: [u8; 4],
    embolden: bool,
    skew: i8,
}

/// Allocation-free raster identity shared by CPU caches and GPU atlases.
///
/// Equality includes all parameters and variation coordinates, not just a hash.
/// Font bytes are not retained; each blob ID must name immutable bytes for its
/// entire lifetime. A key validates raster options, not the font file itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey<'a> {
    identity: Identity,
    coords: &'a [i16],
}

impl<'a> GlyphKey<'a> {
    /// Borrows the variation coordinates and validates raster parameters.
    /// Font parsing and glyph-index validation happen on a raster cache miss.
    pub fn new(
        font: &FontData,
        glyph: u16,
        options: RasterOptions<'a>,
    ) -> Result<Self, GlyphError> {
        validate_size(options.size)?;
        if options
            .offset
            .iter()
            .any(|v| !v.is_finite() || !(0.0..1.0).contains(v))
            || !(-89..=89).contains(&options.skew)
            || options.normalized_coords.len() > 64
            || options
                .normalized_coords
                .iter()
                .any(|c| !(-16_384..=16_384).contains(c))
        {
            return Err(GlyphError::InvalidOptions);
        }
        Ok(Self {
            identity: Identity {
                blob: font.data.id(),
                index: font.index,
                glyph,
                size: options.size.to_bits(),
                offset: options
                    .offset
                    .map(|v| if v == 0.0 { 0 } else { v.to_bits() }),
                hint: options.hint,
                foreground: options.foreground,
                embolden: options.embolden,
                skew: options.skew,
            },
            coords: options.normalized_coords,
        })
    }

    /// Returns an identity independent of foreground color for grayscale masks.
    /// A cache must also check that the stored image is actually a mask; a color
    /// glyph with transparent-black foreground can have this same identity.
    pub fn mask(mut self) -> Self {
        self.identity.foreground = [0; 4];
        self
    }

    /// Copies only the variation coordinates to retain the key after this borrow.
    /// Empty coordinates require no allocation; font bytes are never copied.
    pub fn to_owned(self) -> OwnedGlyphKey {
        OwnedGlyphKey {
            identity: self.identity,
            coords: self.coords.into(),
        }
    }
}

/// Owned raster identity for a retained cache entry, without owning font bytes.
/// Its hash agrees with the borrowed identity returned by [`Self::as_key`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OwnedGlyphKey {
    identity: Identity,
    coords: Box<[i16]>,
}

impl OwnedGlyphKey {
    /// Borrows this entry's identity without allocating or revalidating it.
    pub fn as_key(&self) -> GlyphKey<'_> {
        GlyphKey {
            identity: self.identity,
            coords: &self.coords,
        }
    }
}

pub(crate) fn validate_size(size: f32) -> Result<(), GlyphError> {
    if size.is_finite() && size > 0.0 && size <= 16_384.0 {
        Ok(())
    } else {
        Err(GlyphError::InvalidOptions)
    }
}
