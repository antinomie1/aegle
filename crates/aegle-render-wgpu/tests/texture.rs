//! An application texture on the renderer's device is drawn stretched and
//! filtered like an image; unknown or unregistered textures fail the frame.
#![cfg(feature = "text")]

use aegle_render_wgpu::{Error, Options, Renderer};
use aegle_scene::{Affine, Color, Rect, SceneBuilder};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

#[test]
#[ignore = "requires a GPU adapter; select one with WGPU_BACKEND / WGPU_ADAPTER_NAME"]
fn registered_textures_draw_until_unregistered() -> Result {
    let mut renderer = Renderer::new(Options::default())?;
    // Opaque 2×2 texture: red, green / blue, white.
    let texture = renderer.device().create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let texels = [
        255u8, 0, 0, 255, 0, 255, 0, 255, //
        0, 0, 255, 255, 255, 255, 255, 255,
    ];
    renderer.queue().write_texture(
        texture.as_image_copy(),
        &texels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(2),
        },
        wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    let id = renderer.register_texture(&texture)?;
    let mut builder = SceneBuilder::new();
    builder.texture(id, Rect::new(0.0, 0.0, 32.0, 32.0))?;
    let scene = builder.finish()?;

    let mut frame = renderer.begin_frame(32, 32, Color::BLACK)?;
    frame.draw(&scene, Affine::IDENTITY)?;
    frame.finish()?;
    let mut pixels = vec![0; 32 * 32 * 4];
    renderer.read_pixels(&mut pixels)?;
    let at = |x: usize, y: usize| &pixels[(y * 32 + x) * 4..][..4];
    // Texel centers land at 8 and 24; corners stay pure, the middle blends.
    assert_eq!(at(2, 2), [255, 0, 0, 255]);
    assert_eq!(at(29, 2), [0, 255, 0, 255]);
    assert_eq!(at(2, 29), [0, 0, 255, 255]);
    assert_eq!(at(29, 29), [255, 255, 255, 255]);
    let middle = at(16, 16);
    assert!(
        middle.iter().take(3).all(|&c| c > 100 && c < 220),
        "{middle:?}"
    );

    let storage = renderer.device().create_texture(&wgpu::TextureDescriptor {
        usage: wgpu::TextureUsages::COPY_DST,
        ..texture_descriptor()
    });
    assert!(matches!(
        renderer.register_texture(&storage),
        Err(Error::Unsupported(_))
    ));
    assert!(renderer.unregister_texture(id) && !renderer.unregister_texture(id));
    let mut frame = renderer.begin_frame(32, 32, Color::BLACK)?;
    assert!(matches!(
        frame.draw(&scene, Affine::IDENTITY),
        Err(Error::UnknownTexture)
    ));
    assert!(frame.finish().is_err());
    Ok(())
}

#[test]
#[ignore = "requires a GPU adapter; select one with WGPU_BACKEND / WGPU_ADAPTER_NAME"]
fn device_errors_are_returned_instead_of_panicking() -> Result {
    let mut renderer = Renderer::new(Options::default())?;
    drop(renderer.begin_frame(4, 4, Color::BLACK)?);
    // Invalid work on the shared device poisons it for every later frame.
    let mut empty = texture_descriptor();
    empty.size.width = 0;
    drop(renderer.device().create_texture(&empty));
    assert!(matches!(
        renderer.begin_frame(4, 4, Color::BLACK),
        Err(Error::Gpu(_))
    ));
    Ok(())
}

fn texture_descriptor() -> wgpu::TextureDescriptor<'static> {
    wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    }
}
