//! Compiles Vulkan shader entries at build time; no compiler ships at runtime.

use std::{env, fs, path::PathBuf};

use naga::{
    ShaderStage,
    back::spv::{self, BindingInfo, Options, PipelineOptions, WriterFlags},
    valid::{Capabilities, ValidationFlags, Validator},
};

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo supplies OUT_DIR"));
    let text = env::var_os("CARGO_FEATURE_TEXT").is_some();
    for (name, source) in [
        ("geometry", aegle_gpu::GEOMETRY_WGSL),
        ("resolve", aegle_gpu::RESOLVE_WGSL),
    ] {
        let path = format!("aegle-gpu {name}.wgsl");
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|error| panic!("{path}: {}", error.emit_to_string(source)));
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap_or_else(|error| panic!("{path}: {}", error.emit_to_string(source)));
        let mut options = Options {
            lang_version: (1, 3),        // Vulkan 1.1
            flags: WriterFlags::empty(), // Shaders use Vulkan's positive-height viewport.
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
        for (suffix, shader_stage, entry_point) in [
            ("vert", ShaderStage::Vertex, "vs_main"),
            ("frag", ShaderStage::Fragment, "fs_main"),
            ("text.frag", ShaderStage::Fragment, "fs_text"),
        ] {
            if entry_point == "fs_text" && (name != "geometry" || !text) {
                continue;
            }
            let words = spv::write_vec(
                &module,
                &info,
                &options,
                Some(&PipelineOptions {
                    shader_stage,
                    entry_point: entry_point.into(),
                }),
            )
            .unwrap_or_else(|error| panic!("{path}/{entry_point}: {error}"));
            let bytes: Vec<_> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
            fs::write(out.join(format!("{name}.{suffix}.spv")), bytes)
                .expect("write compiled SPIR-V");
        }
    }
}
