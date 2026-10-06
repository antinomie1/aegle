//! OpenType-SVG glyph documents rendered into one straight sRGB RGBA8 bitmap.
//!
//! The document is static SVG without text. The glyph is the element with id
//! `glyph<N>`, or the whole document when it holds one glyph; document units are
//! font units with y down from the baseline.
use resvg::{
    tiny_skia::{Pixmap, Transform},
    usvg,
};

use crate::{Content, GlyphError, Image, Placement, raster::image_size};

pub(crate) struct Request<'a> {
    pub data: &'a [u8],
    pub glyph: u16,
    pub units_per_em: f32,
    pub size: f32,
    pub offset: [f32; 2],
    pub max_bytes: usize,
}

pub(crate) fn render(
    request: Request<'_>,
    mut reserve: impl FnMut(usize) -> Result<(), GlyphError>,
) -> Result<Image, GlyphError> {
    let tree = usvg::Tree::from_data(request.data, &usvg::Options::default())
        .map_err(|_| GlyphError::InvalidFont)?;
    let node = tree.node_by_id(&format!("glyph{}", request.glyph));
    let bounds = match node {
        Some(node) => node.abs_layer_bounding_box(),
        None => Some(tree.root().abs_layer_bounding_box()),
    };
    let Some(bounds) = bounds else {
        return Ok(Image {
            placement: Placement::default(),
            content: Content::Mask,
            data: Box::new([]),
        });
    };
    let scale = request.size / request.units_per_em;
    let [ox, oy] = request.offset;
    let left = (bounds.x() * scale + ox).floor();
    let top = (bounds.y() * scale + oy).floor();
    let width = ((bounds.right() * scale + ox).ceil() - left).max(0.0);
    let height = ((bounds.bottom() * scale + oy).ceil() - top).max(0.0);
    if !(width < 65_536.0 && height < 65_536.0) {
        return Err(GlyphError::ImageBudget);
    }
    let (width, height) = (width as u32, height as u32);
    reserve(image_size(width, height, 4, request.max_bytes)?)?;
    let mut pixmap = Pixmap::new(width, height).ok_or(GlyphError::ImageBudget)?;
    let place = Transform::from_scale(scale, scale).post_translate(
        bounds.x() * scale + ox - left,
        bounds.y() * scale + oy - top,
    );
    match node {
        Some(node) => resvg::render_node(node, place, &mut pixmap.as_mut()),
        None => {
            resvg::render(
                &tree,
                place.pre_translate(-bounds.x(), -bounds.y()),
                &mut pixmap.as_mut(),
            );
            Some(())
        }
    }
    .ok_or(GlyphError::UnsupportedGlyph)?;
    Ok(Image {
        placement: Placement {
            left: left as i32,
            top: -(top as i32),
            width,
            height,
        },
        content: Content::Color,
        data: pixmap.take_demultiplied().into_boxed_slice(),
    })
}
