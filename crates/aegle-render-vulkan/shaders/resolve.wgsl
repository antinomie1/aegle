@group(0) @binding(0) var linear_image: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corners = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(corners[index], 0.0, 1.0);
}

fn encode_srgb(linear: vec3<f32>) -> vec3<f32> {
    return select(
        1.055 * pow(linear, vec3(1.0 / 2.4)) - 0.055,
        12.92 * linear,
        linear <= vec3(0.0031308),
    );
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let premul = textureLoad(linear_image, vec2<i32>(position.xy), 0);
    let alpha = clamp(premul.a, 0.0, 1.0);
    if alpha == 0.0 {
        return vec4(0.0);
    }
    let straight = clamp(premul.rgb / alpha, vec3(0.0), vec3(1.0));
    // Write UNORM with blending disabled: encode before premultiplying.
    return vec4(encode_srgb(straight) * alpha, alpha);
}
