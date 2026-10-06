use aegle_types::Color;

use crate::RenderError;

/// A borrowed, tightly packed premultiplied sRGB RGBA8 framebuffer.
///
/// Rows run top to bottom. Construction requires exactly `width * height * 4`
/// bytes. No window, allocation, stride padding or platform channel conversion
/// is hidden here; the platform presenter owns the memory and any conversion.
pub struct Surface<'a> {
    pub(crate) data: &'a mut [u8],
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl<'a> Surface<'a> {
    /// Borrows a nonempty framebuffer, validating size and multiplication once.
    /// Existing contents are replaced by [`crate::Renderer::begin_frame`].
    pub fn new(data: &'a mut [u8], width: u32, height: u32) -> Result<Self, RenderError> {
        let bytes = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4));
        if width == 0
            || height == 0
            || width > i32::MAX as u32
            || height > i32::MAX as u32
            || bytes != Some(data.len())
        {
            return Err(RenderError::SurfaceSize);
        }
        Ok(Self {
            data,
            width,
            height,
        })
    }

    /// Width in physical pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in physical pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Premultiplied sRGB RGBA8 bytes for presentation.
    pub fn data(&self) -> &[u8] {
        self.data
    }

    /// Clears the pixel rows `top..bottom`, columns `left..right`.
    pub(crate) fn clear(&mut self, color: Color, [left, top, right, bottom]: [usize; 4]) {
        let [r, g, b, a] = color.to_rgba();
        let premul = |c: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
        let pixel = [premul(r), premul(g), premul(b), a];
        let stride = self.width as usize * 4;
        for row in self.data.chunks_exact_mut(stride).take(bottom).skip(top) {
            for dst in row[left * 4..right * 4].chunks_exact_mut(4) {
                dst.copy_from_slice(&pixel);
            }
        }
    }
}
