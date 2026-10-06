//! Static SVG rasterization through resvg, without text or external resources.

use resvg::{tiny_skia::Pixmap, usvg};

use crate::{Error, budget};
use aegle_scene::Image;

fn parse(bytes: &[u8]) -> Result<usvg::Tree, Error> {
    usvg::Tree::from_data(bytes, &usvg::Options::default()).map_err(|_| Error::Invalid)
}

/// The document's intrinsic size in pixels.
pub fn size(bytes: &[u8]) -> Result<(f32, f32), Error> {
    let size = parse(bytes)?.size();
    Ok((size.width(), size.height()))
}

/// Renders the document stretched to `width` × `height` pixels, limited to
/// [`crate::DEFAULT_MAX_BYTES`]. Text and external files are not supported.
pub fn rasterize(bytes: &[u8], width: u32, height: u32) -> Result<Image, Error> {
    budget(width, height, crate::DEFAULT_MAX_BYTES)?;
    let tree = parse(bytes)?;
    let size = tree.size();
    let mut pixmap = Pixmap::new(width, height).ok_or(Error::TooLarge)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(
            width as f32 / size.width(),
            height as f32 / size.height(),
        ),
        &mut pixmap.as_mut(),
    );
    Image::new(width, height, pixmap.take_demultiplied()).map_err(|_| Error::TooLarge)
}
