//! Custom fragment programs, compiled to SPIR-V on first use and kept as
//! geometry-like pipelines.
#![allow(unsafe_code)]

use aegle_scene::Shader;
use ash::vk;

use crate::{
    Error, Result,
    pipeline::{Blend, Pipeline, graphics},
};

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
    pipeline: vk::Pipeline,
    used: bool,
}

impl Programs {
    /// The pipeline of slot `slot`.
    pub fn pipeline(&self, slot: u32) -> vk::Pipeline {
        self.entries[slot as usize].pipeline
    }

    /// At a frame's start, with no submission pending: forgets programs the
    /// last frame did not draw while more than [`KEPT`] are compiled.
    pub fn trim(&mut self, raw: &ash::Device) {
        if self.entries.len() > KEPT {
            self.entries.retain(|program| {
                if !program.used {
                    // SAFETY: the renderer waited for every submission.
                    unsafe { raw.destroy_pipeline(program.pipeline, None) };
                }
                program.used
            });
        }
        for program in &mut self.entries {
            program.used = false;
        }
    }

    pub fn destroy(&mut self, raw: &ash::Device) {
        for program in self.entries.drain(..) {
            // SAFETY: called from the pipeline's drop, after the renderer waited.
            unsafe { raw.destroy_pipeline(program.pipeline, None) };
        }
    }
}

impl Pipeline {
    /// The slot of `shader`'s pipeline, compiling it first if needed. A
    /// program that does not compile fails with [`Error::Shader`].
    pub fn program(&mut self, shader: &Shader) -> Result<u32> {
        let programs = &mut self.programs.entries;
        if let Some(slot) = programs.iter().position(|p| p.shader == *shader) {
            programs[slot].used = true;
            return Ok(slot as u32);
        }
        let spirv = spirv(shader.source())?;
        let pipeline = graphics(
            self.raw(),
            self.passes[0],
            self.layouts[0],
            Blend::Over,
            include_bytes!(concat!(env!("OUT_DIR"), "/geometry.vert.spv")),
            bytemuck::cast_slice(&spirv),
            c"fs_shader",
        )?;
        let programs = &mut self.programs.entries;
        programs.push(Program {
            shader: shader.clone(),
            pipeline,
            used: true,
        });
        Ok(programs.len() as u32 - 1)
    }
}

/// The fragment stage of a program, as the build script compiles the
/// built-in shaders.
fn spirv(source: &str) -> Result<Vec<u32>> {
    use naga::back::spv::{self, BindingInfo, Options, PipelineOptions, WriterFlags};
    let (module, info) = aegle_gpu::shader_module(source).map_err(Error::Shader)?;
    let mut options = Options {
        lang_version: (1, 3),
        flags: WriterFlags::empty(),
        fake_missing_bindings: false,
        ..Options::default()
    };
    for (_, variable) in module.global_variables.iter() {
        if let Some(binding) = variable.binding {
            options.binding_map.insert(
                binding,
                BindingInfo {
                    descriptor_set: binding.group,
                    binding: binding.binding,
                    binding_array_size: None,
                },
            );
        }
    }
    let stage = PipelineOptions {
        shader_stage: naga::ShaderStage::Fragment,
        entry_point: "fs_shader".into(),
    };
    spv::write_vec(&module, &info, &options, Some(&stage)).map_err(|error| {
        Error::Shader(aegle_gpu::ShaderError {
            location: None,
            message: error.to_string(),
        })
    })
}
