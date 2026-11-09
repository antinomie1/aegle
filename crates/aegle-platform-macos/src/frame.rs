//! Software presentation: the host draws into one tightly packed BGRA8
//! canvas, and the view's `drawRect:` draws the areas AppKit asks for from it.
//!
//! The view's layer keeps a backing store that Core Animation renders with
//! the GPU and redraws only where the view was marked, so a frame marks its
//! damage and each rectangle being drawn becomes a small image copied from
//! the canvas. An image is uploaded whole wherever it is drawn: one the size
//! of the window per frame cost an upload of every pixel each time, while a
//! spinner's rectangle costs a few kilobytes. Core Animation may read an
//! image after `drawRect:` returns, so each owns its copy rather than
//! reading the canvas, which the next frame overwrites.
#![allow(unsafe_code)]
use crate::native::Native;
use aegle_platform::{DrawResult, Error, PixelRect, PixelSize, Region, Result};
use objc2_app_kit::NSGraphicsContext;
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGContext, CGDataProvider, CGImage,
    CGImageAlphaInfo, CGImageByteOrderInfo, CGInterpolationQuality, kCGColorSpaceSRGB,
};
use objc2_foundation::{MainThreadMarker, NSInteger, NSPoint, NSRect, NSSize};
use std::{ffi::c_void, ptr::NonNull, time::Instant};

/// The retained software frame of a window.
#[derive(Default)]
pub(crate) struct Frame {
    /// The latest complete frame, which the host draws into.
    canvas: Vec<u8>,
    /// Size and scale of the complete frame `canvas` holds, if the last draw
    /// succeeded.
    drawn: Option<(PixelSize, f64)>,
}

/// Releases the pixels a data provider held.
unsafe extern "C-unwind" fn release(info: *mut c_void, _data: NonNull<c_void>, _size: usize) {
    // SAFETY: `info` is the boxed copy `image` leaked for this provider, which
    // CoreGraphics releases exactly once.
    drop(unsafe { Box::from_raw(info.cast::<Vec<u8>>()) });
}

/// An image of `rect` of the canvas, a frame `width` pixels wide, that owns a
/// copy of its pixels. The alpha byte is ignored: the window is opaque, so a
/// translucent pixel shows its premultiplied color as if over black.
fn image(canvas: &[u8], width: u32, rect: PixelRect) -> Result<CFRetained<CGImage>> {
    let (stride, row) = (width as usize * 4, rect.width as usize * 4);
    let mut pixels = Vec::with_capacity(row * rect.height as usize);
    for y in rect.y as usize..(rect.y + rect.height) as usize {
        let at = y * stride + rect.x as usize * 4;
        pixels.extend_from_slice(&canvas[at..at + row]);
    }
    let length = pixels.len();
    let held = Box::into_raw(Box::new(pixels));
    // SAFETY: the provider reads the `length` bytes of the leaked copy, which
    // nothing else touches until `release` frees it; on failure the copy is
    // reclaimed here.
    let provider = unsafe {
        CGDataProvider::with_data(held.cast(), (*held).as_ptr().cast(), length, Some(release))
    }
    .ok_or_else(|| {
        // SAFETY: CoreGraphics did not take the copy.
        drop(unsafe { Box::from_raw(held) });
        Error::backend("CGDataProvider creation failed")
    })?;
    // SAFETY: a static CoreGraphics constant.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))
        .ok_or_else(|| Error::backend("sRGB color space unavailable"))?;
    let bitmap =
        CGBitmapInfo(CGImageAlphaInfo::NoneSkipFirst.0 | CGImageByteOrderInfo::Order32Little.0);
    // SAFETY: the provider holds exactly `height` rows of `row` bytes, a
    // 32-bit little-endian skip-first layout; no decode array is passed.
    unsafe {
        CGImage::new(
            rect.width as usize,
            rect.height as usize,
            8,
            32,
            row,
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
    /// Lends a tightly packed, top-down premultiplied-sRGB BGRA8 framebuffer.
    /// `damage` is the area changed since the last frame (`None`: all of it);
    /// `draw` must overwrite the region it is given, which is the whole buffer
    /// after a resize or failed frame. Drawing errors retain the last displayed
    /// frame. `buffer_budget` bounds the canvas; Core Animation's backing
    /// store is compositor storage outside it.
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
        let scale = f64::from(info.scale);
        let region = {
            let mut frame = self.frame.borrow_mut();
            let mut full = Region::default();
            full.add(PixelRect::full(size.width, size.height));
            let region = match frame.drawn.take() {
                Some(drawn) if drawn == (size, scale) => damage.unwrap_or(full),
                _ => full,
            };
            if frame.canvas.len() != length {
                // Free the old canvas first, so a resize never holds both.
                frame.canvas = Vec::new();
                frame.canvas.resize(length, 0);
            }
            draw(&mut frame.canvas, size, &region).map_err(Error::Draw)?;
            frame.drawn = Some((size, scale));
            region
        };
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

    /// Draws the rectangles AppKit is redrawing from the latest frame,
    /// anchored at the top left at the frame's own scale, so a resize shows
    /// the old frame unscaled until the host redraws.
    pub(crate) fn draw_rect(&self) -> Result<()> {
        let frame = self.frame.borrow();
        let Some((size, scale)) = frame.drawn else {
            return Ok(());
        };
        let Some(context) = NSGraphicsContext::currentContext() else {
            return Ok(());
        };
        let context = context.CGContext();
        CGContext::set_interpolation_quality(Some(&context), CGInterpolationQuality::None);
        let top = self.view.bounds().size.height;
        let (mut rects, mut count): (*const NSRect, NSInteger) = (std::ptr::null(), 0);
        // SAFETY: AppKit writes a list it owns for the duration of this draw.
        unsafe { self.view.getRectsBeingDrawn_count(&mut rects, &mut count) };
        let rects = match rects.is_null() {
            true => &[][..],
            // SAFETY: `count` rectangles at `rects`, valid while drawing.
            false => unsafe { std::slice::from_raw_parts(rects, count.max(0) as usize) },
        };
        for rect in rects {
            // The dirty rectangle in frame pixels, rounded outward.
            let logical = aegle_platform::Rect::new(
                rect.origin.x as f32,
                (top - rect.origin.y - rect.size.height) as f32,
                rect.size.width as f32,
                rect.size.height as f32,
            );
            let pixels = PixelRect::covering(logical, scale as f32, size.width, size.height);
            if pixels.width == 0 || pixels.height == 0 {
                continue;
            }
            let image = image(&frame.canvas, size.width, pixels)?;
            let at = CGRect::new(
                CGPoint::new(
                    f64::from(pixels.x) / scale,
                    top - f64::from(pixels.y + pixels.height) / scale,
                ),
                CGSize::new(
                    f64::from(pixels.width) / scale,
                    f64::from(pixels.height) / scale,
                ),
            );
            CGContext::draw_image(Some(&context), at, Some(&image));
        }
        Ok(())
    }
}
