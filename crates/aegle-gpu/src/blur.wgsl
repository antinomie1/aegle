// One box blur pass over a whole scratch image, drawn as a single
// full-target triangle. The instance index carries the pass: the axis in bit
// 31 (0 horizontal, 1 vertical), the box size in bits 16..31 and the pixels
// before the output pixel in bits 0..16. Reads past the image repeat its
// edge texels.

@group(0) @binding(0) var source: texture_2d<f32>;

struct Pass {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) step: u32,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32, @builtin(instance_index) step: u32) -> Pass {
    let corners = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return Pass(vec4(corners[index], 0.0, 1.0), step);
}

@fragment
fn fs_main(input: Pass) -> @location(0) vec4<f32> {
    let axis = select(vec2(1, 0), vec2(0, 1), (input.step >> 31u) == 1u);
    let size = i32((input.step >> 16u) & 0x7fffu);
    let left = i32(input.step & 0xffffu);
    let last = vec2<i32>(textureDimensions(source)) - vec2(1);
    let center = vec2<i32>(input.position.xy);
    var sum = vec4(0.0);
    for (var k = 0; k < size; k += 1) {
        let texel = clamp(center + axis * (k - left), vec2(0), last);
        sum += textureLoad(source, texel, 0);
    }
    return sum / f32(size);
}
