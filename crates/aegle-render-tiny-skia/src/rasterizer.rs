//! Antialiased coverage through tiny-skia, for the shared CPU compositor.
//!
//! tiny-skia's fixed-point scanline rasterizer turns a device-space outline into
//! 8-bit coverage with a quantized horizontal edge. That is the only place this
//! backend differs from another CPU backend; color, clipping, layers, images and
//! text sampling all come from [`aegle_render_cpu`].

use aegle_render_cpu::{Area, FillRule, Mask, Rasterizer, RenderError, Segment, outline};
use aegle_scene::{Affine, LineCap, LineJoin, Path, RoundedRect, Stroke};
use tiny_skia::{FillRule as SkiaRule, IntSize, PathBuilder, PathStroker, Transform};

/// Reusable tiny-skia path storage, the coverage rasterizer of this backend.
#[derive(Default)]
pub struct TinySkia {
    builder: PathBuilder,
}

/// One device-space tiny-skia outline.
pub struct TinyPath {
    path: tiny_skia::Path,
    bounds: aegle_scene::Rect,
}

/// A tiny-skia coverage mask, tightly packed at the mask width.
pub struct TinyMask {
    mask: tiny_skia::Mask,
}

impl Mask for TinyMask {
    fn bytes_for(width: u32, height: u32) -> usize {
        (width as usize).saturating_mul(height as usize)
    }

    fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        let size = IntSize::from_wh(width, height).ok_or(RenderError::SurfaceSize)?;
        let mask =
            tiny_skia::Mask::new(size.width(), size.height()).ok_or(RenderError::Allocation)?;
        Ok(Self { mask })
    }

    fn width(&self) -> u32 {
        self.mask.width()
    }

    fn height(&self) -> u32 {
        self.mask.height()
    }

    fn allocated_bytes(&self) -> usize {
        self.mask.data().len()
    }

    fn clear(&mut self, area: Area) {
        let width = self.mask.width() as usize;
        for y in area.top..area.bottom {
            self.mask.data_mut()[y * width + area.left..y * width + area.right].fill(0);
        }
    }

    fn row(&self, y: usize) -> &[u8] {
        let width = self.mask.width() as usize;
        &self.mask.data()[y * width..(y + 1) * width]
    }

    fn row_mut(&mut self, y: usize) -> &mut [u8] {
        let width = self.mask.width() as usize;
        &mut self.mask.data_mut()[y * width..(y + 1) * width]
    }
}

impl Rasterizer for TinySkia {
    type Path = TinyPath;
    type Mask = TinyMask;

    fn shape(
        &mut self,
        shape: RoundedRect,
        stroke: Option<f32>,
        transform: Affine,
    ) -> Result<Self::Path, RenderError> {
        let mut builder = std::mem::take(&mut self.builder);
        outline::shape_segments(shape, stroke, |segment| push(&mut builder, segment));
        let local = builder.finish().ok_or(RenderError::Coordinates)?;
        finish(local, transform)
    }

    fn path(
        &mut self,
        path: &Path,
        stroke: Option<Stroke>,
        transform: Affine,
    ) -> Result<Self::Path, RenderError> {
        let mut builder = std::mem::take(&mut self.builder);
        outline::path_segments(path, |segment| push(&mut builder, segment));
        // Fewer than two points cover no pixel.
        let local = builder.finish().ok_or(RenderError::Coordinates)?;
        let [a, b, c, d, e, f] = transform.coefficients();
        let device = Transform::from_row(a, b, c, d, e, f);
        let local = match stroke {
            None => local,
            Some(stroke) => {
                let style = tiny_skia::Stroke {
                    width: stroke.width,
                    miter_limit: 4.0,
                    line_cap: match stroke.cap {
                        LineCap::Butt => tiny_skia::LineCap::Butt,
                        LineCap::Round => tiny_skia::LineCap::Round,
                        LineCap::Square => tiny_skia::LineCap::Square,
                    },
                    line_join: match stroke.join {
                        LineJoin::Miter => tiny_skia::LineJoin::Miter,
                        LineJoin::Round => tiny_skia::LineJoin::Round,
                        LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
                    },
                    dash: None,
                };
                let scale = PathStroker::compute_resolution_scale(&device);
                match local.stroke(&style, scale) {
                    Some(outline) => outline,
                    None => return Err(RenderError::Coordinates),
                }
            }
        };
        let path = local.transform(device).ok_or(RenderError::Coordinates)?;
        let bounds = path.bounds();
        outline::validate_bounds([bounds.left(), bounds.top(), bounds.right(), bounds.bottom()])?;
        Ok(TinyPath {
            bounds: aegle_scene::Rect::new(
                bounds.left(),
                bounds.top(),
                bounds.width(),
                bounds.height(),
            ),
            path,
        })
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
        let rule = match rule {
            FillRule::NonZero => SkiaRule::Winding,
            FillRule::EvenOdd => SkiaRule::EvenOdd,
        };
        // tiny-skia clips its scan conversion to the mask rectangle, so the
        // mask's own size already bounds the work to the compositor's area.
        let _ = area;
        coverage
            .mask
            .fill_path(&path.path, rule, true, Transform::identity());
        Ok(())
    }

    fn release(&mut self, path: Self::Path) {
        self.builder = path.path.clear();
    }
}

fn push(builder: &mut PathBuilder, segment: Segment) {
    match segment {
        Segment::Move { x, y } => {
            builder.move_to(x, y);
        }
        Segment::Line { x, y } => {
            builder.line_to(x, y);
        }
        Segment::Cubic {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        } => {
            builder.cubic_to(x1, y1, x2, y2, x, y);
        }
        Segment::Close => builder.close(),
    }
}

fn finish(local: tiny_skia::Path, transform: Affine) -> Result<TinyPath, RenderError> {
    let [a, b, c, d, e, f] = transform.coefficients();
    let path = local
        .transform(Transform::from_row(a, b, c, d, e, f))
        .ok_or(RenderError::Coordinates)?;
    let bounds = path.bounds();
    outline::validate_bounds([bounds.left(), bounds.top(), bounds.right(), bounds.bottom()])?;
    Ok(TinyPath {
        bounds: aegle_scene::Rect::new(
            bounds.left(),
            bounds.top(),
            bounds.width(),
            bounds.height(),
        ),
        path,
    })
}
