//! The key light's shadow map: one depth pass over the scene, sampled by the face shaders.
//!
//! The ground mask in `ground.rs` answers one question about one plane: does a body stand between
//! the sun and a point of the floor. A panel is a different question — the nearest surface along the
//! light ray, whatever plane it lies on — so it needs a depth map: the scene drawn from the sun into
//! a 2048² depth texture, and a comparison against it in the face fragment.
//!
//! Contract: `ShadowMap` owns the depth texture, the comparison sampler, the pass that fills it and
//! the two bind groups a pass reads it through. The light is the frame's key light and nothing else:
//! the map and the ground mask quantise the same direction (see `crate::quantized_light`), so the
//! inside and the floor of a cabinet can never disagree about where the sun is.
//!
//! The whole scene is one box and the map is one orthographic camera over the sphere around it. One
//! cabinet is two metres: a cascade would buy nothing at that size and a second sun is not a thing
//! the document has (ADR 0036), so both are refused rather than deferred (ADR 0143).
//!
//! Refuses: nothing at draw time. A frame with no measured drawable leaves the map cleared, which
//! reads as "nothing between the sun and this point", so an empty scene shades as it always did.
//!
//! Does not own the strength: the switch, the strength and the colour arrive in the camera uniform
//! per view, so turning a shadow off costs no rebuild, and one view of a split may draw the shadow
//! that its neighbour does not.
//!
//! Changes when: the scene's box, the map's resolution or the bias scheme changes.

use geom::{Aabb, DMat4, DVec3};
use scene::POLE_TOLERANCE;
use wgpu::util::DeviceExt as _;

use crate::pipelines::{create_shadow_depth_pipeline, pipeline_layout};
use crate::{Drawn, SHADOW_FORMAT};

/// The size of one side of the shadow map, in texels.
///
/// One cabinet is a couple of metres across, so a 2048² map is about a millimetre and a half per
/// texel: fine enough that a shelf's edge reads as a straight line on the panel behind it, and small
/// enough that the whole map is 16 MB of depth.
pub(crate) const SHADOW_SIZE: u32 = 2048;

/// The smallest box radius the light camera accepts, in millimetres.
///
/// A scene that is one flat quad has a radius of about half its width, and an empty one has none; a
/// near-zero radius would make the projection degenerate instead of merely small.
const MIN_RADIUS_MM: f64 = 1.0;

/// The light's camera: where the sun stands over the scene's box, as one matrix.
///
/// The box is the union of every drawable's world box, so it holds both sides of a shadow: the
/// caster and the surface it darkens. The projection covers the sphere around that box, which is
/// square in the light's own axes whatever direction the sun comes from — the map's texel size
/// therefore does not change when the light turns, only when the scene does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LightSpace {
    /// World space to the light's clip space, depth in `0..1`.
    pub(crate) view_projection: DMat4,
    /// The world size of one texel, in millimetres: the normal offset is expressed in it.
    pub(crate) texel_mm: f64,
}

/// The world-to-view matrix of a camera at `eye` looking at `centre`, with `up` above it.
///
/// The same layout `scene::Camera::view_matrix` builds — the camera looks down its own `-Z`, so clip
/// depth runs towards the viewer — written here because `glam`'s own camera constructors sit behind
/// a feature this workspace does not enable.
fn look_from(eye: DVec3, centre: DVec3, up: DVec3) -> DMat4 {
    let forward = (centre - eye).try_normalize().unwrap_or(-DVec3::Z);
    let right = forward.cross(up).try_normalize().unwrap_or(DVec3::X);
    let up = right.cross(forward);
    DMat4::from_cols_array(&[
        right.x,
        up.x,
        -forward.x,
        0.0,
        right.y,
        up.y,
        -forward.y,
        0.0,
        right.z,
        up.z,
        -forward.z,
        0.0,
        -right.dot(eye),
        -up.dot(eye),
        forward.dot(eye),
        1.0,
    ])
}

/// The projection of a square box of half-size `radius` around the eye, depth in `0..1`.
///
/// The eye stands one radius in front of the box, so the near plane is zero and the far one is twice
/// the radius: every point of the box is in front of the light and inside its own map.
fn orthographic(radius: f64) -> DMat4 {
    let half = radius.max(1.0e-6);
    let (near, far) = (0.0, 2.0 * half);
    DMat4::from_cols_array(&[
        1.0 / half,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0 / half,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0 / (near - far),
        0.0,
        0.0,
        0.0,
        near / (near - far),
        1.0,
    ])
}

impl LightSpace {
    /// The light camera over `bounds`, or nothing when the box or the direction is unusable.
    pub(crate) fn of(bounds: Aabb, light: DVec3) -> Option<Self> {
        let light = light.try_normalize()?;
        let centre = bounds.center();
        let radius = bounds.radius().max(MIN_RADIUS_MM);
        if !centre.is_finite() || !radius.is_finite() || radius <= 0.0 {
            return None;
        }
        // A sun straight overhead has no horizon to be "up", exactly as a top view has none.
        let up = if light.x.abs() <= POLE_TOLERANCE && light.y.abs() <= POLE_TOLERANCE { DVec3::Y } else { DVec3::Z };
        let eye = centre + light * radius;
        let view = look_from(eye, centre, up);
        let projection = orthographic(radius);
        Some(Self { view_projection: projection * view, texel_mm: 2.0 * radius / f64::from(SHADOW_SIZE) })
    }
}

/// The frame the map in hand was drawn from.
///
/// The map is world-space, so the camera is deliberately absent: orbiting never redraws it. The
/// generation covers the geometry *and* the scene's box, because both come from the same upload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ShadowKey {
    generation: u64,
    light: [i32; 3],
}

impl ShadowKey {
    /// The key of one frame.
    pub(crate) fn of(generation: u64, light: DVec3) -> Self {
        Self { generation, light: crate::quantized_light(light) }
    }
}

/// The uniform one face shader reads: the light's matrix and the size of its texels.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ShadowUniform {
    view_projection: [[f32; 4]; 4],
    /// `x` the world size of one texel in millimetres, `y` one when the map holds the frame's scene.
    params: [f32; 4],
}

impl ShadowUniform {
    /// What the map holds before any scene was drawn: nothing, so every surface is lit.
    const DISABLED: Self = Self {
        view_projection: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
        params: [1.0, 0.0, 0.0, 0.0],
    };

    /// The uniform of one light camera, or the disabled one when the frame holds no scene.
    fn of(space: Option<LightSpace>) -> Self {
        match space {
            Some(space) => Self {
                view_projection: space.view_projection.as_mat4().to_cols_array_2d(),
                params: [space.texel_mm as f32, 1.0, 0.0, 0.0],
            },
            None => Self::DISABLED,
        }
    }
}

/// The shadow map, the pass that fills it and the groups a pass reads it through.
pub(crate) struct ShadowMap {
    pass: wgpu::RenderPipeline,
    _depth: wgpu::Texture,
    view: wgpu::TextureView,
    /// The light's matrix and the size of its texels, written when the map is rebuilt.
    uniform: wgpu::Buffer,
    /// The group the face pass reads: the map, its comparison sampler and the uniform.
    group: wgpu::BindGroup,
    /// The group the depth pass binds: the uniform alone, because a pass may not sample the depth
    /// attachment it is writing.
    pass_group: wgpu::BindGroup,
    built: Option<ShadowKey>,
    rebuilds: u64,
}

impl ShadowMap {
    /// The layout of the group the face pass reads.
    pub(crate) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Z3L shadow map layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<ShadowUniform>() as u64),
                    },
                    count: None,
                },
            ],
        })
    }

    /// The layout of the group the depth pass binds: the light's matrix and nothing else.
    fn pass_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Z3L shadow pass layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<ShadowUniform>() as u64),
                },
                count: None,
            }],
        })
    }

    /// Creates the depth texture, the sampler, the uniform and the one pass that fills them.
    ///
    /// `camera` is the view's own group: the depth pass carries it because the shader module
    /// declares the shadow resources once, at the group index the face pass uses, and a pass binds
    /// every group its pipeline layout names. The pass reads neither the camera nor the texture.
    pub(crate) fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        camera: &wgpu::BindGroupLayout,
        layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let pass_layout = Self::pass_layout(device);
        let size = wgpu::Extent3d { width: SHADOW_SIZE, height: SHADOW_SIZE, depth_or_array_layers: 1 };
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Z3L shadow map"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SHADOW_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Z3L shadow map sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            // Nearest, not linear: the face shader takes its own taps, so the penumbra is the
            // kernel it names on every backend instead of a comparison-filter capability.
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..wgpu::SamplerDescriptor::default()
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Z3L shadow map uniform"),
            contents: bytemuck::bytes_of(&ShadowUniform::DISABLED),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Z3L shadow map group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
            ],
        });
        let pass_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Z3L shadow pass group"),
            layout: &pass_layout,
            entries: &[wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() }],
        });
        let pass_pipeline_layout = pipeline_layout(device, "Z3L shadow map pass layout", &[camera, &pass_layout]);
        Self {
            pass: create_shadow_depth_pipeline(device, &pass_pipeline_layout, shader),
            _depth: depth,
            view,
            uniform,
            group,
            pass_group,
            built: None,
            rebuilds: 0,
        }
    }

    /// The group the face pass binds to read the map.
    pub(crate) const fn group(&self) -> &wgpu::BindGroup {
        &self.group
    }

    /// Whether the map in hand was drawn from another frame.
    pub(crate) fn is_stale(&self, key: &ShadowKey) -> bool {
        self.built.as_ref() != Some(key)
    }

    /// How many times the map was drawn, which is what a cost readout and a probe print.
    pub(crate) fn rebuilds(&self) -> u64 {
        self.rebuilds
    }

    /// Redraws the map when the scene, or the sun over it, changed.
    ///
    /// One depth-only pass over the world-space buffers the upload already built, so a rebuild
    /// costs no geometry and no second vertex layout. A frame with no scene clears the map instead:
    /// a cleared depth buffer reads as "the sun reaches this point" and costs no draw at all.
    pub(crate) fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &ShadowKey,
        light_space: Option<LightSpace>,
        camera: (&wgpu::BindGroup, u32),
        drawables: &[Drawn],
    ) {
        if self.built.as_ref() == Some(key) {
            return;
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&ShadowUniform::of(light_space)));
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Z3L shadow encoder") });
        {
            let attachment = Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Z3L shadow map"),
                color_attachments: &[],
                depth_stencil_attachment: attachment,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if light_space.is_some() && !drawables.is_empty() {
                pass.set_pipeline(&self.pass);
                pass.set_bind_group(0, camera.0, &[camera.1]);
                pass.set_bind_group(1, &self.pass_group, &[]);
                for drawn in drawables {
                    pass.set_vertex_buffer(0, drawn.faces.vertex_buffer.slice(..));
                    pass.set_index_buffer(drawn.faces.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..drawn.faces.index_count, 0, 0..1);
                }
            }
        }
        queue.submit(Some(encoder.finish()));
        self.built = Some(*key);
        self.rebuilds = self.rebuilds.saturating_add(1);
    }
}

/// The scene's box: the union of the drawables that came out of the last upload.
pub(crate) fn scene_box(boxes: impl IntoIterator<Item = Option<Aabb>>) -> Option<Aabb> {
    boxes.into_iter().flatten().reduce(Aabb::union)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_of(min: [f64; 3], max: [f64; 3]) -> Aabb {
        Aabb::of_points([DVec3::from(min), DVec3::from(max)]).expect("a box needs one point")
    }

    #[test]
    fn a_light_camera_covers_the_scene_it_was_given() {
        let bounds = box_of([-100.0, -50.0, 0.0], [100.0, 50.0, 400.0]);
        for light in [DVec3::new(-0.35, 0.55, 0.75), DVec3::Z, DVec3::new(1.0, 0.0, 0.0)] {
            let space = LightSpace::of(bounds, light).expect("a measured box has a light camera");
            // Every corner of the box lands inside the light's clip space, and in front of it.
            for corner in bounds.corners() {
                let clip = space.view_projection * corner.extend(1.0);
                assert!(clip.w > 0.0, "the scene is in front of the light");
                let ndc = clip.truncate() / clip.w;
                assert!(ndc.x >= -1.0 && ndc.x <= 1.0, "{ndc:?} is inside the map horizontally");
                assert!(ndc.y >= -1.0 && ndc.y <= 1.0, "{ndc:?} is inside the map vertically");
                assert!(ndc.z >= 0.0 && ndc.z <= 1.0, "{ndc:?} is inside the depth range");
            }
        }
    }

    #[test]
    fn a_turned_sun_keeps_the_texel_size_and_an_edited_scene_moves_it() {
        let bounds = box_of([-100.0, -50.0, 0.0], [100.0, 50.0, 400.0]);
        let overhead = LightSpace::of(bounds, DVec3::Z).expect("a light camera");
        let slanted = LightSpace::of(bounds, DVec3::new(0.3, 0.4, 0.87)).expect("a light camera");
        assert!((overhead.texel_mm - slanted.texel_mm).abs() < 1.0e-9, "the map is square in the light's axes");
        let wider = LightSpace::of(box_of([-1000.0, -50.0, 0.0], [1000.0, 50.0, 400.0]), DVec3::Z).expect("a box");
        assert!(wider.texel_mm > overhead.texel_mm, "a bigger scene spends more millimetres per texel");
    }

    #[test]
    fn a_flat_or_hostile_scene_still_has_a_finite_light_camera() {
        let flat = box_of([-500.0, -500.0, 0.0], [500.0, 500.0, 0.0]);
        let space = LightSpace::of(flat, DVec3::Z).expect("a flat scene still casts");
        assert!(space.texel_mm.is_finite() && space.texel_mm > 0.0);
        assert!(LightSpace::of(flat, DVec3::ZERO).is_none(), "a direction of no length is not a sun");
        // A box only ever comes from measured points, and a hostile corner is skipped there, so the
        // light camera meets a finite box of the remaining point instead of a degenerate one.
        let hostile = box_of([f64::INFINITY, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!(LightSpace::of(hostile, DVec3::Z).is_some());
    }

    #[test]
    fn the_scene_box_holds_every_drawable_and_no_unknown_one() {
        let a = box_of([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
        let b = box_of([-20.0, -20.0, -20.0], [-10.0, -10.0, -10.0]);
        let union = scene_box([Some(a), None, Some(b)]).expect("a scene with two boxes has a box");
        assert_eq!(union.min(), DVec3::new(-20.0, -20.0, -20.0));
        assert_eq!(union.max(), DVec3::new(10.0, 10.0, 10.0));
        assert!(scene_box(Vec::new()).is_none(), "an empty scene has no box");
        assert!(scene_box([None]).is_none(), "an unmeasured drawable is not an empty one");
    }

    #[test]
    fn a_map_is_stale_when_the_scene_or_the_light_moves_and_not_when_the_camera_does() {
        let light = DVec3::new(-0.35, 0.55, 0.75);
        let first = ShadowKey::of(1, light);
        assert_eq!(first, ShadowKey::of(1, light), "the same frame keeps its map");
        assert_ne!(first, ShadowKey::of(2, light), "an edit redraws it");
        assert_ne!(first, ShadowKey::of(1, DVec3::new(0.35, 0.55, 0.75)), "a turned sun redraws it");
        assert_eq!(first, ShadowKey::of(1, light * 1.00005), "a slider that jitters does not thrash");
    }
}
