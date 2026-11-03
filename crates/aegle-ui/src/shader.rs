//! Node backgrounds filled by custom fragment programs.

use std::time::Instant;

use aegle_core::Dirty;
use aegle_scene::{RoundedRect, SceneBuilder, Shader};

use crate::{Node, UiError, style::Decoration};

/// A node's program, its values and when its `time` started.
#[derive(Clone, Debug)]
pub(crate) struct Program {
    pub shader: Option<Shader>,
    pub values: [f32; Shader::VALUES],
    pub start: Instant,
}

impl Node {
    /// Fills this node's background with `shader` (see [`Shader`]) instead
    /// of its background color or gradient, following its corners, or
    /// restores them with `None`. Its `time` counts seconds from this call;
    /// a program that names `time` repaints the node every frame while it
    /// is shown, unless the UI reduces motion, which holds `time` at zero.
    /// Only GPU renderers draw programs; check a source with
    /// `aegle_gpu::check_shader` first, since one that does not compile
    /// fails the frame.
    pub fn set_shader(&self, shader: impl Into<Option<Shader>>) {
        let shader = shader.into();
        self.change(|state, id| {
            let program = &mut state.decorations.entry(id).or_default().program;
            match (program.as_deref_mut(), shader) {
                (Some(program), Some(shader)) if program.shader.as_ref() == Some(&shader) => {
                    return Ok(());
                }
                (Some(program), shader) => {
                    program.shader = shader;
                    program.start = Instant::now();
                }
                (None, None) => {}
                (None, shader) => {
                    *program = Some(Box::new(Program {
                        shader,
                        values: [0.0; Shader::VALUES],
                        start: Instant::now(),
                    }));
                }
            }
            state.trim_program(id);
            state.invalidate(id, Dirty::PAINT)
        })
    }

    /// The program set by [`Self::set_shader`].
    pub fn shader(&self) -> Option<Shader> {
        self.change(|state, id| {
            Ok(state
                .decorations
                .get(&id)
                .and_then(|d| d.program.as_ref()?.shader.clone()))
        })
    }

    /// Sets the program's `values`: at most [`Shader::VALUES`] finite
    /// numbers, the rest zero. They are kept while programs change.
    ///
    /// # Panics
    ///
    /// With more values, or one that is not finite.
    pub fn set_shader_values(&self, values: &[f32]) {
        if values.len() > Shader::VALUES || values.iter().any(|v| !v.is_finite()) {
            panic!("{}", UiError::InvalidValue);
        }
        let mut padded = [0.0; Shader::VALUES];
        padded[..values.len()].copy_from_slice(values);
        self.change(|state, id| {
            let program = state
                .decorations
                .entry(id)
                .or_default()
                .program
                .get_or_insert_with(|| {
                    Box::new(Program {
                        shader: None,
                        values: [0.0; Shader::VALUES],
                        start: Instant::now(),
                    })
                });
            if program.values == padded {
                return Ok(());
            }
            program.values = padded;
            let drawn = program.shader.is_some();
            state.trim_program(id);
            if drawn {
                state.invalidate(id, Dirty::PAINT)?;
            }
            Ok(())
        })
    }
}

impl crate::State {
    /// Drops a program that neither draws nor holds values.
    fn trim_program(&mut self, id: aegle_core::NodeId) {
        if let Some(decoration) = self.decorations.get_mut(&id)
            && decoration
                .program
                .as_ref()
                .is_some_and(|p| p.shader.is_none() && p.values == [0.0; Shader::VALUES])
        {
            decoration.program = None;
        }
        self.trim_decoration(id);
    }
}

/// Fills `shape` with the node's program at `now`, if it has one; returns
/// whether it did, and whether the program wants the next frame.
pub(crate) fn paint(
    decoration: Option<&Decoration>,
    builder: &mut SceneBuilder,
    shape: RoundedRect,
    now: Instant,
    reduced_motion: bool,
) -> (bool, bool) {
    let Some(program) = decoration.and_then(|d| d.program.as_deref()) else {
        return (false, false);
    };
    let Some(shader) = &program.shader else {
        return (false, false);
    };
    let animated = shader.is_animated() && !reduced_motion;
    let time = match animated {
        true => now.saturating_duration_since(program.start).as_secs_f32(),
        false => 0.0,
    };
    builder.shade(shape, shader, time, program.values);
    (true, animated)
}
