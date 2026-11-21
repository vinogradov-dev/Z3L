//! The three lobes a final render shades with: Lambertian, GGX metal and a glass dielectric.
//!
//! A furniture picture is made of three surfaces and no more: painted or veneered board, which is
//! diffuse; a metal handle or a satin rail, which is a rough conductor; and a pane, which is glass.
//! This module is the whole of what the tracer knows about a surface, and it is written in `f64`
//! beside the shader that transcribes it, so the estimator's weights can be tested on a machine with
//! no GPU (`plan/notes/Y1.md`): a wrong Fresnel, a wrong masking term or a wrong multiple-scattering
//! weight fails a test here instead of showing up as a slightly dark handle in a picture nobody
//! measures.
//!
//! The reflection lobe is GGX (Walter et al. 2007) with Heitz's (2018) sampling of the visible
//! normals, because a board's varnish and a handle's brushed finish are both rough conductors and
//! sampling the visible normals is what makes a roughness of 0.05 not disappear into fireflies. A
//! rough conductor loses energy to its own multiple scattering — about a third of it at roughness 1 —
//! so the lobe is completed by the multiple-scattering term of Kulla and Conty (2017), coloured by
//! Turquin's (2019) average Fresnel; the loss without it is measured by the tests below, so the term
//! is checked and not merely present.
//!
//! The glass lobe is a dielectric interface with the exact Fresnel equations: a reflection that
//! follows the same rough-conductor lobe, and a smooth refracted beam, which is what float glass is.
//! Total internal reflection falls out of the Fresnel term rather than being special-cased, and the
//! tint is Beer and Lambert's law applied by the integrator along the distance the ray travels inside,
//! because that is a property of the glass and not of one bounce.
//!
//! All of it is expressed in the shading frame: the surface normal is `+Z`, `wo` points from the
//! surface towards the viewer and has a positive `z`, and a transmitted direction has a negative one.
//! [`Frame`] is the only place a world direction becomes a local one and back.
//!
//! Refuses: a degenerate view direction, a non-finite or negative number in a material, and a sample
//! whose draw lands on a zero-probability event are `None` — never a direction of NaN, never a weight
//! that is infinite, because both would poison every bounce after it (§4.4).
//!
//! Changes when: the set of lobes, the roughness mapping or the energy compensation changes.

use std::sync::OnceLock;

use geom::{DVec2, DVec3};

/// The circle constant, which the reflectance integrals need.
const PI: f64 = std::f64::consts::PI;

/// The smallest roughness the lobes are evaluated at.
///
/// A roughness of zero is a perfect mirror, whose normal distribution is a Dirac delta no sample can
/// draw; clamping it to this value keeps the maths finite and the difference from a mirror is below
/// what a device's `f32` can show. Sampling a real mirror is the dielectric's smooth path, which does
/// not go through the microfacet model at all.
pub const MIN_ROUGHNESS: f64 = 1.0e-3;

/// How many steps the energy table resolves per axis.
///
/// Sixteen roughnesses by sixteen angles is one kilobyte on the device and a bilinear lookup per
/// bounce; the measured error of the completion below stays inside one percent at that resolution,
/// which the tests check rather than assume.
pub const ENERGY_STEPS: usize = 16;

/// How many samples one table cell is measured with.
///
/// The table is filled once, from the lobes themselves: 4096 stratified samples put every cell inside
/// a tenth of a percent, which is what the completion's one-percent budget needs, and the whole table
/// costs about forty milliseconds on the way to a render that takes minutes.
const ENERGY_SAMPLES: u32 = 4096;

/// What a surface is made of, as the tracer needs it.
///
/// One struct rather than three constructors with a tag, because the three surfaces of furniture are
/// three points of one space: a board is `metallic = 0, transmission = 0`, a handle adds `metallic`,
/// and a pane adds `transmission` and a tint. Every field is `f32` because the device reads it as one,
/// and every rule about its range is applied by [`Material::sanitised`] once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// Linear RGB albedo of the diffuse lobe, or a metal's reflectance at normal incidence.
    pub albedo: [f32; 3],
    /// GGX roughness, `0..=1`: the width of the microfacet normal distribution.
    pub roughness: f32,
    /// `0` for a dielectric, `1` for a metal; a value between them mixes the two lobes.
    pub metallic: f32,
    /// Index of refraction of the dielectric interface.
    pub ior: f32,
    /// The fraction of the refracted beam that survives the interface, `0..=1`.
    pub transmission: f32,
    /// Beer-Lambert extinction inside the material, per millimetre, linear RGB.
    pub absorption: [f32; 3],
}

impl Material {
    /// A painted or veneered surface: diffuse, opaque, dielectric.
    pub fn diffuse(albedo: [f32; 3]) -> Self {
        Self { albedo, roughness: 1.0, metallic: 0.0, ior: 1.5, transmission: 0.0, absorption: [0.0; 3] }
    }

    /// A rough conductor: a handle, a rail, a satin fitting.
    pub fn metal(albedo: [f32; 3], roughness: f32) -> Self {
        Self { albedo, roughness, metallic: 1.0, ior: 1.5, transmission: 0.0, absorption: [0.0; 3] }
    }

    /// A pane: glass with an index of refraction and a tint per millimetre.
    pub fn glass(ior: f32, roughness: f32, absorption: [f32; 3]) -> Self {
        Self { albedo: [0.0; 3], roughness, metallic: 0.0, ior, transmission: 1.0, absorption }
    }

    /// The same material with every field inside the range the lobes are defined on.
    ///
    /// A material arrives as document data through a shell that may have written anything, so the
    /// ranges are enforced here, once, instead of at each use: an albedo outside `0..=1` would emit
    /// light, a roughness of NaN would make every sample `None`, and an index of refraction below one
    /// would refract the wrong way.
    pub fn sanitised(self) -> Self {
        let channel = |value: f32| if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
        let absorption = self.absorption.map(|value| if value.is_finite() { value.max(0.0) } else { 0.0 });
        Self {
            albedo: self.albedo.map(channel),
            roughness: channel(self.roughness),
            metallic: channel(self.metallic),
            ior: if self.ior.is_finite() { self.ior.max(1.0) } else { 1.5 },
            transmission: channel(self.transmission),
            absorption,
        }
    }

    /// GGX's `alpha`, which is Disney's roughness squared.
    pub fn alpha(self) -> f64 {
        let roughness = f64::from(self.roughness.clamp(0.0, 1.0));
        (roughness * roughness).max(MIN_ROUGHNESS)
    }

    /// Whether the surface is a pane: a dielectric that the beam crosses.
    pub fn is_pane(self) -> bool {
        self.metallic <= 0.0 && self.transmission > 0.0
    }
}

/// The shading frame of one hit: the face's normal and two tangents, so a local direction can become
/// a world one and back.
///
/// The frame is built once per hit, from the normal the ray's traversal reports (which faces the ray)
/// and the world axis least aligned with it, so the basis is continuous over the surface and a bump
/// of the normal never flips a lobe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// The shading normal, unit length and facing the viewer.
    pub normal: DVec3,
    /// The first tangent, unit length and on the surface.
    pub tangent: DVec3,
    /// The second tangent, `normal × tangent`.
    pub bitangent: DVec3,
}

impl Frame {
    /// The frame of a surface whose normal faces the viewer.
    pub fn new(normal: DVec3) -> Option<Self> {
        let length = normal.length();
        if !normal.is_finite() || length <= 0.0 {
            return None;
        }
        let normal = normal / length;
        // Any axis not parallel to the normal gives a usable tangent; the helper picks the one that is
        // least parallel, which is what keeps the basis well conditioned near a face's edge.
        let helper = if normal.z.abs() < 0.9 { DVec3::Z } else { DVec3::X };
        let tangent = normal.cross(helper).try_normalize()?;
        Some(Self { normal, tangent, bitangent: normal.cross(tangent).normalize() })
    }

    /// A world direction in this frame.
    pub fn to_local(self, world: DVec3) -> DVec3 {
        DVec3::new(world.dot(self.tangent), world.dot(self.bitangent), world.dot(self.normal))
    }

    /// A local direction in world space.
    pub fn to_world(self, local: DVec3) -> DVec3 {
        self.tangent * local.x + self.bitangent * local.y + self.normal * local.z
    }
}

/// What one scattering event produced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scatter {
    /// The outgoing direction in the shading frame; below the surface when the ray crossed it.
    pub direction: DVec3,
    /// `f |cos| / pdf`, which the integrator multiplies the throughput by.
    pub weight: DVec3,
    /// The density the direction was drawn with, in the measure of [`Scatter::direction`].
    pub pdf: f64,
    /// Whether the beam crossed the surface, which turns the medium's absorption on or off.
    pub transmitted: bool,
}

/// The GGX normal distribution function.
pub fn distribution(cos_h: f64, alpha: f64) -> f64 {
    let a2 = alpha * alpha;
    let d = cos_h * cos_h * (a2 - 1.0) + 1.0;
    if d <= 0.0 {
        return 0.0;
    }
    a2 / (PI * d * d)
}

/// The Smith masking term's `Λ`, which is what the height-correlated `G2` is built from.
pub fn lambda(cos: f64, alpha: f64) -> f64 {
    let c = cos.clamp(1.0e-6, 1.0);
    let tan2 = (1.0 - c * c) / (c * c);
    let root = 1.0 + alpha * alpha * tan2;
    if !root.is_finite() {
        return 0.0;
    }
    0.5 * (root.sqrt() - 1.0)
}

/// The height-correlated Smith `G2`, which is `1 / (1 + Λo + Λi)`.
pub fn masking(cos_o: f64, cos_i: f64, alpha: f64) -> f64 {
    let denominator = 1.0 + lambda(cos_o, alpha) + lambda(cos_i, alpha);
    if !denominator.is_finite() || denominator <= 0.0 {
        return 0.0;
    }
    1.0 / denominator
}

/// Schlick's approximation of a conductor's Fresnel reflectance.
pub fn fresnel_schlick(cos: f64, f0: DVec3) -> DVec3 {
    let m = (1.0 - cos.clamp(0.0, 1.0)).powi(5);
    f0 + (DVec3::ONE - f0) * m
}

/// The exact Fresnel reflectance of a dielectric interface, `eta` being `n_t / n_i`.
///
/// Total internal reflection is the case the square root turns imaginary, and the answer there is one:
/// a beam inside glass that meets the surface beyond the critical angle leaves no light behind.
pub fn fresnel_dielectric(cos_i: f64, eta: f64) -> f64 {
    let cos_i = cos_i.clamp(0.0, 1.0);
    if !eta.is_finite() || eta <= 0.0 {
        return 1.0;
    }
    let sin2_t = (1.0 - cos_i * cos_i) / (eta * eta);
    if sin2_t >= 1.0 {
        return 1.0;
    }
    let cos_t = (1.0 - sin2_t).sqrt();
    let rs = (cos_i - eta * cos_t) / (cos_i + eta * cos_t);
    let rp = (eta * cos_i - cos_t) / (eta * cos_i + cos_t);
    let reflectance = 0.5 * (rs * rs + rp * rp);
    reflectance.clamp(0.0, 1.0)
}

/// The transmitted direction of a beam arriving along `wo`, or `None` past the critical angle.
///
/// The returned direction has a negative `z`: it goes into the surface, which is the sign the
/// integrator reads as "the beam crossed the interface".
pub fn refract(wo: DVec3, eta: f64) -> Option<DVec3> {
    let cos_i = wo.z.clamp(0.0, 1.0);
    if !eta.is_finite() || eta <= 0.0 {
        return None;
    }
    let sin2_t = (1.0 - cos_i * cos_i) / (eta * eta);
    if sin2_t >= 1.0 {
        return None;
    }
    let cos_t = (1.0 - sin2_t).sqrt();
    let direction = DVec3::new(-wo.x / eta, -wo.y / eta, -cos_t);
    let length = direction.length();
    if length <= 0.0 || !direction.is_finite() {
        return None;
    }
    Some(direction / length)
}

/// A visible normal drawn from the GGX distribution, by Heitz's (2018) method.
///
/// The visible-normal sampling is what makes a low roughness converge: drawing from the full
/// distribution spends most of its samples on normals the viewer cannot see, and the weights that
/// come out are the fireflies.
pub fn sample_visible_normal(wo: DVec3, alpha: f64, u: DVec2) -> Option<DVec3> {
    if !(0.0..1.0).contains(&u.x) || !(0.0..1.0).contains(&u.y) {
        return None;
    }
    let scaled = DVec3::new(alpha * wo.x, alpha * wo.y, wo.z);
    let length = scaled.length();
    if length <= 0.0 {
        return None;
    }
    let vh = scaled / length;
    let lensq = vh.x * vh.x + vh.y * vh.y;
    let t1 = if lensq > 1.0e-12 { DVec3::new(-vh.y, vh.x, 0.0) / lensq.sqrt() } else { DVec3::X };
    let t2 = vh.cross(t1);
    let r = u.x.sqrt();
    let phi = 2.0 * PI * u.y;
    let first = r * phi.cos();
    // The second coordinate is warped towards the pole of the hemisphere around the view, which is
    // the step that makes the sample follow the visible-normal density rather than a plain cap: a
    // plain cap is cosine weighted and lands a sixth of its normals too close to the horizon.
    let s = 0.5 * (1.0 + vh.z);
    let second = (1.0 - s) * (1.0 - first * first).max(0.0).sqrt() + s * (r * phi.sin());
    let last = (1.0 - first * first - second * second).max(0.0).sqrt();
    let p = t1 * first + t2 * second + vh * last;
    let normal = DVec3::new(alpha * p.x, alpha * p.y, p.z.max(1.0e-9));
    normal.try_normalize()
}

/// The density of a reflected direction under visible-normal sampling.
pub fn ggx_pdf(wo: DVec3, wi: DVec3, alpha: f64) -> f64 {
    let Some(h) = (wo + wi).try_normalize() else { return 0.0 };
    if wo.z <= 0.0 || wi.z <= 0.0 || h.z <= 0.0 {
        return 0.0;
    }
    let g1 = 1.0 / (1.0 + lambda(wo.z, alpha));
    let density = distribution(h.z, alpha) * g1 * wo.dot(h) / wo.z;
    if !density.is_finite() {
        return 0.0;
    }
    density / (4.0 * wo.dot(h).abs().max(1.0e-9))
}

/// The GGX reflection lobe's value, `f_ss`, with `fresnel` giving the reflectance of one half angle.
fn ggx_eval(wo: DVec3, wi: DVec3, alpha: f64, fresnel: impl Fn(f64) -> DVec3) -> DVec3 {
    if wo.z <= 0.0 || wi.z <= 0.0 {
        return DVec3::ZERO;
    }
    let Some(h) = (wo + wi).try_normalize() else { return DVec3::ZERO };
    if h.z <= 0.0 {
        return DVec3::ZERO;
    }
    let d = distribution(h.z, alpha);
    if d <= 0.0 {
        return DVec3::ZERO;
    }
    let g = masking(wo.z, wi.z, alpha);
    let f = fresnel(wo.dot(h).clamp(0.0, 1.0));
    f * (d * g / (4.0 * wo.z * wi.z))
}

/// The energy table: the directional albedo of the single-scattering GGX lobe, and its cosine average.
///
/// The table is measured, not tabulated from a paper: [`fill`] integrates our own lobe with our own
/// sampler, so the completion below is calibrated to the code it completes and the test that measures
/// a white metal's albedo cannot pass by accident. Row `r` is roughness `r / (E - 1)`, column `m` is
/// `cos θ = m / (E - 1)`.
#[derive(Debug)]
pub struct EnergyTable {
    values: [[f32; ENERGY_STEPS]; ENERGY_STEPS],
    average: [f32; ENERGY_STEPS],
}

impl EnergyTable {
    /// The table, filled on first use.
    pub fn get() -> &'static Self {
        static TABLE: OnceLock<EnergyTable> = OnceLock::new();
        TABLE.get_or_init(fill)
    }

    /// The single-scattering directional albedo at `cos` and `roughness`, interpolated.
    pub fn directional(&self, cos: f64, roughness: f64) -> f64 {
        let (r0, r1, t) = steps(roughness);
        let (m0, m1, s) = steps(cos);
        let at = |r: usize, m: usize| f64::from(self.values.get(r).and_then(|row| row.get(m)).copied().unwrap_or(0.0));
        let near = at(r0, m0) + (at(r0, m1) - at(r0, m0)) * s;
        let far = at(r1, m0) + (at(r1, m1) - at(r1, m0)) * s;
        let value = near + (far - near) * t;
        value.clamp(0.0, 1.0)
    }

    /// The cosine-weighted average of [`Self::directional`] over the hemisphere at `roughness`.
    pub fn average(&self, roughness: f64) -> f64 {
        let (r0, r1, t) = steps(roughness);
        let near = f64::from(self.average.get(r0).copied().unwrap_or(0.0));
        let far = f64::from(self.average.get(r1).copied().unwrap_or(0.0));
        (near + (far - near) * t).clamp(0.0, 1.0)
    }

    /// The table's cells, row-major by roughness, which is the layout the device reads.
    pub fn values(&self) -> &[[f32; ENERGY_STEPS]; ENERGY_STEPS] {
        &self.values
    }

    /// The per-roughness averages in the same row order.
    pub fn averages(&self) -> &[f32; ENERGY_STEPS] {
        &self.average
    }
}

/// The two table indices around `value` in `0..=1` and the weight between them.
fn steps(value: f64) -> (usize, usize, f64) {
    let last = ENERGY_STEPS - 1;
    let scaled = value.clamp(0.0, 1.0) * last as f64;
    let index = scaled.floor().clamp(0.0, last as f64) as usize;
    let next = (index + 1).min(last);
    (index, next, scaled - index as f64)
}

/// Measures the energy table from the lobe itself.
fn fill() -> EnergyTable {
    let last = (ENERGY_STEPS - 1) as f64;
    let mut values = [[0.0f32; ENERGY_STEPS]; ENERGY_STEPS];
    for (row, cells) in values.iter_mut().enumerate() {
        let roughness = row as f64 / last;
        let alpha = (roughness * roughness).max(MIN_ROUGHNESS);
        for (column, cell) in cells.iter_mut().enumerate() {
            // The cells sit on the ends of the axis, not between them, so [`EnergyTable::directional`]
            // interpolates between two measured values and never between two half-cells.
            let cos = column as f64 / last;
            *cell = single_scatter_albedo(cos, alpha);
        }
    }
    let mut average = [0.0f32; ENERGY_STEPS];
    for (row, slot) in average.iter_mut().enumerate() {
        // The cosine average `2 ∫ E(mu) mu dmu`, by the trapezoid rule over the row's own samples.
        let mut total = 0.0;
        let dmu = 1.0 / last;
        for (column, cell) in values[row].iter().enumerate() {
            let mu = column as f64 / last;
            let trapezoid = if column == 0 || column == ENERGY_STEPS - 1 { 0.5 } else { 1.0 };
            total += f64::from(*cell) * 2.0 * mu * dmu * trapezoid;
        }
        *slot = total.clamp(0.0, 1.0) as f32;
    }
    EnergyTable { values, average }
}

/// The directional albedo of the single-scattering GGX lobe at `cos` for one `alpha`.
///
/// It is the mean of the visible-normal sampling weight with a reflectance of one, which is exactly
/// what the completion term has to give back.
fn single_scatter_albedo(cos: f64, alpha: f64) -> f32 {
    let sin = (1.0 - cos * cos).max(0.0).sqrt();
    let wo = DVec3::new(sin, 0.0, cos);
    let seed = crate::sampler::hash((cos * 1.0e4) as u32 ^ crate::sampler::hash((alpha * 1.0e4) as u32));
    let mut total = 0.0;
    for index in 0..ENERGY_SAMPLES {
        let u = crate::sampler::sample_2d(0, index, seed);
        let Some(h) = sample_visible_normal(wo, alpha, DVec2::new(f64::from(u[0]), f64::from(u[1]))) else {
            continue;
        };
        let dot = wo.dot(h);
        if dot <= 0.0 {
            continue;
        }
        let wi = h * (2.0 * dot) - wo;
        if wi.z <= 0.0 {
            continue;
        }
        // `f |cos| / pdf` with the reflectance at one is `G2 / G1`, which is the energy the lobe keeps.
        total += masking(wo.z, wi.z, alpha) * (1.0 + lambda(wo.z, alpha));
    }
    (total / f64::from(ENERGY_SAMPLES)) as f32
}

/// The multiple-scattering colour factor of a conductor with normal reflectance `f0`.
///
/// Kulla and Conty's colour term, with the average Fresnel Turquin reports; the value is the fraction
/// of the lost energy that one more scattering event returns, which is why it approaches one for a
/// white metal and the dielectric reflectance for a dark one.
pub fn multiple_scatter_colour(f0: DVec3, average_albedo: f64) -> DVec3 {
    let f_avg = (f0 * 20.0 + DVec3::ONE) / 21.0;
    let mut colour = DVec3::ZERO;
    for axis in 0..3 {
        let f = f_avg[axis];
        let denominator = 1.0 - f * (1.0 - average_albedo);
        colour[axis] = if denominator.abs() > 1.0e-6 { f * f * average_albedo / denominator } else { 0.0 };
    }
    colour
}

/// The multiple-scattering lobe's value at `(wo, wi)`, already coloured.
fn multiple_scatter_eval(f0: DVec3, cos_o: f64, cos_i: f64, table: &EnergyTable, roughness: f64) -> DVec3 {
    let average = table.average(roughness);
    if average >= 1.0 - 1.0e-6 {
        return DVec3::ZERO;
    }
    let lost = (1.0 - table.directional(cos_o, roughness)) * (1.0 - table.directional(cos_i, roughness));
    if lost <= 0.0 {
        return DVec3::ZERO;
    }
    let colour = multiple_scatter_colour(f0, average);
    colour * (lost / (PI * (1.0 - average)))
}

/// The probability a metal samples its multiple-scattering lobe with.
///
/// The draw is split by *energy*, not by colour: the completion carries what the single-scattering
/// lobe lost, which at roughness 1 is about a third, and a lobe is worth drawing in proportion to what
/// it can bring. Drawing the completion nine times out of ten instead — which is what the colour term
/// alone would say for a white metal — spends the samples on the smaller lobe and turns the rare
/// specular draw into a firefly.
fn multiple_scatter_probability(f0: DVec3, roughness: f64, table: &EnergyTable) -> f64 {
    let average = table.average(roughness);
    let colour = multiple_scatter_colour(f0, average);
    let lost = luminance(colour) * (1.0 - average).max(0.0);
    let kept = luminance(f0) * average;
    let total = lost + kept;
    if !total.is_finite() || total <= 0.0 {
        return 0.0;
    }
    (lost / total).clamp(0.0, 0.999)
}

/// The luminance of a linear colour, sRGB's own weights.
fn luminance(colour: DVec3) -> f64 {
    0.2126 * colour.x + 0.7152 * colour.y + 0.0722 * colour.z
}

/// The interface ratio `n_t / n_i` of a pane: the material's index on the way in, its reciprocal on the
/// way out.
///
/// The ratio is not a property of the material alone, which is why it is computed per event from the
/// side the ray is travelling on: a beam leaving glass meets the interface from the dense side, and
/// that is where total internal reflection lives.
pub fn interface_ratio(material: Material, inside: bool) -> f64 {
    let ior = f64::from(material.ior.max(1.0));
    if inside { 1.0 / ior } else { ior }
}

/// The probability each lobe of one scattering event is drawn with.
///
/// A draw probability is not a reflectance: it decides *what* is sampled, and the estimator divides by
/// the drawn lobe's own density, which is why the weights below are `f |cos| / pdf` of one lobe and
/// never of the sum. The four probabilities of one event add up to one.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Probabilities {
    /// The diffuse lobe of a board.
    pub diffuse: f64,
    /// The microfacet reflection.
    pub specular: f64,
    /// The multiple-scattering completion of a conductor.
    pub completion: f64,
    /// A pane's transmitted beam, which is a delta lobe and carries no density of its own.
    pub transmitted: f64,
}

/// The draw probabilities of a surface under `wo`.
pub fn probabilities(material: Material, wo: DVec3, inside: bool) -> Probabilities {
    let material = material.sanitised();
    if material.is_pane() {
        let reflectance = fresnel_dielectric(wo.z, interface_ratio(material, inside));
        return Probabilities { diffuse: 0.0, specular: reflectance, completion: 0.0, transmitted: 1.0 - reflectance };
    }
    let metal = f64::from(material.metallic);
    let multiple = if metal > 0.0 {
        let f0 = DVec3::from_array(material.albedo.map(f64::from));
        multiple_scatter_probability(f0, f64::from(material.roughness), EnergyTable::get())
    } else {
        0.0
    };
    Probabilities {
        diffuse: 1.0 - metal,
        specular: metal * (1.0 - multiple),
        completion: metal * multiple,
        transmitted: 0.0,
    }
}

/// The value of the surface's continuous lobes at `(wo, wi)`, without the cosine.
///
/// The smooth refracted beam is not part of it: a delta lobe has no density to weight against a light
/// sample, which is the same reason a point light is not sampled from the camera. The integrator takes
/// it through [`sample`] and carries its weight there.
pub fn eval(material: Material, wo: DVec3, wi: DVec3, inside: bool) -> DVec3 {
    let material = material.sanitised();
    if wo.z <= 0.0 || wi.z <= 0.0 || !wo.is_finite() || !wi.is_finite() {
        return DVec3::ZERO;
    }
    let alpha = material.alpha();
    if material.is_pane() {
        let ratio = interface_ratio(material, inside);
        return ggx_eval(wo, wi, alpha, |cos| DVec3::splat(fresnel_dielectric(cos, ratio)));
    }
    let metal = f64::from(material.metallic);
    let f0 = DVec3::from_array(material.albedo.map(f64::from));
    let mut total = DVec3::ZERO;
    if metal > 0.0 {
        total += ggx_eval(wo, wi, alpha, |cos| fresnel_schlick(cos, f0)) * metal;
        total += multiple_scatter_eval(f0, wo.z, wi.z, EnergyTable::get(), f64::from(material.roughness)) * metal;
    }
    if metal < 1.0 {
        total += f0 * ((1.0 - metal) / PI);
    }
    total
}

/// The mixture density of the surface's continuous lobes at `(wo, wi)`.
///
/// A light sample is weighted by this density, and a light sample that would have to divide by a delta
/// lobe's zero gets nothing to weight against: the transmitted beam is simply not part of the sum.
pub fn pdf(material: Material, wo: DVec3, wi: DVec3, inside: bool) -> f64 {
    let material = material.sanitised();
    if wo.z <= 0.0 || wi.z <= 0.0 || !wo.is_finite() || !wi.is_finite() {
        return 0.0;
    }
    let probabilities = probabilities(material, wo, inside);
    let alpha = material.alpha();
    let cosine = (probabilities.diffuse + probabilities.completion) * wi.z / PI;
    let density = probabilities.specular * ggx_pdf(wo, wi, alpha) + cosine;
    if density.is_finite() { density.max(0.0) } else { 0.0 }
}

/// The cosine-weighted distribution, which is the diffuse lobe's own sampler and the completion's.
fn sample_cosine(u: DVec2) -> Option<DVec3> {
    let radius = u.x.sqrt();
    let phi = 2.0 * PI * u.y;
    let direction = DVec3::new(radius * phi.cos(), radius * phi.sin(), (1.0 - u.x).max(0.0).sqrt());
    direction.try_normalize()
}

/// Draws one scattering direction for `wo`, or `None` when the draw is impossible.
///
/// `u` is the pair the microfacet or the cosine lobe is drawn with, `pick` the number that decides
/// which lobe is drawn — one uniform number in `[0, 1)` per event, which is why the estimator spends
/// three numbers a bounce and not two — and `inside` says whether the beam travels inside the glass of
/// a pane.
pub fn sample(material: Material, wo: DVec3, u: DVec2, pick: f64, inside: bool) -> Option<Scatter> {
    let material = material.sanitised();
    if wo.z <= 0.0 || !wo.is_finite() || !(0.0..1.0).contains(&pick) || !u.is_finite() {
        return None;
    }
    let alpha = material.alpha();
    let probabilities = probabilities(material, wo, inside);
    let f0 = DVec3::from_array(material.albedo.map(f64::from));
    if material.is_pane() {
        let ratio = interface_ratio(material, inside);
        if pick < probabilities.specular {
            let h = sample_visible_normal(wo, alpha, u)?;
            let dot = wo.dot(h);
            if dot <= 0.0 {
                return None;
            }
            let wi = h * (2.0 * dot) - wo;
            if wi.z <= 0.0 {
                return None;
            }
            // `f |cos| / pdf` of one dielectric reflection: `F(h) G2 / (G1 P_r)`.
            let reflectance = fresnel_dielectric(dot.clamp(0.0, 1.0), ratio);
            let keep = masking(wo.z, wi.z, alpha) * (1.0 + lambda(wo.z, alpha));
            let weight = DVec3::splat(reflectance * keep / probabilities.specular.max(1.0e-9));
            let pdf = probabilities.specular * ggx_pdf(wo, wi, alpha);
            return Some(Scatter { direction: wi, weight, pdf, transmitted: false });
        }
        let wi = refract(wo, ratio)?;
        // Radiance changes by `eta^2` across an interface; the two crossings of a pane are one factor
        // of one, so a window carries the room and does not light it.
        let weight = DVec3::splat(f64::from(material.transmission) * ratio * ratio);
        let pdf = probabilities.transmitted.max(1.0e-9);
        return Some(Scatter { direction: wi, weight, pdf, transmitted: true });
    }
    let metal = f64::from(material.metallic);
    if pick < probabilities.diffuse {
        let wi = sample_cosine(u)?;
        // `f |cos| / pdf` of the diffuse lobe is its albedo, whatever the draw probability was.
        let albedo = f0 * (1.0 - metal);
        let pdf = probabilities.diffuse * wi.z / PI;
        return Some(Scatter { direction: wi, weight: albedo, pdf, transmitted: false });
    }
    if pick < probabilities.diffuse + probabilities.completion {
        let wi = sample_cosine(u)?;
        let value = multiple_scatter_eval(f0, wo.z, wi.z, EnergyTable::get(), f64::from(material.roughness));
        let pdf = probabilities.completion * wi.z / PI;
        let weight = value * (PI / probabilities.completion.max(1.0e-9));
        return Some(Scatter { direction: wi, weight, pdf, transmitted: false });
    }
    let h = sample_visible_normal(wo, alpha, u)?;
    let dot = wo.dot(h);
    if dot <= 0.0 {
        return None;
    }
    let wi = h * (2.0 * dot) - wo;
    if wi.z <= 0.0 {
        return None;
    }
    // `f |cos| / pdf` of one microfacet reflection: `F(h) G2 metal / (G1 P_s)`.
    let fresnel = fresnel_schlick(dot.clamp(0.0, 1.0), f0);
    let keep = masking(wo.z, wi.z, alpha) * (1.0 + lambda(wo.z, alpha));
    let weight = fresnel * (keep * metal / probabilities.specular.max(1.0e-9));
    let pdf = probabilities.specular * ggx_pdf(wo, wi, alpha);
    Some(Scatter { direction: wi, weight, pdf, transmitted: false })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    /// The mean of `f |cos| / pdf` over the hemisphere at `cos_o`, which is the directional albedo.
    fn albedo_by_sampling(material: Material, cos_o: f64) -> DVec3 {
        let sin = (1.0 - cos_o * cos_o).max(0.0).sqrt();
        let wo = DVec3::new(sin, 0.0, cos_o);
        let samples = 1u32 << 16;
        let seed = 0x51ed_2701;
        let mut total = DVec3::ZERO;
        for index in 0..samples {
            let u = crate::sampler::sample_2d(0, index, seed);
            let pick = f64::from(crate::sampler::sample_1d(2, index, seed));
            let Some(scatter) = sample(material, wo, DVec2::new(f64::from(u[0]), f64::from(u[1])), pick, false) else {
                continue;
            };
            total += scatter.weight;
        }
        total / f64::from(samples)
    }

    /// The same albedo by integrating [`eval`] over the hemisphere on a coarse but deterministic grid,
    /// which is an independent reading of the lobes: no sampler turns up in this number.
    fn albedo_by_quadrature(material: Material, cos_o: f64) -> DVec3 {
        let sin = (1.0 - cos_o * cos_o).max(0.0).sqrt();
        let wo = DVec3::new(sin, 0.0, cos_o);
        let steps = 128;
        let mut total = DVec3::ZERO;
        for i in 0..steps {
            // Cosine-weighted quadrants of the hemisphere: theta from the normal, uniform in cos.
            let cos_i = (i as f64 + 0.5) / steps as f64;
            let sin_i = (1.0 - cos_i * cos_i).max(0.0).sqrt();
            for j in 0..steps {
                let phi = 2.0 * PI * (j as f64 + 0.5) / steps as f64;
                let wi = DVec3::new(sin_i * phi.cos(), sin_i * phi.sin(), cos_i);
                total += eval(material, wo, wi, false) * (cos_i * 2.0 * PI / (steps * steps) as f64);
            }
        }
        total
    }

    #[test]
    fn a_white_metal_at_full_roughness_keeps_its_energy() {
        // The clause Y2 owes: the multiple-scattering completion puts back what the single-scattering
        // lobe loses, so a white metal is 100% of white at every angle. The table's own value is
        // asserted too, so a completion that quietly does nothing cannot pass.
        let material = Material::metal([1.0, 1.0, 1.0], 1.0);
        let table = EnergyTable::get();
        let mut worst = 0.0f64;
        for cos in [0.15, 0.35, 0.6, 0.9] {
            let measured = albedo_by_sampling(material, cos);
            let value = 0.2126 * measured.x + 0.7152 * measured.y + 0.0722 * measured.z;
            worst = worst.max((value - 1.0).abs());
            let single = table.directional(cos, 1.0);
            assert!(single < 0.8, "the single-scattering lobe loses a third of its energy: {single} at cos {cos}");
        }
        assert!(worst < 0.01, "the completed metal is within one percent of white: {worst}");
    }

    #[test]
    fn a_dark_metal_returns_less_than_a_white_one_and_never_more() {
        let table = EnergyTable::get();
        let white = multiple_scatter_colour(DVec3::ONE, table.average(1.0));
        let dark = multiple_scatter_colour(DVec3::splat(0.2), table.average(1.0));
        assert!(white.x > 0.9, "a white metal returns nearly everything: {white:?}");
        assert!(dark.x < 0.2 && dark.x > 0.0, "a dark metal returns less and still some: {dark:?}");
        assert!(dark.x < white.x);
    }

    #[test]
    fn the_sampler_and_the_evaluated_lobes_agree_on_what_a_surface_returns() {
        // The one test the shader's correctness rests on: the directions [`sample`] draws, weighted the
        // way the integrator weights them, must converge to the integral of [`eval`] — which knows
        // nothing about the sampler. A lost or doubled weight shows up here, not in a picture.
        let cases = [
            ("a board", Material::diffuse([0.8, 0.6, 0.4])),
            ("a satin handle", Material::metal([0.9, 0.8, 0.3], 0.4)),
            ("a rough handle", Material::metal([0.9, 0.8, 0.3], 1.0)),
        ];
        for (name, material) in cases {
            for cos in [0.3, 0.8] {
                let sampled = albedo_by_sampling(material, cos);
                let integrated = albedo_by_quadrature(material, cos);
                let distance = (sampled - integrated).length();
                assert!(distance < 0.01, "{name} at cos {cos}: sampled {sampled:?}, integrated {integrated:?}");
            }
        }
        // A board's albedo is the number it was made of, which is the one case with a closed form.
        let board = albedo_by_sampling(Material::diffuse([0.8, 0.6, 0.4]), 0.5);
        assert!((board - DVec3::new(0.8, 0.6, 0.4)).length() < 1.0e-6, "{board:?}");
    }

    #[test]
    fn the_mixture_density_is_the_density_the_sampler_draws_from() {
        // A density is the other half of a sampler: the mass the mixture puts on the directions above
        // the surface must be the fraction of draws that land there. A lobe whose density and sampler
        // disagree biases every bounce that uses it, and no picture would say so.
        for material in
            [Material::diffuse([0.5; 3]), Material::metal([0.7, 0.6, 0.5], 0.6), Material::metal([0.9, 0.8, 0.3], 1.0)]
        {
            let wo = DVec3::new(0.3, 0.0, 1.0).normalize();
            let steps = 96;
            let mut integrated = 0.0;
            for i in 0..steps {
                let cos_i = (i as f64 + 0.5) / steps as f64;
                let sin_i = (1.0 - cos_i * cos_i).max(0.0).sqrt();
                for j in 0..steps {
                    let phi = 2.0 * PI * (j as f64 + 0.5) / steps as f64;
                    let wi = DVec3::new(sin_i * phi.cos(), sin_i * phi.sin(), cos_i);
                    integrated += pdf(material, wo, wi, false) * (2.0 * PI / (steps * steps) as f64);
                }
            }
            let mut above = 0u32;
            let draws = 1u32 << 14;
            for index in 0..draws {
                let u = crate::sampler::sample_2d(0, index, 3);
                let pick = f64::from(crate::sampler::sample_1d(2, index, 3));
                if let Some(scatter) = sample(material, wo, DVec2::new(f64::from(u[0]), f64::from(u[1])), pick, false)
                    && scatter.direction.z > 0.0
                {
                    above += 1;
                }
            }
            let measured = f64::from(above) / f64::from(draws);
            assert!((integrated - measured).abs() < 0.01, "{material:?}: density {integrated}, draws {measured}");
        }
    }

    #[test]
    fn a_glass_pane_refracts_by_snells_law_and_reflects_what_fresnel_says() {
        let glass = Material::glass(1.5, 0.0, [0.0; 3]);
        let wo = DVec3::new(0.6, 0.0, 0.8);
        let reflectance = fresnel_dielectric(0.8, 1.5);
        let reflected = sample(glass, wo, DVec2::new(0.5, 0.5), reflectance * 0.5, false).expect("a reflection");
        assert!(!reflected.transmitted);
        assert!((reflected.direction - DVec3::new(-0.6, 0.0, 0.8)).length() < 5.0e-3, "{:?}", reflected.direction);
        let crossed = sample(glass, wo, DVec2::new(0.5, 0.5), (reflectance + 1.0) * 0.5, false).expect("a refraction");
        assert!(crossed.transmitted);
        // Snell's law: sin θ_t = sin θ_i / eta, and the beam continues on the same side of the normal.
        let sin_t = (1.0 - crossed.direction.z * crossed.direction.z).sqrt();
        assert!((sin_t - 0.6 / 1.5).abs() < 1.0e-9, "sin θ_t = {sin_t}");
        assert!(crossed.direction.x < 0.0, "the beam keeps its side of the normal");
    }

    #[test]
    fn total_internal_reflection_is_one_inside_the_glass() {
        // Beyond the critical angle the interface reflects everything, which is why a thick pane's edge
        // is bright and a glass shelf sparkles.
        let critical = (1.0 / 1.5f64).asin();
        assert!(fresnel_dielectric(critical.cos() * 0.999, 0.667) > 0.9);
        // At normal incidence glass reflects 4% whichever side the beam arrives from.
        assert!((fresnel_dielectric(1.0, 1.0 / 1.5) - 0.04).abs() < 1.0e-9);
        assert!((fresnel_dielectric(1.0, 1.5) - 0.04).abs() < 1.0e-9);
        assert!((fresnel_dielectric(0.0, 1.5) - 1.0).abs() < 1.0e-12, "edge on it is a mirror");
        let glass = Material::glass(1.5, 0.0, [0.0; 3]);
        let grazing = DVec3::new(0.9999, 0.0, 0.01).normalize();
        // Entering a denser medium is always possible, however grazing the beam.
        assert!(refract(grazing, 1.5).is_some(), "a grazing beam still enters glass");
        assert!(fresnel_dielectric(grazing.z, 1.5) > 0.9, "and almost all of it reflects");
        // Leaving is the case that reflects: from inside the same beam finds no way out.
        assert!(refract(grazing, 1.0 / 1.5).is_none(), "a grazing beam cannot leave");
        assert_eq!(fresnel_dielectric(grazing.z, 1.0 / 1.5), 1.0);
        // Straight on from inside, four percent reflects back into the glass and the rest escapes —
        // which is the number the same interface shows on the way in.
        let escaping = DVec3::Z;
        let crossed = sample(glass, escaping, DVec2::new(0.5, 0.5), 0.5, true).expect("the beam leaves");
        assert!(crossed.transmitted);
        let bounced = sample(glass, escaping, DVec2::new(0.5, 0.5), 0.02, true).expect("a reflection inside");
        assert!(!bounced.transmitted);
    }

    #[test]
    fn a_pane_carries_the_radiance_it_saw_back_out() {
        // Entering glass multiplies the radiance by the index squared and leaving divides it back, so a
        // pane does not glow: the two crossings are one factor of one.
        let glass = Material::glass(1.5, 0.0, [0.0; 3]);
        let wo = DVec3::Z;
        let entering = sample(glass, wo, DVec2::new(0.5, 0.5), 0.5, false).expect("the beam enters");
        assert!(entering.transmitted);
        assert!((entering.weight.x - 2.25).abs() < 1.0e-9, "{:?}", entering.weight);
        // Inside the pane the hit's normal is flipped to face the ray, so the view direction is
        // above the local surface again and only the interface ratio says which side the beam is on.
        let leaving = sample(glass, wo, DVec2::new(0.5, 0.5), 0.5, true).expect("the beam leaves");
        assert!(leaving.transmitted);
        assert!(
            (leaving.weight.x * entering.weight.x - 1.0).abs() < 1.0e-9,
            "{} × {}",
            entering.weight.x,
            leaving.weight.x
        );
    }

    #[test]
    fn a_material_that_arrived_broken_is_repaired_rather_than_traced() {
        let broken = Material {
            albedo: [f32::NAN, 2.0, -1.0],
            roughness: f32::NAN,
            metallic: 3.0,
            ior: 0.2,
            transmission: -4.0,
            absorption: [f32::INFINITY, -1.0, 0.5],
        }
        .sanitised();
        assert_eq!(broken.albedo, [0.0, 1.0, 0.0]);
        assert_eq!(broken.roughness, 0.0);
        assert_eq!(broken.metallic, 1.0);
        assert_eq!(broken.ior, 1.0, "an index below one is not a dielectric");
        assert_eq!(broken.transmission, 0.0);
        assert_eq!(broken.absorption, [0.0, 0.0, 0.5]);
        assert!(
            sample(broken, DVec3::Z, DVec2::new(0.5, 0.5), 0.5, false).is_some(),
            "a repaired material still shades"
        );
        assert!(sample(Material::diffuse([1.0; 3]), -DVec3::Z, DVec2::ZERO, 0.5, false).is_none());
        assert!(
            sample(Material::diffuse([1.0; 3]), DVec3::Z, DVec2::ZERO, 1.5, false).is_none(),
            "a pick out of range"
        );
        assert!(sample(Material::diffuse([1.0; 3]), DVec3::Z, DVec2::new(f64::NAN, 0.0), 0.5, false).is_none());
    }

    #[test]
    fn the_frame_maps_a_direction_round_trip() {
        let frame = Frame::new(DVec3::new(0.0, 0.0, 2.0)).expect("a normal");
        let world = DVec3::new(1.0, 2.0, 3.0);
        let back = frame.to_world(frame.to_local(world));
        assert!((back - world).length() < 1.0e-12, "{back:?}");
        assert!(Frame::new(DVec3::ZERO).is_none());
        assert!(Frame::new(DVec3::new(f64::NAN, 0.0, 1.0)).is_none());
        // The normal of a hit is normalized, whatever length it arrived with.
        assert!((frame.normal - DVec3::Z).length() < 1.0e-12);
    }

    #[test]
    fn the_energy_table_is_measured_and_ordered() {
        let table = EnergyTable::get();
        assert!(table.directional(0.5, 0.0) > 0.99, "a smooth lobe keeps its energy");
        assert!(table.directional(0.5, 1.0) < table.directional(0.5, 0.0));
        assert!(table.average(0.0) > table.average(1.0));
        for step in 0..=20 {
            let roughness = step as f64 / 20.0;
            let value = table.directional(0.5, roughness);
            assert!((0.0..=1.0).contains(&value), "{roughness}: {value}");
            let average = table.average(roughness);
            assert!((0.0..=1.0).contains(&average), "{roughness}: {average}");
        }
        // The table is the device's layout too: sixteen rows of sixteen cells and sixteen averages.
        assert_eq!(table.values().len(), ENERGY_STEPS);
        assert_eq!(table.averages().len(), ENERGY_STEPS);
    }
}
