// The immutable parent chain is uniform for every fragment of one primitive.
// Storage-buffer loads obscure that fact from the WGSL uniformity analysis.
diagnostic(off, derivative_uniformity);

struct Primitive {
    bounds: vec4<f32>,
    row0: vec4<f32>,
    row1: vec4<f32>,
    rect: vec4<f32>, // shape rect or atlas origin and glyph size, excluding gutter
    params: vec4<f32>, // radius, stroke width (-1 for fill), viewport width/height
    color: vec4<f32>, // linear premultiplied paint, or repeated color-glyph opacity
    header: vec4<u32>, // clip head, geometry/mask/color kind, CPU atlas page, reserved
}

struct Clip {
    row0: vec4<f32>,
    row1: vec4<f32>,
    rect: vec4<f32>,
    extra: vec4<u32>, // radius bits, parent index, reserved
}

var<immediate> primitive: Primitive;
@group(0) @binding(0) var<storage, read> clips: array<Clip>;
@group(1) @binding(0) var glyph_page: texture_2d<f32>;
@group(1) @binding(1) var glyph_sampler: sampler;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corners = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
    );
    let point = mix(primitive.bounds.xy, primitive.bounds.zw, corners[index]);
    // Positive Vulkan viewport height; the SPIR-V writer must not flip Y.
    return vec4(point / primitive.params.zw * 2.0 - 1.0, 0.0, 1.0);
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

fn clip_coverage(position: vec2<f32>) -> f32 {
    var alpha = 1.0;
    var head = primitive.header.x;
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

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let point = local_point(position.xy, primitive.row0, primitive.row1);
    let radius = primitive.params.x;
    let width = primitive.params.y;
    var alpha: f32;
    if width < 0.0 {
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
    // No coverage-dependent branch precedes derivatives in the clip chain.
    return primitive.color * (alpha * clip_coverage(position.xy));
}

@fragment
fn fs_text(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let local = local_point(position.xy, primitive.row0, primitive.row1);
    // A rotated device bounding box includes points far outside the glyph quad.
    // Clamp to transparent gutter texel centers, never a neighboring allocation.
    let bounded = clamp(local, vec2(-0.5), primitive.rect.zw + 0.5);
    let uv = (primitive.rect.xy + bounded) / vec2<f32>(textureDimensions(glyph_page));
    let texel = textureSampleLevel(glyph_page, glyph_sampler, uv, 0.0);
    // R8 coverage modulates the complete premultiplied paint. Color atlas texels
    // decode/filter as linear premultiplied RGBA and receive only run opacity.
    let sampled = select(texel, vec4(texel.r), primitive.header.y == 1u);
    return sampled * primitive.color * clip_coverage(position.xy);
}
