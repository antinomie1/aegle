#[cfg(feature = "text")]
pub(crate) use aegle_types::color_math::encoded_rgba;
pub(crate) use aegle_types::color_math::linear_rgba;
use aegle_types::{Color, color_math::SrgbTransfer};

/// Prepared solid paint; reuse across all covered pixels of one primitive.
pub(crate) struct Solid {
    rgba: [u8; 4],
    linear: [f32; 3],
    alpha: f32,
    transfer: &'static SrgbTransfer,
}

impl Solid {
    pub(crate) fn new(color: Color) -> Self {
        let rgba = color.to_rgba();
        let transfer = SrgbTransfer::get();
        Self {
            rgba,
            linear: std::array::from_fn(|i| transfer.decode(rgba[i] as f32 / 255.0)),
            alpha: rgba[3] as f32 / 255.0,
            transfer,
        }
    }

    /// Blends onto one valid premultiplied RGBA pixel (exactly four bytes).
    /// Coverage is linear geometric coverage, independent of source opacity.
    #[inline]
    pub(crate) fn blend(&self, dst: &mut [u8], coverage: u8) {
        if coverage == 0 || self.rgba[3] == 0 {
            return;
        }
        if coverage == 255 && self.rgba[3] == 255 {
            dst.copy_from_slice(&self.rgba);
            return;
        }
        let source_alpha = self.alpha * (coverage as f32 / 255.0);
        if dst[3] == 0 {
            let alpha = round_u8(source_alpha * 255.0);
            for (i, channel) in dst[..3].iter_mut().enumerate() {
                *channel = (round_u8(self.rgba[i] as f32 * source_alpha)).min(alpha);
            }
            dst[3] = alpha;
            return;
        }
        let source = [
            self.linear[0] * source_alpha,
            self.linear[1] * source_alpha,
            self.linear[2] * source_alpha,
            source_alpha,
        ];
        over_visible(dst, source, self.transfer);
    }
}

/// Source-over of premultiplied linear `source` scaled by `coverage`.
#[inline]
pub(crate) fn blend_linear(dst: &mut [u8], source: [f32; 4], coverage: u8) {
    let source = source.map(|channel| channel * (coverage as f32 / 255.0));
    if source[3] <= 0.0 {
        return;
    }
    let transfer = SrgbTransfer::get();
    if dst[3] == 0 {
        let alpha = round_u8(source[3] * 255.0);
        for (i, channel) in dst[..3].iter_mut().enumerate() {
            let encoded = transfer.encode((source[i] / source[3]).min(1.0));
            *channel = (round_u8(encoded * source[3] * 255.0)).min(alpha);
        }
        dst[3] = alpha;
    } else {
        over_visible(dst, source, transfer);
    }
}

/// Blends onto a pixel with nonzero alpha, in linear light.
#[inline]
fn over_visible(dst: &mut [u8], source: [f32; 4], transfer: &SrgbTransfer) {
    let destination_weight = (dst[3] as f32 / 255.0) * (1.0 - source[3]);
    let output_alpha = source[3] + destination_weight;
    let alpha = round_u8(output_alpha * 255.0);
    let unpremultiply = 1.0 / dst[3] as f32;
    let normalize = 1.0 / output_alpha;
    // Windows are opaque: such a destination's decode depends on its byte only.
    let opaque = (dst[3] == 255).then(opaque_decode);
    for (i, channel) in dst[..3].iter_mut().enumerate() {
        let destination = match opaque {
            Some(decoded) => decoded[*channel as usize],
            None => transfer.decode(*channel as f32 * unpremultiply),
        };
        let linear = ((source[i] + destination * destination_weight) * normalize).min(1.0);
        *channel = (round_u8(transfer.encode(linear) * output_alpha * 255.0)).min(alpha);
    }
    dst[3] = alpha;
}

/// `value.round() as u8`, without the `roundf` call that baseline x86-64
/// makes for `round`: the remainder after truncation is exact below 2^24.
/// Signed truncation keeps it vectorizable; negative, NaN and large values
/// saturate exactly as the cast does.
#[inline]
fn round_u8(value: f32) -> u8 {
    let whole = value as i32;
    let up = i32::from(value - whole as f32 >= 0.5);
    (whole.saturating_add(up)).clamp(0, 255) as u8
}

/// Pixels blended together by the row functions.
const LANES: usize = 8;
/// Pixels whose coverage or source the span functions gather before blending.
const CHUNK: usize = 64;

impl Solid {
    /// Blends the bytes `pixels` of consecutive surface pixels from index
    /// `start`, each with the coverage `coverage(index)` gives.
    pub(crate) fn blend_span(
        &self,
        pixels: &mut [u8],
        start: usize,
        mut coverage: impl FnMut(usize) -> u8,
    ) {
        let mut buffer = [0u8; CHUNK];
        for (chunk, pixels) in pixels.chunks_mut(CHUNK * 4).enumerate() {
            let buffer = &mut buffer[..pixels.len() / 4];
            for (offset, value) in buffer.iter_mut().enumerate() {
                *value = coverage(start + chunk * CHUNK + offset);
            }
            self.blend_row(pixels, buffer);
        }
    }
}

/// [`Solid::blend_span`] with a premultiplied linear source per pixel:
/// `source(index, pixel)` returns it with its coverage, or writes the pixel
/// itself and returns zero coverage.
pub(crate) fn blend_linear_span(
    pixels: &mut [u8],
    start: usize,
    mut source: impl FnMut(usize, &mut [u8]) -> ([f32; 4], u8),
) {
    let (mut sources, mut coverage) = ([[0.0; 4]; CHUNK], [0u8; CHUNK]);
    for (chunk, pixels) in pixels.chunks_mut(CHUNK * 4).enumerate() {
        let count = pixels.len() / 4;
        for (offset, pixel) in pixels.chunks_exact_mut(4).enumerate() {
            (sources[offset], coverage[offset]) = source(start + chunk * CHUNK + offset, pixel);
        }
        blend_linear_row(pixels, &sources[..count], &coverage[..count]);
    }
}

impl Solid {
    /// [`Self::blend`] for each pixel of `pixels` with its `coverage`.
    pub(crate) fn blend_row(&self, pixels: &mut [u8], coverage: &[u8]) {
        for (pixels, coverage) in pixels.chunks_mut(LANES * 4).zip(coverage.chunks(LANES)) {
            let mut lanes = Lanes::default();
            for (offset, (pixel, &coverage)) in pixels.chunks_exact_mut(4).zip(coverage).enumerate()
            {
                if coverage == 0 || self.rgba[3] == 0 {
                    continue;
                }
                if (coverage == 255 && self.rgba[3] == 255) || pixel[3] != 255 {
                    self.blend(pixel, coverage);
                    continue;
                }
                let source_alpha = self.alpha * (coverage as f32 / 255.0);
                let source = [
                    self.linear[0] * source_alpha,
                    self.linear[1] * source_alpha,
                    self.linear[2] * source_alpha,
                    source_alpha,
                ];
                lanes.push(offset, source);
            }
            over_opaque(pixels, &lanes, self.transfer);
        }
    }
}

/// [`blend_linear`] for each pixel of `pixels` with its source and coverage.
pub(crate) fn blend_linear_row(pixels: &mut [u8], sources: &[[f32; 4]], coverage: &[u8]) {
    let chunks = pixels.chunks_mut(LANES * 4);
    for ((pixels, sources), coverage) in chunks
        .zip(sources.chunks(LANES))
        .zip(coverage.chunks(LANES))
    {
        let mut lanes = Lanes::default();
        for (offset, ((pixel, &source), &coverage)) in pixels
            .chunks_exact_mut(4)
            .zip(sources)
            .zip(coverage)
            .enumerate()
        {
            let source = source.map(|channel| channel * (coverage as f32 / 255.0));
            if source[3] <= 0.0 {
                continue;
            }
            if pixel[3] != 255 {
                blend_linear(pixel, source, 255);
                continue;
            }
            lanes.push(offset, source);
        }
        over_opaque(pixels, &lanes, SrgbTransfer::get());
    }
}

/// Pixels of a chunk that need [`over_opaque`], packed into the first lanes.
#[derive(Default)]
struct Lanes {
    /// Pixel offsets within the chunk.
    offsets: [usize; LANES],
    /// Premultiplied linear sources with their coverage applied.
    sources: [[f32; 4]; LANES],
    count: usize,
}

impl Lanes {
    fn push(&mut self, offset: usize, source: [f32; 4]) {
        self.offsets[self.count] = offset;
        self.sources[self.count] = source;
        self.count += 1;
    }
}

/// [`over_visible`] onto the opaque destinations of the packed lanes, as
/// separate per-lane loops with the same operations, which the compiler
/// evaluates several lanes at a time.
fn over_opaque(pixels: &mut [u8], lanes: &Lanes, transfer: &SrgbTransfer) {
    let (count, sources) = (lanes.count, &lanes.sources);
    if count == 0 {
        return;
    }
    let decoded = opaque_decode();
    let mut weight = [0.0f32; LANES];
    let mut output = [0.0f32; LANES];
    let mut normalize = [0.0f32; LANES];
    let mut result = [[0u8; 4]; LANES];
    for lane in 0..count {
        // An opaque destination weighs (255 / 255) * (1 - source alpha).
        weight[lane] = 1.0 * (1.0 - sources[lane][3]);
        output[lane] = sources[lane][3] + weight[lane];
        normalize[lane] = 1.0 / output[lane];
        result[lane][3] = round_u8(output[lane] * 255.0);
    }
    for channel in 0..3 {
        let mut linear = [0.0f32; LANES];
        for lane in 0..count {
            let destination = decoded[pixels[lanes.offsets[lane] * 4 + channel] as usize];
            linear[lane] =
                ((sources[lane][channel] + destination * weight[lane]) * normalize[lane]).min(1.0);
        }
        for lane in 0..count {
            let encoded = transfer.encode(linear[lane]);
            result[lane][channel] = round_u8(encoded * output[lane] * 255.0).min(result[lane][3]);
        }
    }
    for lane in 0..count {
        let offset = lanes.offsets[lane] * 4;
        pixels[offset..offset + 4].copy_from_slice(&result[lane]);
    }
}

/// Linear values of opaque sRGB bytes, computed as `over_visible` would.
fn opaque_decode() -> &'static [f32; 256] {
    static TABLE: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let (transfer, unpremultiply) = (SrgbTransfer::get(), 1.0 / 255.0f32);
        std::array::from_fn(|byte| transfer.decode(byte as f32 * unpremultiply))
    })
}
