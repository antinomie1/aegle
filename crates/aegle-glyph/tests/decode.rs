//! PNG decoding covers every color type, enforces limits and rejects bad input.
use aegle_glyph::{DecodeError, decode_png, decode_png_with_limit};

/// Encodes `data` as a PNG with the given color type and depth.
fn encode(
    size: [u32; 2],
    color: png::ColorType,
    depth: png::BitDepth,
    palette: Option<(&[u8], &[u8])>,
    data: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, size[0], size[1]);
    encoder.set_color(color);
    encoder.set_depth(depth);
    if let Some((palette, alpha)) = palette {
        encoder.set_palette(palette.to_vec());
        encoder.set_trns(alpha.to_vec());
    }
    encoder
        .write_header()
        .unwrap()
        .write_image_data(data)
        .unwrap();
    out
}

#[test]
fn every_color_type_becomes_straight_rgba_and_bad_input_is_rejected() {
    use png::{BitDepth::*, ColorType::*};
    let size = [2, 1];
    let cases: [(&str, Vec<u8>, [u8; 8]); 6] = [
        (
            "rgba",
            encode(size, Rgba, Eight, None, &[10, 20, 30, 40, 50, 60, 70, 80]),
            [10, 20, 30, 40, 50, 60, 70, 80],
        ),
        (
            "rgb",
            encode(size, Rgb, Eight, None, &[1, 2, 3, 4, 5, 6]),
            [1, 2, 3, 255, 4, 5, 6, 255],
        ),
        (
            "gray",
            encode(size, Grayscale, Eight, None, &[9, 200]),
            [9, 9, 9, 255, 200, 200, 200, 255],
        ),
        (
            "gray+alpha",
            encode(size, GrayscaleAlpha, Eight, None, &[9, 100, 200, 0]),
            [9, 9, 9, 100, 200, 200, 200, 0],
        ),
        (
            // Palette entry 1 is transparent through tRNS.
            "indexed",
            encode(
                size,
                Indexed,
                Eight,
                Some((&[255, 0, 0, 0, 0, 255], &[255, 0])),
                &[0, 1],
            ),
            [255, 0, 0, 255, 0, 0, 255, 0],
        ),
        (
            // Sixteen-bit channels keep their high byte.
            "rgb16",
            encode(
                size,
                Rgb,
                Sixteen,
                None,
                &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 1, 2, 3, 4, 5, 6],
            ),
            [0x12, 0x56, 0x9a, 255, 1, 3, 5, 255],
        ),
    ];
    for (name, bytes, expected) in &cases {
        let image = decode_png(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!((image.width, image.height), (2, 1), "{name}");
        assert_eq!(image.pixels, expected, "{name}");
    }

    let png = &cases[0].1;
    assert_eq!(decode_png(b"GIF89a....").unwrap_err(), DecodeError::NotPng);
    assert_eq!(decode_png(&[]).unwrap_err(), DecodeError::NotPng);
    assert_eq!(
        decode_png(&png[..png.len() - 20]).unwrap_err(),
        DecodeError::Invalid
    );
    assert_eq!(decode_png(&png[..12]).unwrap_err(), DecodeError::Invalid);

    // The limit is checked from the header: 2 × 1 RGBA needs 8 bytes.
    assert!(decode_png_with_limit(png, 8).is_ok());
    assert_eq!(
        decode_png_with_limit(png, 7).unwrap_err(),
        DecodeError::TooLarge
    );
    let wide = encode([16_385, 1], Grayscale, Eight, None, &vec![0; 16_385]);
    assert_eq!(decode_png(&wide).unwrap_err(), DecodeError::TooLarge);
}

#[cfg(feature = "scene")]
#[test]
fn decoded_pixels_become_a_scene_image() {
    let bytes = encode(
        [1, 1],
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        None,
        &[1, 2, 3, 4],
    );
    let image = aegle_glyph::decode_image(&bytes).unwrap();
    assert_eq!(
        (image.width(), image.height(), image.pixels()),
        (1, 1, &[1, 2, 3, 4][..])
    );
}
