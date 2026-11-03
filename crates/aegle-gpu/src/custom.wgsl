// The entry of a custom fragment program, after common.wgsl, the globals and
// the program's `shade`.

@fragment
fn fs_shader(input: Vertex) -> @location(0) vec4<f32> {
    let primitive = primitives[input.index];
    let position = input.position;
    let point = local_point(position.xy, primitive.row0, primitive.row1);
    // Coverage first: its derivatives must not follow the program's branches.
    let alpha = coverage(point, primitive.rect, unpack_radii(primitive.row0, primitive.row1))
        * clip_coverage(position.xy, primitive.header.x);
    let row = clips[primitive.header.w];
    time = primitive.params.x;
    values = array(
        row.row0.x, row.row0.y, row.row0.z, row.row0.w,
        row.row1.x, row.row1.y, row.row1.z, row.row1.w,
    );
    let uv = (point - primitive.rect.xy) / primitive.rect.zw;
    let color = clamp(shade(uv, primitive.color.xy), vec4(0.0), vec4(1.0));
    // Unpremultiplied sRGB to linear premultiplied, like every scene color.
    let low = color.rgb / 12.92;
    let high = pow((color.rgb + 0.055) / 1.055, vec3(2.4));
    let linear = select(high, low, color.rgb <= vec3(0.04045));
    return vec4(linear * color.a, color.a) * alpha;
}
