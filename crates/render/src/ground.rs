//! The floor plate and the shadow the key light casts on it.
//!
//! The ground is two things the frame draws and one it does not. The *plate* is a quad on `z = 0`
//! across the square the grid draws: an unlit surface, so a shadow has something to darken. The
//! *mask* is the scene projected along the key light onto that plane — a coverage render, not a
//! depth map, because the receiver is a plane and the only question is whether any body stands
//! between the light and a point of it. A `coverage` render therefore has no bias, no acne and no
//! depth target at all. The *blur* turns the mask's hard silhouette into a penumbra of a fixed
//! world size, and the plate reads the result with one texture fetch.
//!
//! Contract: `Ground` owns the two mask textures, the sampler, the three pipelines and the plate's
//! buffer. It rebuilds the mask only when [`GroundKey`] changes — a new upload, a turned key light
//! or a new ground size — so orbit, pan, zoom and a hundred frames of nothing cost nothing. The
//! mask and the plate share one number, the half-size of the plate, so they cannot disagree about
//! where the ground is.
//!
//! Refuses: nothing at draw time. The mask's size is a constant, a frame with no drawable leaves
//! the mask cleared, and a plate with no extent draws no quad at all.
//!
//! Does not own frame data: the strength and the colour arrive in the camera uniform, which the
//! renderer writes per view, so a strength slider never re-uploads a buffer.
//!
//! Changes when: the mask's resolution, the penumbra, the projection or the plate's shape changes.

use geom::DVec3;
use wgpu::util::DeviceExt as _;

use crate::cache::Rebuilt;
use crate::geometry::{floor_indices, floor_vertices};
use crate::gpu::{GpuTriangles, triangle_list};
use crate::pipelines::{create_floor_pipeline, create_shadow_blur_pipeline, create_shadow_cover_pipeline};
use crate::{Drawn, RenderError};

/// The size of one side of the ground mask, in texels.
///
/// The mask spans the plate, so a 10 m ground is about a centimetre per texel: fine enough that a
/// cabinet's silhouette keeps its shape, and small enough that the whole mask is one megabyte.
pub(crate) const MASK_SIZE: u32 = 1024;

/// The half-width of the shadow's penumbra, in world millimetres.
///
/// A shadow that ends in a hard pixel reads as paint; this is the blur radius the two mask passes
/// aim for, expressed in the world so it does not change with the zoom.
pub(crate) const GROUND_PENUMBRA_MM: f64 = 60.0;

/// The plate's own height above the floor, in millimetres.
///
/// Zero: the plate *is* the floor. The grid lines are drawn a fraction of a millimetre above it, so
/// the lines lie on the ground instead of fighting it.
const PLATE_Z_MM: f32 = 0.0;

/// The ground the last mask was built for.
///
/// A mask is a picture of the scene from one light, so it is stale when any of these move. The
/// camera is deliberately absent: a mask is world-space, and orbiting never rebuilds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GroundKey {
    /// The upload generation of the scene: every upload builds a new mask.
    pub(crate) generation: u64,
    /// The key light's direction, quantised so a hundredth of a degree does not re-render it.
    pub(crate) light: [i32; 3],
    /// The plate's half-size in millimetres.
    pub(crate) extent_mm: f32,
}

impl GroundKey {
    /// The key of one frame.
    pub(crate) fn of(generation: u64, light: DVec3, extent_mm: f64) -> Self {
        let light = light.try_normalize().unwrap_or(DVec3::Z);
        Self {
            generation,
            light: [
                (light.x * 1.0e4).round() as i32,
                (light.y * 1.0e4).round() as i32,
                (light.z * 1.0e4).round() as i32,
            ],
            extent_mm: extent_mm as f32,
        }
    }
}

/// What the plate's vertex buffer was built from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlateKey {
    pub(crate) extent_mm: f32,
    pub(crate) color: [f32; 4],
}

/// The shadow's penumbra as the blur passes read it, in texels.
pub(crate) fn penumbra_texels(extent_mm: f64) -> f32 {
    if !extent_mm.is_finite() || extent_mm <= 0.0 {
        return 1.0;
    }
    let texel_mm = 2.0 * extent_mm / f64::from(MASK_SIZE);
    (GROUND_PENUMBRA_MM / texel_mm).clamp(1.0, 16.0) as f32
}

/// The mask textures, the three pipelines and the plate, in one value.
pub(crate) struct Ground {
    cover: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
    plate_pipeline: wgpu::RenderPipeline,
    /// The coverage and its blurred copy; the two blur passes ping-pong between them.
    /// The two mask textures; the views below borrow them, and a texture outlives its view here.
    _textures: [wgpu::Texture; 2],
    views: [wgpu::TextureView; 2],
    /// One bind group per reader: the plate's, the horizontal blur's and the vertical blur's.
    groups: [wgpu::BindGroup; 3],
    /// The two blur steps: `xy` is one texel along the axis, `z` the penumbra in texels.
    directions: [wgpu::Buffer; 2],
    /// The plate's two triangles, rebuilt only when the ground's size or colour changes.
    plate: Rebuilt<GpuTriangles, PlateKey>,
    built: Option<GroundKey>,
}

/// The three bind groups the ground reads: the plate and the two blur passes.
const PLATE_GROUP: usize = 0;
const BLUR_H_GROUP: usize = 1;
const BLUR_V_GROUP: usize = 2;

impl Ground {
    /// Creates the mask textures, the sampler, the bind groups and the pipelines.
    ///
    /// The blur's two direction uniforms are constants of the mask's size and the penumbra, so they
    /// are written once here rather than per frame.
    pub(crate) fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        camera_layout: &wgpu::BindGroupLayout,
        texture_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let size = wgpu::Extent3d { width: MASK_SIZE, height: MASK_SIZE, depth_or_array_layers: 1 };
        let textures = [0, 1].map(|index| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(if index == 0 { "Z3L ground mask" } else { "Z3L ground mask blur" }),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: crate::MASK_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        });
        let views = [
            textures[0].create_view(&wgpu::TextureViewDescriptor::default()),
            textures[1].create_view(&wgpu::TextureViewDescriptor::default()),
        ];
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Z3L ground mask sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        let directions = [0, 1].map(|index| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(if index == 0 { "Z3L ground blur horizontal" } else { "Z3L ground blur vertical" }),
                contents: bytemuck::bytes_of(&[0.0f32; 4]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            })
        });
        let group = |label: &str, view: &wgpu::TextureView, direction: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: texture_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: direction.as_entire_binding() },
                ],
            })
        };
        let groups = [
            group("Z3L ground plate", &views[0], &directions[0]),
            group("Z3L ground blur horizontal", &views[0], &directions[1]),
            group("Z3L ground blur vertical", &views[1], &directions[0]),
        ];
        let camera_only = pipeline_layout(device, "Z3L ground coverage layout", &[camera_layout]);
        let with_mask = pipeline_layout(device, "Z3L ground plate layout", &[camera_layout, texture_layout]);
        Self {
            cover: create_shadow_cover_pipeline(device, &camera_only, shader),
            blur: create_shadow_blur_pipeline(device, &with_mask, shader),
            plate_pipeline: create_floor_pipeline(device, &with_mask, shader),
            _textures: textures,
            views,
            groups,
            directions,
            plate: Rebuilt::new(),
            built: None,
        }
    }

    /// The plate's pipeline, bound by the view pass that draws the ground.
    pub(crate) const fn plate_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.plate_pipeline
    }

    /// The plate's bind group, which holds the mask the plate reads.
    pub(crate) const fn plate_group(&self) -> &wgpu::BindGroup {
        &self.groups[PLATE_GROUP]
    }

    /// The plate's buffer as the last upload left it, or nothing when the ground drew no quad.
    pub(crate) fn plate_value(&self) -> Option<&GpuTriangles> {
        self.plate.value().map(|plate| plate.as_ref())
    }

    /// The plate's triangle buffer, or nothing when the ground has no area.
    pub(crate) fn plate(
        &mut self,
        device: &wgpu::Device,
        key: PlateKey,
    ) -> Result<Option<std::sync::Arc<GpuTriangles>>, RenderError> {
        self.plate.get(&key, || {
            let vertices = floor_vertices(f64::from(key.extent_mm), key.color, PLATE_Z_MM);
            if vertices.is_empty() {
                return Ok(None);
            }
            Ok(triangle_list(device, "Z3L floor plate", &vertices, &floor_indices()))
        })
    }

    /// Whether the mask the ground holds was built for another frame.
    pub(crate) fn is_stale(&self, key: &GroundKey) -> bool {
        self.built.as_ref() != Some(key)
    }

    /// Rebuilds the mask when the scene, the light or the ground changed its size.
    ///
    /// Three passes on their own submission: the scene projected onto the plane, then one blur
    /// along each axis. A rebuild is a consequence of an edit or a turned light, never of a frame.
    pub(crate) fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &GroundKey,
        camera: (&wgpu::BindGroup, u32),
        drawables: &[Drawn],
    ) {
        if self.built.as_ref() == Some(key) {
            return;
        }
        // The penumbra is a world size, so its length in texels follows the ground's own size.
        let texel = 1.0 / MASK_SIZE as f32;
        let radius = penumbra_texels(f64::from(key.extent_mm));
        queue.write_buffer(&self.directions[0], 0, bytemuck::bytes_of(&[texel, 0.0, radius, 0.0f32]));
        queue.write_buffer(&self.directions[1], 0, bytemuck::bytes_of(&[0.0, texel, radius, 0.0f32]));
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Z3L ground encoder") });
        {
            let attachment = Some(wgpu::RenderPassColorAttachment {
                view: &self.views[0],
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Z3L ground coverage"),
                color_attachments: &[attachment],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.cover);
            pass.set_bind_group(0, camera.0, &[camera.1]);
            for drawn in drawables {
                pass.set_vertex_buffer(0, drawn.faces.vertex_buffer.slice(..));
                pass.set_index_buffer(drawn.faces.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..drawn.faces.index_count, 0, 0..1);
            }
        }
        // Each pass reads the other texture, so the mask is never sampled and written at once.
        for (destination, group) in [(1usize, BLUR_H_GROUP), (0, BLUR_V_GROUP)] {
            let attachment = Some(wgpu::RenderPassColorAttachment {
                view: &self.views[destination],
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Z3L ground blur"),
                color_attachments: &[attachment],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.blur);
            pass.set_bind_group(0, camera.0, &[camera.1]);
            pass.set_bind_group(1, &self.groups[group], &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        self.built = Some(*key);
    }
}

/// One pipeline layout: the camera first, then whatever else the pass reads.
fn pipeline_layout(device: &wgpu::Device, label: &str, layouts: &[&wgpu::BindGroupLayout]) -> wgpu::PipelineLayout {
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &layouts.iter().map(|layout| Some(*layout)).collect::<Vec<_>>(),
        immediate_size: 0,
    })
}
