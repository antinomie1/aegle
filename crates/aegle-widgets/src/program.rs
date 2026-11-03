//! Views filled by custom fragment programs.

use std::path::Path;

use aegle_scene::Shader;
use aegle_theme::ControlKind;
use aegle_ui::{
    Container, Control, HandlerResult, LayoutStyle, Result, Subscription, control::MeasureCx,
    handle,
};

handle! {
    /// A view whose background is a custom fragment program, following its
    /// corners; see [`Shader`] for what a program defines. It has no size
    /// of its own: size it, or stretch it over a `Stack` as a background.
    /// Only GPU renderers draw programs.
    pub ShaderView(ShaderControl)
}

impl ShaderView {
    /// Checks the WGSL `source` and fills the view with it. On failure the
    /// view is emptied, [`Self::error`] holds the reason, with the line and
    /// column in the source, and the [`Self::on_error`] handlers run; a
    /// failure before any handler is registered runs the first ones
    /// registered.
    pub fn set_source(&self, source: &str) -> Result {
        self.show(source, None)
    }

    /// Reads the WGSL file at `path`, relative to the working directory, and
    /// shows it like [`Self::set_source`]; errors name the file.
    pub fn load(&self, path: impl AsRef<Path>) -> Result {
        let path = path.as_ref();
        match std::fs::read_to_string(path) {
            Ok(source) => self.show(&source, Some(path)),
            Err(error) => self.fail(format!("{}: {error}", path.display())),
        }
    }

    /// The program's `values`: at most [`Shader::VALUES`] finite numbers,
    /// the rest zero. They are kept while programs change.
    pub fn set_values(&self, values: &[f32]) {
        self.set_shader_values(values);
    }

    /// Empties the view and forgets a failure.
    pub fn clear(&self) {
        self.set_shader(None);
        self.set_error(None);
    }

    /// Why the last [`Self::set_source`] or [`Self::load`] failed, until a
    /// program is shown or the view cleared.
    pub fn error(&self) -> Option<String> {
        self.read(|view| view.error.clone())
    }

    /// Adds a handler called after [`Self::set_source`] or [`Self::load`] fails.
    pub fn on_error<R: HandlerResult>(
        &self,
        mut callback: impl FnMut(ShaderView) -> R + 'static,
    ) -> Subscription {
        let subscription = self.on_action(move |node| callback(ShaderView(node)).into_result());
        self.change(|state, id| {
            let view = state.control_as::<ShaderControl>(id).unwrap();
            if view.error.is_some() && !std::mem::replace(&mut view.reported, true) {
                state.queue_action(id);
            }
            Ok(())
        });
        subscription
    }

    fn show(&self, source: &str, path: Option<&Path>) -> Result {
        if let Err(error) = aegle_gpu::check_shader(source) {
            return self.fail(match path {
                Some(path) => format!("{}:{error}", path.display()),
                None => error.to_string(),
            });
        }
        self.set_shader(Shader::new(source));
        self.set_error(None);
        Ok(())
    }

    fn fail(&self, message: String) -> Result {
        self.set_shader(None);
        self.set_error(Some(message.clone()));
        self.change(|state, id| {
            let queued = state.queue_action(id);
            state.control_as::<ShaderControl>(id).unwrap().reported = queued;
            Ok(())
        });
        Err(message.into())
    }

    fn set_error(&self, error: Option<String>) {
        self.change(|state, id| {
            let view = state.control_as::<ShaderControl>(id).unwrap();
            view.error = error;
            view.reported = false;
            Ok(())
        })
    }
}

/// The control inside a [`ShaderView`] node; the engine paints its program.
pub struct ShaderControl {
    error: Option<String>,
    /// Whether the `error` handlers were queued.
    reported: bool,
}

impl Control for ShaderControl {
    fn kind(&self) -> &'static ControlKind {
        &aegle_ui::CONTAINER
    }
    fn measure(&mut self, _: &MeasureCx<'_>) -> aegle_types::Size {
        aegle_types::Size::default()
    }
}

pub(crate) fn shader_view(container: &Container) -> ShaderView {
    ShaderView(crate::add(container, |_, _| {
        Ok((
            Box::new(ShaderControl {
                error: None,
                reported: false,
            }) as Box<dyn Control>,
            LayoutStyle::default(),
        ))
    }))
}
