//! The device's view of a frame: flat faces, flat nodes and one uniform.
//!
//! The BVH and the frame's lights are ours in `f64` and `DVec3`; a shader has neither. This module is
//! the one place a final render's numbers become `f32` and a fixed layout, so the CPU side and the
//! WGSL side are two readings of one struct rather than two guesses: every struct here is `#[repr(C)]`
//! with only `vec4`-shaped members, which is why the WGSL declarations at the top of `shader.wgsl`
//! can name the same offsets without a padding attribute anywhere.
//!
//! Refuses: a tree or a frame larger than a `u32` index can name (past [`MAX_TRIANGLES`], which the
//! soup already caps), and a camera or a light whose numbers are not finite — a NaN vertex in a
//! uniform is a whole image of noise, so a hostile value stops the render instead.
//!
//! Changes when: a field of the shader's input changes, or the tree's layout does.

use geom::DVec3;
use scene::{AreaLight, Camera, Lighting, MAX_AREA_LIGHTS, MAX_POINT_LIGHTS, Projection};

use crate::bsdf;
use crate::bvh::Bvh;
use crate::soup::MAX_TRIANGLES;
use crate::{FinalError, Request};

/// The most `f32` coordinates a triangle takes: three vertices, each a `vec4`.
const VERTEX_SLOTS: usize = 3;

/// How many `vec4`s the alias table of [`MAX_POINT_LIGHTS`] columns occupies.
const ALIAS_WORDS: usize = MAX_POINT_LIGHTS.div_ceil(4);

/// The rectangular lights one frame carries, which is the scene's own cap.
const MAX_AREAS: usize = MAX_AREA_LIGHTS;

/// How many `vec4`s the alias table of [`MAX_AREAS`] columns occupies.
const AREA_ALIAS_WORDS: usize = MAX_AREAS.div_ceil(4);

/// One face as the shader reads it: three vertices and the surface's colour.
///
/// The fourth channel of every vector is unused and zero. It is there because a `vec3` in a storage
/// array is padded to sixteen bytes anyway, and a named slot says so instead of leaving the reader to
/// count.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuFace {
    /// The first vertex and the padding.
    pub(crate) a: [f32; 4],
    /// The second vertex and the padding.
    pub(crate) b: [f32; 4],
    /// The third vertex and the padding.
    pub(crate) c: [f32; 4],
    /// Linear RGB albedo and the padding.
    pub(crate) colour: [f32; 4],
    /// The finish: `x` roughness, `y` metallic, `z` the index of refraction, `w` transmission.
    pub(crate) finish: [f32; 4],
    /// The tint: Beer-Lambert extinction per millimetre, linear RGB, and the padding.
    pub(crate) tint: [f32; 4],
    /// The decor's coordinate at the first two corners: `xy` the first, `zw` the second.
    pub(crate) uv_a: [f32; 4],
    /// The third corner's coordinate in `xy`; `z` and `w` are unused.
    pub(crate) uv_b: [f32; 4],
    /// `x` is the picture's index plus one, or zero for a surface with no decor.
    pub(crate) decor: [u32; 4],
}

/// One picture as the shader reads it: where its texels start, and how many there are.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuDecor {
    /// `x` the first texel's index in the texel array, `y` the width, `z` the height.
    pub(crate) info: [u32; 4],
}

/// The pictures the frame names: their texels, packed one per word, and the table that finds them.
pub(crate) struct DecorTable {
    /// Every picture's texels, eight-bit sRGB RGBA packed little-endian into one `u32`.
    pub(crate) texels: Vec<u32>,
    /// One entry per picture, in the order the faces name them.
    pub(crate) entries: Vec<GpuDecor>,
}

/// The frame's decors as the shader reads them: one run of texels per picture, and its entry.
///
/// The pictures are a storage buffer rather than a sampled texture for the reason the Sobol
/// directions are: the shader does its own filtering — a board is longer than the sample scanned
/// from it, so the coordinate repeats and repeats need a wrap, which a `fract` does without a
/// sampler, a bind group or an sRGB view format to keep in step with the viewport's.
pub(crate) fn decors(decors: &[scene::Decor]) -> DecorTable {
    let mut texels = Vec::new();
    let mut entries = Vec::with_capacity(decors.len());
    for decor in decors {
        let image = decor.image();
        let first = texels.len() as u32;
        for texel in image.texels() {
            texels.push(
                u32::from(texel[0])
                    | (u32::from(texel[1]) << 8)
                    | (u32::from(texel[2]) << 16)
                    | (u32::from(texel[3]) << 24),
            );
        }
        entries.push(GpuDecor { info: [first, image.width(), image.height(), 0] });
    }
    if texels.is_empty() {
        // An empty storage buffer cannot be created, and a frame of plain paint is the case a
        // render must not refuse: one black texel answers every coordinate it never asks for.
        texels.push(0);
        entries.push(GpuDecor { info: [0, 1, 1, 0] });
    }
    DecorTable { texels, entries }
}

/// One pixel of a strip as the shader writes it: its samples' sums, and what its primary ray met.
///
/// Four `vec4`s and no more, in one buffer and one binding: the accumulator and its second moment,
/// which are what the mean and the variance come from, and the guide, which is what the filter tells
/// two surfaces apart by. They travel together because one sample writes all four and one filter
/// reads all four — and because a compute shader has eight storage bindings to spend, and the frame's
/// faces, nodes, two tables and two decor arrays already take six of them.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuPixel {
    /// `xyz` the sum of the samples' radiance, `w` how many samples there are.
    pub(crate) sum: [f32; 4],
    /// `xyz` the sum of the squares of those samples, `w` unused.
    pub(crate) squares: [f32; 4],
    /// `xyz` the linear albedo of the surface the primary ray met, or the studio where it met none;
    /// `w` how far that ray travelled in millimetres.
    pub(crate) albedo: [f32; 4],
    /// `xyz` the shading normal that faced the ray, or the ray's own direction where it met nothing.
    pub(crate) normal: [f32; 4],
}

impl GpuPixel {
    /// What the primary ray met, as the filter reads it: `crate::denoise::Guide`.
    pub(crate) fn guide(&self) -> crate::denoise::Guide {
        crate::denoise::Guide {
            albedo: [self.albedo[0], self.albedo[1], self.albedo[2]],
            normal: [self.normal[0], self.normal[1], self.normal[2]],
            distance: self.albedo[3],
        }
    }

    /// The pixel as the filter reads it: its mean, the variance of that mean and its sample count.
    pub(crate) fn sample(&self) -> crate::denoise::Sample {
        let count = self.sum[3];
        // A pixel that never took a sample cannot divide; it is black rather than shifted.
        let divisor = if count > 0.0 { count } else { 1.0 };
        crate::denoise::Sample {
            mean: [self.sum[0] / divisor, self.sum[1] / divisor, self.sum[2] / divisor],
            variance: crate::denoise::variance_of_mean(
                [self.sum[0], self.sum[1], self.sum[2]],
                [self.squares[0], self.squares[1], self.squares[2]],
                count,
            ),
            count: u32::try_from(count.max(0.0) as u64).unwrap_or(u32::MAX),
        }
    }
}

/// One node as the shader reads it: a box and what it means.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuNode {
    /// The minimum corner and the padding.
    pub(crate) min: [f32; 4],
    /// The maximum corner and the padding.
    pub(crate) max: [f32; 4],
    /// `x` is a leaf's first face or an interior node's left child, `y` a leaf's face count, and the
    /// rest is unused.
    pub(crate) info: [u32; 4],
}

/// Everything the shader reads that is not a face or a node: the camera, the lights and the count.
///
/// The camera's basis travels as three unit vectors and a projection kind rather than as a matrix,
/// because the shader builds the ray exactly the way [`scene::Camera::ray_ndc`] does — one formula
/// written twice, in two languages, with a test that compares the two answers (`crate::view`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Uniform {
    /// `xyz` the eye, `w` one for an orthographic view and zero for a perspective one.
    pub(crate) eye: [f32; 4],
    /// `xyz` the view's right vector.
    pub(crate) right: [f32; 4],
    /// `xyz` the view's up vector.
    pub(crate) up: [f32; 4],
    /// `xyz` the view's forward vector.
    pub(crate) forward: [f32; 4],
    /// `x` the half height of the view at unit depth (the tangent of half the vertical field of view,
    /// or half the orthographic height), `y` the aspect ratio, `z` the image width, `w` its height.
    pub(crate) view: [f32; 4],
    /// `x` the face count, `y` the node count, `z` the index of the sample being taken, `w` the light
    /// count including the key light.
    pub(crate) counts: [u32; 4],
    /// `x` the first image row of the strip this dispatch traces, `y` its row count. An image larger
    /// than one accumulator buffer is traced strip by strip (`gpu::MAX_STRIP_PIXELS`), and a strip is
    /// traced wider than the rows it owns so the filter's widest tap has a neighbour.
    pub(crate) strip: [u32; 4],
    /// `x` the fewest samples a pixel takes before it may retire, `y` the relative error a finished
    /// image is held to, `z` the dimmest light a relative error is measured against. All three are
    /// `crate::denoise`'s own numbers, so the shader's retirement test and the filter's blend are one
    /// threshold with one source.
    pub(crate) denoise: [f32; 4],
    /// `xyz` the key light's colour as irradiance on a surface facing it, `w` one when it exists.
    pub(crate) key: [f32; 4],
    /// `xyz` the unit direction from a surface towards the key light.
    pub(crate) key_direction: [f32; 4],
    /// The point lights' world positions, `w` unused.
    pub(crate) point_position: [[f32; 4]; MAX_POINT_LIGHTS],
    /// The point lights' linear colour, `w` their radiant power in watts.
    pub(crate) point_colour: [[f32; 4]; MAX_POINT_LIGHTS],
    /// `xyz` the uniform environment's radiance, `w` unused.
    pub(crate) environment: [f32; 4],
    /// The alias table's own probabilities, one column per point light, four to a vector.
    pub(crate) point_probability: [[f32; 4]; ALIAS_WORDS],
    /// The alias table's other column per point light, the same four to a vector.
    pub(crate) point_alias: [[u32; 4]; ALIAS_WORDS],
    /// The share of the light each point light carries, which is the density of a draw.
    pub(crate) point_share: [[f32; 4]; ALIAS_WORDS],
    /// `xyz` the centre of each rectangular light's face, `w` unused.
    pub(crate) area_origin: [[f32; 4]; MAX_AREAS],
    /// `xyz` the half-edge along the light's own `X` in world space, `w` unused.
    pub(crate) area_half_width: [[f32; 4]; MAX_AREAS],
    /// `xyz` the half-edge along the light's own `Y` in world space, `w` unused.
    pub(crate) area_half_height: [[f32; 4]; MAX_AREAS],
    /// `xyz` the radiance the face emits, `w` its area in square millimetres: the two the sample's
    /// pdf divides by and multiplies with (L2).
    pub(crate) area_radiance: [[f32; 4]; MAX_AREAS],
    /// `x` the number of rectangular lights the frame carries, the rest unused.
    pub(crate) area_count: [u32; 4],
    /// The alias table over the rectangles' emitted power, four columns to a vector.
    pub(crate) area_probability: [[f32; 4]; AREA_ALIAS_WORDS],
    /// The other column of each rectangle's alias, four to a vector.
    pub(crate) area_alias: [[u32; 4]; AREA_ALIAS_WORDS],
    /// The share of the power each rectangle carries, which is the density of a draw.
    pub(crate) area_share: [[f32; 4]; AREA_ALIAS_WORDS],
    /// `x` the lens radius in millimetres, `y` the distance the lens focuses at, `z` one when the view
    /// is a perspective one that can have a lens, `w` unused (Y5).
    pub(crate) lens: [f32; 4],
}

/// The frame's faces as the shader reads them, in the tree's order.
pub(crate) fn faces(bvh: &Bvh) -> Result<Vec<GpuFace>, FinalError> {
    if bvh.face_count() > MAX_TRIANGLES {
        return Err(FinalError::TooManyTriangles { count: bvh.face_count(), max: MAX_TRIANGLES });
    }
    let mut faces = Vec::with_capacity(bvh.face_count());
    for face in bvh.faces() {
        let mut slots = [[0.0f32; 4]; VERTEX_SLOTS];
        for (slot, vertex) in slots.iter_mut().zip(face.triangle.vertices()) {
            *slot = [vertex.x as f32, vertex.y as f32, vertex.z as f32, 0.0];
        }
        let [a, b, c] = slots;
        let surface = face.surface;
        let uv = face.uv;
        faces.push(GpuFace {
            a,
            b,
            c,
            colour: [face.colour[0], face.colour[1], face.colour[2], 0.0],
            finish: [surface.roughness, surface.metallic, surface.ior, surface.transmission],
            tint: [surface.absorption[0], surface.absorption[1], surface.absorption[2], 0.0],
            uv_a: [uv[0][0], uv[0][1], uv[1][0], uv[1][1]],
            uv_b: [uv[2][0], uv[2][1], 0.0, 0.0],
            // One-based, so zero — the value a cleared slot holds — is "no picture".
            decor: [face.decor.map_or(0, |index| index.saturating_add(1)), 0, 0, 0],
        });
    }
    Ok(faces)
}

/// The tree as the shader reads it: root first, children adjacent, a leaf's run of faces contiguous.
pub(crate) fn nodes(bvh: &Bvh) -> Result<Vec<GpuNode>, FinalError> {
    let mut nodes = Vec::with_capacity(bvh.node_count());
    for node in bvh.nodes() {
        let bounds = node.bounds();
        let first = u32::try_from(node.first())
            .map_err(|_| FinalError::TooManyTriangles { count: bvh.face_count(), max: MAX_TRIANGLES })?;
        let count = u32::try_from(node.count())
            .map_err(|_| FinalError::TooManyTriangles { count: bvh.face_count(), max: MAX_TRIANGLES })?;
        let (min, max) = (bounds.min(), bounds.max());
        nodes.push(GpuNode {
            min: [min.x as f32, min.y as f32, min.z as f32, 0.0],
            max: [max.x as f32, max.y as f32, max.z as f32, 0.0],
            info: [first, count, 0, 0],
        });
    }
    Ok(nodes)
}

/// The uniform for one sample of one strip of one image.
///
/// `sample` counts from zero, so the shader's noise changes with it and the same image is reproducible
/// from the same numbers: two renders of one revision and one camera are the same pixels. `counts` is
/// the frame's own face and node counts, and `strip` is the run of rows this dispatch covers.
pub(crate) fn uniform(
    camera: Camera,
    lighting: &Lighting,
    request: Request,
    counts: (u32, u32),
    strip: (u32, u32),
    sample: u32,
) -> Result<Uniform, FinalError> {
    let finite = |vector: DVec3| vector.is_finite();
    let (eye, right, up, forward) = (camera.eye(), camera.right(), camera.up(), camera.forward());
    if !finite(eye) || !finite(right) || !finite(up) || !finite(forward) {
        return Err(refuse("the camera's basis is not finite"));
    }
    let Some(aspect) = crate::view::aspect(request.width(), request.height()) else {
        return Err(FinalError::ImageSize { width: request.width(), height: request.height() });
    };
    let (orthographic, half_height) = match camera.projection() {
        Projection::Perspective { fov_y } => {
            if !fov_y.is_finite() || fov_y <= 0.0 {
                return Err(refuse("the camera's field of view is not a number"));
            }
            (0.0, (fov_y * 0.5).tan())
        }
        Projection::Orthographic { height } => {
            if !height.is_finite() || height <= 0.0 {
                return Err(refuse("the camera's orthographic height is not a number"));
            }
            (1.0, height * 0.5)
        }
    };
    let mut point_position = [[0.0f32; 4]; MAX_POINT_LIGHTS];
    let mut point_colour = [[0.0f32; 4]; MAX_POINT_LIGHTS];
    let mut point_count = 0usize;
    for (index, (position, colour)) in point_position.iter_mut().zip(point_colour.iter_mut()).enumerate() {
        let Some(light) = lighting.points.get(index) else { break };
        if !light.position.is_finite() || !light.power.is_finite() || !light.color.iter().all(|c| c.is_finite()) {
            return Err(refuse("a point light's numbers are not finite"));
        }
        if light.power <= 0.0 {
            continue;
        }
        *position = [light.position.x as f32, light.position.y as f32, light.position.z as f32, 0.0];
        *colour = [light.color[0], light.color[1], light.color[2], light.power];
        point_count = point_count.saturating_add(1);
    }
    // The key light is the viewport's sun: its colour is the irradiance a surface facing it receives.
    // A black key light is no light at all, so it does not count towards the light a sample picks.
    let key_lit = lighting.key_color.iter().all(|channel| channel.is_finite())
        && lighting.key_color.iter().any(|channel| *channel > 0.0)
        && lighting.key_direction.is_finite();
    if !key_lit && lighting.key_color.iter().any(|channel| !channel.is_finite()) {
        return Err(refuse("the key light's colour is not finite"));
    }
    if !lighting.key_direction.is_finite() || lighting.key_direction.length() <= 0.0 {
        return Err(refuse("the key light's direction is not a unit vector"));
    }
    let (face_count, node_count) = counts;
    let direction = lighting.key_direction;
    let Ok(light_count) = u32::try_from(point_count) else {
        return Err(refuse("the frame carries more lights than a sample can name"));
    };
    // The key light is a sun and is traced at every vertex; the point lights are the set a sample
    // picks from, by emitted power. A table that cannot be built is a frame with no lit point light,
    // which is not an error: the key light alone is a legal picture.
    let powers: Vec<f32> = point_colour.iter().take(point_count).map(|colour| colour[3]).collect();
    let alias = crate::light::AliasTable::build(&powers);
    let mut point_probability = [[0.0f32; 4]; ALIAS_WORDS];
    let mut point_alias = [[0u32; 4]; ALIAS_WORDS];
    let mut point_share = [[0.0f32; 4]; ALIAS_WORDS];
    if let Some(table) = &alias {
        for index in 0..point_count {
            let (vector, channel) = (index / 4, index % 4);
            let (probability, instead) = table.column(index);
            if let Some(slot) = point_probability.get_mut(vector).and_then(|row| row.get_mut(channel)) {
                *slot = probability.clamp(0.0, 1.0);
            }
            if let Some(slot) = point_share.get_mut(vector).and_then(|row| row.get_mut(channel)) {
                *slot = table.share(index) as f32;
            }
            if let Some(slot) = point_alias.get_mut(vector).and_then(|row| row.get_mut(channel)) {
                *slot = instead;
            }
        }
    }
    let environment = lighting.environment.map(|channel| if channel.is_finite() { channel.max(0.0) } else { 0.0 });
    let (areas, area_table) = area_lights(&lighting.areas)?;
    Ok(Uniform {
        eye: [eye.x as f32, eye.y as f32, eye.z as f32, orthographic],
        right: [right.x as f32, right.y as f32, right.z as f32, 0.0],
        up: [up.x as f32, up.y as f32, up.z as f32, 0.0],
        forward: [forward.x as f32, forward.y as f32, forward.z as f32, 0.0],
        view: [half_height as f32, aspect as f32, request.width() as f32, request.height() as f32],
        counts: [face_count, node_count, sample, light_count],
        strip: [strip.0, strip.1, 0, 0],
        denoise: [
            crate::denoise::MIN_SAMPLES as f32,
            crate::denoise::TARGET_RELATIVE_ERROR,
            crate::denoise::LIGHT_FLOOR,
            0.0,
        ],
        key: [
            if key_lit { lighting.key_color[0] } else { 0.0 },
            if key_lit { lighting.key_color[1] } else { 0.0 },
            if key_lit { lighting.key_color[2] } else { 0.0 },
            if key_lit { 1.0 } else { 0.0 },
        ],
        key_direction: [direction.x as f32, direction.y as f32, direction.z as f32, 0.0],
        point_position,
        point_colour,
        environment: [environment[0], environment[1], environment[2], 0.0],
        point_probability,
        point_alias,
        point_share,
        area_origin: areas.origin,
        area_half_width: areas.half_width,
        area_half_height: areas.half_height,
        area_radiance: areas.radiance,
        area_count: [areas.count, 0, 0, 0],
        area_probability: area_table.probability,
        area_alias: area_table.alias,
        area_share: area_table.share,
        // A lens is a perspective camera's: an orthographic view has no eye to put a disc at, and the
        // shader reads the flag rather than asking twice (`crate::view::camera_ray_through`).
        lens: [
            camera.aperture() as f32,
            camera.focus() as f32,
            if camera.is_pinhole() || matches!(camera.projection(), Projection::Orthographic { .. }) {
                0.0
            } else {
                1.0
            },
            0.0,
        ],
    })
}

/// The rectangular lights as the shader reads them, and the table that picks one by its power.
struct Areas {
    origin: [[f32; 4]; MAX_AREAS],
    half_width: [[f32; 4]; MAX_AREAS],
    half_height: [[f32; 4]; MAX_AREAS],
    radiance: [[f32; 4]; MAX_AREAS],
    count: u32,
}

/// The alias table of the rectangles, in the same shape the point lights use.
struct AreaTable {
    probability: [[f32; 4]; AREA_ALIAS_WORDS],
    alias: [[u32; 4]; AREA_ALIAS_WORDS],
    share: [[f32; 4]; AREA_ALIAS_WORDS],
}

/// Packs the frame's rectangular emitters into the uniform, refusing a hostile one.
///
/// A face with no extent is skipped rather than uploaded: a radiance over no area is a number that
/// divides an image by zero, and the frame's own builder already guards it (`AreaLight::area_mm2`).
fn area_lights(lights: &[AreaLight]) -> Result<(Areas, AreaTable), FinalError> {
    let mut areas = Areas {
        origin: [[0.0; 4]; MAX_AREAS],
        half_width: [[0.0; 4]; MAX_AREAS],
        half_height: [[0.0; 4]; MAX_AREAS],
        radiance: [[0.0; 4]; MAX_AREAS],
        count: 0,
    };
    let mut powers: Vec<f32> = Vec::new();
    for light in lights.iter().take(MAX_AREAS) {
        let area = light.area_mm2();
        if area <= 0.0 {
            continue;
        }
        let finite = light.origin.is_finite()
            && light.half_width.is_finite()
            && light.half_height.is_finite()
            && light.color.iter().all(|channel| channel.is_finite())
            && light.power.is_finite();
        if !finite {
            return Err(refuse("a rectangular light's numbers are not finite"));
        }
        let index = areas.count as usize;
        let radiance = light.radiance();
        if let (Some(origin), Some(u), Some(v), Some(scale)) = (
            areas.origin.get_mut(index),
            areas.half_width.get_mut(index),
            areas.half_height.get_mut(index),
            areas.radiance.get_mut(index),
        ) {
            *origin = [light.origin.x as f32, light.origin.y as f32, light.origin.z as f32, 0.0];
            *u = [light.half_width.x as f32, light.half_width.y as f32, light.half_width.z as f32, 0.0];
            *v = [light.half_height.x as f32, light.half_height.y as f32, light.half_height.z as f32, 0.0];
            *scale = [radiance[0], radiance[1], radiance[2], area as f32];
        }
        areas.count = areas.count.saturating_add(1);
        powers.push(light.power);
    }
    let mut table = AreaTable {
        probability: [[0.0; 4]; AREA_ALIAS_WORDS],
        alias: [[0u32; 4]; AREA_ALIAS_WORDS],
        share: [[0.0; 4]; AREA_ALIAS_WORDS],
    };
    if let Some(alias) = crate::light::AliasTable::build(&powers) {
        for index in 0..powers.len() {
            let (vector, channel) = (index / 4, index % 4);
            let (probability, instead) = alias.column(index);
            if let Some(slot) = table.probability.get_mut(vector).and_then(|row| row.get_mut(channel)) {
                *slot = probability.clamp(0.0, 1.0);
            }
            if let Some(slot) = table.share.get_mut(vector).and_then(|row| row.get_mut(channel)) {
                *slot = alias.share(index) as f32;
            }
            if let Some(slot) = table.alias.get_mut(vector).and_then(|row| row.get_mut(channel)) {
                *slot = instead;
            }
        }
    }
    Ok((areas, table))
}

/// Sobol's direction numbers, flattened for the device: four dimensions of thirty-two words.
///
/// The shader reads the same words `sampler::direction_numbers` generated, so the sequence on the
/// device is the sequence the tests measured and no constant is retyped.
pub(crate) fn sobol_directions() -> Vec<u32> {
    let table = crate::sampler::direction_numbers();
    let mut words = Vec::with_capacity(table.len() * 32);
    for dimension in table.iter() {
        words.extend_from_slice(dimension);
    }
    words
}

/// The energy table flattened for the device: sixteen rows of sixteen cells, then the sixteen
/// averages, with the tail padded to a whole `vec4`.
pub(crate) fn energy_values() -> Vec<f32> {
    let table = bsdf::EnergyTable::get();
    let mut values = Vec::with_capacity(ENERGY_VALUES);
    for row in table.values().iter() {
        values.extend_from_slice(row);
    }
    values.extend_from_slice(table.averages());
    values.resize(ENERGY_VALUES, 0.0);
    values
}

/// How many `f32`s the energy table occupies, padded to a whole `vec4`.
pub(crate) const ENERGY_VALUES: usize = 288;

/// A refusal the device's input cannot be built from.
fn refuse(reason: &str) -> FinalError {
    FinalError::Device { reason: reason.to_owned() }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use geom::Aabb;
    use scene::{PointLight, SceneFrame};

    use crate::ray::Triangle;
    use crate::soup::Face;

    use super::*;

    fn bvh(faces: Vec<Face>) -> Bvh {
        Bvh::build(faces)
    }

    fn face(v0: DVec3, v1: DVec3, v2: DVec3, colour: [f32; 3]) -> Face {
        Face::matte(Triangle::new(v0, v1, v2), colour)
    }

    fn request() -> Request {
        Request::new(200, 100, 8).expect("a small request")
    }

    #[test]
    fn a_face_reaches_the_device_with_its_vertices_and_its_colour() {
        let tree =
            bvh(vec![face(DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0), DVec3::new(0.0, 10.0, 0.0), [0.25, 0.5, 0.75])]);
        let faces = faces(&tree).expect("a flat face list");
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0].a, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(faces[0].b, [10.0, 0.0, 0.0, 0.0]);
        assert_eq!(faces[0].c, [0.0, 10.0, 0.0, 0.0]);
        assert_eq!(faces[0].colour, [0.25, 0.5, 0.75, 0.0]);
        assert_eq!(faces[0].finish, [1.0, 0.0, 1.5, 0.0], "a face that names no finish is matte paint");
        assert_eq!(faces[0].tint, [0.0; 4]);
    }

    #[test]
    fn a_leaf_names_a_run_of_faces_and_an_interior_node_names_its_left_child() {
        // Eight quads far enough apart that the build must split them.
        let mut soup = Vec::new();
        for index in 0..8 {
            let x = index as f64 * 100.0;
            let a = DVec3::new(x, 0.0, 0.0);
            let b = DVec3::new(x + 10.0, 0.0, 0.0);
            let c = DVec3::new(x, 10.0, 0.0);
            soup.push(face(a, b, c, [1.0, 1.0, 1.0]));
        }
        let tree = bvh(soup);
        let nodes = nodes(&tree).expect("flat nodes");
        assert_eq!(nodes.len(), tree.node_count());
        assert!(nodes.len() > 1, "eight spread quads split");
        for (index, node) in tree.nodes().iter().enumerate() {
            let flat = nodes[index];
            let bounds: Aabb = node.bounds();
            assert!((flat.min[0] - bounds.min().x as f32).abs() < 1e-3);
            assert!((flat.max[2] - bounds.max().z as f32).abs() < 1e-3);
            if node.is_leaf() {
                assert_eq!(flat.info, [node.first() as u32, node.count() as u32, 0, 0]);
                assert!(node.first() + node.count() <= tree.face_count(), "a leaf stays inside the face list");
            } else {
                assert_eq!(flat.info[1], 0, "an interior node holds no faces");
                assert!(nodes.get(node.first() + 1).is_some(), "an interior node names both children");
            }
        }
        // Every face is named by exactly one leaf: the layout loses nothing and repeats nothing.
        let mut named = vec![0usize; tree.face_count()];
        for node in tree.nodes().iter().filter(|node| node.is_leaf()) {
            for index in node.first()..node.first() + node.count() {
                if let Some(slot) = named.get_mut(index) {
                    *slot += 1;
                }
            }
        }
        assert!(named.iter().all(|count| *count == 1), "{named:?}");
    }

    #[test]
    fn the_uniform_carries_the_view_the_aspect_and_the_lights() {
        let mut frame = SceneFrame::default();
        frame.lighting.points.push(PointLight {
            position: DVec3::new(500.0, 0.0, 300.0),
            color: [1.0, 0.9, 0.8],
            power: 1000.0,
        });
        frame.lighting.points.push(PointLight { position: DVec3::new(0.0, 0.0, 0.0), color: [1.0; 3], power: 0.0 });
        let camera = Camera::new(DVec3::new(0.0, 0.0, 500.0), 2000.0).expect("a camera");
        let uniform = uniform(camera, &frame.lighting, request(), (3, 5), (0, 100), 7).expect("a uniform");
        assert_eq!(uniform.counts, [3, 5, 7, 1], "one lit point light: the key light is not in the table");
        assert!((uniform.view[0] - (scene::DEFAULT_FOV_Y * 0.5).tan() as f32).abs() < 1e-6);
        assert!((uniform.view[1] - 2.0).abs() < 1e-6, "200 by 100 is two to one");
        assert_eq!(&uniform.view[2..], &[200.0, 100.0]);
        assert_eq!(uniform.eye[3], 0.0, "a perspective view");
        assert_eq!(uniform.point_position[0], [500.0, 0.0, 300.0, 0.0]);
        assert_eq!(uniform.point_colour[0], [1.0, 0.9, 0.8, 1000.0]);
        assert_eq!(uniform.point_colour[1], [0.0; 4], "a dead light is not uploaded as a lamp");
    }

    #[test]
    fn a_lens_reaches_the_uniform_with_its_radius_and_its_focus() {
        let camera = Camera::new(DVec3::ZERO, 3_000.0).expect("a camera");
        let pinhole =
            uniform(camera, &SceneFrame::default().lighting, request(), (0, 0), (0, 0), 0).expect("a uniform");
        assert_eq!(pinhole.lens, [0.0, 1200.0, 0.0, 0.0], "a pinhole says so, and the shader costs one test");

        let lens = camera.with_lens(45.0, 1_800.0).expect("a lens");
        let packed = uniform(lens, &SceneFrame::default().lighting, request(), (0, 0), (0, 0), 0).expect("a uniform");
        assert_eq!(packed.lens, [45.0, 1_800.0, 1.0, 0.0]);

        // An orthographic view carries the numbers but not the switch: it has no lens to draw from.
        let flat = lens.with_projection(Projection::Orthographic { height: 1_000.0 });
        let packed = uniform(flat, &SceneFrame::default().lighting, request(), (0, 0), (0, 0), 0).expect("a uniform");
        assert_eq!(packed.lens[2], 0.0);
    }

    #[test]
    fn a_rectangular_light_reaches_the_uniform_as_a_face_and_a_share() {
        let mut frame = SceneFrame::default();
        frame.lighting.areas.push(scene::AreaLight {
            origin: DVec3::new(0.0, 0.0, 1_000.0),
            half_width: DVec3::new(600.0, 0.0, 0.0),
            half_height: DVec3::new(0.0, 700.0, 0.0),
            color: [1.0, 0.5, 0.25],
            power: 40.0,
        });
        frame.lighting.areas.push(scene::AreaLight {
            origin: DVec3::new(2_000.0, 0.0, 1_000.0),
            half_width: DVec3::new(100.0, 0.0, 0.0),
            half_height: DVec3::new(0.0, 100.0, 0.0),
            color: [1.0; 3],
            power: 40.0,
        });
        let camera = Camera::new(DVec3::new(0.0, 0.0, 500.0), 2_000.0).expect("a camera");
        let packed = uniform(camera, &frame.lighting, request(), (3, 5), (0, 100), 0).expect("a uniform");
        assert_eq!(packed.area_count[0], 2);
        assert_eq!(packed.area_origin[0], [0.0, 0.0, 1_000.0, 0.0]);
        assert_eq!(packed.area_half_width[0], [600.0, 0.0, 0.0, 0.0]);
        assert_eq!(packed.area_half_height[0], [0.0, 700.0, 0.0, 0.0]);
        // The radiance is the watts over the area and the hemisphere, and the area travels with it so
        // the shader's pdf needs no second conversion.
        let radiance = 40.0 / (std::f32::consts::PI * 1.68);
        assert!((packed.area_radiance[0][0] - radiance).abs() < 1.0e-3, "{:?}", packed.area_radiance[0]);
        assert_eq!(packed.area_radiance[0][1], packed.area_radiance[0][0] * 0.5);
        assert!((packed.area_radiance[0][3] - 1.68e6).abs() < 1.0, "the area is square millimetres");
        // Two equal-wattage faces are drawn equally often.
        assert!((packed.area_share[0][0] - 0.5).abs() < 1.0e-6);
        assert!((packed.area_share[0][1] - 0.5).abs() < 1.0e-6);

        // A face with no extent is not uploaded at all: a radiance over no area has no pdf.
        frame.lighting.areas[1].half_width = DVec3::ZERO;
        let skipped = uniform(camera, &frame.lighting, request(), (3, 5), (0, 100), 0).expect("a uniform");
        assert_eq!(skipped.area_count[0], 1, "a degenerate face is skipped");
        assert_eq!(skipped.area_share[0][0], 1.0, "the one face left takes every draw");

        // A hostile number stops the render instead of poisoning it.
        frame.lighting.areas[0].origin = DVec3::new(f64::NAN, 0.0, 0.0);
        assert!(uniform(camera, &frame.lighting, request(), (3, 5), (0, 100), 0).is_err());
    }

    #[test]
    fn an_orthographic_view_says_so_and_a_black_key_light_is_no_light() {
        let mut camera = Camera::new(DVec3::ZERO, 1000.0).expect("a camera");
        camera.set_orthographic();
        let mut frame = SceneFrame::default();
        frame.lighting.key_color = [0.0; 3];
        let uniform = uniform(camera, &frame.lighting, request(), (1, 1), (0, 100), 0).expect("a uniform");
        assert_eq!(uniform.eye[3], 1.0, "an orthographic view");
        assert_eq!(uniform.counts[3], 0, "a black key light lights nothing");
        assert_eq!(uniform.key, [0.0; 4]);
    }

    #[test]
    fn a_frame_with_no_geometry_at_all_still_builds_a_uniform() {
        let empty = SceneFrame::default();
        let camera = Camera::new(DVec3::ZERO, 1000.0).expect("a camera");
        let uniform = uniform(camera, &empty.lighting, request(), (0, 0), (0, 100), 0).expect("a uniform");
        assert_eq!(uniform.counts[0], 0);
        assert_eq!(uniform.counts[1], 0);
        assert!(faces(&Bvh::default()).expect("no faces").is_empty());
        assert!(nodes(&Bvh::default()).expect("no nodes").is_empty());
        // A default frame has a key light and no point light, so the table a sample draws from is
        // empty and every light it traces is the sun.
        assert_eq!(uniform.counts[3], 0);
        assert_eq!(uniform.key[3], 1.0, "the key light is still there");
    }

    #[test]
    fn a_hostile_camera_or_light_is_a_sentence_not_a_uniform_of_nan() {
        let mut camera = Camera::new(DVec3::ZERO, 1000.0).expect("a camera");
        let mut frame = SceneFrame::default();
        let request = request();
        frame.lighting.points.push(PointLight { position: DVec3::NAN, color: [1.0; 3], power: 10.0 });
        assert!(uniform(camera, &frame.lighting, request, (0, 0), (0, 100), 0).is_err(), "a NaN light is refused");
        frame.lighting.points.clear();
        frame.lighting.key_color = [f32::NAN, 1.0, 1.0];
        assert!(uniform(camera, &frame.lighting, request, (0, 0), (0, 100), 0).is_err(), "a NaN key colour is refused");
        frame.lighting.key_color = [1.0; 3];
        frame.lighting.key_direction = DVec3::ZERO;
        assert!(
            uniform(camera, &frame.lighting, request, (0, 0), (0, 100), 0).is_err(),
            "a sun with no direction is refused"
        );
        frame.lighting.key_direction = DVec3::Z;
        // The scene's own camera refuses a non-finite basis, so a broken view cannot reach the shader
        // through it; the guard here stays for a future caller that builds a uniform from raw numbers.
        camera.roll_to(f64::NAN);
        assert!(camera.right().is_finite() && camera.forward().is_finite(), "the camera keeps a usable basis");
    }
}
