//! Software presentation: the host draws into one bounded BGRA8 buffer, which
//! is shown as an opaque `CGImage` by the view's `drawRect:`.
#![allow(unsafe_code)]
use crate::native::Native;
use aegle_platform::{DrawResult, Error, PixelRect, PixelSize, Region, Result};
use objc2_app_kit::NSGraphicsContext;
use objc2_core_foundation::{CFData, CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGContext, CGDataProvider, CGImage,
    CGImageAlphaInfo, CGImageByteOrderInfo, CGInterpolationQuality, kCGColorSpaceSRGB,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
use std::time::Instant;

/// The retained software framebuffer of a window.
#[derive(Default)]
pub(crate) struct Frame {
    pub pixels: Vec<u8>,
    /// Size of the complete frame `pixels` holds, if any.
    pub drawn: Option<PixelSize>,
    /// The latest frame as the system draws it, with its scale.
    pub image: Option<(CFRetained<CGImage>, f64)>,
}

/// A snapshot of `pixels` as an image whose alpha byte is ignored: the
/// window is opaque, so a translucent pixel shows its premultiplied color as
/// if over black.
fn image(pixels: &[u8], size: PixelSize) -> Result<CFRetained<CGImage>> {
    let data = CFData::from_bytes(pixels);
    let provider = CGDataProvider::with_cf_data(Some(&data))
        .ok_or_else(|| Error::backend("CGDataProvider creation failed"))?;
    // SAFETY: a static CoreGraphics constant.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))
        .ok_or_else(|| Error::backend("sRGB color space unavailable"))?;
    let bitmap =
        CGBitmapInfo(CGImageAlphaInfo::NoneSkipFirst.0 | CGImageByteOrderInfo::Order32Little.0);
    let (width, height) = (size.width as usize, size.height as usize);
    // SAFETY: the provider holds exactly `height` rows of `width * 4` bytes, a
    // 32-bit little-endian skip-first layout; no decode array is passed.
    unsafe {
        CGImage::new(
            width,
            height,
            8,
            32,
            width * 4,
            Some(&space),
            bitmap,
            Some(&provider),
            std::ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
    .ok_or_else(|| Error::backend("CGImage creation failed"))
}

impl Native {
    /// Borrows a tightly packed, top-down premultiplied-sRGB BGRA8 framebuffer.
    /// `damage` is the area changed since the last frame (`None`: all of it);
    /// `draw` must overwrite the region it is given, which is the whole buffer
    /// after a resize or failed frame. Drawing errors retain the last displayed
    /// frame. One CPU buffer is reused; the image snapshot and compositor
    /// storage are outside `buffer_budget`.
    pub(crate) fn present(
        &self,
        mtm: MainThreadMarker,
        damage: Option<Region<PixelRect>>,
        draw: impl FnOnce(&mut [u8], PixelSize, &Region<PixelRect>) -> DrawResult,
    ) -> Result<bool> {
        self.redraw_queued.set(false);
        let info = self.info.get();
        if !info.configured {
            return Ok(false);
        }
        let size = info.buffer;
        let length = (size.width as usize)
            .checked_mul(size.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(Error::InvalidSize)?;
        if length > self.budget {
            return Err(Error::BufferBudget {
                required: length,
                budget: self.budget,
            });
        }
        let mut full = Region::default();
        full.add(PixelRect::full(size.width, size.height));
        let region = {
            let mut frame = self.frame.borrow_mut();
            if frame.pixels.capacity() < length {
                // Free an obsolete allocation first; a resize never holds two
                // full framebuffers against a single-buffer budget.
                frame.pixels = Vec::new();
                frame
                    .pixels
                    .try_reserve_exact(length)
                    .map_err(Error::backend)?;
            }
            frame.pixels.resize(length, 0);
            let region = match frame.drawn.take() {
                Some(drawn) if drawn == size => damage.unwrap_or(full),
                _ => full,
            };
            draw(&mut frame.pixels, size, &region).map_err(Error::Draw)?;
            let snapshot = image(&frame.pixels, size)?;
            frame.image = Some((snapshot, f64::from(info.scale)));
            frame.drawn = Some(size);
            region
        };
        let scale = f64::from(info.scale);
        let height = self.view.bounds().size.height;
        for rect in region.rects() {
            let (x, y) = (f64::from(rect.x) / scale, f64::from(rect.y) / scale);
            let (width, rows) = (
                f64::from(rect.width) / scale,
                f64::from(rect.height) / scale,
            );
            let area = NSRect::new(NSPoint::new(x, height - y - rows), NSSize::new(width, rows));
            self.view.setNeedsDisplayInRect(area);
        }
        self.show(mtm);
        // Draws synchronously, so the frame is on screen when this returns.
        self.view.displayIfNeeded();
        self.dirty.set(false);
        self.last_frame.set(Some(Instant::now()));
        Ok(true)
    }

    /// Draws the latest frame, anchored at the top left at its own scale so a
    /// resize shows the old frame unscaled until the host redraws.
    pub(crate) fn draw_rect(&self) {
        let frame = self.frame.borrow();
        let Some((image, scale)) = &frame.image else {
            return;
        };
        let Some(context) = NSGraphicsContext::currentContext() else {
            return;
        };
        let context = context.CGContext();
        let width = CGImage::width(Some(image)) as f64 / scale;
        let height = CGImage::height(Some(image)) as f64 / scale;
        let top = self.view.bounds().size.height;
        let rect = CGRect::new(CGPoint::new(0.0, top - height), CGSize::new(width, height));
        CGContext::set_interpolation_quality(Some(&context), CGInterpolationQuality::None);
        CGContext::draw_image(Some(&context), rect, Some(image));
    }
}
