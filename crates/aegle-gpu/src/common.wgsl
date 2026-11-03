// Shared by the geometry shader and custom fragment programs: records,
// storage, the instanced quad and shape and clip coverage.

// A flat primitive index and its immutable parent chain are uniform for every
// fragment of one primitive; storage loads obscure that from WGSL analysis.
diagnostic(off, derivative_uniformity);

// Geometry and clip rows end in the corner radii, see `radii`.
struct Primitive {
    bounds: vec4<f32>,
    row0: vec4<f32>,
    row1: vec4<f32>,
    rect: vec4<f32>, // shape rect or atlas origin and glyph size, excluding gutter
    params: vec4<f32>, // mask contrast, stroke width (-1 for fill), viewport size (negative height flips Y)
    color: vec4<f32>, // linear premultiplied paint, or repeated color-glyph opacity
    header: vec4<u32>, // clip head, geometry/mask/color-glyph/image kind, atlas page or first stop row, effect | stop count << 8
}

struct Clip {
    row0: vec4<f32>,
    row1: vec4<f32>,
    rect: vec4<f32>,
    extra: vec4<u32>, // reserved, parent index, reserved
}
// Gradient stops reuse clip rows, two per row: colors in row0 and row1,
// offsets in rect.xy.

struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) index: u32,
}

@group(0) @binding(0) var<storage, read> clips: array<Clip>;
@group(0) @binding(1) var<storage, read> primitives: array<Primitive>;

// One instance per primitive, so adjacent primitives share a single draw.
@vertex
fn vs_main(
    @builtin(vertex_index) index: u32,
    @builtin(instance_index) instance: u32,
) -> Vertex {
    let primitive = primitives[instance];
    let corners = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
    );
    let point = mix(primitive.bounds.xy, primitive.bounds.zw, corners[index]);
    // A positive viewport height keeps Vulkan's downward clip space. A negative
    // one selects WebGPU's upward clip space, which flips device Y.
    var ndc = point / abs(primitive.params.zw) * 2.0 - 1.0;
    ndc.y = ndc.y * sign(primitive.params.w);
    return Vertex(vec4(ndc, 0.0, 1.0), instance);
}

fn local_point(point: vec2<f32>, row0: vec4<f32>, row1: vec4<f32>) -> vec2<f32> {
    let homogeneous = vec3(point, 1.0);
    return vec2(dot(row0.xyz, homogeneous), dot(row1.xyz, homogeneous));
}

// Top-left, top-right, bottom-right and bottom-left radii, packed as 16-bit
// fractions of the normalized unit in the rows' w lanes.
fn unpack_radii(row0: vec4<f32>, row1: vec4<f32>) -> vec4<f32> {
    return vec4(unpack2x16unorm(bitcast<u32>(row0.w)), unpack2x16unorm(bitcast<u32>(row1.w)));
}

// The radius of the corner in the quadrant of `p`, relative to the center.
fn corner(p: vec2<f32>, radii: vec4<f32>) -> f32 {
    let side = select(radii.xw, radii.yz, p.x > 0.0);
    return select(side.x, side.y, p.y > 0.0);
}

fn coverage(point: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>) -> f32 {
    // Integrating opposing straight edges preserves subpixel rectangles and
    // fractional axis-aligned clips; SDF derivatives alone can flatten between
    // two equally distant samples and turn a thin rectangle fully opaque.
    let footprint = max(fwidth(point), vec2(0.000001));
    let far = clamp((rect.xy + rect.zw - point) / footprint + 0.5, vec2(0.0), vec2(1.0));
    let near = clamp((rect.xy - point) / footprint + 0.5, vec2(0.0), vec2(1.0));
    let box_alpha = (far.x - near.x) * (far.y - near.y);
    if all(radii == vec4(0.0)) {
        return box_alpha;
    }
    let half_size = rect.zw * 0.5;
    let p = point - rect.xy - half_size;
    let radius = corner(p, radii);
    let q = abs(p) - half_size + radius;
    let distance = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
    return min(box_alpha, clamp(0.5 - distance / max(fwidth(distance), 0.000001), 0.0, 1.0));
}

fn clip_coverage(position: vec2<f32>, first: u32) -> f32 {
    var alpha = 1.0;
    var head = first;
    for (var depth = 0u; depth < 8u; depth += 1u) {
        if head == 0xffffffffu {
            break;
        }
        let clip = clips[head];
        let local = local_point(position, clip.row0, clip.row1);
        alpha *= coverage(local, clip.rect, unpack_radii(clip.row0, clip.row1));
        head = clip.extra.y;
    }
    return alpha;
}
