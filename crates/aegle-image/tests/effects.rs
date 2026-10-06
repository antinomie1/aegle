//! Gradient and shadow images: stop interpolation, geometry and input bounds.
#![cfg(feature = "effects")]
use aegle_image::effects::{Stop, linear_gradient, radial_gradient, shadow};
use aegle_scene::SceneError;
use aegle_types::Color;

fn stops() -> [Stop; 2] {
    [
        Stop {
            offset: 0.0,
            color: Color::BLACK,
        },
        Stop {
            offset: 1.0,
            color: Color::WHITE,
        },
    ]
}

fn pixel(image: &aegle_scene::Image, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * image.width() + x) * 4) as usize;
    image.pixels()[i..i + 4].try_into().unwrap()
}

#[test]
fn gradients_and_shadows_fill_images_from_validated_input() {
    let across = linear_gradient(64, 8, 0.0, &stops()).unwrap();
    let (left, middle, right) = (
        pixel(&across, 0, 4),
        pixel(&across, 32, 4),
        pixel(&across, 63, 4),
    );
    assert!(left[0] < 40 && right[0] > 240 && left[3] == 255);
    // Midpoint in linear light is brighter than the sRGB average.
    assert!(middle[0] > 128 + 20, "{middle:?}");
    assert_eq!(
        pixel(&across, 10, 0),
        pixel(&across, 10, 7),
        "constant down a column"
    );
    let down = linear_gradient(8, 64, std::f32::consts::FRAC_PI_2, &stops()).unwrap();
    assert!(pixel(&down, 4, 0)[0] < pixel(&down, 4, 63)[0]);
    let radial = radial_gradient(33, 33, &stops()).unwrap();
    assert!(pixel(&radial, 16, 16)[0] < 8 && pixel(&radial, 0, 0)[0] == 255);
    // A transparent stop interpolates alpha without darkening the color.
    let fade = linear_gradient(
        32,
        4,
        0.0,
        &[
            Stop {
                offset: 0.0,
                color: Color::rgba(255, 0, 0, 0),
            },
            Stop {
                offset: 1.0,
                color: Color::rgba(255, 0, 0, 255),
            },
        ],
    )
    .unwrap();
    let half = pixel(&fade, 16, 1);
    assert!(half[0] > 250 && (110..146).contains(&half[3]), "{half:?}");

    let (soft, margin) = shadow(20.0, 10.0, 4.0, 8.0, Color::rgba(0, 0, 0, 200)).unwrap();
    assert_eq!((soft.width(), soft.height(), margin), (44, 34, 12));
    let center = pixel(&soft, 22, 17)[3];
    let edge = pixel(&soft, margin, 17)[3];
    let outside = pixel(&soft, 1, 17)[3];
    assert!(
        center > 150 && (60..140).contains(&edge) && outside < 10,
        "{center} {edge} {outside}"
    );

    let one_stop = &stops()[..1];
    assert_eq!(
        linear_gradient(4, 4, 0.0, one_stop).unwrap_err(),
        SceneError::InvalidImage
    );
    let unordered = [stops()[1], stops()[0]];
    assert!(radial_gradient(4, 4, &unordered).is_err());
    assert!(linear_gradient(4, 4, f32::NAN, &stops()).is_err());
    assert!(linear_gradient(0, 4, 0.0, &stops()).is_err());
    assert!(
        linear_gradient(16_384, 16_384, 0.0, &stops()).is_err(),
        "over the byte budget"
    );
    assert!(shadow(10.0, 10.0, 0.0, 0.0, Color::BLACK).is_err());
}
