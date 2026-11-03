//! Custom fragment programs: their records, and with `shaders` their checked
//! WGSL modules.
use aegle_scene::{RoundedRect, Shader};

use crate::{
    Clip, Primitive, Recording, Result, State,
    records::{bounds, local_shape, reserve},
};

/// The primitive kind of a custom program; its page lane holds the
/// backend's pipeline slot, so batches of it bind that pipeline.
pub const KIND_SHADER: u32 = 5;

impl Recording {
    /// Records `shape` filled by the program in pipeline `slot` at `time`
    /// with `values`, appending the values to the clip rows. Returns whether
    /// a row was added.
    #[allow(clippy::too_many_arguments)]
    pub fn shader(
        &mut self,
        state: State,
        shape: RoundedRect,
        slot: u32,
        time: f32,
        values: [f32; Shader::VALUES],
        viewport: [f32; 2],
    ) -> Result<bool> {
        let area = bounds(shape, state.transform, 0.0, 1.0)?;
        if !crate::visible(area, state.bounds) {
            return Ok(false);
        }
        let local = local_shape(shape, state.transform, -1.0)?;
        let row = u32::try_from(self.clips.len()).map_err(|_| crate::Error::Coordinates)?;
        reserve(&mut self.clips)?;
        self.clips.push(Clip {
            row0: [values[0], values[1], values[2], values[3]],
            row1: [values[4], values[5], values[6], values[7]],
            rect: [0.0; 4],
            extra: [0; 4],
        });
        let size = shape.rect().size;
        self.record(
            Primitive {
                bounds: area,
                row0: local.row0,
                row1: local.row1,
                rect: local.rect,
                params: [time, -1.0, viewport[0], viewport[1]],
                color: [size.width, size.height, 0.0, 0.0],
                header: [state.clip, KIND_SHADER, slot, row],
            },
            state.bounds,
        )
    }
}

#[cfg(feature = "shaders")]
pub use checked::{ShaderError, check_shader, shader_module, shader_wgsl};

#[cfg(feature = "shaders")]
mod checked {
    use std::fmt;

    /// What comes before a program: shared records and coverage, and the
    /// globals it reads.
    const PRELUDE: &str = concat!(
        include_str!("common.wgsl"),
        "\n// A program's inputs beside uv and size.\n",
        "var<private> time: f32;\n",
        "var<private> values: array<f32, 8>;\n\n",
    );
    const ENTRY: &str = include_str!("custom.wgsl");

    /// Why a program does not compile, at a place in its own source.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ShaderError {
        /// 1-based line and column in the program, if the error has one there.
        pub location: Option<(u32, u32)>,
        /// What is wrong.
        pub message: String,
    }

    impl fmt::Display for ShaderError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self.location {
                Some((line, column)) => write!(f, "{line}:{column}: {}", self.message),
                None => f.write_str(&self.message),
            }
        }
    }

    impl std::error::Error for ShaderError {}

    /// The whole module of a program: entries `vs_main` and `fs_shader`.
    pub fn shader_wgsl(source: &str) -> String {
        let mut wgsl = String::with_capacity(PRELUDE.len() + source.len() + ENTRY.len() + 2);
        wgsl.push_str(PRELUDE);
        wgsl.push_str(source);
        wgsl.push('\n');
        wgsl.push_str(ENTRY);
        wgsl
    }

    /// Parses and validates a program's module, see [`aegle_scene::Shader`].
    pub fn shader_module(
        source: &str,
    ) -> Result<(naga::Module, naga::valid::ModuleInfo), ShaderError> {
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let wgsl = shader_wgsl(source);
        let prelude_lines = PRELUDE.lines().count() as u32;
        let user_lines = source.lines().count() as u32;
        // Places in the prelude or entry are no place in the program.
        let place = |location: Option<naga::SourceLocation>| {
            location
                .filter(|l| l.line_number > prelude_lines)
                .filter(|l| l.line_number <= prelude_lines + user_lines)
                .map(|l| (l.line_number - prelude_lines, l.line_position))
        };
        // An error outside the program is about how it meets the entry.
        let error = |location: Option<(u32, u32)>, message: String| ShaderError {
            message: match location {
                Some(_) => message,
                None => format!(
                    "a program defines fn shade(uv: vec2f, size: vec2f) -> vec4f: {message}"
                ),
            },
            location,
        };
        let module = naga::front::wgsl::parse_str(&wgsl)
            .map_err(|e| error(place(e.location(&wgsl)), e.message().to_owned()))?;
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .map_err(|e| {
                let mut message = e.as_inner().to_string();
                let mut cause = std::error::Error::source(e.as_inner());
                while let Some(inner) = cause {
                    message = format!("{message}: {inner}");
                    cause = inner.source();
                }
                error(place(e.location(&wgsl)), message)
            })?;
        Ok((module, info))
    }

    /// Checks that a program compiles, as renderers would compile it:
    /// `shade(uv: vec2f, size: vec2f) -> vec4f`, reading `time` and `values`.
    pub fn check_shader(source: &str) -> Result<(), ShaderError> {
        shader_module(source).map(|_| ())
    }
}
