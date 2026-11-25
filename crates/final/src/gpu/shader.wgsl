// The final render's integrator: a path tracer with three lobes, next-event estimation and a
// stratified sampler.
//
// This file is the transcription of the Rust beside it and nothing more: `sampler.rs` owns the
// sequence, `bsdf.rs` owns the lobes and `light.rs` the alias table, and every expression here is the
// one those modules write and test. A change to one without the other is a picture that is wrong in a
// way no test can name, so the rule is the reverse of the usual one: the maths is decided and tested
// on the CPU, and this is where it is retyped for the device. The two tables — Sobol's direction
// numbers and the conductors' energy completion — are uploaded from the same Rust values for the same
// reason, so no number is copied by hand.
//
// The estimator is a path integral:
//
//     L(x, wo) = sum over bounces of  throughput * (direct light + BSDF sample)
//
// where a diffuse surface's direct light is `albedo / PI * E * cos` and every other lobe is evaluated
// by `bsdf_eval`. A point light is chosen by emitted power through the alias table and divided by the
// share it was given; the key light is a sun and is traced at every vertex, because a picture without
// its main shadow is not the picture a person framed. The environment is what a ray sees when it
// leaves the scene — a uniform radiance, not the viewport's background colour — and it is what makes a
// shadow a value instead of a hole.
//
// The one clamp is `MAX_SAMPLE_RADIANCE`: a path that walks into a bright light through a pane can
// carry an arbitrarily large number, and an `f32` accumulator shared by four thousand samples has no
// room for it. Its bias is measured against an unclamped reference rather than chosen by feel
// (`plan/notes/Y2.md`).
//
// Russian roulette past `RR_DEPTH` keeps the tail of a path honest: a path is continued with the
// probability of the throughput it still carries, and divided by that probability, so a long path
// costs its own share of the samples and nothing more.
//
// Refuses: nothing. Every draw is checked before use and a direction that comes out of the surface, a
// zero pdf or a draw below the surface ends the path with the radiance it has, never with a NaN.

const PI: f32 = 3.141592653589793;
const MAX_POINT_LIGHTS: u32 = 8u;
const MAX_AREA_LIGHTS: u32 = 4u;
const MAX_STACK: u32 = 64u;
// How far a ray must travel before it may hit anything: the camera's near plane, in millimetres.
const T_MIN: f32 = 0.05;
// How far a ray may travel before it hits nothing, which is also the distance a background pixel's
// guide reports (`crate::denoise::FAR_AWAY`).
const FAR_AWAY: f32 = 1.0e7;
// How far off a surface a secondary ray starts, so it does not meet the face it left.
const SURFACE_BIAS: f32 = 0.1;
const TILE: u32 = 8u;
// The most one sample may contribute, which is where an `f32` accumulator is stopped before it overflows.
const MAX_SAMPLE_RADIANCE: f32 = 1.0e6;
// How deep a path may go before it is only kept by its own throughput.
const RR_DEPTH: u32 = 3u;
// The hard stop: a pane between two mirrors is a path that never ends.
const MAX_BOUNCES: u32 = 24u;
// The dimensions the Sobol generator carries, which is `sampler::DIMENSIONS`.
const DIMENSIONS: u32 = 8u;
// The energy table's resolution, which is `bsdf::ENERGY_STEPS`.
const ENERGY_STEPS: u32 = 16u;
// The smallest roughness the lobes are evaluated at, which is `bsdf::MIN_ROUGHNESS`.
const MIN_ROUGHNESS: f32 = 1.0e-3;
// The light index that means "no point light", which the alias table never produces.
const NO_LIGHT: u32 = 4294967295u;

struct Face {
    a: vec4<f32>,
    b: vec4<f32>,
    c: vec4<f32>,
    colour: vec4<f32>,
    // x: roughness, y: metallic, z: the index of refraction, w: transmission.
    finish: vec4<f32>,
    // rgb: Beer-Lambert extinction per millimetre.
    tint: vec4<f32>,
    // The decor's coordinate at the first two corners: xy the first, zw the second.
    uv_a: vec4<f32>,
    // The third corner's coordinate in xy.
    uv_b: vec4<f32>,
    // x: the picture's index plus one, or zero for a surface with no decor.
    decor: vec4<u32>,
};

// One picture: where its texels start in `decor_texels`, and how many wide and high it is.
struct Decor {
    // x: the first texel, y: the width, z: the height.
    info: vec4<u32>,
};

struct Node {
    min: vec4<f32>,
    max: vec4<f32>,
    // x: a leaf's first face or an interior node's left child; y: a leaf's face count.
    info: vec4<u32>,
};

// One pixel of a strip: its samples' sums and what its primary ray met.
//
// It is one buffer because a compute shader has eight storage bindings in a stage and the frame takes
// six, and because one sample writes all four vectors anyway. The sums and the guide die with the
// strip: the host reads them once, averages them, filters them, and keeps only the picture.
struct Pixel {
    // xyz: the sum of the samples' radiance; w: how many samples there are.
    sum: vec4<f32>,
    // xyz: the sum of the squares of those samples, which is the variance's numerator.
    squares: vec4<f32>,
    // xyz: the linear albedo of the face the ray met, or the studio where it met nothing;
    // w: how far the ray travelled in millimetres, or FAR_AWAY.
    albedo: vec4<f32>,
    // xyz: the shading normal that faced the ray, or the ray's own direction where it met nothing.
    normal: vec4<f32>,
};

struct Uniform {
    eye: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    forward: vec4<f32>,
    // x: the tangent of half the vertical field of view, or half the orthographic height;
    // y: the aspect ratio; z and w: the image's width and height.
    view: vec4<f32>,
    // x: the face count; y: the node count; z: the sample index; w: the point-light count.
    counts: vec4<u32>,
    // x: the first image row of this strip; y: its row count; z and w: unused.
    strip: vec4<u32>,
    // x: the fewest samples a pixel takes before it may retire; y: the relative error a finished image
    // is held to; z: the dimmest light a relative error is measured against (`crate::denoise::MIN_SAMPLES`,
    // `TARGET_RELATIVE_ERROR` and `LIGHT_FLOOR`); w: unused.
    denoise: vec4<f32>,
    // xyz: the key light's irradiance; w: one when it exists.
    key: vec4<f32>,
    key_direction: vec4<f32>,
    point_position: array<vec4<f32>, 8>,
    // rgb: linear colour; a: radiant power in watts.
    point_colour: array<vec4<f32>, 8>,
    // xyz: the uniform environment's radiance.
    environment: vec4<f32>,
    // The alias table's own threshold per column, four columns to a vector.
    point_probability: array<vec4<f32>, 2>,
    // The alias table's other column per column.
    point_alias: array<vec4<u32>, 2>,
    // The share of the light each point light carries, which is the density of a pick.
    point_share: array<vec4<f32>, 2>,
    // The rectangular lights: a window, a softbox or a strip (L2). `xyz` is the centre of the face and
    // the two half-edges lie in its own plane, so a corner is `origin +- half_width +- half_height`.
    area_origin: array<vec4<f32>, 4>,
    area_half_width: array<vec4<f32>, 4>,
    area_half_height: array<vec4<f32>, 4>,
    // xyz: the radiance the face emits; w: its area in square millimetres.
    area_radiance: array<vec4<f32>, 4>,
    // x: how many rectangular lights the frame carries.
    area_count: vec4<u32>,
    // The alias table of the rectangles, by emitted power, in the point lights' own shape.
    area_probability: vec4<f32>,
    area_alias: vec4<u32>,
    area_share: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Uniform;
@group(0) @binding(1) var<storage, read> faces: array<Face>;
@group(0) @binding(2) var<storage, read> nodes: array<Node>;
// Sobol's direction numbers, four dimensions of thirty-two words, from `sampler::direction_numbers`.
@group(0) @binding(3) var<storage, read> sobol: array<u32>;
// The single-scattering directional albedo table, row-major by roughness, then one average per row,
// from `bsdf::EnergyTable`.
@group(0) @binding(4) var<storage, read> energy: array<f32>;
// The board a face is cut from: its picture, packed one texel per word, and the table that finds it.
// A storage buffer and not a sampled texture, for the reason the Sobol directions are: the shader
// wraps and filters the coordinate itself, so the viewport and the tracer read one picture with the
// same arithmetic instead of trusting two views of it to agree.
@group(0) @binding(5) var<storage, read> decor_texels: array<u32>;
@group(0) @binding(6) var<storage, read> decor_entries: array<Decor>;
// One strip's pixels: their sums, their second moment and their guide, in one buffer.
@group(1) @binding(0) var<storage, read_write> pixels: array<Pixel>;

struct Ray {
    origin: vec3<f32>,
    direction: vec3<f32>,
    inverse: vec3<f32>,
};

// The nearest hit and the normal facing the ray; `t` is negative when nothing was met.
struct Hit {
    t: f32,
    normal: vec3<f32>,
    face: u32,
};

// What a surface is made of, in the shading frame.
struct Material {
    albedo: vec3<f32>,
    roughness: f32,
    metallic: f32,
    ior: f32,
    transmission: f32,
    absorption: vec3<f32>,
};

// The probability each lobe of one scattering event is drawn with, which adds up to one.
struct Probabilities {
    diffuse: f32,
    specular: f32,
    completion: f32,
    transmitted: f32,
};

struct Scatter {
    valid: bool,
    // The outgoing direction in the shading frame; below the surface when the ray crossed it.
    direction: vec3<f32>,
    // `f |cos| / pdf`, which the integrator multiplies the throughput by.
    weight: vec3<f32>,
    pdf: f32,
    transmitted: bool,
};

// The shading frame of one hit: the normal and two tangents, so a local direction becomes a world one.
struct Frame {
    normal: vec3<f32>,
    tangent: vec3<f32>,
    bitangent: vec3<f32>,
};

fn safe_normalize(value: vec3<f32>) -> vec3<f32> {
    let length = sqrt(dot(value, value));
    return select(vec3<f32>(0.0), value / length, length > 1.0e-9);
}

// ---------------------------------------------------------------------------------------------------
// The sampler: `sampler.rs`, word for word.
// ---------------------------------------------------------------------------------------------------

// Wang's hash, so one sample's noise is a property of the pixel and the sample index alone.
fn hash(value: u32) -> u32 {
    var x = value;
    x = (x ^ 61u) ^ (x >> 16u);
    x = x * 9u;
    x = x ^ (x >> 4u);
    x = x * 0x27d4eb2du;
    x = x ^ (x >> 15u);
    return x;
}

fn permute(x: u32, seed: u32) -> u32 {
    var value = x + seed;
    value = value ^ (value * 0x6c50b47cu);
    value = value ^ (value * 0xb82f1e52u);
    value = value ^ (value * 0xc7afe638u);
    value = value ^ (value * 0x8d22f6e6u);
    return value;
}

// Owen's scramble of one direction-number word: the most significant bit is the coarsest stratum, so
// the permutation is applied to the bit-reversed word and the result is read back the same way.
fn scramble(x: u32, seed: u32) -> u32 {
    return reverseBits(permute(reverseBits(x), seed));
}

fn direction(dimension: u32, bit: u32) -> u32 {
    return sobol[(dimension % DIMENSIONS) * 32u + bit];
}

// The direction-number word of one dimension at one index, read along the index's Gray code.
fn word(dimension: u32, index: u32) -> u32 {
    let gray = index ^ (index >> 1u);
    var bits = 0u;
    var k = 0u;
    loop {
        if (k >= 32u) {
            break;
        }
        if ((gray >> k) & 1u) == 1u {
            bits = bits ^ direction(dimension, k);
        }
        k = k + 1u;
    }
    return bits;
}

fn fraction(bits: u32) -> f32 {
    return f32(bits >> 8u) * (1.0 / 16777216.0);
}

fn sample_1d(dimension: u32, index: u32, seed: u32) -> f32 {
    return fraction(scramble(word(dimension, index), seed ^ hash(dimension)));
}

fn sample_pair(pair: u32, index: u32, seed: u32) -> vec2<f32> {
    let first = (pair % (DIMENSIONS / 2u)) * 2u;
    return vec2<f32>(sample_1d(first, index, seed), sample_1d(first + 1u, index, seed));
}

// ---------------------------------------------------------------------------------------------------
// Traversal: `ray.rs` and `bvh.rs`.
// ---------------------------------------------------------------------------------------------------

fn make_ray(origin: vec3<f32>, direction: vec3<f32>) -> Ray {
    let unit = safe_normalize(direction);
    var ray: Ray;
    ray.origin = origin;
    ray.direction = unit;
    ray.inverse = vec3<f32>(1.0) / unit;
    return ray;
}

fn bounds_entry(ray: Ray, low: vec3<f32>, high: vec3<f32>, t_max: f32) -> f32 {
    var entry = T_MIN;
    var exit = t_max;
    for (var axis = 0u; axis < 3u; axis = axis + 1u) {
        if (ray.direction[axis] == 0.0) {
            if (ray.origin[axis] < low[axis] || ray.origin[axis] > high[axis]) {
                return -1.0;
            }
            continue;
        }
        let inverse = ray.inverse[axis];
        let near = (low[axis] - ray.origin[axis]) * inverse;
        let far = (high[axis] - ray.origin[axis]) * inverse;
        entry = max(entry, min(near, far));
        exit = min(exit, max(near, far));
        if (exit < entry) {
            return -1.0;
        }
    }
    return entry;
}

fn intersect_face(ray: Ray, face: Face, t_max: f32) -> Hit {
    var miss: Hit;
    miss.t = -1.0;
    miss.normal = vec3<f32>(0.0);
    miss.face = 0u;
    let v0 = face.a.xyz;
    let edge1 = face.b.xyz - v0;
    let edge2 = face.c.xyz - v0;
    let pvec = cross(ray.direction, edge2);
    let determinant = dot(edge1, pvec);
    if (abs(determinant) <= 1.0e-9) {
        return miss;
    }
    let inverse = 1.0 / determinant;
    let tvec = ray.origin - v0;
    let u = dot(tvec, pvec) * inverse;
    if (u < 0.0 || u > 1.0) {
        return miss;
    }
    let qvec = cross(tvec, edge1);
    let v = dot(ray.direction, qvec) * inverse;
    if (v < 0.0 || u + v > 1.0) {
        return miss;
    }
    let t = dot(edge2, qvec) * inverse;
    if (t <= T_MIN || t >= t_max) {
        return miss;
    }
    var normal = safe_normalize(cross(edge1, edge2));
    if (dot(normal, ray.direction) > 0.0) {
        normal = -normal;
    }
    var hit: Hit;
    hit.t = t;
    hit.normal = normal;
    hit.face = 0u;
    return hit;
}

fn trace(ray: Ray, t_max: f32) -> Hit {
    var miss: Hit;
    miss.t = -1.0;
    miss.normal = vec3<f32>(0.0);
    miss.face = 0u;
    if (camera.counts.y == 0u) {
        return miss;
    }
    var stack: array<u32, MAX_STACK>;
    stack[0] = 0u;
    var top = 1u;
    var best = t_max;
    var best_face = -1;
    var best_normal = vec3<f32>(0.0);
    while (top > 0u) {
        top = top - 1u;
        let index = stack[top];
        if (index >= camera.counts.y) {
            continue;
        }
        let node = nodes[index];
        if (bounds_entry(ray, node.min.xyz, node.max.xyz, best) < 0.0) {
            continue;
        }
        if (node.info.y > 0u) {
            let end = min(node.info.x + node.info.y, camera.counts.x);
            for (var face = node.info.x; face < end; face = face + 1u) {
                let hit = intersect_face(ray, faces[face], best);
                if (hit.t > 0.0) {
                    best = hit.t;
                    best_face = i32(face);
                    best_normal = hit.normal;
                }
            }
            continue;
        }
        let left = node.info.x;
        if (left + 1u >= camera.counts.y) {
            continue;
        }
        if (top + 2u > MAX_STACK) {
            break;
        }
        let left_entry = bounds_entry(ray, nodes[left].min.xyz, nodes[left].max.xyz, best);
        let right_entry = bounds_entry(ray, nodes[left + 1u].min.xyz, nodes[left + 1u].max.xyz, best);
        let left_hit = left_entry >= 0.0;
        let right_hit = right_entry >= 0.0;
        if (left_hit && right_hit) {
            if (left_entry <= right_entry) {
                stack[top] = left + 1u;
                stack[top + 1u] = left;
            } else {
                stack[top] = left;
                stack[top + 1u] = left + 1u;
            }
            top = top + 2u;
        } else if (left_hit) {
            stack[top] = left;
            top = top + 1u;
        } else if (right_hit) {
            stack[top] = left + 1u;
            top = top + 1u;
        }
    }
    if (best_face < 0) {
        return miss;
    }
    var hit: Hit;
    hit.t = best;
    hit.normal = best_normal;
    hit.face = u32(best_face);
    return hit;
}

fn occluded(origin: vec3<f32>, direction: vec3<f32>, distance: f32) -> bool {
    return trace(make_ray(origin, direction), distance).t > 0.0;
}

// ---------------------------------------------------------------------------------------------------
// The lobes: `bsdf.rs`, word for word.
// ---------------------------------------------------------------------------------------------------

fn frame_of(normal: vec3<f32>) -> Frame {
    let unit = safe_normalize(normal);
    let helper = select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0, 0.0, 0.0), abs(unit.z) >= 0.9);
    var frame: Frame;
    frame.normal = unit;
    frame.tangent = safe_normalize(cross(unit, helper));
    frame.bitangent = normalize(cross(unit, frame.tangent));
    return frame;
}

fn to_local(frame: Frame, world: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(world, frame.tangent), dot(world, frame.bitangent), dot(world, frame.normal));
}

fn to_world(frame: Frame, local: vec3<f32>) -> vec3<f32> {
    return frame.tangent * local.x + frame.bitangent * local.y + frame.normal * local.z;
}

// One eight-bit sRGB channel as linear light, the curve the viewport's sampler applies for it.
fn srgb_to_linear(channel: f32) -> f32 {
    return select(channel / 12.92, pow((channel + 0.055) / 1.055, 2.4), channel > 0.04045);
}

// An integer wrapped into `0..size`, which is what a print does when the board outruns the sample.
fn wrapped(value: i32, size: i32) -> u32 {
    let remainder = value % size;
    return u32(select(remainder + size, remainder, remainder >= 0));
}

// One texel of the picture, as linear light.
fn decor_texel(entry: Decor, x: u32, y: u32) -> vec3<f32> {
    let width = max(entry.info.y, 1u);
    let height = max(entry.info.z, 1u);
    let packed = decor_texels[entry.info.x + (y % height) * width + (x % width)];
    let red = f32(packed & 255u) / 255.0;
    let green = f32((packed >> 8u) & 255u) / 255.0;
    let blue = f32((packed >> 16u) & 255u) / 255.0;
    return vec3<f32>(srgb_to_linear(red), srgb_to_linear(green), srgb_to_linear(blue));
}

// The picture at `uv`, filtered bilinearly and repeated, which is the sampler the viewport binds.
fn decor_sample(index: u32, uv: vec2<f32>) -> vec3<f32> {
    if (index >= arrayLength(&decor_entries)) {
        return vec3<f32>(1.0);
    }
    let entry = decor_entries[index];
    let width = max(entry.info.y, 1u);
    let height = max(entry.info.z, 1u);
    let across = uv.x * f32(width) - 0.5;
    let down = uv.y * f32(height) - 0.5;
    let x = floor(across);
    let y = floor(down);
    let fx = across - x;
    let fy = down - y;
    let left = wrapped(i32(x), i32(width));
    let right = wrapped(i32(x) + 1, i32(width));
    let top = wrapped(i32(y), i32(height));
    let bottom = wrapped(i32(y) + 1, i32(height));
    let upper = mix(decor_texel(entry, left, top), decor_texel(entry, right, top), fx);
    let lower = mix(decor_texel(entry, left, bottom), decor_texel(entry, right, bottom), fx);
    return mix(upper, lower, fy);
}

// The corner coordinates interpolated where the ray met the face.
//
// The three corners' coordinates are projected from the panel's own frame and carried on the face,
// so this is the barycentric weight of a point in a triangle and nothing else — the same triangle
// the ray test already solved.
fn decor_uv(face: Face, point: vec3<f32>) -> vec2<f32> {
    let a = face.a.xyz;
    let edge0 = face.b.xyz - a;
    let edge1 = face.c.xyz - a;
    let offset = point - a;
    let d00 = dot(edge0, edge0);
    let d01 = dot(edge0, edge1);
    let d11 = dot(edge1, edge1);
    let d20 = dot(offset, edge0);
    let d21 = dot(offset, edge1);
    let denominator = d00 * d11 - d01 * d01;
    if (abs(denominator) < 1.0e-12) {
        return face.uv_a.xy;
    }
    let second = (d11 * d20 - d01 * d21) / denominator;
    let third = (d00 * d21 - d01 * d20) / denominator;
    let first = 1.0 - second - third;
    return first * face.uv_a.xy + second * face.uv_a.zw + third * face.uv_b.xy;
}

// The surface's own colour where the ray met it: the picture multiplies the colour, which is what
// every renderer does with a tinted print and what the viewport's fragment computes.
fn albedo_at(face: Face, point: vec3<f32>) -> vec3<f32> {
    let base = clamp(face.colour.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    if (face.decor.x == 0u) {
        return base;
    }
    return base * decor_sample(face.decor.x - 1u, decor_uv(face, point));
}

fn material_of(face: Face, point: vec3<f32>) -> Material {
    // Every field is clamped where it is used, exactly as `Material::sanitised` does: a document that
    // wrote a NaN must shade as a repaired surface, not poison the accumulator.
    var material: Material;
    material.albedo = albedo_at(face, point);
    material.roughness = clamp(face.finish.x, 0.0, 1.0);
    material.metallic = clamp(face.finish.y, 0.0, 1.0);
    material.ior = max(face.finish.z, 1.0);
    material.transmission = clamp(face.finish.w, 0.0, 1.0);
    material.absorption = max(face.tint.rgb, vec3<f32>(0.0));
    return material;
}

fn alpha_of(material: Material) -> f32 {
    let roughness = clamp(material.roughness, 0.0, 1.0);
    return max(roughness * roughness, MIN_ROUGHNESS);
}

fn is_pane(material: Material) -> bool {
    return material.metallic <= 0.0 && material.transmission > 0.0;
}

fn distribution(cos_h: f32, alpha: f32) -> f32 {
    let a2 = alpha * alpha;
    let d = cos_h * cos_h * (a2 - 1.0) + 1.0;
    if (d <= 0.0) {
        return 0.0;
    }
    return a2 / (PI * d * d);
}

fn lambda_(cos: f32, alpha: f32) -> f32 {
    let c = clamp(cos, 1.0e-6, 1.0);
    let tan2 = (1.0 - c * c) / (c * c);
    let root = 1.0 + alpha * alpha * tan2;
    if (!(root < 1.0e30)) {
        return 0.0;
    }
    return 0.5 * (sqrt(root) - 1.0);
}

fn masking(cos_o: f32, cos_i: f32, alpha: f32) -> f32 {
    let denominator = 1.0 + lambda_(cos_o, alpha) + lambda_(cos_i, alpha);
    if (!(denominator > 0.0)) {
        return 0.0;
    }
    return 1.0 / denominator;
}

fn fresnel_schlick(cos: f32, f0: vec3<f32>) -> vec3<f32> {
    let m = pow(1.0 - clamp(cos, 0.0, 1.0), 5.0);
    return f0 + (vec3<f32>(1.0) - f0) * m;
}

fn fresnel_dielectric(cos_i_in: f32, eta: f32) -> f32 {
    let cos_i = clamp(cos_i_in, 0.0, 1.0);
    if (!(eta > 0.0)) {
        return 1.0;
    }
    let sin2_t = (1.0 - cos_i * cos_i) / (eta * eta);
    if (sin2_t >= 1.0) {
        return 1.0;
    }
    let cos_t = sqrt(1.0 - sin2_t);
    let rs = (cos_i - eta * cos_t) / (cos_i + eta * cos_t);
    let rp = (eta * cos_i - cos_t) / (eta * cos_i + cos_t);
    return clamp(0.5 * (rs * rs + rp * rp), 0.0, 1.0);
}

fn refract(wo: vec3<f32>, eta: f32) -> vec3<f32> {
    let cos_i = clamp(wo.z, 0.0, 1.0);
    let sin2_t = (1.0 - cos_i * cos_i) / (eta * eta);
    let cos_t = sqrt(max(0.0, 1.0 - sin2_t));
    return vec3<f32>(-wo.x / eta, -wo.y / eta, -cos_t);
}

// Heitz's visible-normal sampling, with the warp that makes it the visible distribution rather than a
// plain cap: `bsdf::sample_visible_normal`.
fn sample_visible_normal(wo: vec3<f32>, alpha: f32, u: vec2<f32>) -> vec3<f32> {
    let scaled = vec3<f32>(alpha * wo.x, alpha * wo.y, wo.z);
    let length = sqrt(dot(scaled, scaled));
    if (length <= 0.0) {
        return vec3<f32>(0.0, 0.0, 1.0);
    }
    let vh = scaled / length;
    let lensq = vh.x * vh.x + vh.y * vh.y;
    var t1 = vec3<f32>(1.0, 0.0, 0.0);
    if (lensq > 1.0e-12) {
        t1 = vec3<f32>(-vh.y, vh.x, 0.0) / sqrt(lensq);
    }
    let t2 = cross(vh, t1);
    let r = sqrt(u.x);
    let phi = 2.0 * PI * u.y;
    let first = r * cos(phi);
    let s = 0.5 * (1.0 + vh.z);
    let second = (1.0 - s) * sqrt(max(0.0, 1.0 - first * first)) + s * (r * sin(phi));
    let last = sqrt(max(0.0, 1.0 - first * first - second * second));
    let p = t1 * first + t2 * second + vh * last;
    return safe_normalize(vec3<f32>(alpha * p.x, alpha * p.y, max(p.z, 1.0e-9)));
}

fn ggx_pdf(wo: vec3<f32>, wi: vec3<f32>, alpha: f32) -> f32 {
    let h = safe_normalize(wo + wi);
    if (wo.z <= 0.0 || wi.z <= 0.0 || h.z <= 0.0) {
        return 0.0;
    }
    let g1 = 1.0 / (1.0 + lambda_(wo.z, alpha));
    let density = distribution(h.z, alpha) * g1 * dot(wo, h) / wo.z;
    return density / (4.0 * max(abs(dot(wo, h)), 1.0e-9));
}

fn ggx_eval(wo: vec3<f32>, wi: vec3<f32>, alpha: f32, f0: vec3<f32>) -> vec3<f32> {
    if (wo.z <= 0.0 || wi.z <= 0.0) {
        return vec3<f32>(0.0);
    }
    let h = safe_normalize(wo + wi);
    if (h.z <= 0.0) {
        return vec3<f32>(0.0);
    }
    let d = distribution(h.z, alpha);
    if (d <= 0.0) {
        return vec3<f32>(0.0);
    }
    let g = masking(wo.z, wi.z, alpha);
    let f = fresnel_schlick(clamp(dot(wo, h), 0.0, 1.0), f0);
    return f * (d * g / (4.0 * wo.z * wi.z));
}

fn ggx_eval_dielectric(wo: vec3<f32>, wi: vec3<f32>, alpha: f32, eta: f32) -> vec3<f32> {
    if (wo.z <= 0.0 || wi.z <= 0.0) {
        return vec3<f32>(0.0);
    }
    let h = safe_normalize(wo + wi);
    if (h.z <= 0.0) {
        return vec3<f32>(0.0);
    }
    let d = distribution(h.z, alpha);
    if (d <= 0.0) {
        return vec3<f32>(0.0);
    }
    let g = masking(wo.z, wi.z, alpha);
    let f = fresnel_dielectric(clamp(dot(wo, h), 0.0, 1.0), eta);
    return vec3<f32>(f * (d * g / (4.0 * wo.z * wi.z)));
}

// The energy table's two indices around a value in `0..=1` and the weight between them.
struct Steps {
    low: u32,
    high: u32,
    between: f32,
};

fn steps_of(value: f32) -> Steps {
    let last = ENERGY_STEPS - 1u;
    let scaled = clamp(value, 0.0, 1.0) * f32(last);
    let index = u32(clamp(floor(scaled), 0.0, f32(last)));
    var result: Steps;
    result.low = index;
    result.high = min(index + 1u, last);
    result.between = scaled - floor(scaled);
    return result;
}

fn energy_cell(row: u32, column: u32) -> f32 {
    let index = min(row, ENERGY_STEPS - 1u) * ENERGY_STEPS + min(column, ENERGY_STEPS - 1u);
    return max(energy[index], 0.0);
}

fn energy_directional(cos: f32, roughness: f32) -> f32 {
    let rows = steps_of(roughness);
    let columns = steps_of(cos);
    let near = mix(energy_cell(rows.low, columns.low), energy_cell(rows.low, columns.high), columns.between);
    let far = mix(energy_cell(rows.high, columns.low), energy_cell(rows.high, columns.high), columns.between);
    return clamp(mix(near, far, rows.between), 0.0, 1.0);
}

fn energy_average(roughness: f32) -> f32 {
    let rows = steps_of(roughness);
    let base = ENERGY_STEPS * ENERGY_STEPS;
    let near = max(energy[min(base + rows.low, base + ENERGY_STEPS - 1u)], 0.0);
    let far = max(energy[min(base + rows.high, base + ENERGY_STEPS - 1u)], 0.0);
    return clamp(mix(near, far, rows.between), 0.0, 1.0);
}

fn luminance(colour: vec3<f32>) -> f32 {
    return 0.2126 * colour.x + 0.7152 * colour.y + 0.0722 * colour.z;
}

fn multiple_scatter_colour(f0: vec3<f32>, average_albedo: f32) -> vec3<f32> {
    let f_avg = (f0 * 20.0 + vec3<f32>(1.0)) / 21.0;
    var colour = vec3<f32>(0.0);
    for (var axis = 0u; axis < 3u; axis = axis + 1u) {
        let f = f_avg[axis];
        let denominator = 1.0 - f * (1.0 - average_albedo);
        if (abs(denominator) > 1.0e-6) {
            colour[axis] = f * f * average_albedo / denominator;
        }
    }
    return colour;
}

fn multiple_scatter_probability(f0: vec3<f32>, roughness: f32) -> f32 {
    let average = energy_average(roughness);
    let colour = multiple_scatter_colour(f0, average);
    let lost = luminance(colour) * max(1.0 - average, 0.0);
    let kept = luminance(f0) * average;
    let total = lost + kept;
    if (!(total > 0.0)) {
        return 0.0;
    }
    return clamp(lost / total, 0.0, 0.999);
}

fn multiple_scatter_eval(f0: vec3<f32>, cos_o: f32, cos_i: f32, roughness: f32) -> vec3<f32> {
    let average = energy_average(roughness);
    if (average >= 1.0 - 1.0e-6) {
        return vec3<f32>(0.0);
    }
    let lost = (1.0 - energy_directional(cos_o, roughness)) * (1.0 - energy_directional(cos_i, roughness));
    if (lost <= 0.0) {
        return vec3<f32>(0.0);
    }
    return multiple_scatter_colour(f0, average) * (lost / (PI * (1.0 - average)));
}

fn interface_ratio(material: Material, inside: bool) -> f32 {
    return select(material.ior, 1.0 / material.ior, inside);
}

fn probabilities_of(material: Material, wo: vec3<f32>, inside: bool) -> Probabilities {
    var probabilities: Probabilities;
    probabilities.diffuse = 0.0;
    probabilities.specular = 0.0;
    probabilities.completion = 0.0;
    probabilities.transmitted = 0.0;
    if (is_pane(material)) {
        let reflectance = fresnel_dielectric(wo.z, interface_ratio(material, inside));
        probabilities.specular = reflectance;
        probabilities.transmitted = 1.0 - reflectance;
        return probabilities;
    }
    let metal = material.metallic;
    var multiple = 0.0;
    if (metal > 0.0) {
        multiple = multiple_scatter_probability(material.albedo, material.roughness);
    }
    probabilities.diffuse = 1.0 - metal;
    probabilities.specular = metal * (1.0 - multiple);
    probabilities.completion = metal * multiple;
    return probabilities;
}

fn bsdf_eval(material: Material, wo: vec3<f32>, wi: vec3<f32>, inside: bool) -> vec3<f32> {
    if (wo.z <= 0.0 || wi.z <= 0.0) {
        return vec3<f32>(0.0);
    }
    let alpha = alpha_of(material);
    if (is_pane(material)) {
        return ggx_eval_dielectric(wo, wi, alpha, interface_ratio(material, inside));
    }
    let metal = material.metallic;
    var total = vec3<f32>(0.0);
    if (metal > 0.0) {
        total = total + ggx_eval(wo, wi, alpha, material.albedo) * metal;
        total = total + multiple_scatter_eval(material.albedo, wo.z, wi.z, material.roughness) * metal;
    }
    if (metal < 1.0) {
        total = total + material.albedo * ((1.0 - metal) / PI);
    }
    return total;
}

// The mixture density of the surface's continuous lobes at `(wo, wi)`, which is `bsdf::pdf`.
//
// A light sample is taken with this weight: the path it finds could also have been found by a sample
// of the surface, so the two strategies share it instead of counting it twice. A pane's transmitted
// beam is smooth and its density is not part of the sum, so `probabilities_of` gives it no cosine
// term and the formula is the one the Rust writes.
fn bsdf_pdf(material: Material, wo: vec3<f32>, wi: vec3<f32>, inside: bool) -> f32 {
    if (wo.z <= 0.0 || wi.z <= 0.0) {
        return 0.0;
    }
    let alpha = alpha_of(material);
    let probabilities = probabilities_of(material, wo, inside);
    let cosine = (probabilities.diffuse + probabilities.completion) * wi.z / PI;
    let density = probabilities.specular * ggx_pdf(wo, wi, alpha) + cosine;
    if (!(density > 0.0)) {
        return 0.0;
    }
    return density;
}

fn sample_cosine(u: vec2<f32>) -> vec3<f32> {
    let radius = sqrt(u.x);
    let phi = 2.0 * PI * u.y;
    return safe_normalize(vec3<f32>(radius * cos(phi), radius * sin(phi), sqrt(max(0.0, 1.0 - u.x))));
}

fn sample_bsdf(material: Material, wo: vec3<f32>, u: vec2<f32>, pick: f32, inside: bool) -> Scatter {
    var scatter: Scatter;
    scatter.valid = false;
    scatter.direction = vec3<f32>(0.0);
    scatter.weight = vec3<f32>(0.0);
    scatter.pdf = 0.0;
    scatter.transmitted = false;
    if (wo.z <= 0.0 || pick < 0.0 || pick >= 1.0) {
        return scatter;
    }
    let alpha = alpha_of(material);
    let probabilities = probabilities_of(material, wo, inside);
    if (is_pane(material)) {
        let ratio = interface_ratio(material, inside);
        if (pick < probabilities.specular) {
            let h = sample_visible_normal(wo, alpha, u);
            let dot_oh = dot(wo, h);
            if (dot_oh <= 0.0) {
                return scatter;
            }
            let wi = h * (2.0 * dot_oh) - wo;
            if (wi.z <= 0.0) {
                return scatter;
            }
            let reflectance = fresnel_dielectric(clamp(dot_oh, 0.0, 1.0), ratio);
            let keep = masking(wo.z, wi.z, alpha) * (1.0 + lambda_(wo.z, alpha));
            scatter.valid = true;
            scatter.direction = wi;
            scatter.weight = vec3<f32>(reflectance * keep / max(probabilities.specular, 1.0e-9));
            scatter.pdf = probabilities.specular * ggx_pdf(wo, wi, alpha);
            return scatter;
        }
        if (probabilities.transmitted <= 0.0) {
            return scatter;
        }
        let wi = refract(wo, ratio);
        scatter.valid = true;
        scatter.direction = wi;
        scatter.weight = vec3<f32>(material.transmission * ratio * ratio);
        scatter.pdf = max(probabilities.transmitted, 1.0e-9);
        scatter.transmitted = true;
        return scatter;
    }
    let metal = material.metallic;
    if (pick < probabilities.diffuse) {
        let wi = sample_cosine(u);
        scatter.valid = true;
        scatter.direction = wi;
        scatter.weight = material.albedo * (1.0 - metal);
        scatter.pdf = probabilities.diffuse * wi.z / PI;
        return scatter;
    }
    if (pick < probabilities.diffuse + probabilities.completion) {
        let wi = sample_cosine(u);
        let value = multiple_scatter_eval(material.albedo, wo.z, wi.z, material.roughness);
        scatter.valid = true;
        scatter.direction = wi;
        scatter.weight = value * (PI / max(probabilities.completion, 1.0e-9));
        scatter.pdf = probabilities.completion * wi.z / PI;
        return scatter;
    }
    let h = sample_visible_normal(wo, alpha, u);
    let dot_oh = dot(wo, h);
    if (dot_oh <= 0.0) {
        return scatter;
    }
    let wi = h * (2.0 * dot_oh) - wo;
    if (wi.z <= 0.0) {
        return scatter;
    }
    let fresnel = fresnel_schlick(clamp(dot_oh, 0.0, 1.0), material.albedo);
    let keep = masking(wo.z, wi.z, alpha) * (1.0 + lambda_(wo.z, alpha));
    scatter.valid = true;
    scatter.direction = wi;
    scatter.weight = fresnel * (keep * metal / max(probabilities.specular, 1.0e-9));
    scatter.pdf = probabilities.specular * ggx_pdf(wo, wi, alpha);
    return scatter;
}

// ---------------------------------------------------------------------------------------------------
// The lights: `light.rs`.
// ---------------------------------------------------------------------------------------------------

fn point_threshold(index: u32) -> f32 {
    return camera.point_probability[index / 4u][index % 4u];
}

fn point_alias(index: u32) -> u32 {
    return camera.point_alias[index / 4u][index % 4u];
}

fn point_share(index: u32) -> f32 {
    return camera.point_share[index / 4u][index % 4u];
}

// Vose's alias draw: one number picks the column, the other decides whether it answers for itself.
fn pick_point_light(u: vec2<f32>) -> u32 {
    let count = camera.counts.w;
    if (count == 0u) {
        return NO_LIGHT;
    }
    let scaled = clamp(u.x, 0.0, 1.0) * f32(count);
    let column = min(u32(scaled), count - 1u);
    if ((scaled - f32(column)) < point_threshold(column)) {
        return column;
    }
    return point_alias(column);
}

// The power heuristic of Veach: the weight a sample is taken with when a second strategy could have
// found the same path. A point light's direction is exact, so the BSDF's own density there is zero and
// the light sample keeps the whole contribution; an area emitter would fill both terms.
fn balance(first: f32, second: f32) -> f32 {
    let a = max(first, 0.0);
    let b = max(second, 0.0);
    let squares = a * a + b * b;
    if (!(squares > 0.0)) {
        return 0.0;
    }
    return clamp(a * a / squares, 0.0, 1.0);
}

// ---------------------------------------------------------------------------------------------------
// The rectangular lights: a window is a face, and a face is sampled by solid angle.
// ---------------------------------------------------------------------------------------------------

fn area_threshold(index: u32) -> f32 {
    return camera.area_probability[index % 4u];
}

fn area_alias_of(index: u32) -> u32 {
    return camera.area_alias[index % 4u];
}

fn area_share_of(index: u32) -> f32 {
    return camera.area_share[index % 4u];
}

// The same alias draw the point lights use, over the rectangles' emitted power.
fn pick_area_light(u: vec2<f32>) -> u32 {
    let count = camera.area_count.x;
    if (count == 0u) {
        return NO_LIGHT;
    }
    let scaled = clamp(u.x, 0.0, 1.0) * f32(count);
    let column = min(u32(scaled), count - 1u);
    if ((scaled - f32(column)) < area_threshold(column)) {
        return column;
    }
    return area_alias_of(column);
}

// The unit normal of a rectangle's face, which points the way it emits.
//
// The ring the frame builds runs counter-clockwise seen from the emitting side, so its normal is the
// cross of the *height* edge into the width edge — the order `AreaLight::corners` writes (L2). The
// other order would make every window emit into the wall behind it.
fn area_normal(index: u32) -> vec3<f32> {
    return safe_normalize(cross(camera.area_half_height[index].xyz, camera.area_half_width[index].xyz));
}

// One point of the face, uniform over its area: `u` and `v` in `[0, 1)` are the two halves.
fn area_point(index: u32, u: vec2<f32>) -> vec3<f32> {
    let across = camera.area_half_width[index].xyz * (2.0 * u.x - 1.0);
    let down = camera.area_half_height[index].xyz * (2.0 * u.y - 1.0);
    return camera.area_origin[index].xyz + across + down;
}

// The density of one point of a face being drawn: uniform over the area, converted from square
// millimetres to a solid angle at `seen_from`, divided by the cosine the face presents there.
//
// It is the density of the *face*, not of the strategy: a caller multiplies it by the share the alias
// table gave the light, so both strategies price one path the same way (L2).
fn area_pdf(index: u32, point: vec3<f32>, seen_from: vec3<f32>) -> f32 {
    let area = camera.area_radiance[index].a;
    if (!(area > 0.0)) {
        return 0.0;
    }
    let to_light = point - seen_from;
    let distance_squared = dot(to_light, to_light);
    if (!(distance_squared > 0.0)) {
        return 0.0;
    }
    let direction = to_light * inverseSqrt(distance_squared);
    let cosine = abs(dot(area_normal(index), direction));
    if (!(cosine > 1.0e-6)) {
        return 0.0;
    }
    return distance_squared / (area * cosine);
}

// The distance to the face of rectangle `index` along `ray`, or -1 for no hit or a point outside it.
fn intersect_area(ray: Ray, index: u32, t_max: f32) -> f32 {
    let origin = camera.area_origin[index].xyz;
    let normal = area_normal(index);
    let denominator = dot(normal, ray.direction);
    if (abs(denominator) <= 1.0e-9) {
        return -1.0;
    }
    let t = dot(origin - ray.origin, normal) / denominator;
    if (t <= T_MIN || t >= t_max) {
        return -1.0;
    }
    let point = ray.origin + ray.direction * t;
    let local = point - origin;
    let across = camera.area_half_width[index].xyz;
    let down = camera.area_half_height[index].xyz;
    let a = dot(local, across) / max(dot(across, across), 1.0e-9);
    let b = dot(local, down) / max(dot(down, down), 1.0e-9);
    if (abs(a) > 1.0 || abs(b) > 1.0) {
        return -1.0;
    }
    return t;
}

// The nearest face of any rectangle along the ray, or -1 for none. The index is the one the alias
// table and the radiance array share.
fn trace_area(ray: Ray, t_max: f32) -> i32 {
    var best = t_max;
    var found = -1;
    let count = min(camera.area_count.x, MAX_AREA_LIGHTS);
    for (var index = 0u; index < count; index = index + 1u) {
        let t = intersect_area(ray, index, best);
        if (t > 0.0) {
            best = t;
            found = i32(index);
        }
    }
    return found;
}

// The direct light one vertex receives from the key light, one point light drawn by emitted power,
// and one rectangle drawn by emitted power and sampled by solid angle.
fn direct_light(
    material: Material,
    frame: Frame,
    wo: vec3<f32>,
    position: vec3<f32>,
    u: vec2<f32>,
    pick: vec2<f32>,
    across_down: vec2<f32>,
    inside: bool,
) -> vec3<f32> {
    var radiance = vec3<f32>(0.0);
    // The sun: a delta emitter, traced at every vertex, with no share to divide by.
    if (camera.key.w > 0.5) {
        let local = to_local(frame, camera.key_direction.xyz);
        if (local.z > 0.0) {
            let value = bsdf_eval(material, wo, local, inside);
            let origin = position + frame.normal * SURFACE_BIAS;
            if (!occluded(origin, camera.key_direction.xyz, 1.0e7)) {
                radiance = radiance + value * local.z * camera.key.xyz;
            }
        }
    }
    // One point light, drawn by emitted power. Every way out of this block is a *continue*, not a
    // return: the rectangles below are a second strategy and must run whatever the draw landed on.
    let picked = pick_point_light(u);
    if (picked != NO_LIGHT && picked < camera.counts.w) {
        let share = point_share(picked);
        let to_light = camera.point_position[picked].xyz - position;
        let distance_squared = dot(to_light, to_light);
        if (share > 0.0 && distance_squared > 0.0) {
            let distance = sqrt(distance_squared);
            let direction = to_light / distance;
            let local = to_local(frame, direction);
            let origin = position + frame.normal * SURFACE_BIAS;
            if (local.z > 0.0 && !occluded(origin, direction, distance - SURFACE_BIAS)) {
                let value = bsdf_eval(material, wo, local, inside);
                // A point light's radiant power is watts: the irradiance it gives a surface is
                // `power / (4 PI d^2)`, and it is never cut off at a range (ADR 0039).
                let irradiance = camera.point_colour[picked].a / (4.0 * PI * distance_squared);
                let mis = balance(1.0 / share, 0.0);
                radiance = radiance + value * local.z * camera.point_colour[picked].rgb * irradiance * (mis / share);
            }
        }
    }
    return radiance + area_light(material, frame, wo, position, pick, across_down, inside);
}

// The light one rectangular emitter gives a vertex: a point on its face, the shadow ray to it, and
// the two strategies' weights. The face is drawn by solid angle, so `pdf` is a per-steradian density
// and the surface's own density for the same direction competes with it (L2).
fn area_light(
    material: Material,
    frame: Frame,
    wo: vec3<f32>,
    position: vec3<f32>,
    pick: vec2<f32>,
    across_down: vec2<f32>,
    inside: bool,
) -> vec3<f32> {
    let picked = pick_area_light(pick);
    if (picked == NO_LIGHT || picked >= min(camera.area_count.x, MAX_AREA_LIGHTS)) {
        return vec3<f32>(0.0);
    }
    let share = area_share_of(picked);
    if (!(share > 0.0)) {
        return vec3<f32>(0.0);
    }
    let point = area_point(picked, across_down);
    let to_light = point - position;
    let distance_squared = dot(to_light, to_light);
    if (!(distance_squared > 0.0)) {
        return vec3<f32>(0.0);
    }
    let distance = sqrt(distance_squared);
    let direction = to_light / distance;
    // The face emits from one side: a vertex behind it sees nothing.
    if (dot(area_normal(picked), direction) >= 0.0) {
        return vec3<f32>(0.0);
    }
    let local = to_local(frame, direction);
    if (local.z <= 0.0) {
        return vec3<f32>(0.0);
    }
    let density = area_pdf(picked, point, position);
    if (!(density > 0.0)) {
        return vec3<f32>(0.0);
    }
    let origin = position + frame.normal * SURFACE_BIAS;
    if (occluded(origin, direction, distance - SURFACE_BIAS)) {
        return vec3<f32>(0.0);
    }
    let value = bsdf_eval(material, wo, local, inside);
    let pdf = share * density;
    let mis = balance(pdf, bsdf_pdf(material, wo, local, inside));
    return value * local.z * camera.area_radiance[picked].rgb * (mis / pdf);
}

// ---------------------------------------------------------------------------------------------------
// The integrator.
// ---------------------------------------------------------------------------------------------------

// The camera ray of one sample, built the way `view::camera_ray` builds it: the sample is the pixel's
// centre plus a jitter inside the pixel, the normalised coordinate grows upwards while the row index
// grows downwards, and the projection is the viewport's own.
fn camera_ray(sample_position: vec2<f32>) -> Ray {
    let width = camera.view.z;
    let height = camera.view.w;
    let x = clamp(sample_position.x / width * 2.0 - 1.0, -16.0, 16.0);
    let y = clamp(1.0 - sample_position.y / height * 2.0, -16.0, 16.0);
    let half_height = camera.view.x;
    let aspect = camera.view.y;
    if (camera.eye.w > 0.5) {
        let half_width = half_height * aspect;
        let origin = camera.eye.xyz + camera.right.xyz * (x * half_width) + camera.up.xyz * (y * half_height);
        return make_ray(origin, camera.forward.xyz);
    }
    let direction =
        camera.forward.xyz + camera.right.xyz * (x * half_height * aspect) + camera.up.xyz * (y * half_height);
    return make_ray(camera.eye.xyz, direction);
}

// The variance of the mean of `count` samples, from their sum and their sum of squares, exactly as
// `crate::denoise::variance_of_mean` writes it: the unbiased sample variance divided by the count
// again. The two implementations are one formula, and the test that renders a shape whose variance is
// known is what keeps them one.
fn variance_of_mean(sum: vec3<f32>, squares: vec3<f32>, count: f32) -> vec3<f32> {
    if (!(count > 1.0)) {
        return vec3<f32>(0.0);
    }
    let denominator = count * (count - 1.0);
    return max(vec3<f32>(0.0), (squares - sum * sum / count) / denominator);
}

// The luminance of a colour, Rec. 709's weights, as `crate::denoise::luminance` writes them.
fn luminance_of(colour: vec3<f32>) -> f32 {
    return dot(colour, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Whether a pixel has taken enough samples and has a small enough error to stop being traced:
// `crate::denoise::retires`, whose threshold arrives in `camera.denoise`.
fn retires(pixel: u32) -> bool {
    let count = pixels[pixel].sum.w;
    if (count < camera.denoise.x) {
        return false;
    }
    let mean = pixels[pixel].sum.xyz / count;
    let variance = variance_of_mean(pixels[pixel].sum.xyz, pixels[pixel].squares.xyz, count);
    let light = max(luminance_of(mean), camera.denoise.z);
    let width = sqrt(max(luminance_of(variance), 0.0));
    return width / light < camera.denoise.y;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = u32(camera.view.z);
    if (id.x >= width || id.y >= camera.strip.y) {
        return;
    }
    let pixel = id.y * width + id.x;
    if (pixel >= arrayLength(&pixels)) {
        return;
    }
    // A converged pixel costs nothing from here on: the sample it would take cannot move its mean
    // further than the image tolerates, and the count it stops at is the divisor the host averages by.
    if (retires(pixel)) {
        return;
    }
    let sample = camera.counts.z;
    let pixel_seed = hash(pixel * 0x9e3779b9u ^ (sample + 1u) * 0x85ebca6bu);
    let jitter = sample_pair(0u, sample, pixel_seed) - vec2<f32>(0.5);
    // The row is the image's own row, not the strip's: the camera ray must not know about strips.
    let row = f32(camera.strip.x + id.y);
    var ray = camera_ray(vec2<f32>(f32(id.x), row) + vec2<f32>(0.5) + jitter);

    var throughput = vec3<f32>(1.0);
    var radiance = vec3<f32>(0.0);
    // The medium the ray travels in, as its extinction per millimetre: air outside a pane, the
    // material's own tint inside it.
    var extinction = vec3<f32>(0.0);
    var inside = false;
    var bounce = 0u;
    // The density of the surface sample that produced the current ray, which is what a rectangle the
    // ray walks into is weighted against: the path could have been found by a light sample too (L2).
    var previous_pdf = 0.0;
    loop {
        let hit = trace(ray, FAR_AWAY);
        let face_of_light = trace_area(ray, select(FAR_AWAY, hit.t, hit.t > 0.0));
        if (face_of_light >= 0) {
            // A rectangle is opaque and emits from one side: a ray that walks into its back is black,
            // and one that walks into its front ends the path with the face's radiance.
            let index = u32(face_of_light);
            let distance = intersect_area(ray, index, FAR_AWAY);
            let point = ray.origin + ray.direction * distance;
            let normal = area_normal(index);
            let emitting = dot(normal, ray.direction) < 0.0;
            if (sample == 0u && bounce == 0u) {
                // The guide faces the ray, exactly as a body's hit normal does, so the filter compares
                // two normals written the same way.
                pixels[pixel].albedo = vec4<f32>(camera.area_radiance[index].rgb, distance);
                pixels[pixel].normal = vec4<f32>(select(-normal, normal, emitting), 0.0);
            }
            if (emitting) {
                // What a light sample would have paid for this path: the face's own density times the
                // share the alias table gave it, so the two strategies share one path exactly.
                let light_pdf = area_share_of(index) * area_pdf(index, point, ray.origin);
                let weight = select(1.0, balance(previous_pdf, light_pdf), bounce > 0u);
                radiance = radiance + throughput * camera.area_radiance[index].rgb * weight;
            }
            break;
        }
        if (hit.t <= 0.0) {
            // Nothing there: what the ray sees is the environment, which is the studio the workshop
            // does not build.
            radiance = radiance + throughput * camera.environment.xyz;
            if (sample == 0u && bounce == 0u) {
                pixels[pixel].albedo = vec4<f32>(camera.environment.xyz, FAR_AWAY);
                pixels[pixel].normal = vec4<f32>(-ray.direction, 0.0);
            }
            break;
        }
        // Beer and Lambert: the beam lost this much on the way to the surface it just met.
        throughput = throughput * exp(-extinction * hit.t);
        let face = faces[hit.face];
        let position = ray.origin + ray.direction * hit.t;
        let material = material_of(face, position);
        let frame = frame_of(hit.normal);
        let wo = to_local(frame, -ray.direction);
        if (wo.z <= 0.0) {
            break;
        }
        let bounce_seed = hash(pixel_seed ^ hash(bounce * 2u + 1u));
        // The guide is written on the first sample only: the primary hit does not depend on where
        // inside the pixel that sample landed, so a second write would only repeat it.
        if (sample == 0u && bounce == 0u) {
            pixels[pixel].albedo = vec4<f32>(material.albedo, hit.t);
            pixels[pixel].normal = vec4<f32>(frame.normal, 0.0);
        }
        let u = sample_pair(0u, sample, bounce_seed);
        let pick = sample_1d(2u, sample, bounce_seed);
        // The rectangle's two draws: which window, and where on its face. They are their own pair of
        // dimensions so the light sample is not correlated with the surface's own (L2).
        let area_pick = sample_pair(2u, sample, bounce_seed);
        let area_point_uv = sample_pair(3u, sample, bounce_seed);
        radiance = radiance + throughput * direct_light(material, frame, wo, position, u, area_pick, area_point_uv, inside);
        let scatter = sample_bsdf(material, wo, u, pick, inside);
        if (!scatter.valid) {
            break;
        }
        previous_pdf = scatter.pdf;
        throughput = throughput * scatter.weight;
        if (scatter.transmitted) {
            inside = !inside;
            extinction = select(vec3<f32>(0.0), material.absorption, inside);
        }
        let direction = to_world(frame, scatter.direction);
        ray = make_ray(position + direction * SURFACE_BIAS, direction);
        bounce = bounce + 1u;
        if (bounce >= MAX_BOUNCES) {
            break;
        }
        if (bounce > RR_DEPTH) {
            // Russian roulette: a path is continued with the probability of the light it still
            // carries, and divided by it, so a long path costs its own share of the samples.
            let keep = clamp(max(throughput.r, max(throughput.g, throughput.b)), 0.05, 1.0);
            let roulette = sample_1d(3u, sample, bounce_seed);
            if (roulette >= keep) {
                break;
            }
            throughput = throughput / keep;
        }
    }
    // One clamp, documented rather than felt: a pane can concentrate a light into a number an `f32`
    // accumulator has no room for. Its bias is measured against an unclamped reference in the note.
    radiance = clamp(radiance, vec3<f32>(0.0), vec3<f32>(MAX_SAMPLE_RADIANCE));
    pixels[pixel].sum = pixels[pixel].sum + vec4<f32>(radiance, 1.0);
    pixels[pixel].squares = pixels[pixel].squares + vec4<f32>(radiance * radiance, 0.0);
}
