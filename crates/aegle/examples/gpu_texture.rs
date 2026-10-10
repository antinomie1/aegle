//! A wgpu render pass draws into an application texture every frame; a canvas
//! shows it inside ordinary controls, like a game or 3D viewport in an editor.
use std::{cell::Cell, rc::Rc, time::Instant};

use aegle::{prelude::*, scene::TextureId, wgpu};

const SHADER: &str = "
struct Out { @builtin(position) position: vec4<f32>, @location(0) color: vec3<f32> }
fn corner(i: u32) -> Out {
    let corners = array(vec2(0.0, 0.7), vec2(-0.6, -0.5), vec2(0.6, -0.5));
    let colors = array(vec3(1.0, 0.2, 0.2), vec3(0.2, 1.0, 0.3), vec3(0.2, 0.4, 1.0));
    return Out(vec4(corners[i], 0.0, 1.0), colors[i]);
}
struct Spin { angle: f32 }
@group(0) @binding(0) var<uniform> spin: Spin;
@vertex fn vs_spin(@builtin(vertex_index) i: u32) -> Out {
    var out = corner(i);
    let s = sin(spin.angle);
    let c = cos(spin.angle);
    out.position = vec4(c * out.position.x - s * out.position.y, s * out.position.x + c * out.position.y, 0.0, 1.0);
    return out;
}
@fragment fn fs(input: Out) -> @location(0) vec4<f32> { return vec4(input.color, 1.0); }
";

fn main() -> Result<()> {
    let app = App::with_options(AppOptions {
        renderer: RendererBackend::Wgpu,
        ..Default::default()
    })?;
    let window = app.window("Aegle — GPU texture")?;
    window.text("A wgpu render pass draws the viewport below every frame.")?;
    let gpu = app
        .wgpu()
        .ok_or("the window did not create a wgpu device")?;
    let (device, queue) = (gpu.device().clone(), gpu.queue().clone());
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("viewport"),
        size: wgpu::Extent3d {
            width: 320,
            height: 240,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let id: TextureId = gpu.register_texture(&texture)?;
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_spin"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::TextureFormat::Rgba8UnormSrgb.into())],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    let viewport = window.canvas(move |builder, size| {
        builder.texture(
            id,
            aegle::scene::Rect::new(0.0, 0.0, size.width, size.height),
        )?;
        Ok(())
    })?;
    viewport.set_width(320.0)?;
    viewport.set_height(240.0)?;
    let frames = Rc::new(Cell::new(0u32));
    let label = window.text("")?;
    let (start, view) = (Instant::now(), texture.create_view(&Default::default()));
    let count = frames.clone();
    viewport.on_frame(move |_, now| {
        let angle = now.duration_since(start).as_secs_f32();
        queue.write_buffer(&uniform, 0, &angle.to_ne_bytes());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.02,
                            g: 0.03,
                            b: 0.06,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        // Submitted before Aegle's frame on the same queue, so it is complete when sampled.
        queue.submit([encoder.finish()]);
        count.set(count.get() + 1);
        label.set_text(&format!(
            "Frames rendered into the texture: {}",
            count.get()
        ))?;
        Ok(true)
    })?;
    app.run()
}
