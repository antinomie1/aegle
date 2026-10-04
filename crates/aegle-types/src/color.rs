/// An unpremultiplied sRGB color packed as `0xRRGGBBAA`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Color(pub u32);

impl Color {
    /// Transparent black.
    pub const TRANSPARENT: Self = Self(0);
    /// Opaque black.
    pub const BLACK: Self = Self(0x000000ff);
    /// Opaque white.
    pub const WHITE: Self = Self(0xffffffff);

    /// Creates an opaque color from byte channels.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 255)
    }

    /// Creates a color from byte channels.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self(u32::from_be_bytes([r, g, b, a]))
    }

    /// Returns unpremultiplied sRGB red, green, blue and alpha bytes.
    pub const fn to_rgba(self) -> [u8; 4] {
        self.0.to_be_bytes()
    }
}
