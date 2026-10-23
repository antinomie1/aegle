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

    /// The same color with alpha `alpha` (0–255).
    pub const fn with_alpha(self, alpha: u8) -> Self {
        Self(self.0 & 0xffffff00 | alpha as u32)
    }

    /// This color painted over `base` (source-over, in sRGB as design tools
    /// blend). A translucent state overlay such as a hover tint thus darkens or
    /// lightens any fill beneath it; over a transparent base the result is
    /// this color itself.
    pub fn over(self, base: Self) -> Self {
        let [r, g, b, a] = self.to_rgba().map(f32::from);
        let [br, bg, bb, ba] = base.to_rgba().map(f32::from);
        let (a, ba) = (a / 255.0, ba / 255.0);
        let alpha = a + ba * (1.0 - a);
        if alpha == 0.0 {
            return Self::TRANSPARENT;
        }
        let channel = |top: f32, bottom: f32| byte((top * a + bottom * ba * (1.0 - a)) / alpha);
        Self::rgba(
            channel(r, br),
            channel(g, bg),
            channel(b, bb),
            byte(alpha * 255.0),
        )
    }

    /// The sRGB mix `t` of the way from this color to `other`, alpha
    /// included; `t` is clamped to 0–1.
    pub fn mix(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let [a, b] = [self, other].map(|c| c.to_rgba().map(f32::from));
        Self::rgba(
            byte(a[0] + (b[0] - a[0]) * t),
            byte(a[1] + (b[1] - a[1]) * t),
            byte(a[2] + (b[2] - a[2]) * t),
            byte(a[3] + (b[3] - a[3]) * t),
        )
    }
}

/// Rounds a 0–255 channel to a byte.
fn byte(value: f32) -> u8 {
    (value + 0.5) as u8
}
