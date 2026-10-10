//! An application image on the renderer's device draws like a scene image;
//! unregistered textures and too many textures per frame fail the frame.
#![cfg(feature = "text")]
#![allow(unsafe_code)]

use aegle_render_vulkan::{Error, Options, Renderer};
use aegle_scene::{Affine, Color, Rect, SceneBuilder};
use ash::vk;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

/// A 4×4 sRGB image cleared to `linear` and left in shader-read layout.
struct Image {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
}

fn cleared(raw: &aegle_render_vulkan::RawDevice, linear: [f32; 4]) -> Result<Image> {
    let device = &raw.device;
    let range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1);
    // SAFETY: every handle comes from this device; the submission is fenced
    // before return, and the queue is not used by the renderer concurrently.
    unsafe {
        let image = device.create_image(
            &vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_SRGB)
                .extent(vk::Extent3D {
                    width: 4,
                    height: 4,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST),
            None,
        )?;
        let requirements = device.get_image_memory_requirements(image);
        let properties = raw
            .instance
            .get_physical_device_memory_properties(raw.physical);
        let kind = (0..properties.memory_type_count)
            .find(|&i| {
                requirements.memory_type_bits & (1 << i) != 0
                    && properties.memory_types[i as usize]
                        .property_flags
                        .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
            })
            .ok_or("no device-local memory type")?;
        let memory = device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(kind),
            None,
        )?;
        device.bind_image_memory(image, memory, 0)?;
        let view = device.create_image_view(
            &vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_SRGB)
                .subresource_range(range),
            None,
        )?;
        let pool = device.create_command_pool(
            &vk::CommandPoolCreateInfo::default().queue_family_index(raw.family),
            None,
        )?;
        let buffer = device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )?[0];
        device.begin_command_buffer(
            buffer,
            &vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
        )?;
        let barrier = |old, new, src, dst| {
            vk::ImageMemoryBarrier::default()
                .old_layout(old)
                .new_layout(new)
                .src_access_mask(src)
                .dst_access_mask(dst)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(range)
        };
        device.cmd_pipeline_barrier(
            buffer,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier(
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_WRITE,
            )],
        );
        device.cmd_clear_color_image(
            buffer,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &vk::ClearColorValue { float32: linear },
            &[range],
        );
        device.cmd_pipeline_barrier(
            buffer,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier(
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
            )],
        );
        device.end_command_buffer(buffer)?;
        let fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
        let buffers = [buffer];
        device.queue_submit(
            raw.queue,
            &[vk::SubmitInfo::default().command_buffers(&buffers)],
            fence,
        )?;
        device.wait_for_fences(&[fence], true, u64::MAX)?;
        device.destroy_fence(fence, None);
        device.destroy_command_pool(pool, None);
        Ok(Image {
            image,
            memory,
            view,
        })
    }
}

#[test]
#[ignore = "requires an explicitly selected Vulkan ICD/device"]
fn registered_images_draw_until_unregistered() -> Result {
    let mut renderer = Renderer::new(Options::default())?;
    let raw = renderer.raw_device();
    // Linear 0.2158 encodes to sRGB 128.
    let image = cleared(&raw, [1.0, 0.2158, 0.0, 1.0])?;
    // SAFETY: the view is a sampled sRGB color view of this device in
    // shader-read layout, kept alive until after the renderer waits below.
    let id = unsafe { renderer.register_texture(image.view, [4, 4])? };
    let mut builder = SceneBuilder::new();
    builder.texture(id, Rect::new(8.0, 8.0, 16.0, 16.0));
    let scene = builder.finish();
    let mut frame = renderer.begin_frame(32, 32, Color::BLACK)?;
    frame.draw(&scene, Affine::IDENTITY)?;
    frame.finish()?;
    let mut pixels = vec![0; 32 * 32 * 4];
    renderer.read_pixels(&mut pixels)?;
    let at = |x: usize, y: usize| &pixels[(y * 32 + x) * 4..][..4];
    let inside = at(16, 16);
    assert!(
        inside[0] >= 253 && inside[1].abs_diff(128) <= 2 && inside[2] <= 2 && inside[3] == 255,
        "{inside:?}"
    );
    assert_eq!(at(2, 2), [0, 0, 0, 255]);

    // Seventeen distinct textures exceed one submission's reserved bindings:
    // the frame is drawn in two parts.
    let mut many = SceneBuilder::new();
    let ids: Vec<_> = (0..17)
        // SAFETY: as above.
        .map(|_| unsafe { renderer.register_texture(image.view, [4, 4]) })
        .collect::<std::result::Result<_, _>>()?;
    for (index, &extra) in ids.iter().enumerate() {
        let (x, y) = ((index % 8) as f32 * 4.0, (index / 8) as f32 * 4.0);
        many.texture(extra, Rect::new(x, y, 4.0, 4.0));
    }
    let many = many.finish();
    let mut frame = renderer.begin_frame(32, 32, Color::BLACK)?;
    frame.draw(&many, Affine::IDENTITY)?;
    frame.finish()?;
    let mut parts = vec![0; 32 * 32 * 4];
    renderer.read_pixels(&mut parts)?;
    for index in 0..17 {
        let (x, y) = ((index % 8) * 4 + 2, (index / 8) * 4 + 2);
        let pixel = &parts[(y * 32 + x) * 4..][..4];
        assert!(
            pixel[0] >= 253 && pixel[3] == 255,
            "texture {index}: {pixel:?}"
        );
    }
    // Textures clipped out entirely take no binding: the sixteen above the
    // clip leave room for the seventeenth in a single submission.
    let mut frame = renderer.begin_frame(32, 32, Color::BLACK)?;
    frame.draw_clipped(
        &many,
        Affine::IDENTITY,
        Some(Rect::new(0.0, 8.0, 32.0, 24.0)),
    )?;
    frame.finish()?;
    renderer.read_pixels(&mut parts)?;
    assert_eq!(&parts[(2 * 32 + 2) * 4..][..4], [0, 0, 0, 255]);
    assert!(parts[(10 * 32 + 2) * 4] >= 253);

    assert!(renderer.unregister_texture(id) && !renderer.unregister_texture(id));
    let mut frame = renderer.begin_frame(32, 32, Color::BLACK)?;
    assert!(matches!(
        frame.draw(&scene, Affine::IDENTITY),
        Err(Error::UnknownTexture)
    ));
    assert!(frame.finish().is_err());
    renderer.wait()?;
    for extra in ids {
        renderer.unregister_texture(extra);
    }
    // SAFETY: no frame uses the image any longer.
    unsafe {
        raw.device.destroy_image_view(image.view, None);
        raw.device.destroy_image(image.image, None);
        raw.device.free_memory(image.memory, None);
    }
    Ok(())
}
