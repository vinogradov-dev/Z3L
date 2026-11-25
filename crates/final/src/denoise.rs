//! The noise a render leaves behind, and the filter that takes it away.
//!
//! A path tracer at the sample counts a workshop waits for is grainy: the error of a Monte Carlo
//! estimator falls with the inverse square root of its samples, so the four-fold work that halves it
//! is the difference between a catalogue image and a preview. This module is the answer a production
//! renderer gives — a spatial filter guided by what each pixel actually saw — written once, in Rust,
//! and run by the host, so the filter that ships is the filter the tests measure. There is no second
//! implementation in WGSL to drift away from it.
//!
//! Three things travel beside a pixel's radiance. Its **variance** is tracked by the sample loop
//! (`gpu/shader.wgsl` accumulates the sum of squares beside the sum) and is the error bar everything
//! here reasons with. Its **guide** is what the primary ray met — the surface's albedo, the shading
//! normal that faced it and how far away it was — and it is where an edge lives: two neighbouring
//! pixels belong to one surface only when all three agree. The **edge-avoiding à-trous wavelet**
//! (Dammertz et al. 2010) then widens its kernel level by level, so five passes see a 32-pixel
//! neighbourhood while every tap is still stopped by the guide.
//!
//! The blend is ours: `α = r² / (r² + t²)`, where `r` is the pixel's own relative error and `t` is the
//! error a catalogue image tolerates. A converged pixel keeps its raw value, so the filter leaves no
//! permanent blur behind it — the floor a fixed blend would put under every image — and a noisy pixel
//! takes the filtered value. The same test, `r < t` after [`MIN_SAMPLES`], is what lets the tracer
//! retire a pixel early: one threshold, one meaning, two savings.
//!
//! What this is not: there is no temporal history (a final render is one image, not a sequence), no
//! albedo demodulation (the guide is an edge stop, which is enough for a picture made of flat boards)
//! and no learned model (a denoiser we cannot explain is a denoiser we cannot debug). The reference
//! project's filter was measured before this was written — `plan/notes/Y1.md` and the note of Y4 — and
//! its numbers are a hypothesis, not a specification.
//!
//! Refuses: a frame whose pixel or guide count does not match its size is a [`FinalError::Accumulation`]
//! sentence, never a shifted image. A value that is not finite is treated as no light (a NaN radiance
//! has no mean to protect), and no arithmetic here divides by zero.
//!
//! Changes when: what the guide carries, the kernel, the threshold a pixel is held to, or the cost of
//! one image changes.

use crate::FinalError;

/// How many à-trous levels the filter runs.
///
/// The noise of a path tracer is white — one pixel's error says nothing about its neighbour's — so the
/// first level takes most of it: the five-by-five kernel's own spread is a quarter of one tap's, and
/// three levels reach eight pixels with a factor of a hundred off the noise. More levels smooth further
/// and buy a *bias* instead: a board lit by a lamp is convex, and a wider kernel flattens the falloff
/// under it. Three is where the measurements of the note put the trade, and the entry allows three.
pub const LEVELS: u32 = 3;

/// The fewest samples a pixel takes before it may retire early.
///
/// A variance estimated from one or two samples says nothing, so a pixel is traced to at least this
/// count whatever the picture looks like; below it the filter also trusts itself rather than the error
/// bar, because there is no error bar yet.
pub const MIN_SAMPLES: u32 = 32;

/// The relative error a catalogued image tolerates: a fiftieth of a code value, a fifth of a percent.
///
/// The file this crate writes has eight bits per channel, and one code value of a mid grey is about
/// four tenths of a percent of its own light, so a fifth of a percent is under a tenth of a code value
/// — the quantisation of the file hides it. Below this the filter stops replacing a pixel and the
/// tracer stops tracing it: one threshold, one meaning, two savings.
pub const TARGET_RELATIVE_ERROR: f32 = 0.002;

/// The luminance a pixel's noise is measured against, Rec. 709's weights.
pub const LUMINANCE: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// The dimmest light a relative error is measured against, in linear radiance.
///
/// A black pixel has no relative error — `0.2/0.0` says nothing — and this is dark enough that the
/// absolute error there is invisible. Without the floor every pixel of a shadow would divide by
/// nothing and never converge.
pub const LIGHT_FLOOR: f32 = 1.0e-3;

/// How different two albedos must be for the filter to treat them as two surfaces.
///
/// The stop falls as `1 / (1 + (Δ/σ)²)`, so an albedo step of this size halves the weight and one of
/// five times this size keeps a hundredth of it: a decor's grain or a dark seam survives, while two
/// boards of the same paint are one surface and are filtered together. The stop is tight because the
/// guide is exact — it is one primary hit, not an estimate — so stopping on a real paint line costs
/// nothing but the noise that line would have smoothed.
pub const SIGMA_ALBEDO: f32 = 0.05;

/// How fast a turn of the surface stops the filter, as the power of the cosine the normals make.
///
/// Two normals ten degrees apart keep 0.61 of their weight, thirty degrees keep 0.01 and a fold of
/// ninety degrees keeps nothing, so a cabinet's corners are never blurred while a curved shell is.
pub const SIGMA_NORMAL: i32 = 32;

/// How far a neighbour may sit off the plane the centre pixel's depth implies.
///
/// A distance in millimetres would be wrong on a surface seen at a glancing angle, whose depth changes
/// by hundreds of millimetres across the image: the tolerance is this share of the pixel's own
/// distance, so three metres of distance tolerates sixty millimetres.
pub const SIGMA_DISTANCE: f32 = 0.02;

/// The smallest tolerance of the distance stop, in millimetres, so a close-up is not exact to the bit.
pub const DISTANCE_FLOOR: f32 = 1.0;

/// The distance a primary ray reports when it met nothing, in millimetres.
///
/// The tracer's own far plane: the background is one surface at one distance, so a ray that leaves the
/// room is never mixed into a board in front of it.
pub const FAR_AWAY: f32 = 1.0e7;

/// The fewest pixels worth sharing a level between threads, so a unit test is not a thread each.
const PARALLEL_PIXELS: usize = 1 << 17;

/// B3-spline weights of one tap, the kernel of the à-trous pass; they sum to one.
const KERNEL: [f32; 5] = [1.0 / 16.0, 1.0 / 4.0, 3.0 / 8.0, 1.0 / 4.0, 1.0 / 16.0];

/// What the primary ray met at one pixel: the surface's albedo, its normal and its distance.
///
/// This is the whole of the filter's knowledge of the scene, and it is deliberately small: a filter
/// that knows the material stops at a paint line, and a filter that knows the geometry stops at a
/// fold. A ray that met nothing reports the studio's own colour, its own direction and [`FAR_AWAY`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Guide {
    /// The linear albedo of the surface, or the studio the ray saw.
    pub albedo: [f32; 3],
    /// The shading normal that faced the ray; [`Guide::unit_normal`] makes it a unit vector.
    pub normal: [f32; 3],
    /// How far the primary ray travelled, in millimetres.
    pub distance: f32,
}

impl Guide {
    /// The same guide with its normal scaled to unit length, a zero vector taken as the `+Z` axis.
    pub fn unit_normal(&self) -> [f32; 3] {
        let [x, y, z] = self.normal;
        let length = (x * x + y * y + z * z).sqrt();
        if !length.is_finite() || length <= 0.0 {
            return [0.0, 0.0, 1.0];
        }
        [x / length, y / length, z / length]
    }
}

/// One pixel's samples as the filter reads them: their mean, the variance of that mean, and how many.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    /// The mean linear radiance of the pixel's samples.
    pub mean: [f32; 3],
    /// The variance of that mean, per channel, in squared radiance.
    pub variance: [f32; 3],
    /// How many samples the mean averages, which is what the blend's error bar rests on.
    pub count: u32,
}

/// One image's pixels and guides, which is everything the filter reads.
///
/// The two slices are the same length and are indexed together, so a caller cannot hand over a guide
/// that belongs to another pixel. Construction checks that once instead of on every tap.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    width: u32,
    height: u32,
    pixels: Vec<Sample>,
    guides: Vec<Guide>,
}

impl Frame {
    /// A frame of `width` by `height` pixels, or a refusal when the slices do not fit it.
    pub fn new(width: u32, height: u32, pixels: Vec<Sample>, guides: Vec<Guide>) -> Result<Self, FinalError> {
        let expected = u64::from(width).checked_mul(u64::from(height));
        let fitted = expected == Some(pixels.len() as u64) && guides.len() == pixels.len() && !pixels.is_empty();
        if !fitted {
            return Err(FinalError::Accumulation {
                pixels: pixels.len().min(guides.len()),
                width,
                height,
                samples: pixels.first().map_or(0, |pixel| pixel.count),
            });
        }
        Ok(Self { width, height, pixels, guides })
    }

    /// The image width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The image height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The pixels, row-major from the top.
    pub fn pixels(&self) -> &[Sample] {
        &self.pixels
    }

    /// The guides, row-major from the top and beside their pixels.
    pub fn guides(&self) -> &[Guide] {
        &self.guides
    }

    /// One pixel, or `None` outside the image.
    pub fn sample_at(&self, x: u32, y: u32) -> Option<&Sample> {
        self.index(x, y).and_then(|at| self.pixels.get(at))
    }

    /// One guide, or `None` outside the image.
    pub fn guide_at(&self, x: u32, y: u32) -> Option<&Guide> {
        self.index(x, y).and_then(|at| self.guides.get(at))
    }

    /// The radiance of every pixel after the filter, row-major and unblended.
    ///
    /// These are what five à-trous levels produce on their own; [`Frame::denoise`] is the value a
    /// picture is made of.
    pub fn filtered(&self) -> Vec<[f32; 3]> {
        let mut source: Vec<[f32; 3]> = self.pixels.iter().map(|pixel| finite(pixel.mean)).collect();
        let mut target = vec![[0.0f32; 3]; source.len()];
        // The normals are made unit once instead of once per tap, which is a square root per pixel
        // and level rather than twenty-five.
        let normals: Vec<[f32; 3]> = self.guides.iter().map(Guide::unit_normal).collect();
        let width = self.width as usize;
        // A level is one pass over the whole image and every row of it is independent, so the rows are
        // shared out: a megapixel of three levels is a second of one core and a fraction of that on
        // several, and the render beside it has the frames' worth of work. A picture too small to pay
        // for a thread's own start-up keeps one thread.
        let cores = if source.len() >= PARALLEL_PIXELS {
            std::thread::available_parallelism().map(|count| count.get()).unwrap_or(1)
        } else {
            1
        };
        let threads = cores.clamp(1, (self.height as usize).max(1));
        let band = (self.height as usize).div_ceil(threads).max(1);
        for level in 0..LEVELS {
            let step = 1i32 << level;
            let (rows, source_now) = (self.height, &source);
            let run = |first: usize, chunk: &mut [[f32; 3]]| {
                for (row, line) in chunk.chunks_mut(width).enumerate() {
                    let Some(y) = u32::try_from(first + row).ok().filter(|y| *y < rows) else { continue };
                    for (x, slot) in line.iter_mut().enumerate() {
                        *slot = self.filter_at(source_now, &normals, x as u32, y, step);
                    }
                }
            };
            if threads == 1 {
                run(0, &mut target);
            } else {
                std::thread::scope(|scope| {
                    for (index, chunk) in target.chunks_mut(band * width).enumerate() {
                        let run = &run;
                        scope.spawn(move || run(index * band, chunk));
                    }
                });
            }
            std::mem::swap(&mut source, &mut target);
        }
        source
    }

    /// The radiance of every pixel after the filter and the blend: the picture the render writes.
    ///
    /// The blend is the pixel's own error bar against [`TARGET_RELATIVE_ERROR`]: a converged pixel is
    /// left exactly as it was traced, so the filter adds no blur of its own to an image that did not
    /// need it.
    pub fn denoise(&self) -> Vec<[f32; 3]> {
        let filtered = self.filtered();
        self.pixels
            .iter()
            .zip(filtered)
            .map(|(pixel, filtered)| {
                let weight = blend_weight(pixel.count, pixel.mean, pixel.variance);
                let raw = finite(pixel.mean);
                [
                    raw[0] + (filtered[0] - raw[0]) * weight,
                    raw[1] + (filtered[1] - raw[1]) * weight,
                    raw[2] + (filtered[2] - raw[2]) * weight,
                ]
            })
            .collect()
    }

    /// The pixel index of one coordinate, or `None` outside the image.
    fn index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        usize::try_from(u64::from(y) * u64::from(self.width) + u64::from(x)).ok()
    }

    /// One pixel of one à-trous level: the guide-stopped weighted mean of its neighbourhood.
    fn filter_at(&self, source: &[[f32; 3]], normals: &[[f32; 3]], x: u32, y: u32, step: i32) -> [f32; 3] {
        let Some(centre_at) = self.index(x, y) else { return [0.0; 3] };
        let centre = self.guides.get(centre_at).copied().unwrap_or_default();
        let centre_normal = normals.get(centre_at).copied().unwrap_or([0.0, 0.0, 1.0]);
        let raw = finite(source.get(centre_at).copied().unwrap_or([0.0; 3]));
        // The depth a neighbour is compared against is the one the centre's own surface predicts, not
        // the centre's depth: on a floor seen at a glancing angle a neighbour sixteen pixels away is
        // hundreds of millimetres further, and it is still the same floor.
        let (gradient_x, gradient_y) = self.distance_gradient(x, y);
        let tolerance = SIGMA_DISTANCE * centre.distance.abs().max(DISTANCE_FLOOR);
        let (x, y) = (i64::from(x), i64::from(y));
        // The five columns and rows the kernel reads, clamped to the picture and worked out once: a tap
        // outside the image is the edge pixel, and its offset is still the one the prediction needs.
        let columns: [(usize, i32); KERNEL.len()] = std::array::from_fn(|tap| {
            let offset = (tap as i32 - 2) * step;
            (self.clamped_column(x + i64::from(offset)), offset)
        });
        let rows: [(usize, i32); KERNEL.len()] = std::array::from_fn(|tap| {
            let offset = (tap as i32 - 2) * step;
            (self.clamped_row(y + i64::from(offset)), offset)
        });
        let mut sum = [0.0f32; 3];
        let mut total = 0.0f32;
        for (tap_y, kernel_y) in KERNEL.iter().enumerate() {
            let Some(&(row, offset_y)) = rows.get(tap_y) else { continue };
            for (tap_x, kernel_x) in KERNEL.iter().enumerate() {
                let Some(&(column, offset_x)) = columns.get(tap_x) else { continue };
                // The two-dimensional kernel is the product of the two one-dimensional ones: a corner
                // tap is a two-hundred-and-fifty-sixth and the centre is a hundred and forty-four
                // sixteenths.
                let mut weight = kernel_x * kernel_y;
                let other_at = row.saturating_add(column);
                if offset_x != 0 || offset_y != 0 {
                    let other = self.guides.get(other_at).copied().unwrap_or_default();
                    let other_normal = normals.get(other_at).copied().unwrap_or([0.0, 0.0, 1.0]);
                    weight *= edge_weight(
                        (&centre, &centre_normal),
                        (&other, &other_normal),
                        (gradient_x, gradient_y),
                        (offset_x, offset_y),
                        tolerance,
                    );
                }
                // A weight that is zero or not a number is a tap that is not there.
                if !weight.is_finite() || weight <= 0.0 {
                    continue;
                }
                let value = source.get(other_at).copied().unwrap_or([0.0; 3]);
                sum[0] += value[0] * weight;
                sum[1] += value[1] * weight;
                sum[2] += value[2] * weight;
                total += weight;
            }
        }
        if !total.is_finite() || total <= 0.0 {
            return raw;
        }
        [sum[0] / total, sum[1] / total, sum[2] / total]
    }

    /// The row offset of a row clamped to the image, so a tap outside it is the edge row.
    ///
    /// The clamp happens in the signed type the kernel works in: a tap two steps above the first row is
    /// `-1`, and `-1` cast to an unsigned index is the last row of the picture — the far edge of the
    /// image is not the neighbour of its near edge.
    fn clamped_row(&self, y: i64) -> usize {
        let y = y.clamp(0, i64::from(self.height).saturating_sub(1));
        usize::try_from(y).unwrap_or(0).saturating_mul(self.width as usize)
    }

    /// The column a column clamps to, for the reason [`Frame::clamped_row`] gives.
    fn clamped_column(&self, x: i64) -> usize {
        let x = x.clamp(0, i64::from(self.width).saturating_sub(1));
        usize::try_from(x).unwrap_or(0)
    }

    /// The distance of one pixel on the edge's own axis, clamped to the image.
    fn distance_near(&self, x: i64, y: i64) -> f32 {
        let x = x.clamp(0, i64::from(self.width).saturating_sub(1));
        let y = y.clamp(0, i64::from(self.height).saturating_sub(1));
        let at = usize::try_from(y * i64::from(self.width) + x).unwrap_or(0);
        self.guides.get(at).map_or(FAR_AWAY, |guide| guide.distance)
    }

    /// How much the distance changes per pixel at one pixel, in millimetres, along both axes.
    ///
    /// The smaller of the two one-sided differences on each axis, which is what makes a step a step: a
    /// pixel on the near side of a metre-wide step has a forward difference of a metre and a backward
    /// difference of nothing, and it takes the nothing, so the neighbour beyond the step cannot be
    /// "predicted". A slanted floor has the same difference both ways and is predicted exactly.
    fn distance_gradient(&self, x: u32, y: u32) -> (f32, f32) {
        let (x, y) = (i64::from(x), i64::from(y));
        let here = self.distance_near(x, y);
        let axis = |forward: f32, backward: f32| if forward.abs() <= backward.abs() { forward } else { backward };
        (
            axis(self.distance_near(x + 1, y) - here, here - self.distance_near(x - 1, y)),
            axis(self.distance_near(x, y + 1) - here, here - self.distance_near(x, y - 1)),
        )
    }
}

/// The variance of a mean of `count` samples from the sum and the sum of squares.
///
/// The unbiased estimator of the sample variance is `(Σx² − (Σx)²/n) / (n − 1)`, and the variance of
/// the mean is that divided by `n` again; both factors are written out here because the shader that
/// accumulates the two sums is this function's transcription.
pub fn variance_of_mean(sum: [f32; 3], squares: [f32; 3], count: f32) -> [f32; 3] {
    if !count.is_finite() || count <= 1.0 {
        // One sample has no spread to measure, and no sample has none either.
        return [0.0; 3];
    }
    let denominator = count * (count - 1.0);
    let mut variance = [0.0f32; 3];
    for channel in 0..3 {
        let (total, squares) = (sum[channel], squares[channel]);
        variance[channel] = ((squares - total * total / count) / denominator).max(0.0);
    }
    variance
}

/// The light of a colour, weighted the way the eye weights it.
pub fn luminance(colour: [f32; 3]) -> f32 {
    colour[0] * LUMINANCE[0] + colour[1] * LUMINANCE[1] + colour[2] * LUMINANCE[2]
}

/// The relative error of one pixel: the width of its noise over the light it carries.
///
/// Measured on the luminance, with [`LIGHT_FLOOR`] as the smallest light a ratio is taken against, so
/// a black pixel is judged by its absolute error and "0.2 of nothing" never divides.
pub fn relative_error(mean: [f32; 3], variance: [f32; 3]) -> f32 {
    let light = luminance(finite(mean)).max(LIGHT_FLOOR);
    let spread = luminance(variance);
    // A variance that is not a number is an error of unknown size, which is the worst error there is.
    if !spread.is_finite() {
        return f32::INFINITY;
    }
    spread.max(0.0).sqrt() / light
}

/// Whether a pixel has converged enough to stop tracing: enough samples and an error under the target.
///
/// `count` is the number of samples the pixel has taken, which the device accumulates. This is the
/// function the shader's retirement test transcribes.
pub fn retires(count: f32, mean: [f32; 3], variance: [f32; 3]) -> bool {
    // A number that is not a number is never a reason to stop looking at a pixel.
    if !count.is_finite() || mean.iter().chain(variance.iter()).any(|value| !value.is_finite()) {
        return false;
    }
    count >= MIN_SAMPLES as f32 && relative_error(mean, variance) < TARGET_RELATIVE_ERROR
}

/// How much of the filtered pixel replaces the raw one: `α = r² / (r² + t²)`.
///
/// Zero for a converged pixel, one for a pixel whose error is far above the target, and one half at
/// the target itself. A pixel that has taken fewer than [`MIN_SAMPLES`] has no error bar yet and takes
/// the filtered value whole — it is the noisiest pixel there is.
pub fn blend_weight(count: u32, mean: [f32; 3], variance: [f32; 3]) -> f32 {
    if count < MIN_SAMPLES {
        return 1.0;
    }
    let error = relative_error(mean, variance);
    let target = TARGET_RELATIVE_ERROR;
    let (squared, tolerance) = (error * error, target * target);
    if !squared.is_finite() {
        return 1.0;
    }
    (squared / (squared + tolerance)).clamp(0.0, 1.0)
}

/// How much a neighbouring pixel belongs to the centre one: the three guides that separate them.
///
/// Three stops, each ours: the albedo's difference, the cosine between the normals raised to
/// [`SIGMA_NORMAL`], and the distance's residual against the plane the centre's gradient predicts.
/// Each is a rational falloff rather than an exponential, because this filter runs on the host and a
/// picture is millions of taps: `1/(1+x)` is four operations where `exp(-x)` is thirty.
fn edge_weight(
    centre: (&Guide, &[f32; 3]),
    other: (&Guide, &[f32; 3]),
    gradient: (f32, f32),
    offset: (i32, i32),
    tolerance: f32,
) -> f32 {
    let (centre, centre_normal) = centre;
    let (other, other_normal) = other;
    let mut difference = 0.0f32;
    for channel in 0..3 {
        difference = difference.max((centre.albedo[channel] - other.albedo[channel]).abs());
    }
    let step = difference / SIGMA_ALBEDO;
    let albedo = 1.0 / (1.0 + step * step);
    let cosine =
        (centre_normal[0] * other_normal[0] + centre_normal[1] * other_normal[1] + centre_normal[2] * other_normal[2])
            .clamp(0.0, 1.0);
    let normal = cosine.powi(SIGMA_NORMAL);
    let predicted = centre.distance + gradient.0 * offset.0 as f32 + gradient.1 * offset.1 as f32;
    let distance = 1.0 / (1.0 + (other.distance - predicted).abs() / tolerance);
    albedo * normal * distance
}

/// One colour with every channel a finite non-negative number: a NaN is no light to protect.
fn finite(colour: [f32; 3]) -> [f32; 3] {
    let channel = |value: f32| if value.is_finite() { value.max(0.0) } else { 0.0 };
    [channel(colour[0]), channel(colour[1]), channel(colour[2])]
}

#[cfg(test)]
mod tests {
    //! The filter's own measurements: what it does to noise, and what it must never do to an edge.
    #![allow(clippy::expect_used)]

    use super::*;

    /// A deterministic stream of noise in `-1..=1`, so a measurement is the same on every machine.
    fn noise(seed: &mut u64) -> f32 {
        // A 64-bit xorshift: no dependency, no floating point, no state a test has to share.
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        let word = (*seed >> 40) as u32;
        word as f32 / 16_777_215.5 - 1.0
    }

    /// A guide of one flat surface facing the camera at `distance` millimetres.
    fn flat(albedo: f32, distance: f32) -> Guide {
        Guide { albedo: [albedo; 3], normal: [0.0, 0.0, 1.0], distance }
    }

    /// A frame of `width` by `height` uniform pixels of `radiance` plus noise of `amplitude`.
    fn uniform(width: u32, height: u32, radiance: f32, amplitude: f32, count: u32) -> Frame {
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let guides = vec![flat(0.6, 2000.0); (width * height) as usize];
        let pixels = guides
            .iter()
            .map(|_| Sample { mean: [radiance + amplitude * noise(&mut seed); 3], variance: [0.0; 3], count })
            .collect();
        Frame::new(width, height, pixels, guides).expect("a frame of one surface")
    }

    /// The standard deviation of the red channel over a rectangle of a picture.
    fn deviation(values: &[[f32; 3]], width: u32, x0: u32, x1: u32, y0: u32, y1: u32) -> f32 {
        let mut sum = 0.0f64;
        let mut squares = 0.0f64;
        let mut counted = 0.0f64;
        for y in y0..y1 {
            for x in x0..x1 {
                let Some(value) = values.get((y * width + x) as usize) else { continue };
                sum += f64::from(value[0]);
                squares += f64::from(value[0]) * f64::from(value[0]);
                counted += 1.0;
            }
        }
        if counted <= 1.0 {
            return 0.0;
        }
        let mean = sum / counted;
        ((squares / counted - mean * mean).max(0.0)).sqrt() as f32
    }

    /// The mean red channel over a rectangle of a picture.
    fn average(values: &[[f32; 3]], width: u32, x0: u32, x1: u32, y0: u32, y1: u32) -> f32 {
        let mut sum = 0.0f64;
        let mut counted = 0.0f64;
        for y in y0..y1 {
            for x in x0..x1 {
                let Some(value) = values.get((y * width + x) as usize) else { continue };
                sum += f64::from(value[0]);
                counted += 1.0;
            }
        }
        if counted == 0.0 { 0.0 } else { (sum / counted) as f32 }
    }

    #[test]
    fn the_sample_loop_s_variance_is_the_error_bar_of_its_mean() {
        // Six samples whose sum is eighteen and whose squares sum to sixty-four: mean three, a sample
        // variance of ten over five, and the variance of that mean a thirtieth of the sum of squares'
        // residual — worked out by hand, not by the code.
        let variance = variance_of_mean([18.0; 3], [64.0; 3], 6.0);
        for channel in variance {
            assert!((channel - 10.0 / 30.0).abs() < 1.0e-6, "the error bar of a mean of six: {channel}");
        }
        // A constant stream has no spread at all, which is the case a flat surface is.
        assert_eq!(variance_of_mean([6.0; 3], [6.0; 3], 6.0), [0.0; 3]);
        // One sample cannot say what its spread is, and a count of zero must not divide.
        assert_eq!(variance_of_mean([1.0; 3], [1.0; 3], 1.0), [0.0; 3]);
        assert_eq!(variance_of_mean([1.0; 3], [1.0; 3], 0.0), [0.0; 3]);
        // A hostile sum of squares below the sum's own square is no variance rather than a negative
        // one that would poison every weight downstream.
        assert_eq!(variance_of_mean([10.0; 3], [0.0; 3], 4.0), [0.0; 3]);
    }

    #[test]
    fn a_pixel_retires_at_the_target_and_never_before_its_thirty_second_sample() {
        let converged = [0.5f32; 3];
        // A relative error of exactly the target on a light of one half: the width is the target times
        // the light.
        let width = TARGET_RELATIVE_ERROR * luminance(converged);
        let variance = [width * width; 3];
        assert!((relative_error(converged, variance) - TARGET_RELATIVE_ERROR).abs() < 1.0e-6);
        assert!(!retires(31.0, converged, variance), "a pixel cannot judge its own error below thirty-two samples");
        assert!(!retires(32.0, converged, variance), "the target itself is not yet under the target");
        let better = [width * 0.5 * width * 0.5; 3];
        assert!(retires(32.0, converged, better), "half the target error retires at the thirty-second sample");
        assert!(retires(4096.0, converged, better));
        // A light under the floor is judged by its absolute error, so a black pixel can converge.
        assert!(retires(64.0, [0.0; 3], [0.0; 3]), "a shader that reports no light and no noise is converged");
        assert!(!retires(64.0, [0.0; 3], [1.0e-3; 3]), "and one that reports noise is not");
        // A broken value is never a reason to stop tracing.
        assert!(!retires(64.0, [f32::NAN; 3], [0.0; 3]));
    }

    #[test]
    fn the_blend_keeps_a_converged_pixel_and_hands_a_noisy_one_to_the_filter() {
        let mean = [0.5f32; 3];
        let width = TARGET_RELATIVE_ERROR * luminance(mean);
        // At the target the two values are equally trusted; a tenth of it keeps almost the raw pixel.
        assert!((blend_weight(64, mean, [width * width; 3]) - 0.5).abs() < 1.0e-6);
        assert!(blend_weight(64, mean, [width * 0.1 * width * 0.1; 3]) < 0.01);
        assert!(blend_weight(64, mean, [width * 10.0 * width * 10.0; 3]) > 0.99);
        // A pixel with no error bar yet takes the filtered value whole, and a broken one does too.
        assert_eq!(blend_weight(1, mean, [0.0; 3]), 1.0);
        assert_eq!(blend_weight(64, mean, [f32::INFINITY; 3]), 1.0);
    }

    #[test]
    fn the_filter_softens_noise_and_leaves_a_flat_surface_where_it_was() {
        // A flat surface at one radiance, seen with noise of a hundredth: five levels must take most of
        // the noise away without moving the surface.
        let frame = uniform(64, 64, 0.5, 0.01, 128);
        let raw: Vec<[f32; 3]> = frame.pixels().iter().map(|pixel| pixel.mean).collect();
        let filtered = frame.filtered();
        let before = deviation(&raw, 64, 8, 56, 8, 56);
        let after = deviation(&filtered, 64, 8, 56, 8, 56);
        assert!(after < before * 0.5, "the filter must take at least half the spread: {before} became {after}");
        // The surface is not moved: the filtered mean is the mean that came in, not a shifted one — and
        // the blend keeps it so, because a pixel with no variance reported takes the filter whole.
        let denoised = frame.denoise();
        let raw_mean = average(&raw, 64, 8, 56, 8, 56);
        let denoised_mean = average(&denoised, 64, 8, 56, 8, 56);
        assert!(
            (denoised_mean - raw_mean).abs() < 1.0e-3,
            "the surface stays where it was: {raw_mean} became {denoised_mean}"
        );
    }

    #[test]
    fn a_two_millimetre_band_on_a_thousand_millimetre_panel_keeps_its_edge() {
        // The entry's own fixture: 1000 mm across 512 columns, so one pixel is 1.95 mm and the band is
        // two columns of dark paint on a pale board with noise of a hundredth.
        let (width, height) = (512u32, 32u32);
        let centre = (width / 2) as usize;
        let mut seed = 0x1234_5678_9abc_def0u64;
        let mut pixels = Vec::new();
        let mut guides = Vec::new();
        for _y in 0..height {
            for x in 0..width {
                let in_band = x as usize == centre || x as usize == centre + 1;
                let albedo = if in_band { 0.05 } else { 0.8 };
                guides.push(flat(albedo, 1000.0));
                let light = albedo * 0.75;
                pixels.push(Sample { mean: [light + 0.01 * noise(&mut seed); 3], variance: [0.0; 3], count: 128 });
            }
        }
        let frame = Frame::new(width, height, pixels, guides).expect("a bound frame");
        let raw: Vec<[f32; 3]> = frame.pixels().iter().map(|pixel| pixel.mean).collect();
        let filtered = frame.filtered();
        let at = |values: &[[f32; 3]], x: usize| values[16 * width as usize + x][0];
        let raw_step = at(&raw, centre + 4) - at(&raw, centre);
        let filtered_step = at(&filtered, centre + 4) - at(&filtered, centre);
        assert!(raw_step > 0.5, "the fixture has a band: {raw_step}");
        assert!(
            filtered_step > raw_step * 0.9,
            "a two millimetre band keeps its edge: {raw_step} became {filtered_step}"
        );
        // And the board beside the band is smoothed, which is what the five levels are for.
        let before = deviation(&raw, width, centre as u32 + 8, width - 8, 4, 28);
        let after = deviation(&filtered, width, centre as u32 + 8, width - 8, 4, 28);
        assert!(after < before * 0.7, "the board beside the band is smoothed: {before} became {after}");
    }

    #[test]
    fn a_step_in_depth_and_a_fold_in_the_surface_both_stop_the_filter() {
        // Two surfaces of one paint, one a metre behind the other: the depth stop alone separates them,
        // which is the case an albedo stop cannot see.
        let (width, height) = (64u32, 16u32);
        let mut pixels = Vec::new();
        let mut guides = Vec::new();
        for _y in 0..height {
            for x in 0..width {
                let near = x < 32;
                guides.push(flat(0.5, if near { 1000.0 } else { 2000.0 }));
                pixels.push(Sample { mean: [if near { 0.5 } else { 0.1 }; 3], variance: [0.0; 3], count: 256 });
            }
        }
        let frame = Frame::new(width, height, pixels, guides).expect("a bound frame");
        let filtered = frame.filtered();
        let step = filtered[8 * 64 + 31][0] - filtered[8 * 64 + 33][0];
        assert!(step > 0.35, "a metre of depth over one pixel is not blurred: {step}");

        // A fold of ninety degrees is one surface of one paint: the normal stops it.
        let mut pixels = Vec::new();
        let mut guides = Vec::new();
        for _y in 0..height {
            for x in 0..width {
                let left = x < 32;
                guides.push(Guide {
                    albedo: [0.5; 3],
                    normal: if left { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] },
                    distance: 1000.0,
                });
                pixels.push(Sample { mean: [if left { 0.5 } else { 0.1 }; 3], variance: [0.0; 3], count: 256 });
            }
        }
        let frame = Frame::new(width, height, pixels, guides).expect("a bound frame");
        let filtered = frame.filtered();
        let fold = filtered[8 * 64 + 31][0] - filtered[8 * 64 + 33][0];
        assert!(fold > 0.35, "a right-angle fold is not blurred: {fold}");
    }

    #[test]
    fn a_glancing_floor_is_one_surface_although_its_depth_changes_fastest() {
        // A floor seen at a shallow angle: the distance climbs ten millimetres per pixel, so the depth
        // sixteen pixels away is a hundred and sixty millimetres off the centre's. The filter predicts
        // that from the gradient instead of tolerating it, so the floor is filtered as one surface.
        let (width, height) = (64u32, 16u32);
        let mut pixels = Vec::new();
        let mut guides = Vec::new();
        let mut seed = 0x5eed_5eedu64;
        for _y in 0..height {
            for x in 0..width {
                guides.push(flat(0.5, 1000.0 + 10.0 * x as f32));
                pixels.push(Sample { mean: [0.5 + 0.01 * noise(&mut seed); 3], variance: [0.0; 3], count: 128 });
            }
        }
        let frame = Frame::new(width, height, pixels, guides).expect("a bound frame");
        let raw: Vec<[f32; 3]> = frame.pixels().iter().map(|pixel| pixel.mean).collect();
        let filtered = frame.filtered();
        let before = deviation(&raw, width, 8, 56, 4, 12);
        let after = deviation(&filtered, width, 8, 56, 4, 12);
        assert!(after < before * 0.7, "a slanted floor is still smoothed: {before} became {after}");
    }

    #[test]
    fn the_background_is_its_own_surface_and_a_missing_pixel_is_a_refusal() {
        // A board in front of the studio: the guide's distance is the far plane behind it, so two
        // surfaces of the same paint do not mix.
        let (width, height) = (32u32, 8u32);
        let mut guides = Vec::new();
        for _y in 0..height {
            for x in 0..width {
                guides.push(flat(0.5, if x < 16 { 2000.0 } else { FAR_AWAY }));
            }
        }
        let pixels = vec![Sample { mean: [0.5; 3], variance: [0.0; 3], count: 64 }; guides.len()];
        let frame = Frame::new(width, height, pixels, guides).expect("a bound frame");
        let filtered = frame.filtered();
        assert!((filtered[4 * 32 + 15][0] - 0.5).abs() < 1.0e-3);

        let short = Frame::new(4, 4, vec![Sample::default(); 15], vec![Guide::default(); 16]);
        assert!(matches!(short, Err(FinalError::Accumulation { .. })), "{short:?}");
        let no_guides = Frame::new(4, 4, vec![Sample::default(); 16], vec![Guide::default(); 15]);
        assert!(matches!(no_guides, Err(FinalError::Accumulation { .. })), "{no_guides:?}");
        assert!(Frame::new(0, 4, Vec::new(), Vec::new()).is_err(), "an empty frame has no picture in it");
    }

    #[test]
    fn the_filter_is_the_identity_on_a_frame_of_one_surface_and_one_value() {
        // Five levels of a kernel that sums to one, over a neighbourhood that agrees: the picture comes
        // back unchanged, or the blend's "no permanent blur" claim would be about rounding instead.
        let frame = uniform(16, 16, 0.25, 0.0, 4096);
        let filtered = frame.filtered();
        for (pixel, value) in frame.pixels().iter().zip(&filtered) {
            assert!((pixel.mean[0] - value[0]).abs() < 1.0e-6, "{} is not {}", pixel.mean[0], value[0]);
        }
    }

    #[test]
    fn the_denoised_picture_is_the_raw_one_where_every_pixel_has_converged() {
        // The 4096-sample claim, in its pure form: a converged pixel's blend weight is zero, so the
        // picture is what the tracer produced and the filter is a no-op on it.
        let frame = uniform(32, 32, 0.5, 0.002, 4096);
        let denoised = frame.denoise();
        let raw: Vec<f32> = frame.pixels().iter().map(|pixel| pixel.mean[0]).collect();
        let moved = raw.iter().zip(&denoised).map(|(raw, denoised)| (raw - denoised[0]).abs()).fold(0.0f32, f32::max);
        assert_eq!(moved, 0.0, "a frame with no variance carried is not touched at all");
    }
}
