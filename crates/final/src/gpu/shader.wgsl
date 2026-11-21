// The final render's integrator: one primary ray per sample, direct light only, diffuse only.
//
// The shape of this file follows the Rust that owns it: `gpu/scene.rs` declares the same three
// structs and the same uniform, `ray.rs` carries the intersection test and `bvh.rs` the traversal,
// and this shader is their transcription for the device. A change to one without the other is a
// picture that is wrong in a way no test can name, so the maths here is deliberately the same
// expression the Rust writes, and `crate::view` compares the two cameras at a known point.
//
// The estimator is the direct light of a Lambertian surface, which is the whole of Y1:
//
//     L(x) = albedo / PI * sum over lights of  V(x, l) * E(x, l) * cos(theta_l)
//
// where a point light's irradiance is `power / (4 * PI * d^2)` (watts, inverse square, ADR 0039) and
// the key light's colour already *is* the irradiance on a surface facing it. One light is chosen
// uniformly per sample and weighted by the count, which is uniform sampling of a discrete set and
// therefore unbiased; next-event estimation over an area, multiple importance sampling and every
// other lobe are Y2's work. No sample bounces, so a shadow here is black rather than softly lit by
// the wall beside it.

const PI: f32 = 3.141592653589793;
const MAX_POINT_LIGHTS: u32 = 8u;
// The stack the traversal may occupy: one slot per tree level plus the root, with room to spare.
// `bvh::MAX_DEPTH` bounds the tree, and this bound is what keeps a hostile node graph from reading
// past the array.
const MAX_STACK: u32 = 64u;
// How far a ray must travel before it may hit anything: the camera's near plane, in millimetres.
const T_MIN: f32 = 0.05;
// How far off a surface a secondary ray starts, so it does not meet the face it left.
const SHADOW_BIAS: f32 = 0.1;
// The workgroup is square, so a pixel row is one row of invocations.
const TILE: u32 = 8u;
// The most one sample may contribute, which is where an `f32` division is stopped before it overflows.
const MAX_SAMPLE_RADIANCE: f32 = 1.0e6;

struct Face {
    a: vec4<f32>,
    b: vec4<f32>,
    c: vec4<f32>,
    colour: vec4<f32>,
};

struct Node {
    min: vec4<f32>,
    max: vec4<f32>,
    // x: a leaf's first face or an interior node's left child; y: a leaf's face count.
    info: vec4<u32>,
};

struct Uniform {
    eye: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    forward: vec4<f32>,
    // x: the tangent of half the vertical field of view, or half the orthographic height;
    // y: the aspect ratio; z and w: the image's width and height.
    view: vec4<f32>,
    // x: the face count; y: the node count; z: the sample index; w: the light count.
    counts: vec4<u32>,
    // x: the first image row of this strip; y: its row count. A print is traced strip by strip,
    // because one accumulator buffer is bounded and one image is not.
    strip: vec4<u32>,
    // xyz: the key light's irradiance; w: one when it exists.
    key: vec4<f32>,
    key_direction: vec4<f32>,
    point_position: array<vec4<f32>, 8>,
    // rgb: linear colour; a: radiant power in watts.
    point_colour: array<vec4<f32>, 8>,
};

@group(0) @binding(0) var<uniform> camera: Uniform;
@group(0) @binding(1) var<storage, read> faces: array<Face>;
@group(0) @binding(2) var<storage, read> nodes: array<Node>;
// The accumulator: one `vec4` per pixel, summed over samples and divided by the count in `crate::image`.
@group(1) @binding(0) var<storage, read_write> accum: array<vec4<f32>>;

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

fn safe_normalize(value: vec3<f32>) -> vec3<f32> {
    let length = sqrt(dot(value, value));
    return select(vec3<f32>(0.0), value / length, length > 1.0e-9);
}

// A ray from `origin` towards `direction`; the direction is unit length, so `t` is a distance in
// millimetres and every comparison below is a comparison of distances (`crate::ray`).
fn make_ray(origin: vec3<f32>, direction: vec3<f32>) -> Ray {
    let unit = safe_normalize(direction);
    var ray: Ray;
    ray.origin = origin;
    ray.direction = unit;
    ray.inverse = vec3<f32>(1.0) / unit;
    return ray;
}

// The distance at which the ray enters the box, or a negative number when it misses. The slabs are
// inclusive because a point on a face is on the box (`Ray::bounds_entry`).
fn bounds_entry(ray: Ray, low: vec3<f32>, high: vec3<f32>, t_max: f32) -> f32 {
    var entry = T_MIN;
    var exit = t_max;
    for (var axis = 0u; axis < 3u; axis = axis + 1u) {
        if (ray.direction[axis] == 0.0) {
            // A direction exactly along another axis: the slab has no thickness to cross, so the
            // origin decides it, and `0 * inf` never reaches a comparison. The test is on the
            // direction rather than on its reciprocal, because WGSL has no infinite literal.
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

// Moeller and Trumbore (1997), as `Triangle::intersect` writes it, for one face of the tree.
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

// The nearest face the ray meets, or a negative `t`. Children are entered nearest first and a hit
// shortens the far end of every later box test, which is `Bvh::intersect` and changes no answer.
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

// Whether anything stands between `origin` and `distance` along `direction`.
fn occluded(origin: vec3<f32>, direction: vec3<f32>, distance: f32) -> bool {
    return trace(make_ray(origin, direction), distance).t > 0.0;
}

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

// A uniform number in `[0, 1)` from a hash.
fn unit(index: u32) -> f32 {
    return f32(hash(index) >> 8u) / 16777216.0;
}

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

// The direct light a diffuse surface at `position` with `normal` and `albedo` receives, from one
// uniformly chosen light, weighted by the count so the estimator stays unbiased.
fn direct_light(position: vec3<f32>, normal: vec3<f32>, albedo: vec3<f32>, seed: u32) -> vec3<f32> {
    let count = camera.counts.w;
    if (count == 0u) {
        return vec3<f32>(0.0);
    }
    let pick = min(u32(unit(seed) * f32(count)), count - 1u);
    let weight = f32(count);
    if (pick == 0u) {
        let n_dot_l = dot(normal, camera.key_direction.xyz);
        if (n_dot_l <= 0.0) {
            return vec3<f32>(0.0);
        }
        if (occluded(position + normal * SHADOW_BIAS, camera.key_direction.xyz, 1.0e7)) {
            return vec3<f32>(0.0);
        }
        return albedo * camera.key.xyz * n_dot_l * (weight / PI);
    }
    let index = pick - 1u;
    let to_light = camera.point_position[index].xyz - position;
    let distance_squared = dot(to_light, to_light);
    if (distance_squared <= 0.0) {
        return vec3<f32>(0.0);
    }
    let distance = sqrt(distance_squared);
    let direction = to_light / distance;
    let n_dot_l = dot(normal, direction);
    if (n_dot_l <= 0.0) {
        return vec3<f32>(0.0);
    }
    if (occluded(position + normal * SHADOW_BIAS, direction, distance - SHADOW_BIAS)) {
        return vec3<f32>(0.0);
    }
    // A point light's radiant power is watts: the irradiance it gives a surface is
    // `power / (4 * PI * d^2)` and never a hard cutoff at a range (ADR 0039).
    let irradiance = camera.point_colour[index].a / (4.0 * PI * distance_squared);
    return albedo * camera.point_colour[index].rgb * irradiance * n_dot_l * (weight / PI);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = u32(camera.view.z);
    if (id.x >= width || id.y >= camera.strip.y) {
        return;
    }
    let pixel = id.y * width + id.x;
    if (pixel >= arrayLength(&accum)) {
        return;
    }
    let sample = camera.counts.z;
    let seed = hash(pixel * 0x9e3779b9u ^ (sample + 1u) * 0x85ebca6bu);
    let jitter = vec2<f32>(unit(seed), unit(seed ^ 0x68bc21ebu)) - vec2<f32>(0.5);
    // The row is the image's own row, not the strip's: the camera ray must not know about strips.
    let row = f32(camera.strip.x + id.y);
    let ray = camera_ray(vec2<f32>(f32(id.x), row) + vec2<f32>(0.5) + jitter);
    let hit = trace(ray, 1.0e7);
    if (hit.t <= 0.0) {
        // Nothing there: the frame's background is black until the catalogue task gives it a studio.
        return;
    }
    let face = faces[hit.face];
    let position = ray.origin + ray.direction * hit.t;
    // One clamp, documented rather than felt: a light is a finite number of watts and a surface is
    // finite, so a sample can only leave the range where an `f32` division overflows. Clamping there
    // keeps a hostile sample out of the accumulator its neighbours share, and it costs nothing in an
    // image that is tone-mapped a thousand times darker than the clamp.
    let radiance = clamp(direct_light(position, hit.normal, face.colour.rgb, seed ^ 0x1b873593u), vec3<f32>(0.0), vec3<f32>(MAX_SAMPLE_RADIANCE));
    accum[pixel] = accum[pixel] + vec4<f32>(radiance, 1.0);
}
