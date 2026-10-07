//! Each enabled decoder yields straight sRGB RGBA8 within the caller's budget.
use std::io::Cursor;

use aegle_image::{Error, decode, decode_with_limit};
use image::{DynamicImage, ImageFormat, RgbaImage};

fn encode(image: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, format).unwrap();
    out.into_inner()
}

fn pixel(image: &aegle_scene::Image, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * image.width() + x) * 4) as usize;
    image.pixels()[i..i + 4].try_into().unwrap()
}

/// 16 × 8: red left half, blue right half, so lossy codecs stay close.
fn halves() -> RgbaImage {
    RgbaImage::from_fn(16, 8, |x, _| {
        if x < 8 {
            image::Rgba([255, 0, 0, 255])
        } else {
            image::Rgba([0, 0, 255, 255])
        }
    })
}

#[cfg(any(feature = "jpeg", feature = "gif"))]
fn near(a: [u8; 4], b: [u8; 4], tolerance: u8) -> bool {
    a.iter().zip(&b).all(|(a, b)| a.abs_diff(*b) <= tolerance)
}

#[test]
fn formats_decode_by_signature_within_budgets() {
    let source = DynamicImage::ImageRgba8(halves());
    let red = [255, 0, 0, 255];
    let blue = [0, 0, 255, 255];
    let png = decode(&encode(&source, ImageFormat::Png)).unwrap();
    assert_eq!((pixel(&png, 3, 3), pixel(&png, 12, 3)), (red, blue));
    #[cfg(feature = "jpeg")]
    {
        let rgb = DynamicImage::ImageRgb8(image::RgbImage::from_fn(16, 8, |x, _| {
            if x < 8 {
                image::Rgb([255, 0, 0])
            } else {
                image::Rgb([0, 0, 255])
            }
        }));
        let jpeg = decode(&encode(&rgb, ImageFormat::Jpeg)).unwrap();
        assert_eq!((jpeg.width(), jpeg.height()), (16, 8));
        assert!(near(pixel(&jpeg, 2, 4), red, 40) && near(pixel(&jpeg, 13, 4), blue, 40));
    }
    #[cfg(feature = "webp")]
    {
        let webp = decode(&encode(&source, ImageFormat::WebP)).unwrap();
        assert_eq!((pixel(&webp, 2, 4), pixel(&webp, 13, 4)), (red, blue));
        let opaque = DynamicImage::ImageRgb8(source.to_rgb8());
        let webp = decode(&encode(&opaque, ImageFormat::WebP)).unwrap();
        assert_eq!(pixel(&webp, 2, 4), red, "RGB data gains an opaque alpha");
    }
    #[cfg(feature = "gif")]
    {
        let gif = decode(&encode(&source, ImageFormat::Gif)).unwrap();
        assert!(near(pixel(&gif, 2, 4), red, 8) && near(pixel(&gif, 13, 4), blue, 8));
    }
    let bytes = encode(&source, ImageFormat::Png);
    assert!(decode_with_limit(&bytes, 16 * 8 * 4).is_ok());
    assert_eq!(
        decode_with_limit(&bytes, 16 * 8 * 4 - 1).unwrap_err(),
        Error::TooLarge
    );
    assert_eq!(
        decode(&bytes[..bytes.len() / 2]).unwrap_err(),
        Error::Invalid
    );
    assert_eq!(decode(b"not an image").unwrap_err(), Error::Unsupported);
    #[cfg(feature = "gif")]
    {
        let gif = encode(&source, ImageFormat::Gif);
        assert_eq!(decode_with_limit(&gif, 16).unwrap_err(), Error::TooLarge);
        assert!(decode(&gif[..gif.len() / 2]).is_err());
    }
}

#[test]
#[cfg(feature = "svg")]
fn svg_rasterizes_at_the_requested_size() {
    use aegle_image::svg;
    let document = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
        <rect width="10" height="10" fill="#ff0000"/>
        <rect x="10" width="10" height="10" fill="#0000ff" fill-opacity="0.5"/>
    </svg>"##;
    assert_eq!(svg::size(document).unwrap(), (20.0, 10.0));
    let image = svg::rasterize(document, 40, 20).unwrap();
    assert_eq!((image.width(), image.height()), (40, 20));
    assert_eq!(pixel(&image, 5, 10), [255, 0, 0, 255]);
    let half = pixel(&image, 35, 10);
    assert!(
        half[2] > 250 && half[0] < 5 && (120..136).contains(&half[3]),
        "straight alpha: {half:?}"
    );
    assert_eq!(
        svg::rasterize(document, 0, 20).unwrap_err(),
        Error::TooLarge
    );
    assert_eq!(svg::rasterize(b"<svg", 4, 4).unwrap_err(), Error::Invalid);
}
