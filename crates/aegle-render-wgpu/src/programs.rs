//! Custom fragment programs, compiled on first use into pipelines that draw
//! like geometry.

use aegle_scene::Shader;
use wgpu::{BlendState, RenderPipeline, ShaderModuleDescriptor, ShaderSource};

use crate::{Error, Renderer, Result, gpu};

/// Programs kept beyond this are dropped at a frame's start unless the last
/// frame drew them, bounding pipelines a hot-reloading host leaves behind.
const KEPT: usize = 8;

/// Compiled programs; a record's page lane is its index here.
#[derive(Default)]
pub(crate) struct Programs {
    entries: Vec<Program>,
}

struct Program {
    shader: Shader,
    pipeline: RenderPipeline,
    used: bool,
}

impl Programs {
    /// The pipeline of slot `slot`.
    pub fn pipeline(&self, slot: u32) -> &RenderPipeline {
        &self.entries[slot as usize].pipeline
    }

    /// At a frame's start: forgets programs the last frame did not draw
    /// while more than [`KEPT`] are compiled. wgpu keeps a pipeline alive
    /// while submitted work uses it.
    pub fn trim(&mut self) {
        if self.entries.len() > KEPT {
            self.entries.retain(|program| program.used);
        }
        for program in &mut self.entries {
            program.used = false;
        }
    }
}

impl Renderer {
    /// The slot of `shader`'s pipeline, compiling it first if needed. A
    /// program that does not compile fails with [`Error::Shader`].
    pub(crate) fn program(&mut self, shader: &Shader) -> Result<u32> {
        let programs = &mut self.programs.entries;
        if let Some(slot) = programs.iter().position(|p| p.shader == *shader) {
            programs[slot].used = true;
            return Ok(slot as u32);
        }
        // Checked first: wgpu reports an invalid module only as a device error.
        aegle_gpu::check_shader(shader.source()).map_err(Error::Shader)?;
        let wgsl = aegle_gpu::shader_wgsl(shader.source());
        let module = self
            .gpu
            .device
            .create_shader_module(ShaderModuleDescriptor {
                label: Some("aegle program"),
                source: ShaderSource::Wgsl(wgsl.into()),
            });
        let pipeline = gpu::pipeline(
            &self.gpu.device,
            &module,
            "fs_shader",
            &[Some(&self.gpu.records)],
            gpu::LINEAR,
            BlendState::PREMULTIPLIED_ALPHA_BLENDING,
        );
        let programs = &mut self.programs.entries;
        programs.push(Program {
            shader: shader.clone(),
            pipeline,
            used: true,
        });
        Ok(programs.len() as u32 - 1)
    }
}
