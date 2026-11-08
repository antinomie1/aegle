//! Software presentation: the host draws into one of two BGRA8 buffers, which
//! is shown as an opaque `CGImage` by the view's `drawRect:`.
//!
//! The image reads its buffer in place through a data provider that holds a
//! second reference to it, so nothing is copied per frame. The buffers
//! alternate: the host draws into the one CoreGraphics has let go of, after
//! the area that changed since it was last shown is copied over from the
//! other, so an animation's small damage costs a small copy.
#![allow(unsafe_code)]
use crate::native::Native;
use aegle_platform::{DrawResult, Error, PixelRect, PixelSize, Region, Result};
use objc2_app_kit::NSGraphicsContext;
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGContext, CGDataProvider, CGImage,
    CGImageAlphaInfo, CGImageByteOrderInfo, CGInterpolationQuality, kCGColorSpaceSRGB,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
use std::{ffi::c_void, ptr::NonNull, sync::Arc, time::Instant};

/// One frame of pixels. CoreGraphics holds a second reference while an image
/// may read it, so the buffer is written only while this one is unique.
struct Buffer {
    pixels: Arc<[u8]>,
    size: PixelSize,
    /// What the shown frame changed since this buffer held it; `None` when
    /// unknown, so all of it is copied.
    behind: Option<Region<PixelRect>>,
}

/// The retained software framebuffers of a window.
#[derive(Default)]
pub(crate) struct Frame {
    /// The buffer of the frame on screen.
    shown: Option<Buffer>,
    /// The other buffer, which the next frame is drawn into.
    spare: Option<Buffer>,
    /// Size of the complete frame `shown` holds, if the last draw succeeded.
    pub drawn: Option<PixelSize>,
    /// The latest frame as the system draws it, with its scale.
    pub image: Option<(CFRetained<CGImage>, f64)>,
}

/// Releases the buffer reference a data provider held.
unsafe extern "C-unwind" fn release(info: *mut c_void, _data: NonNull<c_void>, _size: usize) {
    // SAFETY: `info` is the boxed reference `image` leaked for this provider,
    // which CoreGraphics releases exactly once.
    drop(unsafe { Box::from_raw(info.cast::<Arc<[u8]>>()) });
}

/// An image reading `pixels` in place, whose alpha byte is ignored: the
/// window is opaque, so a translucent pixel shows its premultiplied color as
/// if over black.
fn image(pixels: &Arc<[u8]>, size: PixelSize) -> Result<CFRetained<CGImage>> {
    let held = Box::into_raw(Box::new(pixels.clone()));
    // SAFETY: the provider reads `pixels.len()` bytes that the leaked
    // reference keeps alive and unwritten (writes need a unique `Arc`) until
    // `release` returns it; on failure the reference is reclaimed here.
    let provider = unsafe {
        CGDataProvider::with_data(
            held.cast(),
            pixels.as_ptr().cast(),
            pixels.len(),
            Some(release),
        )
    }
    .ok_or_else(|| {
        // SAFETY: CoreGraphics did not take the reference.
        drop(unsafe { Box::from_raw(held) });
        Error::backend("CGDataProvider creation failed")
    })?;
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

/// Copies the rectangles of `area` from `from` to `to`, both full frames of
/// `width` pixels per row.
fn copy(area: &Region<PixelRect>, from: &[u8], to: &mut [u8], width: u32) {
    let stride = width as usize * 4;
    for rect in area.rects() {
        let (left, columns) = (rect.x as usize * 4, rect.width as usize * 4);
        for row in rect.y as usize..(rect.y + rect.height) as usize {
            let at = row * stride + left;
            to[at..at + columns].copy_from_slice(&from[at..at + columns]);
        }
    }
}

impl Native {
    /// Lends a tightly packed, top-down premultiplied-sRGB BGRA8 framebuffer.
    /// `damage` is the area changed since the last frame (`None`: all of it);
    /// `draw` must overwrite the region it is given, which is the whole buffer
    /// after a resize or failed frame. Drawing errors retain the last displayed
    /// frame. `buffer_budget` bounds one frame; the shown buffer, which
    /// CoreGraphics reads, and compositor storage are outside it.
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
            let frame = &mut *frame;
            let region = match frame.drawn.take() {
                Some(drawn) if drawn == size => damage.unwrap_or(full),
                _ => full,
            };
            // The spare, if it fits and CoreGraphics has let go of it; else a
            // new buffer, after freeing one of another size.
            // No weak references exist, so one strong reference is unique.
            let spare = (frame.spare.take())
                .filter(|spare| spare.size == size && Arc::strong_count(&spare.pixels) == 1);
            let mut target = match spare {
                Some(spare) => spare,
                None => Buffer {
                    pixels: std::iter::repeat_n(0, length).collect(),
                    size,
                    behind: None,
                },
            };
            let pixels = Arc::get_mut(&mut target.pixels).expect("written only while unique");
            if region != full
                && let Some(shown) = &frame.shown
            {
                match &target.behind {
                    Some(behind) => copy(behind, &shown.pixels, pixels, size.width),
                    None => pixels.copy_from_slice(&shown.pixels),
                }
            }
            if let Err(error) = draw(pixels, size, &region) {
                target.behind = None;
                frame.spare = Some(target);
                return Err(Error::Draw(error));
            }
            frame.image = Some((image(&target.pixels, size)?, f64::from(info.scale)));
            target.behind = Some(Region::default());
            if let Some(mut previous) = frame.shown.replace(target) {
                previous.behind = Some(region);
                frame.spare = Some(previous);
            }
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
