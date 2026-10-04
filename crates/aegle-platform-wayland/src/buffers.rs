//! Two independently owned SHM mappings; active storage is never resized or reused.

use smithay_client_toolkit::shm::{
    Shm,
    slot::{Buffer, SlotPool},
};
use wayland_client::protocol::wl_shm;

use crate::{Error, PixelSize, PresentError};

struct Image {
    // Destroy the buffer before releasing its pool when the image is idle.
    buffer: Buffer,
    pool: SlotPool,
    width: u32,
}

impl Image {
    fn is_idle(&self) -> bool {
        !self.buffer.slot().has_active_buffers()
    }

    fn matches(&self, size: PixelSize) -> bool {
        self.width == size.width && self.buffer.height() as u32 == size.height
    }
}

/// Bounded storage for software frames, without a second color framebuffer.
///
/// The allowance includes SCTK's 64-byte slot alignment, but excludes page
/// rounding, object metadata and compositor/driver copies.
pub(crate) struct SoftwareBuffers {
    images: [Option<Image>; 2],
    budget: usize,
}

impl SoftwareBuffers {
    pub(crate) fn new(budget: usize) -> Self {
        Self {
            images: [None, None],
            budget,
        }
    }

    pub(crate) fn allocated_bytes(&self) -> usize {
        self.images
            .iter()
            .flatten()
            .map(|image| image.pool.len())
            .sum()
    }

    pub(crate) fn has_free(&self) -> bool {
        self.images
            .iter()
            .any(|image| image.as_ref().is_none_or(Image::is_idle))
    }

    /// Draw a complete frame as tightly packed premultiplied sRGB RGBA8.
    ///
    /// The caller must clear or overwrite every pixel: a reused mapping contains
    /// a previous frame in native ARGB format. A failed draw stays unattached and
    /// reusable. The returned buffer has not been activated; attach and commit it
    /// before drawing another frame. `None` means both slots await server release.
    pub(crate) fn paint<E>(
        &mut self,
        shm: &Shm,
        size: PixelSize,
        draw: impl FnOnce(&mut [u8]) -> Result<(), E>,
    ) -> Result<Option<&Buffer>, PresentError<E>> {
        let (stride, mapping_bytes) = dimensions(size).map_err(PresentError::Platform)?;

        // Release only idle old-size images, before considering the new allocation.
        // Each pool has one slot: no growth or orphaned free-list fragments.
        for image in &mut self.images {
            if image
                .as_ref()
                .is_some_and(|image| image.is_idle() && !image.matches(size))
            {
                *image = None;
            }
        }
        let Some(index) = self
            .images
            .iter()
            .position(|image| {
                image
                    .as_ref()
                    .is_some_and(|image| image.is_idle() && image.matches(size))
            })
            .or_else(|| self.images.iter().position(Option::is_none))
        else {
            return Ok(None);
        };

        if self.images[index].is_none() {
            let required = self.allocated_bytes() + mapping_bytes;
            if required > self.budget {
                return Err(PresentError::Platform(Error::BufferBudget {
                    required,
                    budget: self.budget,
                }));
            }
            let mut pool = SlotPool::new(mapping_bytes, shm)
                .map_err(|error| PresentError::Platform(Error::backend(error)))?;
            let (buffer, _) = pool
                .create_buffer(
                    size.width as i32,
                    size.height as i32,
                    stride,
                    wl_shm::Format::Argb8888,
                )
                .map_err(|error| PresentError::Platform(Error::backend(error)))?;
            self.images[index] = Some(Image {
                buffer,
                pool,
                width: size.width,
            });
        }

        let image = self.images[index]
            .as_mut()
            .expect("selected image is allocated");
        let pixels = image
            .buffer
            .canvas(&mut image.pool)
            .expect("selected image is idle");
        draw(pixels).map_err(PresentError::Draw)?;
        for pixel in pixels.chunks_exact_mut(4) {
            let argb = u32::from_be_bytes([pixel[3], pixel[0], pixel[1], pixel[2]]);
            pixel.copy_from_slice(&argb.to_ne_bytes());
        }
        Ok(Some(&image.buffer))
    }
}

fn dimensions(size: PixelSize) -> Result<(i32, usize), Error> {
    if size.width == 0 || size.height == 0 || size.height > i32::MAX as u32 {
        return Err(Error::InvalidSize);
    }
    let stride = size
        .width
        .checked_mul(4)
        .and_then(|stride| i32::try_from(stride).ok())
        .ok_or(Error::InvalidSize)?;
    // SCTK aligns its slot to 64 bytes; allocate that size up front so its
    // automatic growth never doubles the requested mapping behind the budget.
    let mapping_bytes = (stride as usize)
        .checked_mul(size.height as usize)
        .and_then(|bytes| bytes.checked_add(63))
        .map(|bytes| bytes & !63)
        .filter(|bytes| *bytes <= i32::MAX as usize)
        .ok_or(Error::InvalidSize)?;
    Ok((stride, mapping_bytes))
}
