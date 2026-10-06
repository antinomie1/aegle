// A flat primitive index and its immutable parent chain are uniform for every
// fragment of one primitive; storage loads obscure that from WGSL analysis.
diagnostic(off, derivative_uniformity);

struct Primitive {
    bounds: vec4<f32>,
    row0: vec4<f32>,
    row1: vec4<f32>,
    rect: vec4<f32>, // shape rect or atlas origin and glyph size, excluding gutter
    params: vec4<f32>, // radius or mask contrast, stroke width (-1 for fill), viewport size (negative height flips Y)
    color: vec4<f32>, // linear premultiplied paint, or repeated color-glyph opacity
    header: vec4<u32>, // clip head, geometry/mask/color-glyph/image kind, atlas page or first stop row, effect | stop count << 8
}

struct Clip {
    row0: vec4<f32>,
    row1: vec4<f32>,
    rect: vec4<f32>,
    extra: vec4<u32>, // radius bits, parent index, reserved
}
// Gradient stops reuse clip rows, two per row: colors in row0 and row1,
// offsets in rect.xy.

struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) index: u32,
}

@group(0) @binding(0) var<storage, read> clips: array<Clip>;
@group(0) @binding(1) var<storage, read> primitives: array<Primitive>;
@group(1) @binding(0) var glyph_page: texture_2d<f32>;
@group(1) @binding(1) var glyph_sampler: sampler;

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

fn coverage(point: vec2<f32>, rect: vec4<f32>, radius: f32) -> f32 {
    // Integrating opposing straight edges preserves subpixel rectangles and
    // fractional axis-aligned clips; SDF derivatives alone can flatten between
    // two equally distant samples and turn a thin rectangle fully opaque.
    let footprint = max(fwidth(point), vec2(0.000001));
    let far = clamp((rect.xy + rect.zw - point) / footprint + 0.5, vec2(0.0), vec2(1.0));
    let near = clamp((rect.xy - point) / footprint + 0.5, vec2(0.0), vec2(1.0));
    let box_alpha = (far.x - near.x) * (far.y - near.y);
    if radius == 0.0 {
        return box_alpha;
    }
    let half_size = rect.zw * 0.5;
    let q = abs(point - rect.xy - half_size) - half_size + radius;
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
        alpha *= coverage(local, clip.rect, bitcast<f32>(clip.extra.x));
        head = clip.extra.y;
    }
    return alpha;
}

fn stop_color(first: u32, index: u32) -> vec4<f32> {
    let row = clips[first + index / 2u];
    return select(row.row0, row.row1, index % 2u == 1u);
}

fn stop_offset(first: u32, index: u32) -> f32 {
    return clips[first + index / 2u].rect[index % 2u];
}

// Premultiplied linear interpolation, padded with the end colors.
fn gradient(primitive: Primitive, point: vec2<f32>) -> vec4<f32> {
    let g = primitive.color;
    var t: f32;
    if (primitive.header.w & 0xffu) == 1u {
        let direction = g.zw - g.xy;
        t = dot(point - g.xy, direction) / dot(direction, direction);
    } else {
        t = length(point - g.xy) / g.z;
    }
    let first = primitive.header.z;
    var color = stop_color(first, 0u);
    var offset = stop_offset(first, 0u);
    if t <= offset {
        return color;
    }
    let count = primitive.header.w / 256u;
    for (var index = 1u; index < count; index += 1u) {
        let next = stop_color(first, index);
        let next_offset = stop_offset(first, index);
        if t < next_offset {
            return mix(color, next, (t - offset) / (next_offset - offset));
        }
        color = next;
        offset = next_offset;
    }
    return color;
}

// Abramowitz–Stegun erf approximation, within 5e-4.
fn erf2(x: vec2<f32>) -> vec2<f32> {
    let s = sign(x);
    let a = abs(x);
    var r = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
    r = r * r;
    return s - s / (r * r);
}

// Horizontal integral of the shadow at row offset y from the shape center.
fn shadow_row(x: f32, y: f32, sigma: f32, corner: f32, half: vec2<f32>) -> f32 {
    let delta = min(half.y - corner - abs(y), 0.0);
    let curved = half.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    let integral = 0.5 + 0.5 * erf2((x + vec2(-curved, curved)) * (0.70710677 / sigma));
    return integral.y - integral.x;
}

// A rounded rectangle convolved with a Gaussian: exact across x; across y,
// four rows weighted by the Gaussian's exact mass over their intervals within
// four deviations, so straight edges are exact (after Evan Wallace).
fn shadow(point: vec2<f32>, rect: vec4<f32>, corner: f32, sigma: f32) -> f32 {
    let half = rect.zw * 0.5;
    let p = point - rect.xy - half;
    let start = clamp(-4.0 * sigma, p.y - half.y, p.y + half.y);
    let end = clamp(4.0 * sigma, p.y - half.y, p.y + half.y);
    let step = (end - start) * 0.25;
    let scale = 0.70710677 / sigma;
    var value = 0.0;
    for (var index = 0; index < 4; index += 1) {
        let edges = start + step * vec2(f32(index), f32(index + 1));
        let mass = erf2(edges * scale);
        let y = (edges.x + edges.y) * 0.5;
        value += shadow_row(p.x, p.y - y, sigma, corner, half) * 0.5 * (mass.y - mass.x);
    }
    return clamp(value, 0.0, 1.0);
}

@fragment
fn fs_main(input: Vertex) -> @location(0) vec4<f32> {
    let primitive = primitives[input.index];
    let position = input.position;
    let point = local_point(position.xy, primitive.row0, primitive.row1);
    let radius = primitive.params.x;
    let width = primitive.params.y;
    let effect = primitive.header.w & 0xffu;
    var paint = primitive.color;
    var alpha: f32;
    if effect == 3u {
        alpha = shadow(point, primitive.rect, radius, width);
    } else if width < 0.0 {
        alpha = coverage(point, primitive.rect, radius);
    } else {
        let half_width = width * 0.5;
        let outer = vec4(primitive.rect.xy - half_width, primitive.rect.zw + width);
        // A square stroke keeps its mitered corners. abs(SDF) would round them.
        let outer_radius = select(radius + half_width, 0.0, radius == 0.0);
        alpha = coverage(point, outer, outer_radius);
        if all(primitive.rect.zw > vec2(width)) {
            let inner = vec4(primitive.rect.xy + half_width, primitive.rect.zw - width);
            alpha = max(alpha - coverage(point, inner, max(radius - half_width, 0.0)), 0.0);
        }
    }
    if effect == 1u || effect == 2u {
        paint = gradient(primitive, point);
    }
    // No coverage-dependent branch precedes derivatives in the clip chain.
    return paint * (alpha * clip_coverage(position.xy, primitive.header.x));
}

@fragment
fn fs_text(input: Vertex) -> @location(0) vec4<f32> {
    let primitive = primitives[input.index];
    let position = input.position;
    let local = local_point(position.xy, primitive.row0, primitive.row1);
    // A rotated device bounding box includes points far outside the glyph quad.
    // Clamp to transparent gutter texel centers, never a neighboring allocation.
    // Images (kind 3) clamp to their edge texels and take analytic edge
    // coverage instead, so scaled images keep sharp boundaries.
    let image = primitive.header.y == 3u;
    let edge = select(1.0, coverage(local, vec4(vec2(0.0), primitive.rect.zw), 0.0), image);
    let inset = select(vec2(-0.5), vec2(0.5), image);
    let bounded = clamp(local, inset, primitive.rect.zw - inset);
    let uv = (primitive.rect.xy + bounded) / vec2<f32>(textureDimensions(glyph_page));
    let texel = textureSampleLevel(glyph_page, glyph_sampler, uv, 0.0);
    // R8 coverage, after the shared contrast curve, modulates the complete
    // premultiplied paint. Color atlas texels decode/filter as linear
    // premultiplied RGBA and receive only run opacity.
    let mask = texel.r + texel.r * (1.0 - texel.r) * primitive.params.x;
    let sampled = select(texel, vec4(mask), primitive.header.y == 1u);
    return sampled * primitive.color * (edge * clip_coverage(position.xy, primitive.header.x));
}
