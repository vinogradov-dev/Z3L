//! The sampler a final render draws its numbers from: Owen-scrambled Sobol points.
//!
//! White noise is the sampler everyone writes first and the one a catalog image cannot afford: at two
//! hundred samples a pixel it is a visible grain, and the same two hundred samples on a stratified
//! sequence are a smooth plate. The sequence here is Sobol's (1967), scrambled by Owen's (1995) nested
//! uniform permutation through the hash of Laine and Karras (2019) as Burley (2020) recommends, all of
//! which are published algorithms rather than art: a scramble that keeps the sequence's stratification
//! is the whole point, and Owen's theorem — a scrambled net is still a net — is what the tests below
//! check directly.
//!
//! The points are integers until the last step. A sample is the XOR of the direction numbers of the
//! index's Gray code, which is the sequence's own definition; the scramble permutes the bits of that
//! word, and only then is it divided by `2^32`. A sampler written this way is testable without a GPU:
//! the stratification of every power-of-two prefix is an exact count, not a statistical impression,
//! and a wrong direction number fails it at the first four points.
//!
//! The WGSL in `gpu/shader.wgsl` is this module's transcription — the same table, the same Gray code,
//! the same hash — so a wrong index or a lost scramble in the shader is a picture that does not
//! converge and never a silent reinterpretation of a sample.
//!
//! Refuses: nothing. Every input is a `u32`, every output is a number in `[0, 1)`, and a dimension
//! past the table is folded back into it rather than reading past the end.
//!
//! Changes when: the dimension count, the scramble or the sequence changes.

use std::sync::OnceLock;

/// The number of dimensions the generator carries.
///
/// A path tracer asks for more numbers per sample than any single Sobol point has, so the pairs are
/// reused with a different seed per bounce and per lobe: the first pair — the one the eye notices,
/// because it jitters the pixel and the lens — keeps the sequence's stratification, and every deeper
/// pair is a scramble of the same sequence, which decorrelates it from the first.
pub const DIMENSIONS: usize = 8;

/// How many dimensions one [`sample_2d`] call consumes.
pub const PAIR: usize = 2;

/// The primitive polynomial each dimension's direction numbers come from, in Joe and Kuo's (2008)
/// order: the degree `s`, the polynomial's inner coefficients `a`, and the first `s` direction
/// integers.
///
/// Dimension zero is the van der Corput sequence, which is Sobol's first dimension. The rows are the
/// first eight of the standard table, and the generator below reads any row the same way, so a ninth
/// is a row and not a new function.
const TABLE: [(u32, u32, [u32; 5]); DIMENSIONS] = [
    (0, 0, [0, 0, 0, 0, 0]),
    (1, 0, [1, 0, 0, 0, 0]),
    (2, 1, [1, 3, 0, 0, 0]),
    (3, 1, [1, 3, 1, 0, 0]),
    // The pairs a rectangular light spends: one draw picks which window, the next lands on its face
    // (L2).
    (3, 2, [1, 1, 1, 0, 0]),
    (4, 1, [1, 1, 3, 3, 0]),
    (4, 4, [1, 3, 5, 13, 0]),
    (5, 2, [1, 1, 5, 5, 7]),
];

/// The direction numbers of every dimension, each a `u32` per bit of an index.
///
/// Built once, because they are a pure function of the table above: 32 words per dimension, and the
/// sequence is then one XOR per set bit of the Gray code. The device reads the same words from a
/// buffer this function fills ([`crate::gpu::layout`]), so the shader transcribes the algorithm and
/// never retypes 128 constants.
pub fn direction_numbers() -> &'static [[u32; 32]; DIMENSIONS] {
    static NUMBERS: OnceLock<[[u32; 32]; DIMENSIONS]> = OnceLock::new();
    NUMBERS.get_or_init(|| {
        let mut table = [[0u32; 32]; DIMENSIONS];
        for (dimension, &(s, a, m)) in TABLE.iter().enumerate() {
            if let Some(row) = table.get_mut(dimension) {
                *row = build(s, a, m);
            }
        }
        table
    })
}

/// The direction numbers of one dimension, from its polynomial and its initial integers.
///
/// The recurrence is Bratley and Fox's (1988) reading of Sobol's construction: the first `s` words
/// come from the table, and every later word is the words the polynomial links together, shifted by
/// the polynomial's degree.
fn build(s: u32, a: u32, m: [u32; 5]) -> [u32; 32] {
    let mut v = [0u32; 32];
    if s == 0 {
        // The first dimension is the van der Corput sequence: bit `k` of the index becomes bit
        // `31 - k` of the value.
        for (k, word) in v.iter_mut().enumerate() {
            *word = 1u32 << (31 - k as u32);
        }
        return v;
    }
    let degree = s as usize;
    for (k, word) in v.iter_mut().enumerate().take(degree) {
        let Ok(shift) = u32::try_from(31 - k) else { continue };
        *word = m.get(k).copied().unwrap_or(0) << shift;
    }
    for k in degree..32 {
        let Some(previous) = v.get(k - degree).copied() else { continue };
        let mut value = previous ^ (previous >> s);
        for i in 1..degree {
            // Bit `i` of `a` is the polynomial's coefficient on the term one degree lower.
            if (a >> (degree - 1 - i)) & 1 == 1
                && let Some(word) = v.get(k - i)
            {
                value ^= word;
            }
        }
        if let Some(slot) = v.get_mut(k) {
            *slot = value;
        }
    }
    v
}

/// The direction number word of one dimension at one index, before the scramble.
fn word(dimension: usize, index: u32) -> u32 {
    let table = direction_numbers();
    let Some(row) = table.get(dimension % DIMENSIONS) else { return 0 };
    // Sobol's sequence is generated along the Gray code of the index, so consecutive indices differ by
    // one word and the sequence visits each point once.
    let gray = index ^ (index >> 1);
    let mut bits = 0u32;
    for (k, entry) in row.iter().enumerate() {
        if let Ok(shift) = u32::try_from(k)
            && (gray >> shift) & 1 == 1
        {
            bits ^= entry;
        }
    }
    bits
}

/// Wang's 32-bit hash, which turns an index into a well-spread word.
///
/// Every scramble and every per-pixel seed in the tracer comes through here, so one sample's noise is
/// a function of the pixel, the sample index and the dimension and never of the order the dispatches
/// ran in.
pub fn hash(value: u32) -> u32 {
    let mut x = value;
    x = (x ^ 61) ^ (x >> 16);
    x = x.wrapping_mul(9);
    x = x ^ (x >> 4);
    x = x.wrapping_mul(0x27d4_eb2d);
    x = x ^ (x >> 15);
    x
}

/// The Laine and Karras permutation of one bit-reversed word, which applied bitwise is a nested
/// uniform scramble: every trailing-bit level gets its own random permutation, and the value stays in
/// `[0, 1)` because the word is interpreted as a fraction below the binary point.
fn permute(x: u32, seed: u32) -> u32 {
    let mut x = x.wrapping_add(seed);
    x ^= x.wrapping_mul(0x6c50_b47c);
    x ^= x.wrapping_mul(0xb82f_1e52);
    x ^= x.wrapping_mul(0xc7af_e638);
    x ^= x.wrapping_mul(0x8d22_f6e6);
    x
}

/// Owen's scramble of one direction-number word.
///
/// The word is read with its most significant bit as the coarsest stratum, so the permutation is
/// applied to the bit-reversed word and the result is read back the same way: the high bits of `x`
/// decide the big cells and the low bits only refine inside them, which is what keeps a net a net.
pub fn scramble(x: u32, seed: u32) -> u32 {
    permute(x.reverse_bits(), seed).reverse_bits()
}

/// One dimension's number of sample `index`, in `[0, 1)`.
///
/// `seed` must stay the same for every sample of one pixel and one dimension: a scramble that changed
/// per sample would move each point independently and destroy the stratification the sequence exists
/// for. The dimension itself changes the seed, so two dimensions of one sample are two different
/// sequences rather than two readings of the same one.
pub fn sample_1d(dimension: usize, index: u32, seed: u32) -> f32 {
    let dimension_seed = seed ^ hash(dimension as u32);
    fraction(scramble(word(dimension, index), dimension_seed))
}

/// One two-dimensional sample of `index`: a point of the pair `pair * 2`, `pair * 2 + 1`.
///
/// A pair is the unit a path tracer consumes whole — a pixel's jitter, a sphere's two angles, a light's
/// two coordinates — so the pair is the argument and the two dimensions are always taken together.
pub fn sample_2d(pair: usize, index: u32, seed: u32) -> [f32; 2] {
    let first = (pair % (DIMENSIONS / PAIR)) * PAIR;
    [sample_1d(first, index, seed), sample_1d(first + 1, index, seed)]
}

/// The concentric map of Shirley and Chiu: a point of the unit square becomes a point of the unit
/// disc, uniformly by area.
///
/// A lens is a disc, and a lens sample that is uniform in the square has corners the lens does not
/// have. The map folds the square's four quadrants onto the disc around the four axes, so no point is
/// nearer the centre than the area it speaks for and no seam runs out of the middle of the blur
/// (Y5). The device transcribes this function; the two are one map, checked by the test below.
pub fn concentric_disc(u: [f32; 2]) -> [f32; 2] {
    let x = 2.0 * u[0] - 1.0;
    let y = 2.0 * u[1] - 1.0;
    if x == 0.0 && y == 0.0 {
        return [0.0, 0.0];
    }
    let (radius, angle) = if x.abs() > y.abs() {
        (x, std::f32::consts::FRAC_PI_4 * (y / x))
    } else {
        (y, std::f32::consts::FRAC_PI_2 - std::f32::consts::FRAC_PI_4 * (x / y))
    };
    [radius * angle.cos(), radius * angle.sin()]
}

/// A 32-bit word read as a fraction below the binary point, in `[0, 1)`.
///
/// The low eight bits are dropped rather than rounded: an `f32` has 24 bits of mantissa, and dividing
/// by exactly `2^24` keeps every step of the conversion exact, so the CPU and the GPU agree on the
/// value to the bit.
fn fraction(bits: u32) -> f32 {
    const SCALE: f32 = 1.0 / 16_777_216.0;
    (bits >> 8) as f32 * SCALE
}

/// The number of samples a citizen sequence of `m` bits splits the unit square into per axis.
#[cfg(test)]
fn cells(m: u32) -> u32 {
    1u32 << m
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    /// The cell of a `2^a` by `2^b` grid a point falls in.
    fn cell(point: [f32; 2], across: u32, down: u32) -> (u32, u32) {
        let x = (point[0] * across as f32) as u32;
        let y = (point[1] * down as f32) as u32;
        (x.min(across - 1), y.min(down - 1))
    }

    #[test]
    fn the_eight_direction_tables_are_the_published_ones() {
        let table = direction_numbers();
        // Dimension zero is the van der Corput sequence: bit k of the index is bit 31 - k of the word.
        assert_eq!(table[0][0], 0x8000_0000);
        assert_eq!(table[0][1], 0x4000_0000);
        assert_eq!(table[0][2], 0x2000_0000);
        // Dimension one is the degree-one polynomial x + 1: 0.5, 0.75, 0.625, 0.9375.
        assert_eq!(table[1][0], 0x8000_0000);
        assert_eq!(table[1][1], 0xc000_0000);
        assert_eq!(table[1][2], 0xa000_0000);
        assert_eq!(table[1][3], 0xf000_0000);
        // The pair a rectangular light draws with (L2) is the standard table's fifth dimension,
        // `s = 3, a = 2, m = [1, 1, 1]`: its fourth word is v0 ^ (v0 >> 3) ^ v2.
        assert_eq!(table[4][0], 0x8000_0000);
        assert_eq!(table[4][1], 0x4000_0000);
        assert_eq!(table[4][2], 0x2000_0000);
        assert_eq!(table[4][3], 0xb000_0000);
        // And its sixth, `s = 4, a = 1, m = [1, 1, 3, 3]`, whose fifth word is v0 ^ (v0 >> 4) ^ v1.
        assert_eq!(table[5][0], 0x8000_0000);
        assert_eq!(table[5][2], 0x6000_0000);
        assert_eq!(table[5][4], 0xc800_0000);
        // Every word is a direction number: odd initial integers, and each later one below 2^32.
        for row in table.iter() {
            assert_ne!(row[31], 0, "the finest bit of every dimension is set");
        }
    }

    #[test]
    fn the_first_four_points_are_the_net_the_sequence_is_famous_for() {
        // The published 2D Sobol prefix: (0.5, 0.5), (0.75, 0.25), (0.25, 0.75), (0.375, 0.375).
        let unscrambled = |index: u32| [fraction(word(0, index)), fraction(word(1, index))];
        let points: Vec<[f32; 2]> = (1..=4).map(unscrambled).collect();
        assert_eq!(points[0], [0.5, 0.5]);
        assert_eq!(points[1], [0.75, 0.25]);
        assert_eq!(points[2], [0.25, 0.75]);
        assert_eq!(points[3], [0.375, 0.375]);
    }

    #[test]
    fn every_power_of_two_prefix_fills_every_cell_exactly_once() {
        // The property a white-noise sampler fails on the first try: for any split of the first `2^m`
        // points into `2^a` by `2^b` cells with `a + b = m`, every cell holds exactly one point.
        for m in 1..=10u32 {
            let count = cells(m);
            for across_bits in 0..=m {
                let (across, down) = (cells(across_bits), cells(m - across_bits));
                let mut seen = vec![0u32; (across * down) as usize];
                for index in 0..count {
                    let point = [fraction(word(0, index)), fraction(word(1, index))];
                    let (x, y) = cell(point, across, down);
                    if let Some(slot) = seen.get_mut((y * across + x) as usize) {
                        *slot += 1;
                    }
                }
                assert!(seen.iter().all(|count| *count == 1), "m = {m}, {across} by {down} cells: {seen:?}");
            }
        }
    }

    #[test]
    fn a_scramble_keeps_the_net_and_moves_the_points() {
        // Owen's theorem in one assertion: the scrambled prefix is still a net, and it is not the
        // unscrambled prefix, so the scramble is doing something.
        let unscrambled: Vec<[f32; 2]> =
            (0..64).map(|index| [fraction(word(0, index)), fraction(word(1, index))]).collect();
        let mut moved = 0usize;
        for seed in [0u32, 1, 0x1234_5678, u32::MAX] {
            let mut seen = vec![0u32; 8 * 8];
            for index in 0..64u32 {
                let point = sample_2d(0, index, seed);
                let (x, y) = cell(point, 8, 8);
                if let Some(slot) = seen.get_mut((y * 8 + x) as usize) {
                    *slot += 1;
                }
                if point != unscrambled[index as usize] {
                    moved += 1;
                }
            }
            assert!(seen.iter().all(|count| *count == 1), "seed {seed:#x}: {seen:?}");
        }
        assert!(moved > 200, "the scramble moved only {moved} of the points");
    }

    #[test]
    fn a_square_becomes_a_disc_by_area_and_not_by_a_radius() {
        // Every point is inside the disc, the four corners of the square are its four cardinal
        // points, and each ring of half the radius holds a quarter of the samples: a mapping that
        // gathered points at the centre would blur a picture unevenly (Y5).
        // The square's four edges land on the disc's four cardinal points and its corners on the
        // diagonals between them, which is what makes the map continuous all the way round.
        let diagonal = std::f32::consts::FRAC_1_SQRT_2;
        for (corner, expected) in [
            ([0.5f32, 1.0f32], [0.0f32, 1.0]),
            ([1.0, 0.5], [1.0, 0.0]),
            ([0.5, 0.0], [0.0, -1.0]),
            ([0.0, 0.5], [-1.0, 0.0]),
            ([1.0, 1.0], [diagonal, diagonal]),
            ([0.0, 0.0], [-diagonal, -diagonal]),
        ] {
            let point = concentric_disc(corner);
            assert!((point[0] - expected[0]).abs() < 1.0e-6 && (point[1] - expected[1]).abs() < 1.0e-6, "{corner:?}");
        }
        assert_eq!(concentric_disc([0.5, 0.5]), [0.0, 0.0], "the middle of the square is the middle of the lens");

        let (mut inner, mut outer) = (0u32, 0u32);
        for index in 0..4096u32 {
            let u = sample_2d(1, index, 0x51ed_2701);
            let point = concentric_disc(u);
            let radius = point[0].hypot(point[1]);
            assert!(radius <= 1.0 + 1.0e-6, "{u:?} left the disc at {radius}");
            if radius < 0.5 {
                inner += 1;
            } else {
                outer += 1;
            }
        }
        let share = f64::from(inner) / f64::from(inner + outer);
        assert!((share - 0.25).abs() < 0.02, "the inner half of the disc holds {share} of the samples");
    }

    #[test]
    fn the_first_8192_two_dimensional_points_are_unique() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for index in 0..8192u32 {
            let point = sample_2d(0, index, 0x9e37_79b9);
            let key = point.map(f32::to_bits);
            assert!(seen.insert(key), "point {index} repeats: {point:?}");
        }
        assert_eq!(seen.len(), 8192);
    }

    #[test]
    fn a_deeper_pair_is_decorrelated_from_the_first() {
        // Two dimensions of one pair must not be the same number, and two pairs must not be either: a
        // sampler that returns x for both axes traces a diagonal of samples, not an area.
        let mut identical = 0usize;
        for index in 0..512u32 {
            let near = sample_2d(0, index, 7);
            let far = sample_2d(1, index, 7);
            if near[0] == near[1] {
                identical += 1;
            }
            assert_ne!(near, far, "pair 0 and pair 1 are the same point at {index}");
        }
        assert_eq!(identical, 0, "a pair's two axes are two sequences");
    }

    #[test]
    fn every_sample_is_inside_the_unit_interval() {
        for dimension in 0..DIMENSIONS {
            for index in [0u32, 1, 2, 3, 8191, 65_535, u32::MAX] {
                let value = sample_1d(dimension, index, 0xdead_beef);
                assert!((0.0..1.0).contains(&value), "dimension {dimension} index {index}: {value}");
            }
        }
        assert_eq!(fraction(0), 0.0);
        assert!(fraction(u32::MAX) < 1.0, "the largest word is still below one");
    }

    #[test]
    fn a_pixel_seed_moves_the_sequence_but_not_its_shape() {
        // Two neighbouring pixels must not draw the same noisy sequence, yet each must stratify: this
        // is what makes the image converge instead of showing the sequence's own pattern.
        for pixel in 0..4u32 {
            let mut seen = vec![0u32; 16];
            for index in 0..16u32 {
                let point = sample_2d(0, index, hash(pixel + 1));
                let (x, y) = cell(point, 4, 4);
                if let Some(slot) = seen.get_mut((y * 4 + x) as usize) {
                    *slot += 1;
                }
            }
            assert!(seen.iter().all(|count| *count == 1), "pixel {pixel}: {seen:?}");
        }
        let first: Vec<[f32; 2]> = (0..8).map(|index| sample_2d(0, index, hash(1))).collect();
        let second: Vec<[f32; 2]> = (0..8).map(|index| sample_2d(0, index, hash(2))).collect();
        assert_ne!(first, second, "two pixels draw two different points");
    }
}
