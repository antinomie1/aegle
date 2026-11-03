// Fills, strokes, gradients, shadows and atlas texels, after common.wgsl.

@group(1) @binding(0) var glyph_page: texture_2d<f32>;
@group(1) @binding(1) var glyph_sampler: sampler;

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

// Horizontal integral of the shadow at row offset y from the shape center,
// whose left and right edges follow the corners on the row's side.
fn shadow_row(x: f32, y: f32, sigma: f32, radii: vec4<f32>, half: vec2<f32>) -> f32 {
    let corners = select(radii.xy, radii.wz, y > 0.0);
    let delta = min(half.y - corners - abs(y), vec2(0.0));
    let curved = half.x - corners + sqrt(max(vec2(0.0), corners * corners - delta * delta));
    let integral = 0.5 + 0.5 * erf2((x + vec2(-curved.y, curved.x)) * (0.70710677 / sigma));
    return integral.y - integral.x;
}

// A rounded rectangle convolved with a Gaussian: exact across x; across y,
// four rows weighted by the Gaussian's exact mass over their intervals within
// four deviations, so straight edges are exact (after Evan Wallace).
fn shadow(point: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>, sigma: f32) -> f32 {
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
        value += shadow_row(p.x, p.y - y, sigma, radii, half) * 0.5 * (mass.y - mass.x);
    }
    return clamp(value, 0.0, 1.0);
}

@fragment
fn fs_main(input: Vertex) -> @location(0) vec4<f32> {
    let primitive = primitives[input.index];
    let position = input.position;
    let point = local_point(position.xy, primitive.row0, primitive.row1);
    let radii = unpack_radii(primitive.row0, primitive.row1);
    let width = primitive.params.y;
    let effect = primitive.header.w & 0xffu;
    var paint = primitive.color;
    var alpha: f32;
    if effect == 3u {
        alpha = shadow(point, primitive.rect, radii, width);
    } else if effect == 4u {
        // Inside the clip of the shaded shape, away from the hole.
        alpha = 1.0 - shadow(point, primitive.rect, radii, width);
    } else if width < 0.0 {
        alpha = coverage(point, primitive.rect, radii);
    } else {
        let half_width = width * 0.5;
        let outer = vec4(primitive.rect.xy - half_width, primitive.rect.zw + width);
        // A square stroke keeps its mitered corners. abs(SDF) would round them.
        let outer_radii = select(radii + half_width, vec4(0.0), radii == vec4(0.0));
        alpha = coverage(point, outer, outer_radii);
        if all(primitive.rect.zw > vec2(width)) {
            let inner = vec4(primitive.rect.xy + half_width, primitive.rect.zw - width);
            alpha = max(alpha - coverage(point, inner, max(radii - half_width, vec4(0.0))), 0.0);
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
    let edge = select(1.0, coverage(local, vec4(vec2(0.0), primitive.rect.zw), vec4(0.0)), image);
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
