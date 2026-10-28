//! Antialiased coverage through the system cairo library.
//!
//! Cairo supplies only coverage: paths are handed to cairo in their local
//! coordinates with the device transform set on the context, and cairo's image
//! backend rasterizes them into an `A8` mask that the shared compositor then
//! blends. Color, clips, layers, gradients, shadows, images and text sampling all
//! come from [`aegle_render_cpu`].
//!
//! A cairo `A8` image surface requires a stride that is a multiple of four, so
//! mask rows are padded to that. The compositor never reads the padding, and its
//! mask budget counts it.

#![allow(unsafe_code)]

use aegle_render_cpu::{Area, FillRule, Mask, Rasterizer, RenderError, Segment, outline};
use aegle_scene::{Affine, LineCap, LineJoin, Path, Point, RoundedRect, Stroke};

use crate::sys::{self, Cairo, Matrix};

/// Curve flattening tolerance in device pixels.
///
/// cairo's default of 0.1 leaves visible error on shallow arcs at UI radii, so a
/// tenth of that is requested: the chord error stays far below one coverage
/// level, and cairo's flattening is then as accurate as the shared outline's
/// cubic approximation allows.
const TOLERANCE: f64 = 0.01;

/// Reusable cairo path storage, the coverage rasterizer of this backend.
///
/// Outlines are recorded as segments and replayed into cairo when filled, because
/// a cairo path belongs to a context and a context belongs to one target. Finished
/// segment buffers are recycled.
#[derive(Default)]
pub struct CairoRaster {
    spare: Vec<Vec<Segment>>,
    /// Set once a cairo call reports an error, which aborts the frame.
    failed: Option<RenderError>,
}

impl CairoRaster {
    /// A rasterizer bound to the system cairo library.
    ///
    /// Fails with [`RenderError::BackendUnavailable`] when `libcairo.so.2` is not
    /// installed; nothing is loaded until the first use, so constructing this
    /// reports the same condition a first frame would.
    pub fn new() -> Result<Self, RenderError> {
        sys::require()?;
        Ok(Self::default())
    }

    /// The loaded cairo version, or `None` when the library is not installed.
    pub fn version() -> Option<&'static str> {
        sys::version()
    }
}

/// One device-space outline: segments in local coordinates plus what to do with
/// them, so cairo can replay it into whichever mask is being filled.
pub struct CairoPath {
    segments: Vec<Segment>,
    transform: Affine,
    stroke: Option<Stroke>,
    bounds: aegle_scene::Rect,
}

impl Rasterizer for CairoRaster {
    type Path = CairoPath;
    type Mask = CairoMask;

    fn shape(
        &mut self,
        shape: RoundedRect,
        stroke: Option<f32>,
        transform: Affine,
    ) -> Result<Self::Path, RenderError> {
        let mut segments = self.spare.pop().unwrap_or_default();
        segments.clear();
        outline::shape_segments(shape, stroke, |segment| segments.push(segment));
        // A shape's `stroke` width means "outline the ring of a centered border",
        // which the shared outline already expresses as a two-subpath ring filled
        // even-odd. Cairo must therefore fill it, never stroke it.
        finish(segments, transform, None)
    }

    fn path(
        &mut self,
        path: &Path,
        stroke: Option<Stroke>,
        transform: Affine,
    ) -> Result<Self::Path, RenderError> {
        let mut segments = self.spare.pop().unwrap_or_default();
        segments.clear();
        outline::path_segments(path, |segment| segments.push(segment));
        finish(segments, transform, stroke)
    }

    fn bounds(&self, path: &Self::Path) -> aegle_scene::Rect {
        path.bounds
    }

    fn fill(
        &mut self,
        path: &Self::Path,
        rule: FillRule,
        area: Area,
        coverage: &mut Self::Mask,
    ) -> Result<(), RenderError> {
        if let Some(error) = self.failed {
            return Err(error);
        }
        if let Err(error) = self.draw(path, rule, area, coverage) {
            // Cairo's context is in an error state now; every later fill would
            // fail the same way, so the frame is abandoned.
            self.failed = Some(error);
            return Err(error);
        }
        Ok(())
    }

    fn release(&mut self, mut path: Self::Path) {
        path.segments.clear();
        self.spare.push(path.segments);
    }
}

impl CairoRaster {
    /// Replays one outline into the mask's context and rasterizes it.
    fn draw(
        &mut self,
        path: &CairoPath,
        rule: FillRule,
        area: Area,
        coverage: &mut CairoMask,
    ) -> Result<(), RenderError> {
        let cairo = sys::require()?;
        let context = coverage.context(cairo)?;
        // SAFETY: `cairo` is the loaded library and `context` is a live cairo
        // context created from this mask's surface. Every call below is made in
        // the documented order, and `coverage` keeps the surface alive.
        unsafe {
            (cairo.save)(context);
            (cairo.reset_clip)(context);
            (cairo.set_antialias)(context, sys::ANTIALIAS_DEFAULT);
            (cairo.set_tolerance)(context, TOLERANCE);
            (cairo.set_operator)(context, sys::OPERATOR_SOURCE);
            (cairo.set_source_rgba)(context, 0.0, 0.0, 0.0, 1.0);
            // Whole pixels only: the compositor cleared them, and cairo writes
            // zero coverage as "leave the destination", which is already zero.
            (cairo.rectangle)(
                context,
                area.left as f64,
                area.top as f64,
                area.width() as f64,
                area.height() as f64,
            );
            (cairo.clip)(context);
            (cairo.new_path)(context);
            // The matrix must be current while segments are added: cairo bakes the
            // transform into the path as it is constructed.
            let [a, b, c, d, e, f] = path.transform.coefficients();
            let matrix = Matrix {
                xx: a as f64,
                yx: b as f64,
                xy: c as f64,
                yy: d as f64,
                x0: e as f64,
                y0: f as f64,
            };
            (cairo.set_matrix)(context, &matrix);
            replay(cairo, context, &path.segments);
            match path.stroke {
                None => {
                    (cairo.set_fill_rule)(
                        context,
                        match rule {
                            FillRule::NonZero => sys::FILL_RULE_WINDING,
                            FillRule::EvenOdd => sys::FILL_RULE_EVEN_ODD,
                        },
                    );
                    (cairo.fill)(context);
                }
                Some(stroke) => {
                    (cairo.set_line_width)(context, stroke.width as f64);
                    (cairo.set_miter_limit)(context, 4.0);
                    (cairo.set_line_cap)(
                        context,
                        match stroke.cap {
                            LineCap::Butt => sys::LINE_CAP_BUTT,
                            LineCap::Round => sys::LINE_CAP_ROUND,
                            LineCap::Square => sys::LINE_CAP_SQUARE,
                        },
                    );
                    (cairo.set_line_join)(
                        context,
                        match stroke.join {
                            LineJoin::Miter => sys::LINE_JOIN_MITER,
                            LineJoin::Round => sys::LINE_JOIN_ROUND,
                            LineJoin::Bevel => sys::LINE_JOIN_BEVEL,
                        },
                    );
                    (cairo.stroke)(context);
                }
            }
            (cairo.restore)(context);
            let status = (cairo.status)(context);
            if status != sys::STATUS_SUCCESS {
                return Err(status_error(status));
            }
        }
        coverage.mark_dirty(cairo);
        Ok(())
    }
}

/// A cairo `A8` image surface over one reusable coverage buffer.
///
/// Row stride is the mask width rounded up to a multiple of four, as cairo
/// requires. The context is created once and reused across fills.
pub struct CairoMask {
    data: Vec<u8>,
    stride: usize,
    width: u32,
    height: u32,
    surface: *mut std::os::raw::c_void,
    context: *mut std::os::raw::c_void,
}

impl Mask for CairoMask {
    fn bytes_for(width: u32, height: u32) -> usize {
        // Rows are padded to a four-byte multiple, as cairo requires.
        let stride = width as usize + (4 - width as usize % 4) % 4;
        stride.saturating_mul(height as usize)
    }

    fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        let cairo = sys::require()?;
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err(RenderError::SurfaceSize);
        }
        let stride = width as usize + (4 - width as usize % 4) % 4;
        let mut data = Vec::new();
        data.try_reserve_exact(stride * height as usize)
            .map_err(|_| RenderError::Allocation)?;
        data.resize(stride * height as usize, 0);
        let mut mask = Self {
            data,
            stride,
            width,
            height,
            surface: std::ptr::null_mut(),
            context: std::ptr::null_mut(),
        };
        // SAFETY: the buffer is allocated, tightly sized and never reallocated for
        // the life of this mask; cairo only reads its address once here.
        unsafe {
            let surface = (cairo.image_surface_create_for_data)(
                mask.data.as_mut_ptr(),
                sys::FORMAT_A8,
                width as i32,
                height as i32,
                stride as i32,
            );
            if surface.is_null() {
                return Err(RenderError::Allocation);
            }
            let status = (cairo.surface_status)(surface);
            if status != sys::STATUS_SUCCESS {
                (cairo.surface_destroy)(surface);
                return Err(status_error(status));
            }
            let context = (cairo.create)(surface);
            if context.is_null() {
                (cairo.surface_destroy)(surface);
                return Err(RenderError::Allocation);
            }
            mask.surface = surface;
            mask.context = context;
        }
        Ok(mask)
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn allocated_bytes(&self) -> usize {
        self.data.len()
    }

    fn clear(&mut self, area: Area) {
        for y in area.top..area.bottom {
            self.data[y * self.stride + area.left..y * self.stride + area.right].fill(0);
        }
    }

    fn row(&self, y: usize) -> &[u8] {
        &self.data[y * self.stride..(y + 1) * self.stride]
    }

    fn row_mut(&mut self, y: usize) -> &mut [u8] {
        &mut self.data[y * self.stride..(y + 1) * self.stride]
    }
}

impl CairoMask {
    /// The reusable cairo context of this mask.
    fn context(&mut self, cairo: &'static Cairo) -> Result<*mut std::os::raw::c_void, RenderError> {
        // SAFETY: the context was created from this mask's surface and is not
        // shared; cairo contexts are not thread safe, and the compositor is not.
        unsafe {
            let status = (cairo.status)(self.context);
            if status != sys::STATUS_SUCCESS {
                return Err(status_error(status));
            }
        }
        Ok(self.context)
    }

    /// Tells cairo the buffer changed underneath it.
    fn mark_dirty(&mut self, cairo: &'static Cairo) {
        // SAFETY: the surface is live and owns no other reference to the buffer.
        unsafe {
            (cairo.surface_flush)(self.surface);
            (cairo.surface_mark_dirty)(self.surface);
        }
    }
}

impl Drop for CairoMask {
    fn drop(&mut self) {
        let Some(cairo) = sys::cairo() else {
            return;
        };
        // SAFETY: both handles were created by this module and are destroyed once,
        // context before its surface, while the library is still mapped.
        unsafe {
            if !self.context.is_null() {
                (cairo.destroy)(self.context);
            }
            if !self.surface.is_null() {
                (cairo.surface_destroy)(self.surface);
            }
        }
    }
}

fn replay(cairo: &'static Cairo, context: *mut std::os::raw::c_void, segments: &[Segment]) {
    // SAFETY: `context` is a live cairo context owned by the caller.
    unsafe {
        for segment in segments {
            match *segment {
                Segment::Move { x, y } => (cairo.move_to)(context, x as f64, y as f64),
                Segment::Line { x, y } => (cairo.line_to)(context, x as f64, y as f64),
                Segment::Cubic {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => (cairo.curve_to)(
                    context, x1 as f64, y1 as f64, x2 as f64, y2 as f64, x as f64, y as f64,
                ),
                Segment::Close => (cairo.close_path)(context),
            }
        }
    }
}

/// Validates the outline's device range and records its conservative bounds.
fn finish(
    segments: Vec<Segment>,
    transform: Affine,
    stroke: Option<Stroke>,
) -> Result<CairoPath, RenderError> {
    let mut low = Point::new(f32::INFINITY, f32::INFINITY);
    let mut high = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for segment in &segments {
        let points: &[(f32, f32)] = match *segment {
            Segment::Move { x, y } | Segment::Line { x, y } => &[(x, y)],
            Segment::Cubic {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => &[(x1, y1), (x2, y2), (x, y)],
            Segment::Close => &[],
        };
        for &(x, y) in points {
            let mapped = transform.map_point(Point::new(x, y));
            low = Point::new(low.x.min(mapped.x), low.y.min(mapped.y));
            high = Point::new(high.x.max(mapped.x), high.y.max(mapped.y));
        }
    }
    // Miter joins reach at most twice the width from the outline.
    let outset = stroke.map_or(0.0, |stroke| stroke.width * 2.0);
    let bounds = if segments.is_empty() {
        aegle_scene::Rect::new(0.0, 0.0, 0.0, 0.0)
    } else {
        aegle_scene::Rect::new(
            low.x - outset,
            low.y - outset,
            (high.x - low.x) + outset * 2.0,
            (high.y - low.y) + outset * 2.0,
        )
    };
    outline::validate_bounds([
        bounds.origin.x,
        bounds.origin.y,
        bounds.origin.x + bounds.size.width,
        bounds.origin.y + bounds.size.height,
    ])?;
    Ok(CairoPath {
        segments,
        transform,
        stroke,
        bounds,
    })
}

/// A failed cairo call. The caller discards the frame, as for any rendering
/// error; the status code names the specific cairo condition.
fn status_error(status: i32) -> RenderError {
    RenderError::Backend { status }
}

/// A cairo error as text, for diagnostics such as [`crate::Renderer::new`]'s
/// message. Falls back to the numeric status.
pub fn status_text(status: i32) -> &'static str {
    let Some(cairo) = sys::cairo() else {
        return "cairo is not available";
    };
    // SAFETY: cairo_status_to_string returns a static NUL-terminated string for any
    // status value, including unknown ones.
    let text = unsafe { (cairo.status_to_string)(status) };
    if text.is_null() {
        return "cairo error";
    }
    // SAFETY: non-null, NUL-terminated and static, as documented by cairo.
    unsafe { std::ffi::CStr::from_ptr(text as *const _) }
        .to_str()
        .unwrap_or("cairo error")
}
