const MAX_POINT_LIGHTS: u32 = 8u;
const PI: f32 = 3.141592653589793;

// Fixed viewport exposure: a 1000 W point light at one metre shades a white surface at about half
// intensity. A scene exposure control can replace this constant without touching the light data.
const POINT_EXPOSURE: f32 = 0.006;

struct Camera {
    view_projection: mat4x4<f32>,
    camera_position: vec4<f32>,
    light_direction: vec4<f32>,
    // rgb = key colour, a = ambient fraction
    light_color: vec4<f32>,
    point_count: vec4<u32>,
    // xyz = world position, w = unused
    point_position: array<vec4<f32>, 8>,
    // rgb = linear colour, a = radiant power in watts
    point_color: array<vec4<f32>, 8>,
    // rgb = gamma-space shadow colour, a = ground-shadow strength
    shadow: vec4<f32>,
    // x = half-size of the floor plate in mm, y and z = one mask texel, w = penumbra in texels
    shadow_plane: vec4<f32>,
    // x = how opaque the floor plate is, 0 to 1; the rest is unused
    floor: vec4<f32>,
    // x = whether this view draws the decor's tiling, y = one line's half-width in physical pixels,
    // z and w = unused (Y16)
    decor_grid: vec4<f32>,
    // rgb = the ink of that grid in gamma space, a = how much of it one line lays over the board
    decor_grid_ink: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;

// The key light's shadow map: the scene drawn from the sun into depth alone (ADR 0143). The face
// shader compares a surface's own depth against it, so a face darkens where another face stands
// between it and the light; the depth pass reads the same uniform from this group.
struct ShadowCamera {
    view_projection: mat4x4<f32>,
    // x = the world size of one texel in mm, y = 1 when the map holds this frame's scene
    params: vec4<f32>,
};

// The board the face being drawn is cut from: the decor's picture, sampled where its own frame
// projects it. A surface with no decor is bound to a one-texel white picture, so the multiply below
// changes nothing and the sampler is never branched over (which would need uniform control flow).
@group(2) @binding(0) var decor_texture: texture_2d<f32>;
@group(2) @binding(1) var decor_sampler: sampler;
// x = 1 when the bound texels are a board's picture and 0 for the white stand-in, which is what keeps
// the tiling guide off a surface nobody can tile (Y16).
@group(2) @binding(2) var<uniform> decor_layer: vec4<f32>;

@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;
@group(1) @binding(2) var<uniform> shadow_camera: ShadowCamera;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};

// A shaded triangle carries one attribute a line does not: the coordinate its decor is read at. The
// panel's own frame is where the grain is defined, so the CPU projects it once per vertex and the
// fragment only samples — see `scene::Decor::uv`.
struct FaceVertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
};

struct MeshOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
};

@vertex
fn vs_mesh(vertex: FaceVertexIn) -> MeshOut {
    var out: MeshOut;
    out.clip_position = camera.view_projection * vec4<f32>(vertex.position, 1.0);
    out.normal = vertex.normal;
    out.world_position = vertex.position;
    out.color = vertex.color;
    out.uv = vertex.uv;
    return out;
}

fn safe_normalize(value: vec3<f32>) -> vec3<f32> {
    let length = sqrt(dot(value, value));
    return select(vec3<f32>(0.0), value / length, length > 1.0e-9);
}

// Inverse-square falloff with no hard range cutoff. The power is in watts and the exposure maps
// 1000 W at one metre to about half intensity.
fn point_lighting(normal: vec3<f32>, world_position: vec3<f32>, view_direction: vec3<f32>) -> vec3<f32> {
    var total = vec3<f32>(0.0);
    let count = min(camera.point_count.x, MAX_POINT_LIGHTS);
    for (var index = 0u; index < count; index = index + 1u) {
        let offset = camera.point_position[index].xyz - world_position;
        let distance_squared = max(dot(offset, offset), 1.0);
        let direction = offset * inverseSqrt(distance_squared);
        let attenuation = POINT_EXPOSURE / (4.0 * PI * distance_squared);
        let power = camera.point_color[index].a;
        let diffuse = max(dot(normal, direction), 0.0);
        let half_vector = safe_normalize(direction + view_direction);
        let specular = pow(max(dot(normal, half_vector), 0.0), 32.0) * 0.12;
        total += camera.point_color[index].rgb * (diffuse + specular) * power * attenuation;
    }
    return total;
}

// The shadow map's texels, and the three numbers that keep a surface from shadowing itself.
const SHADOW_TEXELS: f32 = 2048.0;
// How far the receiver moves off its own surface along its normal, in texels: a face is compared
// with a point in front of it, never with its own depth.
const SHADOW_NORMAL_TEXELS: f32 = 1.5;
// The depth bias in texels at a perpendicular and at a grazing angle. The map is square in the
// light's axes, so one texel is the same distance across the map as it is deep into it.
const SHADOW_BIAS_TEXELS: f32 = 1.5;
const SHADOW_SLOPE_TEXELS: f32 = 2.5;
// The grazing angle past which a bigger offset and a bigger bias buy nothing: past it the surface
// faces away from the sun and receives no key light at all.
const SHADOW_TAN_LIMIT: f32 = 4.0;

// How much of the key light reaches a surface: 1 where nothing stands between it and the sun.
//
// The two coplanar cases decide the three constants above: a band lying on a panel face and a shelf
// flush with the side it meets. Both sit at the caster's own depth, so the comparison is
// `LessEqual` and a surface equal to the map is lit, while the normal offset keeps a band that is a
// hair above the face from cutting a gap under itself.
fn shadow_factor(normal: vec3<f32>, world_position: vec3<f32>) -> f32 {
    if shadow_camera.params.y <= 0.0 {
        return 1.0;
    }
    let light = safe_normalize(camera.light_direction.xyz);
    let cos_theta = clamp(dot(normal, light), 0.0, 1.0);
    let tan_theta = min(sqrt(max(1.0 - cos_theta * cos_theta, 0.0)) / max(cos_theta, 0.05), SHADOW_TAN_LIMIT);
    let offset = normal * (SHADOW_NORMAL_TEXELS * shadow_camera.params.x * (1.0 + tan_theta));
    let clip = shadow_camera.view_projection * vec4<f32>(world_position + offset, 1.0);
    let ndc = clip.xyz / clip.w;
    // The target's first row is the top of the screen, which is +Y in clip space, so v counts down.
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let reference = ndc.z - (SHADOW_BIAS_TEXELS + SHADOW_SLOPE_TEXELS * tan_theta) / SHADOW_TEXELS;
    // A point outside the map is behind nothing it can see, so it stays lit; the coordinate is
    // clamped instead of branched on, because sampling must stay in uniform control flow.
    let inside = uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0;
    let centre = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let texel = vec2<f32>(1.0 / SHADOW_TEXELS, 1.0 / SHADOW_TEXELS);
    var lit = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            lit = lit + textureSampleCompare(shadow_map, shadow_sampler, centre + vec2<f32>(f32(x), f32(y)) * texel, reference);
        }
    }
    return select(1.0, lit / 9.0, inside);
}

// The scene drawn from the sun: position alone, because the map holds depth and nothing else.
@vertex
fn vs_shadow(vertex: FaceVertexIn) -> @builtin(position) vec4<f32> {
    return shadow_camera.view_projection * vec4<f32>(vertex.position, 1.0);
}

struct MeshIn {
    @location(0) normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @builtin(front_facing) front_facing: bool,
};

// The surface's own colour under its decor: the picture multiplies the colour, which is what every
// renderer does with a tinted print. The picture is `Rgba8UnormSrgb`, so the sampler hands back
// linear light and the multiply stays in the space the lighting is written in.
fn albedo_of(color: vec4<f32>, uv: vec2<f32>) -> vec3<f32> {
    return color.rgb * textureSample(decor_texture, decor_sampler, uv).rgb;
}

// Where the tiling of a board's picture draws, as one number between 0 and 1 (Y16).
//
// The step is the file's and not a constant here: the vertex carries the coordinate `Decor::uv`
// computed from the record's `covers` and the node's own mapping, and that coordinate is one whole
// picture wide, so a line falls at every whole coordinate and the seam is exactly where the sampler
// repeats the image. The weight is the theme's, in physical pixels, and the derivative turns it into
// the units the coordinate counts in — that is the one calculation a screen-space line cannot do
// without, and it is why the guide is drawn here rather than in the line pass.
fn decor_tiling(uv: vec2<f32>) -> f32 {
    let half = max(camera.decor_grid.y, 0.0);
    // How far this pixel is from the nearest cell edge, and how much of the coordinate one pixel
    // spans. Both axes lay a line, because the picture is a rectangle in its own right.
    let edge = abs(fract(uv + vec2<f32>(0.5)) - vec2<f32>(0.5));
    let per_pixel = max(fwidth(uv), vec2<f32>(1.0e-9));
    return max(tiling_line(edge.x, per_pixel.x, half), tiling_line(edge.y, per_pixel.y, half));
}

// One line of the tiling grid along one axis of the picture's coordinate.
//
// `edge` is the distance to the nearest seam and `per_pixel` how much of the coordinate one pixel
// spans, both in cells; `half` is the line's half-width in pixels. A pixel of softness either side
// keeps a board at a grazing angle from crawling.
fn tiling_line(edge: f32, per_pixel: f32, half: f32) -> f32 {
    let cell = 1.0 / per_pixel;
    // A cell narrower than a few lines is a wash, not a tiling: this axis of the guide steps back
    // where a picture repeats faster than it can be read, and the other axis keeps its lines.
    let fade = saturate((cell - 2.0 * half) / (6.0 * half));
    return (1.0 - smoothstep(half - 0.5, half + 0.5, edge / per_pixel)) * fade;
}

// The colour a shaded surface is drawn with: its lit albedo, and the tiling guide over it when the
// view asks for one. The two mix in gamma space, where the target and the chrome live.
fn board_of(
    normal: vec3<f32>,
    front_facing: bool,
    world_position: vec3<f32>,
    color: vec4<f32>,
    uv: vec2<f32>,
) -> vec3<f32> {
    let rgb = gamma_encode(shade_surface(normal, front_facing, world_position, color));
    let guide = decor_tiling(uv) * camera.decor_grid.x * decor_layer.x;
    return mix(rgb, camera.decor_grid_ink.rgb, guide * camera.decor_grid_ink.a);
}

// Linear colour of one shaded surface, before the gamma encode the target and egui expect.
fn shade_surface(
    normal_input: vec3<f32>,
    front_facing: bool,
    world_position: vec3<f32>,
    color: vec4<f32>,
) -> vec3<f32> {
    // Panels are open on both faces, so flip the normal for back-facing triangles instead of
    // culling them or lighting the inside of a cutout black.
    let normal = normalize(select(-normal_input, normal_input, front_facing));
    let light_direction = safe_normalize(camera.light_direction.xyz);
    let view_direction = safe_normalize(camera.camera_position.xyz - world_position);
    let half_vector = safe_normalize(light_direction + view_direction);

    let diffuse = max(dot(normal, light_direction), 0.0);
    let specular = pow(max(dot(normal, half_vector), 0.0), 32.0) * 0.12;
    // One strength for both masks: the floor's silhouette and a face's occlusion are the same
    // light, so turning the shadow off leaves the model as flat as the ground (U107).
    var occlusion = 1.0;
    if (camera.shadow.a > 0.0) {
        occlusion = 1.0 - camera.shadow.a * (1.0 - shadow_factor(normal, world_position));
    }
    let key = camera.light_color.rgb * (diffuse * occlusion) + vec3<f32>(camera.light_color.a);
    let points = point_lighting(normal, world_position, view_direction);
    return color.rgb * (key + points) + vec3<f32>(specular * occlusion);
}

// The offscreen target is Rgba8Unorm, which is what egui registers as a native texture.
// Encode gamma here so the viewport matches the rest of the interface.
fn gamma_encode(rgb: vec3<f32>) -> vec3<f32> {
    return pow(rgb, vec3<f32>(1.0 / 2.2));
}

@fragment
fn fs_mesh(fragment: MeshIn) -> @location(0) vec4<f32> {
    let color = vec4<f32>(albedo_of(fragment.color, fragment.uv), fragment.color.a);
    let rgb = board_of(fragment.normal, fragment.front_facing, fragment.world_position, color, fragment.uv);
    return vec4<f32>(rgb, fragment.color.a);
}

// The translucent pass reduces opacity so blending reveals what lies behind; with alpha 1 the
// blend would copy the surface and the style would be indistinguishable from *shaded*.
const GHOST_ALPHA: f32 = 0.22;

@fragment
fn fs_mesh_translucent(fragment: MeshIn) -> @location(0) vec4<f32> {
    let color = vec4<f32>(albedo_of(fragment.color, fragment.uv), fragment.color.a);
    let rgb = board_of(fragment.normal, fragment.front_facing, fragment.world_position, color, fragment.uv);
    return vec4<f32>(rgb, min(fragment.color.a, GHOST_ALPHA));
}

// The face of a source: a window is what gives the light, so no light falls on it. Its colour is drawn
// as it stands and blended over the model at the opacity it carries (L2).
@fragment
fn fs_mesh_source(fragment: MeshIn) -> @location(0) vec4<f32> {
    return vec4<f32>(gamma_encode(albedo_of(fragment.color, fragment.uv)), fragment.color.a);
}

struct LineOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_line(vertex: VertexIn) -> LineOut {
    var out: LineOut;
    out.clip_position = camera.view_projection * vec4<f32>(vertex.position, 1.0);
    out.color = vertex.color;
    return out;
}

struct LineIn {
    @location(0) color: vec4<f32>,
};

@fragment
fn fs_line(fragment: LineIn) -> @location(0) vec4<f32> {
    // Grid and path line colours are already in gamma space.
    return fragment.color;
}

// The ground shadow has three parts: the scene projected along the key light into a coverage mask,
// one separable blur, and the floor plate that reads the result. None of them lights anything, so
// none of them reads the point lights, and the strength and the colour arrive in one uniform.

// The half-size of the floor plate, in world millimetres. The plate, the mask and the blur all read
// this one number, so they cannot disagree about where the ground is.
fn ground_extent() -> f32 {
    return max(camera.shadow_plane.x, 1.0);
}

// Where a light ray through `position` meets the floor plane. A light that is below the horizon has
// its strength zeroed on the CPU, so the clamped depth here only keeps the divide finite.
@vertex
fn vs_shadow_cover(vertex: FaceVertexIn) -> @builtin(position) vec4<f32> {
    let light = safe_normalize(camera.light_direction.xyz);
    let depth = select(0.02, light.z, light.z > 0.02);
    let plane = vertex.position.xy - light.xy * (vertex.position.z / depth);
    return vec4<f32>(plane / ground_extent(), 0.5, 1.0);
}

@fragment
fn fs_shadow_cover() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}

struct BlurOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// One full-screen triangle: no vertex buffer, no camera, the same geometry for both blur axes.
@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> BlurOut {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: BlurOut;
    out.clip_position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

@group(2) @binding(0) var mask_texture: texture_2d<f32>;
@group(2) @binding(1) var mask_sampler: sampler;
// xy = one texel along the axis being blurred, z = penumbra radius in texels
@group(2) @binding(2) var<uniform> blur: vec4<f32>;

const BLUR_TAPS: i32 = 4;

@fragment
fn fs_blur(fragment: BlurOut) -> @location(0) vec4<f32> {
    var total = 0.0;
    var weights = 0.0;
    for (var tap = -BLUR_TAPS; tap <= BLUR_TAPS; tap = tap + 1) {
        let offset = f32(tap) / f32(BLUR_TAPS) * blur.z;
        let weight = exp(-f32(tap * tap) / 8.0);
        total = total + textureSample(mask_texture, mask_sampler, fragment.uv + blur.xy * offset).r * weight;
        weights = weights + weight;
    }
    return vec4<f32>(total / weights, 0.0, 0.0, 1.0);
}

struct FloorOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs_floor(vertex: VertexIn) -> FloorOut {
    var out: FloorOut;
    out.clip_position = camera.view_projection * vec4<f32>(vertex.position, 1.0);
    out.world_position = vertex.position;
    out.color = vertex.color;
    return out;
}

// The plate's own opacity, and the reason the shadow survives it.
//
// Two layers meet in one fragment: the plate with the workspace's opacity, and the shadow on top of
// it with the workspace's strength. One blend can carry only one of them, so the plate's alpha is
// the *larger* of the two: at 40 % the plate tints what stands under it, while a shadow stronger
// than that opacity keeps its own weight instead of being washed into the background — a
// transparent floor cannot erase its own shadow. Zero is the one case the rule cannot express: a
// shadow needs a surface to darken, so an empty floor draws nothing at all.
@fragment
fn fs_floor(fragment: FloorOut) -> @location(0) vec4<f32> {
    let extent = ground_extent();
    // The target's first row is the top of the screen, which is +Y here, so v counts down.
    let uv = vec2<f32>(fragment.world_position.x / (2.0 * extent) + 0.5, 0.5 - fragment.world_position.y / (2.0 * extent));
    let cover = textureSample(mask_texture, mask_sampler, uv).r;
    let shadow = cover * camera.shadow.a;
    let opacity = clamp(camera.floor.x, 0.0, 1.0);
    let rgb = mix(fragment.color.rgb, camera.shadow.rgb, shadow);
    let alpha = select(0.0, max(opacity, shadow), opacity > 0.0);
    return vec4<f32>(rgb, alpha);
}
