//! Images.

use std::{io::Read, path::Path};

use aegle_core::Dirty;
use aegle_scene::{Image, Rect};
use aegle_theme::ControlKind;
use aegle_types::Size;
use aegle_ui::{Align, LayoutStyle};
use aegle_ui::{
    Container, Control, HandlerResult, Result, Subscription,
    control::{MeasureCx, PaintCx},
    handle,
};

handle! {
    /// A retained image placed in its bounds by its [`Fit`]; it is not
    /// focusable. An empty view has no size and draws nothing.
    pub ImageView(ImageControl)
}

/// How an image fills its bounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fit {
    /// Stretched over the bounds.
    #[default]
    Fill,
    /// Scaled to fit inside the bounds and centered, keeping its aspect ratio.
    Contain,
    /// Scaled to cover the bounds and centered, keeping its aspect ratio;
    /// the overflow is clipped to the control's shape.
    Cover,
}

impl ImageView {
    /// Returns the shared image, or `None` while the view is empty.
    pub fn image(&self) -> Option<Image> {
        self.read(|view| view.image.clone())
    }
    /// Replaces the image and its intrinsic size, and forgets a load error.
    pub fn set_image(&self, image: &Image) {
        self.replace(Some(image.clone()), None);
    }
    /// Empties the view and forgets a load error.
    pub fn clear(&self) {
        self.replace(None, None);
    }
    /// How the image fills the bounds.
    pub fn fit(&self) -> Fit {
        self.read(|view| view.fit)
    }
    /// Sets how the image fills the bounds.
    pub fn set_fit(&self, fit: Fit) {
        self.update(|view| view.fit = fit)
    }
    /// Reads and decodes the image file at `path`, relative to the working
    /// directory, and shows it. PNG is always supported; JPEG, WebP and GIF
    /// follow `aegle-image`'s features. Decoding runs on this thread and is
    /// bounded by [`aegle_image::DEFAULT_MAX_BYTES`] for both the file and its
    /// pixels.
    ///
    /// On failure the view is emptied, [`Self::error`] holds the reason and
    /// the [`Self::on_error`] handlers run; a failure before any handler is
    /// registered runs the first ones registered.
    pub fn load(&self, path: impl AsRef<Path>) -> Result {
        let path = path.as_ref();
        match read(path) {
            Ok(image) => {
                self.set_image(&image);
                Ok(())
            }
            Err(reason) => {
                let message = format!("{}: {reason}", path.display());
                self.replace(None, Some(message.clone()));
                self.change(|state, id| {
                    let queued = state.queue_action(id);
                    state.control_as::<ImageControl>(id).unwrap().reported = queued;
                    Ok(())
                });
                Err(message.into())
            }
        }
    }
    /// Why the last [`Self::load`] failed, until an image is set or cleared.
    pub fn error(&self) -> Option<String> {
        self.read(|view| view.error.clone())
    }
    /// Adds a handler called after [`Self::load`] fails.
    pub fn on_error<R: HandlerResult>(
        &self,
        mut callback: impl FnMut(ImageView) -> R + 'static,
    ) -> Subscription {
        let subscription = self.on_action(move |node| callback(ImageView(node)).into_result());
        self.change(|state, id| {
            let view = state.control_as::<ImageControl>(id).unwrap();
            if view.error.is_some() && !std::mem::replace(&mut view.reported, true) {
                state.queue_action(id);
            }
            Ok(())
        });
        subscription
    }
    fn replace(&self, image: Option<Image>, error: Option<String>) {
        self.change(|state, id| {
            let view = state.control_as::<ImageControl>(id).unwrap();
            view.image = image;
            view.error = error;
            view.reported = false;
            state.tree.mark_dirty(id, Dirty::ALL)?;
            Ok(())
        })
    }
}

/// Reads at most the decoding budget, so an oversized file fails before
/// it is held in memory.
fn read(path: &Path) -> std::result::Result<Image, String> {
    let limit = aegle_image::DEFAULT_MAX_BYTES;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(limit as u64 + 1).read_to_end(&mut bytes))
        .map_err(|error| error.to_string())?;
    if bytes.len() > limit {
        return Err(aegle_image::Error::TooLarge.to_string());
    }
    aegle_image::decode(&bytes).map_err(|error| error.to_string())
}

/// The control inside an [`ImageView`] node.
pub struct ImageControl {
    image: Option<Image>,
    fit: Fit,
    error: Option<String>,
    /// Whether the `error` handlers were queued.
    reported: bool,
}

impl Control for ImageControl {
    fn kind(&self) -> &'static ControlKind {
        &aegle_ui::CONTAINER
    }
    /// The pixel size, scaled down with its aspect ratio to the offered width.
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Size {
        let Some(image) = &self.image else {
            return Size::default();
        };
        let (width, height) = (image.width() as f32, image.height() as f32);
        match cx.width {
            Some(offered) if offered < width => Size::new(offered, height * offered / width),
            _ => Size::new(width, height),
        }
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let Some(image) = &self.image else {
            return;
        };
        let (width, height) = (image.width() as f32, image.height() as f32);
        let scale = match self.fit {
            Fit::Fill => None,
            Fit::Contain => Some((cx.size.width / width).min(cx.size.height / height)),
            Fit::Cover => Some((cx.size.width / width).max(cx.size.height / height)),
        };
        let Some(scale) = scale else {
            cx.builder
                .image(image, Rect::new(0.0, 0.0, cx.size.width, cx.size.height));
            return;
        };
        let (width, height) = (width * scale, height * scale);
        let rect = Rect::new(
            (cx.size.width - width) / 2.0,
            (cx.size.height - height) / 2.0,
            width,
            height,
        );
        let clip = self.fit == Fit::Cover;
        if clip {
            cx.builder.push_clip(cx.shape);
        }
        cx.builder.image(image, rect);
        if clip {
            cx.builder.pop();
        }
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        cx.set_role(aegle_ui::Role::Image);
    }
}

pub(crate) fn image(container: &Container, image: Option<&Image>) -> ImageView {
    ImageView(crate::add(container, |_, _| {
        Ok((
            Box::new(ImageControl {
                image: image.cloned(),
                fit: Fit::Fill,
                error: None,
                reported: false,
            }) as Box<dyn Control>,
            {
                let mut style = LayoutStyle::default();
                style.set_align_self(Some(Align::Start)).set_shrink(0.0);
                style
            },
        ))
    }))
}
