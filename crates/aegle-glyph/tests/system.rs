//! System font handles dispatch to the rasterizer registered for their backend.

use aegle_glyph::{
    Blob, CacheLimits, Content, FontData, FontSource, GlyphCache, GlyphError, Image, Placement,
    RasterOptions, SystemFont, SystemRasterizer, register_system_rasterizer,
    system_rasterizer_registered,
};

/// Fills a 3×2 mask with the requested glyph id and reports the options it saw.
struct Fake {
    backend: u8,
    last: std::sync::Arc<std::sync::Mutex<Option<(u16, f32)>>>,
}

impl SystemRasterizer for Fake {
    fn rasterize(
        &mut self,
        font: &SystemFont,
        glyph: u16,
        options: RasterOptions<'_>,
        _limits: CacheLimits,
        reserve: &mut dyn FnMut(usize) -> Result<(), GlyphError>,
    ) -> Result<Image, GlyphError> {
        if font.backend != self.backend {
            return Err(GlyphError::UnsupportedGlyph);
        }
        *self.last.lock().unwrap() = Some((glyph, options.size));
        reserve(6)?;
        Ok(Image {
            placement: Placement {
                left: 1,
                top: 2,
                width: 3,
                height: 2,
            },
            content: Content::Mask,
            data: vec![glyph as u8; 6].into_boxed_slice(),
        })
    }
}

#[test]
fn system_handles_dispatch_to_their_backend_only() {
    let last = std::sync::Arc::new(std::sync::Mutex::new(None));
    assert!(register_system_rasterizer(
        251,
        Box::new(Fake {
            backend: 251,
            last: last.clone(),
        })
    ));
    assert!(system_rasterizer_registered(251));
    // One rasterizer per backend; a second registration is refused.
    assert!(!register_system_rasterizer(
        251,
        Box::new(Fake {
            backend: 251,
            last: last.clone(),
        })
    ));

    let mut cache = GlyphCache::with_limits(CacheLimits::default());
    let font = FontSource::System(SystemFont {
        backend: 251,
        id: 7,
    });
    let options = RasterOptions {
        size: 21.0,
        ..Default::default()
    };
    let image = cache.rasterize(&font, 42, options).expect("fake backend");
    assert_eq!(image.content, Content::Mask);
    assert_eq!(
        image.placement,
        Placement {
            left: 1,
            top: 2,
            width: 3,
            height: 2,
        }
    );
    assert_eq!(image.data, [42; 6]);
    assert_eq!(*last.lock().unwrap(), Some((42, 21.0)));

    // A hit is served from the cache, not the backend.
    *last.lock().unwrap() = None;
    cache.rasterize(&font, 42, options).expect("cached");
    assert_eq!(*last.lock().unwrap(), None);

    // Another face on the same backend is a different key and reaches it.
    cache
        .rasterize(
            &FontSource::System(SystemFont {
                backend: 251,
                id: 8,
            }),
            42,
            options,
        )
        .expect("second face");
    assert_eq!(*last.lock().unwrap(), Some((42, 21.0)));

    // No registered backend is unsupported; it never falls back to bytes.
    let unknown = FontSource::System(SystemFont {
        backend: 252,
        id: 7,
    });
    assert_eq!(
        cache.rasterize(&unknown, 42, options).unwrap_err(),
        GlyphError::UnsupportedGlyph
    );
    assert!(!system_rasterizer_registered(252));
}

#[test]
fn shared_bytes_reach_the_registered_backend_as_one_handle() {
    // A second fake backend so this test does not depend on the first one.
    struct Bytes(usize);
    impl SystemRasterizer for Bytes {
        fn rasterize(
            &mut self,
            font: &SystemFont,
            glyph: u16,
            _: RasterOptions<'_>,
            _: CacheLimits,
            reserve: &mut dyn FnMut(usize) -> Result<(), GlyphError>,
        ) -> Result<Image, GlyphError> {
            let source = aegle_glyph::system_face(*font).ok_or(GlyphError::InvalidFont)?;
            match source {
                aegle_glyph::FaceSource::Bytes { data, index } => {
                    self.0 += 1;
                    assert_eq!(index, 0);
                    assert_eq!(data.len(), 32);
                    reserve(1)?;
                    Ok(Image {
                        placement: Placement::default(),
                        content: Content::Mask,
                        data: vec![glyph as u8].into_boxed_slice(),
                    })
                }
                aegle_glyph::FaceSource::File { .. } => Err(GlyphError::UnsupportedGlyph),
            }
        }
    }
    assert!(register_system_rasterizer(250, Box::new(Bytes(0))));

    let font = FontData::new(Blob::from(vec![7u8; 32]), 0);
    let first = aegle_glyph::shared_face(250, &font);
    let again = aegle_glyph::shared_face(250, &font);
    assert_eq!(first, again, "one handle per blob and index");

    let mut cache = GlyphCache::with_limits(CacheLimits::default());
    let image = cache
        .rasterize(&FontSource::System(first), 9, RasterOptions::default())
        .expect("shared bytes rasterize");
    assert_eq!(image.data, [9]);
    // A second face index of the same blob is a distinct handle.
    let other = aegle_glyph::shared_face(250, &FontData::new(Blob::from(vec![7u8; 32]), 1));
    assert_ne!(other, first);
}
