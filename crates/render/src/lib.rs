//! The wgpu renderer.
//!
//! Turns one scene frame into pixels: it owns the offscreen colour target, the depth and stencil
//! attachment, the uploaded geometry and the passes that draw it. The pipeline state lives in
//! `pipelines.rs`, the buffer wrappers in `gpu.rs`, the vertex builders in `geometry.rs`, the floor
//! plate with its ground mask in `ground.rs` and the key light's shadow map in `shadow.rs`. It
//! knows nothing about `egui` and never
//! creates a window: the shell registers the colour texture with `egui_wgpu` and displays it as an
//! image. Scene geometry is uploaded when the document changes; camera changes only rewrite a small
//! uniform buffer.
//!
//! Layer: L5, above `scene` (which says what to draw) and below `ui` (which never links wgpu).
//!
//! Refuses: malformed geometry is a [`RenderError`] the shell can report, never a panic and never a
//! silently missing object; a hostile target size is clamped, not trusted.
//!
//! Does not build a single vertex: `geometry.rs` flattens what `scene` hands over, and a pass here
//! draws the buffers that came back. It does not decide which frame to draw, and it holds no
//! document data.
//!
//! Changes when: what a frame uploads, how a pass draws it, or the order the passes run in changes.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod cache;
mod geometry;
mod gpu;
mod ground;
mod pipelines;
mod shadow;

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use geom::{Aabb, DMat4, DVec3, MeshData};
use scene::{
    Camera, ContactShadows, Decor, DecorId, Frustum, Grid, Lighting, MAX_POINT_LIGHTS, Polyline, ProductEnvelope,
    SceneFrame, ShadingStyle,
};
use thiserror::Error;
use wgpu::util::DeviceExt as _;

use self::cache::{EdgeKey, FaceKey, Rebuilt, SlotCache};
use self::geometry::{
    edge_ink, edge_vertices, faces_centre, grid_vertices, polyline_vertices, product_envelope_vertices, sanitize_color,
    sort_back_to_front, stencil_slot, world_vertices,
};
use self::gpu::{DecorGroup, GpuEdges, GpuLineList, GpuMesh, clamp_size, line_list};
use self::ground::{Ground, GroundKey, PlateKey};
use self::pipelines::{
    create_line_pipeline, create_triangle_pipeline, edge_depth_state, line_depth_state, pipeline_layout,
};
use self::shadow::{LightSpace, ShadowKey, ShadowMap, scene_box};

/// The offscreen colour format. `egui_wgpu::Renderer::register_native_texture` requires this
/// exact format, and it is not an sRGB format, so the shader writes gamma-corrected values.
pub(crate) const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// The depth and stencil attachment of the offscreen target.
///
/// Depth chooses between the surfaces the frame draws. The stencil value records which drawable owns
/// each pixel, which is what keeps the *hidden edges* style honest: a line shows through its own
/// object and never through the object in front of it.
pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;
/// The ground mask's single-channel format: coverage, so one byte per texel is all it holds.
pub(crate) const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
/// The shadow map's format: depth, because a panel needs the nearest surface along every light ray.
///
/// `Depth32Float` is the one depth format every backend can both render to and compare against; the
/// comparison sampler reads it as `texture_depth_2d`, and the map is 16 MB.
pub(crate) const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The most views one frame may split into; the camera uniform holds one slot per view.
pub const MAX_VIEWS: usize = 4;

/// The camera slot the ground mask reads, one past the last view.
const GROUND_SLOT: u64 = MAX_VIEWS as u64;

/// The key light's direction as the frame keys it, quantised so a slider does not rebuild a mask.
///
/// The ground's coverage mask and the shadow map both name the sun this way: one direction means one
/// quantisation, so the floor and the panels can never disagree about where the light is. A tenth of
/// a thousandth of a unit is far below what a turned sun changes on screen.
pub(crate) fn quantized_light(direction: DVec3) -> [i32; 3] {
    let light = direction.try_normalize().unwrap_or(DVec3::Z);
    [(light.x * 1.0e4).round() as i32, (light.y * 1.0e4).round() as i32, (light.z * 1.0e4).round() as i32]
}

/// The passes one display style turns on, derived without a GPU.
///
/// A style promises passes, not pipeline objects: the renderer turns this into the four pipelines it
/// built, and a test can assert the promise without a device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StylePasses {
    /// Filled faces drawn with a depth write.
    pub opaque_faces: bool,
    /// Filled faces blended with no depth write.
    pub translucent_faces: bool,
    /// Edges tested against the faces.
    pub visible_edges: bool,
    /// Edges drawn through the faces.
    pub through_edges: bool,
}

impl StylePasses {
    /// The passes one style promises.
    pub const fn new(style: ShadingStyle) -> Self {
        Self {
            opaque_faces: style.draws_faces() && !style.is_translucent(),
            translucent_faces: style.is_translucent(),
            visible_edges: style.draws_edges() && !style.edges_through_faces(),
            through_edges: style.draws_edges() && style.edges_through_faces(),
        }
    }
}

/// A reason why scene geometry could not be uploaded.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RenderError {
    /// A drawable has no triangles.
    #[error("drawable {index} has an empty mesh")]
    EmptyMesh {
        /// Position of the drawable in the frame.
        index: usize,
    },
    /// A drawable has malformed vertices, normals or indices.
    #[error("drawable {index} has an invalid mesh")]
    InvalidMesh {
        /// Position of the drawable in the frame.
        index: usize,
    },
    /// A drawable exceeds the renderer's vertex budget.
    #[error("drawable {index} has too many vertices")]
    MeshTooLarge {
        /// Position of the drawable in the frame.
        index: usize,
    },
    /// A polyline has fewer than two points, so it has no segment to draw.
    #[error("polyline {index} has fewer than two points")]
    EmptyPolyline {
        /// Position of the polyline in the frame.
        index: usize,
    },
    /// A polyline has malformed points or a non-finite transform.
    #[error("polyline {index} has invalid points")]
    InvalidPolyline {
        /// Position of the polyline in the frame.
        index: usize,
    },
    /// A polyline exceeds the renderer's vertex budget.
    #[error("polyline {index} has too many points")]
    PolylineTooLarge {
        /// Position of the polyline in the frame.
        index: usize,
    },
    /// A drawable's edge list needs more line vertices than one buffer holds.
    #[error("drawable {index} has more edges than one buffer holds")]
    EdgeListTooLarge {
        /// Position of the drawable in the frame.
        index: usize,
    },
    /// A product envelope has a non-finite or non-positive size, or a malformed transform.
    #[error("product envelope {index} is malformed")]
    InvalidProductEnvelope {
        /// Position of the envelope in the frame.
        index: usize,
    },
    /// A product envelope's dashes exceed the renderer's vertex budget.
    #[error("product envelope {index} has too many dashes")]
    ProductEnvelopeTooLarge {
        /// Position of the envelope in the frame.
        index: usize,
    },
    /// The colour target could not be copied back to the host.
    ///
    /// A device that cannot map a staging buffer, or a copy it rejects, is a sentence the caller
    /// reports; the alternative is a picture nobody can check.
    #[error("the coloured target could not be read back: {reason}")]
    ReadBack {
        /// What the device said.
        reason: String,
    },
}

/// Uniform data shared by the mesh and grid pipelines.
///
/// One slot describes one view: the view-projection, the eye, and the lighting that view shades
/// with. Splitting the workspace into several views therefore adds slots to one buffer instead of a
/// second camera buffer per view.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct CameraUniform {
    view_projection: [[f32; 4]; 4],
    camera_position: [f32; 4],
    light_direction: [f32; 4],
    light_color: [f32; 4],
    point_count: [u32; 4],
    point_position: [[f32; 4]; MAX_POINT_LIGHTS],
    /// Linear RGB and, in `a`, the radiant power in watts.
    point_color: [[f32; 4]; MAX_POINT_LIGHTS],
    /// Gamma-space RGB and, in `a`, the shadow strength: the ground's plate and the faces' occlusion.
    shadow: [f32; 4],
    /// `x` the floor plate's half-size in millimetres, `y` and `z` one mask texel, `w` the penumbra
    /// in texels. The plate, the mask and the blur read this one value.
    shadow_plane: [f32; 4],
    /// `x` how opaque the floor plate is, from 0 to 1; the rest is unused and keeps the slot aligned.
    ///
    /// The plate's alpha comes from here, because a change is read at draw time: a drag of the
    /// slider costs one uniform write and no upload of any buffer (U109).
    floor_opacity: [f32; 4],
}

struct RenderTargets {
    color_texture: wgpu::Texture,
    color_view: wgpu::TextureView,
    _depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

impl RenderTargets {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        // `COPY_SRC` is what lets the picture be copied back after the pass: the agent's preview
        // writes one PNG from it (J10) and an offscreen test reads it. The usage costs nothing
        // while nothing copies the target.
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Z3L viewport colour"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Z3L viewport depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        Self { color_texture: color, color_view, _depth_texture: depth, depth_view }
    }
}

/// One view's rectangle inside the shared offscreen target, in physical pixels.
///
/// `x` and `y` are the top-left corner counted from the target's top-left, which is also the
/// framebuffer origin wgpu uses: a scissor and a viewport share it, so no axis is flipped. Every
/// view is drawn with its own scissor into the one texture, so several views share one egui texture
/// id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewRegion {
    /// Left edge, in pixels.
    pub x: u32,
    /// Top edge, in pixels.
    pub y: u32,
    /// Width, in pixels; at least one.
    pub width: u32,
    /// Height, in pixels; at least one.
    pub height: u32,
}

impl ViewRegion {
    /// The wgpu scissor and viewport rectangle for this region.
    ///
    /// The rectangle is clamped to the target, so a rounding edge at the workspace boundary can
    /// never ask for an out-of-range scissor, which wgpu would reject.
    fn scissor(self, target_width: u32, target_height: u32) -> (u32, u32, u32, u32) {
        let x = self.x.min(target_width.saturating_sub(1));
        let y = self.y.min(target_height.saturating_sub(1));
        let width = self.width.min(target_width.saturating_sub(x)).max(1);
        let height = self.height.min(target_height.saturating_sub(y)).max(1);
        (x, y, width, height)
    }
}

/// One view's camera, rectangle, display style and ground shadow.
///
/// The style travels per view, so two neighbours in one workspace can draw the same scene with
/// different passes while one texture and one edge buffer still serve them (ADR 0070). The shadow
/// travels with it: its strength and colour are the workspace's, its switch is this view's, so one
/// cell of a split may darken the floor while its neighbour does not (U107).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewSpec {
    /// The camera this view looks through.
    pub camera: Camera,
    /// The rectangle it owns in the shared target, in physical pixels.
    pub region: ViewRegion,
    /// The display style it draws with.
    pub shading: ShadingStyle,
    /// The ground shadow it draws with: the workspace's colour and strength under this view's
    /// switch. A change is read when the uniform is written, so it never costs an upload.
    pub shadows: ContactShadows,
}

impl ViewSpec {
    /// The passes this view's style turns on.
    ///
    /// The renderer asks each view for its own passes inside the pass loop; a test can assert the
    /// mapping without a device.
    pub const fn passes(self) -> StylePasses {
        StylePasses::new(self.shading)
    }
}

/// The product envelopes and the two inks their dashes are drawn with.
type EnvelopeInput = (Vec<ProductEnvelope>, [f32; 4], [f32; 4]);

/// One drawable of the last upload: its buffers, and the stencil value its faces stamp.
///
/// The buffers may be shared with an earlier frame, because they are keyed by the mesh, the
/// transform and the colour that built them (X57). The stencil slot is not shared: it is the
/// drawable's place in this frame, and a drawable that moved down the list moves its slot. The box
/// is the drawable's world extent, read from the frame's own value, so a view decides whether to
/// draw it before it names a buffer (X58).
pub(crate) struct Drawn {
    pub(crate) faces: Arc<GpuMesh>,
    edges: Option<Arc<GpuEdges>>,
    /// The picture the face pass binds for this drawable: its own decor, or the white texel.
    decor: Arc<DecorGroup>,
    pub(crate) stencil_reference: u32,
    /// World-space box, or `None` when the frame never measured the drawable's extent.
    bounds: Option<Aabb>,
}

/// An offscreen scene renderer for one viewport.
///
/// The shell owns the window and the egui integration; this type owns the GPU scene resources and
/// exposes the colour view that egui should display.
pub struct ViewportRenderer {
    mesh_pipeline: wgpu::RenderPipeline,
    translucent_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    edge_pipeline: wgpu::RenderPipeline,
    hidden_edge_pipeline: wgpu::RenderPipeline,
    /// The floor plate, the shadow mask the key light casts on it and the blur between them.
    ground: Ground,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    /// The stride between two camera slots, which is the device's uniform offset alignment.
    camera_stride: u64,
    /// The drawables of the last upload, in frame order.
    drawables: Vec<Drawn>,
    /// World-space centres parallel to `drawables`, in upload order.
    centres: Vec<DVec3>,
    /// A reusable index list, sorted back to front for each translucent view.
    draw_order: Vec<usize>,
    /// A reusable index list: the drawables the last view drew, in frame order (X58).
    ///
    /// Every pass of one view reads it — faces, the translucent order and both edge passes — so a
    /// culled drawable costs nothing at all instead of two buffer binds and a draw call.
    visible: Vec<usize>,
    /// Face buffers kept between uploads, keyed by the mesh, transform, colour and decor they hold.
    faces: SlotCache<FaceKey, GpuMesh>,
    /// The layout the face pipelines bind their decor in, and the rule every decor is read with.
    decor_layout: wgpu::BindGroupLayout,
    decor_sampler: wgpu::Sampler,
    /// The decors of the last upload, one device picture per id instead of one per drawable.
    decors: SlotCache<DecorId, DecorGroup>,
    /// The white texel a drawable with no decor is bound to, so the fragment needs no branch.
    ///
    /// Built by the first upload, because a renderer that never uploads a frame has no picture to
    /// bind and no reason to own a texture.
    white_decor: Option<Arc<DecorGroup>>,
    /// Edge buffers kept between uploads, keyed by the edge list, transform and ink they hold.
    edge_slots: SlotCache<EdgeKey, GpuEdges>,
    /// The grid's lines, rebuilt only when the grid changes.
    grid: Rebuilt<GpuLineList, Grid>,
    /// The frame's line chains, rebuilt only when the chains or the ink change.
    polylines: Rebuilt<GpuLineList, (Vec<Polyline>, [f32; 4])>,
    /// The dashed boxes of the product envelopes, rebuilt only when a box or an ink changes.
    product_envelopes: Rebuilt<GpuLineList, EnvelopeInput>,
    /// The key light's shadow map: the depth pass the face shaders compare against (ADR 0143).
    shadow: ShadowMap,
    /// The box around the drawables of the last upload, which is what the light camera covers.
    scene_bounds: Option<Aabb>,
    /// How many uploads the renderer has taken, which is what makes a mask stale.
    generation: u64,
    targets: RenderTargets,
    size: [u32; 2],
}

impl ViewportRenderer {
    /// Creates a renderer with an offscreen target of the given physical size.
    ///
    /// The size is clamped to at least one pixel and to the device's maximum texture dimension.
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let size = clamp_size(width, height, device.limits().max_texture_dimension_2d);
        let targets = RenderTargets::new(device, size[0], size[1]);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Z3L scene shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });

        let uniform_size = std::mem::size_of::<CameraUniform>() as u64;
        let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(1);
        let camera_stride = uniform_size.next_multiple_of(alignment);
        // One slot past the views is the ground's: the mask is built before any view and needs the
        // light and the plate's size, which are the first two fields of the same uniform.
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Z3L camera uniform"),
            size: camera_stride * (MAX_VIEWS as u64 + 1),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Z3L camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(uniform_size),
                },
                count: None,
            }],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Z3L camera bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &camera_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(uniform_size),
                }),
            }],
        });
        let camera_only = pipeline_layout(device, "Z3L scene pipeline layout", &[&bind_group_layout]);
        // The frame's shadow map, second because the face shader reads it beside the camera: one
        // depth pass over the scene, then a comparison in every face fragment (ADR 0143).
        let shadow_layout = ShadowMap::layout(device);
        let shadow = ShadowMap::new(device, &shader, &bind_group_layout, &shadow_layout);
        // The decor of the face being drawn is the third group: a panel is a board the shop buys, and
        // its picture is a texture the shell uploads once per decor rather than once per panel.
        let decor_layout = DecorGroup::layout(device);
        let decor_sampler = DecorGroup::sampler(device);
        let mesh_layout =
            pipeline_layout(device, "Z3L shaded pipeline layout", &[&bind_group_layout, &shadow_layout, &decor_layout]);

        let mesh_pipeline = create_triangle_pipeline(
            device,
            &mesh_layout,
            &shader,
            "Z3L shaded mesh pipeline",
            "fs_mesh",
            ShadingStyle::Shaded,
        );
        let translucent_pipeline = create_triangle_pipeline(
            device,
            &mesh_layout,
            &shader,
            "Z3L translucent mesh pipeline",
            "fs_mesh_translucent",
            ShadingStyle::Translucent,
        );
        let line_pipeline =
            create_line_pipeline(device, &camera_only, &shader, "Z3L line pipeline", line_depth_state());
        let edge_pipeline =
            create_line_pipeline(device, &camera_only, &shader, "Z3L edge pipeline", edge_depth_state(false));
        let hidden_edge_pipeline =
            create_line_pipeline(device, &camera_only, &shader, "Z3L hidden edge pipeline", edge_depth_state(true));
        // The ground's own group: the mask it reads and the two blur steps it takes.
        let mask_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Z3L ground mask layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
            ],
        });
        let ground = Ground::new(device, &shader, &bind_group_layout, &shadow_layout, &mask_layout);

        Self {
            mesh_pipeline,
            translucent_pipeline,
            line_pipeline,
            edge_pipeline,
            hidden_edge_pipeline,
            shadow,
            ground,
            camera_buffer,
            camera_bind_group,
            camera_stride,
            drawables: Vec::new(),
            centres: Vec::new(),
            draw_order: Vec::new(),
            visible: Vec::new(),
            faces: SlotCache::new(),
            decor_layout: decor_layout.clone(),
            decor_sampler,
            decors: SlotCache::new(),
            white_decor: None,
            edge_slots: SlotCache::new(),
            grid: Rebuilt::new(),
            polylines: Rebuilt::new(),
            product_envelopes: Rebuilt::new(),
            scene_bounds: None,
            generation: 0,
            targets,
            size,
        }
    }

    /// The current colour target's size in physical pixels.
    pub const fn size(&self) -> [u32; 2] {
        self.size
    }

    /// How many drawables the last view drew, after the view volume dropped the rest (X58).
    ///
    /// The passes walk the list themselves; this is the count a test asserts and a cost readout
    /// prints, so a dense scene can say what one frame actually pays for.
    pub fn drawn_drawables(&self) -> usize {
        self.visible.len()
    }

    /// How many times the key light's shadow map was drawn since the renderer was created.
    ///
    /// An upload and a turned sun redraw it; an orbiting camera does not, which is the promise a
    /// probe reads here instead of counting passes on a GPU timeline (ADR 0143).
    pub fn shadow_rebuilds(&self) -> u64 {
        self.shadow.rebuilds()
    }

    /// The view egui should sample.
    pub const fn color_view(&self) -> &wgpu::TextureView {
        &self.targets.color_view
    }

    /// Copies the colour target back as RGBA8 rows, top row first, without the copy's padding.
    ///
    /// The rows of a texture copy are aligned to `wgpu::COPY_BYTES_PER_ROW_ALIGNMENT`, so a picture
    /// whose width is not a multiple of 64 pixels arrives padded; stripping that padding is this
    /// crate's job because this crate owns the target's format. The bytes are what the shader wrote
    /// — already gamma-corrected — so the caller encodes them and applies no curve.
    ///
    /// A target the device cannot map, or a copy it rejects, is a [`RenderError::ReadBack`].
    pub fn read_color(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Vec<u8>, RenderError> {
        let [width, height] = self.size;
        let row = (width as usize).saturating_mul(4);
        let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
        let padded = row.div_ceil(alignment).saturating_mul(alignment);
        let bytes = (padded as u64).saturating_mul(u64::from(height));
        if bytes == 0 || bytes > device.limits().max_buffer_size {
            return Err(RenderError::ReadBack {
                reason: format!("a {width} x {height} picture does not fit a buffer"),
            });
        }
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Z3L viewport read-back"),
            size: bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Z3L viewport read-back") });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.targets.color_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded as u32),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        queue.submit([encoder.finish()]);
        let slice = staging.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        device.poll(wgpu::PollType::wait_indefinitely()).map_err(|error| RenderError::ReadBack {
            reason: format!("the picture could not be read back: {error}"),
        })?;
        receiver
            .recv()
            .map_err(|_| RenderError::ReadBack { reason: "the read-back never answered".to_owned() })?
            .map_err(|error| RenderError::ReadBack { reason: format!("the picture could not be mapped: {error}") })?;
        let mut pixels = Vec::with_capacity(row.saturating_mul(height as usize));
        {
            let mapped = slice
                .get_mapped_range()
                .map_err(|error| RenderError::ReadBack { reason: format!("the picture is not mapped: {error}") })?;
            for line in mapped.chunks(padded.max(1)).take(height as usize) {
                let Some(line) = line.get(..row) else {
                    return Err(RenderError::ReadBack { reason: "a row was shorter than its picture".to_owned() });
                };
                pixels.extend_from_slice(line);
            }
        }
        staging.unmap();
        Ok(pixels)
    }

    /// Recreates the colour and depth targets when their size changes.
    ///
    /// Returns `true` when the target was recreated and the shell must re-register the colour view
    /// with `egui_wgpu`.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) -> bool {
        let size = clamp_size(width, height, device.limits().max_texture_dimension_2d);
        if size == self.size {
            return false;
        }
        self.targets = RenderTargets::new(device, size[0], size[1]);
        self.size = size;
        true
    }

    /// Rebuilds the GPU geometry for a scene frame.
    ///
    /// Call this only when the document revision changes; camera movement does not need an upload.
    ///
    /// A drawable whose mesh, transform and colour are the same as the last upload keeps its
    /// buffers, so an edit costs the drawables it changed and the walk of the frame, not the
    /// scene. A frame that no longer names a drawable drops its buffers (X57).
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &SceneFrame,
    ) -> Result<(), RenderError> {
        let ink = edge_ink(frame.edge_strength);
        self.faces.begin();
        self.edge_slots.begin();
        self.decors.begin();
        let decor_layout = self.decor_layout.clone();
        let decor_sampler = self.decor_sampler.clone();
        let white = Arc::clone(
            self.white_decor
                .get_or_insert_with(|| Arc::new(DecorGroup::white(device, queue, &decor_layout, &decor_sampler))),
        );
        let mut drawables = Vec::with_capacity(frame.drawables().len());
        for (index, drawable) in frame.drawables().iter().enumerate() {
            // One device picture per decor, however many panels are cut from it: the id is the
            // record's, and two drawables of one board bind one texture.
            let decor = match drawable.decor.as_ref() {
                Some(decor) => self
                    .decors
                    .slot(decor.id(), || Ok(DecorGroup::of(device, queue, &decor_layout, &decor_sampler, decor)))?,
                None => Arc::clone(&white),
            };
            let faces = self.faces.slot(
                FaceKey::new(&drawable.mesh, drawable.world_transform, drawable.color, drawable.decor.as_ref()),
                || {
                    build_mesh(
                        device,
                        index,
                        &drawable.mesh,
                        drawable.world_transform,
                        drawable.color,
                        drawable.decor.as_ref(),
                    )
                },
            )?;
            // A drawable has no line pass when the list the current style reads is absent; the
            // other list stays out of the key, so switching the diagnostic style builds the
            // lines it needs and sweeps the ones it left.
            let list = if frame.all_edges { drawable.all_edges.as_ref() } else { drawable.edges.as_ref() };
            let edges = match list {
                Some(list) => {
                    let key = EdgeKey::new(&drawable.mesh, list, drawable.world_transform, ink);
                    // The key owns the two `Arc`s the builders read, so the closure never holds a
                    // borrow of the cache it is passed to.
                    let mesh = Arc::clone(key.mesh());
                    let list = Arc::clone(key.edges());
                    Some(self.edge_slots.slot(key, || {
                        let vertices = edge_vertices(index, &mesh, &list, drawable.world_transform, ink)?;
                        GpuEdges::new(device, "Z3L drawable edges", vertices)
                    })?)
                }
                None => None,
            };
            drawables.push(Drawn {
                faces,
                edges,
                decor,
                stencil_reference: stencil_slot(index),
                bounds: drawable.world_bounds(),
            });
        }
        self.faces.sweep();
        self.decors.sweep();
        self.edge_slots.sweep();

        // A frame-level group is rebuilt only when its own input changed, so an edit that moves one
        // panel leaves the grid, the chains, the envelopes and the plate exactly as they were. The
        // value itself is kept by the group; the draw pass reads it from there.
        self.grid.get(&frame.grid, || line_list(device, "Z3L grid vertices", grid_vertices(&frame.grid)))?;
        self.polylines.get(&(frame.polylines().to_vec(), frame.line_ink), || {
            line_list(device, "Z3L path vertices", polyline_vertices(frame.polylines(), frame.line_ink)?)
        })?;
        let envelopes: EnvelopeInput =
            (frame.product_envelopes().to_vec(), frame.product_envelope_active, frame.product_envelope_inactive);
        self.product_envelopes.get(&envelopes, || {
            line_list(
                device,
                "Z3L product envelope vertices",
                product_envelope_vertices(
                    frame.product_envelopes(),
                    frame.product_envelope_active,
                    frame.product_envelope_inactive,
                )?,
            )
        })?;
        self.ground.plate(device, PlateKey { extent_mm: frame.grid.extent_mm as f32, color: frame.floor })?;
        self.generation = self.generation.wrapping_add(1);

        self.scene_bounds = scene_box(drawables.iter().map(|drawn| drawn.bounds));
        self.centres.clear();
        self.centres.extend(drawables.iter().map(|drawn| drawn.faces.centre));
        self.draw_order.clear();
        self.draw_order.extend(0..drawables.len());
        self.drawables = drawables;
        Ok(())
    }

    /// Draws the frame into the offscreen colour target.
    ///
    /// The camera and lighting are read from the frame, so orbit, pan and zoom cost one uniform
    /// write rather than a geometry upload.
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &SceneFrame) {
        self.render_with_lighting(device, queue, frame, &frame.lighting);
    }

    /// Draws the frame into the whole target with lighting that may differ from the uploaded scene.
    ///
    /// A drag preview commits nothing, but the uniform can still follow a moved light: a sun
    /// direction or a point-light position updates here without touching geometry buffers.
    pub fn render_with_lighting(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &SceneFrame,
        lighting: &Lighting,
    ) {
        let region = ViewRegion { x: 0, y: 0, width: self.size[0], height: self.size[1] };
        let view = ViewSpec { camera: frame.camera, region, shading: frame.shading, shadows: frame.contact_shadows };
        self.render_views(device, queue, frame, lighting, &[view]);
    }

    /// Draws the frame once per view into one target, each pass scissored to its own rectangle.
    ///
    /// One texture and one egui texture id serve the whole workspace, so splitting the viewport
    /// costs a pass and a uniform slot rather than a second texture. The passes share one cleared
    /// attachment: the first clears the colour and depth, every other pass loads them, and each
    /// draws inside its own viewport and scissor so a view never writes over its neighbour.
    ///
    /// Each view carries its own camera, rectangle, display style and shadow switch, so a split
    /// workspace draws a neighbour with a different pass set and a different ground shadow without a
    /// second target or edge buffer. The two masks — the ground's coverage and the key light's depth
    /// map — are built once, from the frame's own scene and light: they are geometry, and only the
    /// strength the face and the plate read is per view.
    ///
    /// Each view also culls: a drawable whose world box misses the view's volume is left out of
    /// every pass of that view, so faces and edges are dropped together (X58).
    ///
    /// At most [`MAX_VIEWS`] views are drawn; a longer slice is truncated. An empty slice clears the
    /// target and draws nothing.
    pub fn render_views(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &SceneFrame,
        lighting: &Lighting,
        views: &[ViewSpec],
    ) {
        let background = sanitize_color(frame.background);
        let clear = wgpu::Color {
            r: f64::from(background[0]),
            g: f64::from(background[1]),
            b: f64::from(background[2]),
            a: f64::from(background[3]),
        };

        let views = &views[..views.len().min(MAX_VIEWS)];
        let mut offsets = Vec::with_capacity(views.len());
        let mut uniforms = Vec::with_capacity(views.len());
        for (index, view) in views.iter().enumerate() {
            let region = view.region;
            let aspect = f64::from(region.width.max(1)) / f64::from(region.height.max(1));
            uniforms.push(camera_uniform(
                view.camera,
                lighting,
                aspect,
                &view.shadows,
                frame.grid.extent_mm,
                frame.floor_opacity,
            ));
            offsets.push((index as u64 * self.camera_stride) as u32);
            let (x, y, width, height) = region.scissor(self.size[0], self.size[1]);
            debug_assert!(x.saturating_add(width) <= self.size[0] && y.saturating_add(height) <= self.size[1]);
        }
        for (index, uniform) in uniforms.iter().enumerate() {
            queue.write_buffer(&self.camera_buffer, index as u64 * self.camera_stride, bytemuck::bytes_of(uniform));
        }

        let ground = GroundKey::of(self.generation, lighting.key_direction, frame.grid.extent_mm);
        if self.ground.is_stale(&ground) {
            let ground_offset = (GROUND_SLOT * self.camera_stride) as u32;
            queue.write_buffer(
                &self.camera_buffer,
                u64::from(ground_offset),
                bytemuck::bytes_of(&camera_uniform(
                    views.first().map_or(Camera::default(), |view| view.camera),
                    lighting,
                    1.0,
                    &frame.contact_shadows,
                    frame.grid.extent_mm,
                    frame.floor_opacity,
                )),
            );
            self.ground.update(
                device,
                queue,
                &ground,
                (&self.camera_bind_group, ground_offset),
                self.shadow.group(),
                &self.drawables,
            );
        }

        // The sun's map over the scene's box: one depth pass on an upload or a turned key light, and
        // nothing at all while the camera orbits (ADR 0143).
        let light_space = self.scene_bounds.and_then(|bounds| LightSpace::of(bounds, lighting.key_direction));
        let shadow = ShadowKey::of(self.generation, lighting.key_direction);
        if self.shadow.is_stale(&shadow) {
            self.shadow.update(device, queue, &shadow, light_space, (&self.camera_bind_group, 0), &self.drawables);
        }

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Z3L scene encoder") });
        // One clear for the whole target, so a view that does not cover every pixel of a rounding
        // edge leaves the background there instead of the previous frame's image.
        {
            let color_attachment = Some(wgpu::RenderPassColorAttachment {
                view: &self.targets.color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store },
            });
            let depth_attachment = wgpu::RenderPassDepthStencilAttachment {
                view: &self.targets.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0), store: wgpu::StoreOp::Store }),
            };
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Z3L clear pass"),
                color_attachments: &[color_attachment],
                depth_stencil_attachment: Some(depth_attachment),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }

        // The shared clear resets depth and stencil together. The stencil slot is what tells the
        // hidden-edge pass whose inside it is looking at.
        for (view, offset) in views.iter().zip(offsets) {
            let camera = view.camera;
            let region = view.region;
            // Each view derives its own passes, so a neighbour may draw wireframe while this one
            // draws shaded faces with hidden edges.
            let passes = view.passes();
            let (x, y, width, height) = region.scissor(self.size[0], self.size[1]);
            let color_attachment = Some(wgpu::RenderPassColorAttachment {
                view: &self.targets.color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            });
            let depth_attachment = wgpu::RenderPassDepthStencilAttachment {
                view: &self.targets.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                // Each view owns its own rectangle but shares the stencil buffer, so the slots a
                // neighbour stamped are reset before this view stamps its own.
                stencil_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0), store: wgpu::StoreOp::Store }),
            };
            // Culling comes first: two views see one frame from different sides, so the list is
            // rebuilt per view from the drawables' own boxes, and a drawable outside the view costs
            // nothing — no buffer bind, no draw call, no edge line (X58).
            let aspect = f64::from(region.width.max(1)) / f64::from(region.height.max(1));
            let frustum = Frustum::of_camera(camera, aspect);
            frustum.visible(self.drawables.iter().map(|drawn| drawn.bounds), &mut self.visible);

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Z3L view pass"),
                color_attachments: &[color_attachment],
                depth_stencil_attachment: Some(depth_attachment),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(x as f32, y as f32, width as f32, height as f32, 0.0, 1.0);
            pass.set_scissor_rect(x, y, width, height);
            pass.set_bind_group(0, &self.camera_bind_group, &[offset]);
            pass.set_bind_group(1, self.shadow.group(), &[]);

            if passes.opaque_faces && !self.visible.is_empty() {
                pass.set_pipeline(&self.mesh_pipeline);
                for &index in &self.visible {
                    let Some(drawn) = self.drawables.get(index) else {
                        continue;
                    };
                    // The reference is the slot this drawable stamps; the hidden-edge pass reads it
                    // back before drawing the same drawable's lines.
                    pass.set_stencil_reference(drawn.stencil_reference);
                    pass.set_bind_group(2, drawn.decor.group(), &[]);
                    pass.set_vertex_buffer(0, drawn.faces.vertex_buffer.slice(..));
                    pass.set_index_buffer(drawn.faces.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..drawn.faces.index_count, 0, 0..1);
                }
            }

            // The ground after the opaque bodies and before every pass that writes no depth, so the
            // plate is blended *over* what stands under it: a part below the floor is tinted by the
            // plate instead of being erased by a depth-writing ground or drawn over an opaque one
            // (U108, U109). The blended faces, the chains and the edges come later because none of
            // them writes depth, and the plate could not be occluded by them: an opaque plate drawn
            // after a ghost would paint the floor over the whole model.
            //
            // It writes no depth and carries the shadow in its own fragment, so the shadow costs no
            // pass of its own. It is drawn in every display style, because the floor is not part of
            // the model.
            if let Some(plate) = self.ground.plate_value() {
                pass.set_pipeline(self.ground.plate_pipeline());
                pass.set_bind_group(2, self.ground.plate_group(), &[]);
                pass.set_vertex_buffer(0, plate.vertex_buffer.slice(..));
                pass.set_index_buffer(plate.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..plate.index_count, 0, 0..1);
            }

            // The grid is floor, not model, so it is drawn on the plate and after the opaque bodies:
            // a line of the work plane lies in front of a part below the floor, which is where a
            // floor marking belongs, and behind a body that stands on it, whose depth is nearer. It
            // writes no depth, so it can never hide anything drawn after it (U108, U109).
            if let Some(grid) = self.grid.value() {
                pass.set_pipeline(&self.line_pipeline);
                pass.set_vertex_buffer(0, grid.vertex_buffer.slice(..));
                pass.draw(0..grid.vertex_count, 0..1);
            }

            if self.polylines.value().is_some() {
                pass.set_pipeline(&self.line_pipeline);
                if let Some(polylines) = self.polylines.value() {
                    pass.set_vertex_buffer(0, polylines.vertex_buffer.slice(..));
                    pass.draw(0..polylines.vertex_count, 0..1);
                }
            }

            // Blended faces go after the opaque scene and back to front, so a nearer panel tints a
            // farther one instead of hiding it. The order is per view: two views see the same
            // centre from opposite sides, and only the drawables the view kept are ordered.
            if passes.translucent_faces && !self.visible.is_empty() {
                self.draw_order.clear();
                self.draw_order.extend_from_slice(&self.visible);
                sort_back_to_front(&self.centres, camera.eye(), &mut self.draw_order);
                pass.set_pipeline(&self.translucent_pipeline);
                for &index in &self.draw_order {
                    let Some(drawn) = self.drawables.get(index) else {
                        continue;
                    };
                    pass.set_bind_group(2, drawn.decor.group(), &[]);
                    pass.set_vertex_buffer(0, drawn.faces.vertex_buffer.slice(..));
                    pass.set_index_buffer(drawn.faces.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..drawn.faces.index_count, 0, 0..1);
                }
            }

            // Edges last, so a line sits on top of the faces it belongs to. *Shaded with hidden
            // edges* uses the stencil-gated pass, *wireframe* reaches here with no faces before it.
            //
            // Every drawable owns its own line buffer (X57), so both styles walk the visible
            // drawables: the list that kept a drawable's faces keeps its lines too, and the hidden
            // one also names the drawable's stencil slot, which is what keeps an inside edge from
            // showing through a neighbour.
            if passes.visible_edges {
                pass.set_pipeline(&self.edge_pipeline);
                for &index in &self.visible {
                    let Some(drawn) = self.drawables.get(index) else {
                        continue;
                    };
                    let Some(edges) = drawn.edges.as_deref() else {
                        continue;
                    };
                    let Some(buffer) = edges.vertex_buffer.as_ref() else {
                        continue;
                    };
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..edges.vertex_count, 0..1);
                }
            } else if passes.through_edges {
                pass.set_pipeline(&self.hidden_edge_pipeline);
                for &index in &self.visible {
                    let Some(drawn) = self.drawables.get(index) else {
                        continue;
                    };
                    let Some(edges) = drawn.edges.as_deref() else {
                        continue;
                    };
                    let Some(buffer) = edges.vertex_buffer.as_ref() else {
                        continue;
                    };
                    pass.set_stencil_reference(drawn.stencil_reference);
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..edges.vertex_count, 0..1);
                }
            }

            // The product envelopes draw last, after every solid pass: a box is reserved space,
            // never a body, so its dashes write no depth and a panel in front still hides it.
            if let Some(envelopes) = self.product_envelopes.value() {
                pass.set_pipeline(&self.line_pipeline);
                pass.set_vertex_buffer(0, envelopes.vertex_buffer.slice(..));
                pass.draw(0..envelopes.vertex_count, 0..1);
            }
        }
        queue.submit(Some(encoder.finish()));
    }
}

/// The uniform one view shades with.
///
/// The ground travels in it too, because the plate is not the model: its size, its opacity and the
/// shadow it carries are all read when the fragment runs, so a slider of any of them costs one
/// uniform write and no buffer (U109).
fn camera_uniform(
    camera: Camera,
    lighting: &Lighting,
    aspect: f64,
    shadows: &ContactShadows,
    extent_mm: f64,
    floor_opacity: f32,
) -> CameraUniform {
    let eye = camera.eye();
    let light = lighting.key_direction.try_normalize().unwrap_or(DVec3::Z);
    let (point_count, point_position, point_color) = point_uniforms(lighting);
    CameraUniform {
        view_projection: camera.view_projection(aspect).as_mat4().to_cols_array_2d(),
        camera_position: [eye.x as f32, eye.y as f32, eye.z as f32, 1.0],
        light_direction: [light.x as f32, light.y as f32, light.z as f32, 0.0],
        light_color: [
            lighting.key_color[0],
            lighting.key_color[1],
            lighting.key_color[2],
            lighting.ambient.clamp(0.0, 1.0),
        ],
        point_count,
        point_position,
        point_color,
        shadow: shadow_uniform(shadows, light),
        shadow_plane: [
            extent_mm as f32,
            1.0 / ground::MASK_SIZE as f32,
            1.0 / ground::MASK_SIZE as f32,
            ground::penumbra_texels(extent_mm),
        ],
        floor_opacity: [sanitize_opacity(floor_opacity), 0.0, 0.0, 0.0],
    }
}

/// The floor plate's opacity as the fragment reads it, from the frame's own number.
///
/// A frame built by hand may carry anything; a NaN or a negative would turn the plate's blend into
/// garbage, so the one place that writes the slot clamps it to the closed unit interval.
fn sanitize_opacity(opacity: f32) -> f32 {
    if opacity.is_finite() { opacity.clamp(0.0, 1.0) } else { 1.0 }
}

/// Builds one drawable's vertex and index buffers from its mesh, world transform and colour.
///
/// The world-space centre of the built vertices travels with the buffers, because it is derived
/// from the same data and a drawable that keeps its buffers never measures its box again.
fn build_mesh(
    device: &wgpu::Device,
    index: usize,
    mesh: &MeshData,
    transform: DMat4,
    color: [f32; 4],
    decor: Option<&Decor>,
) -> Result<GpuMesh, RenderError> {
    let (vertices, indices) = world_vertices(index, mesh, transform, color, decor)?;
    let index_count = u32::try_from(indices.len()).map_err(|_| RenderError::MeshTooLarge { index })?;
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Z3L drawable vertices"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Z3L drawable indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    Ok(GpuMesh { vertex_buffer, index_buffer, index_count, centre: faces_centre(&vertices) })
}

/// The shadow colour and strength the fragment shaders read.
///
/// One strength serves both masks — the plate's silhouette and a face's occlusion — because they are
/// the same sun (ADR 0143). A switch that is off, a zero strength, a hostile value or a key light
/// that shines from below the floor collapses to zero, so the plate keeps its own colour and no face
/// loses its key light; the colour is kept so turning the strength back up needs no upload.
fn shadow_uniform(shadows: &ContactShadows, light: DVec3) -> [f32; 4] {
    let lit = light.z > MIN_SHADOW_SUN_Z;
    let strength = if shadows.draws() && lit { shadows.strength.clamp(0.0, 1.0) } else { 0.0 };
    [shadows.color[0], shadows.color[1], shadows.color[2], strength]
}

/// The flattest key light that can still cast a floor shadow: below it the projection runs to
/// infinity and the shadow would cover the whole plate.
const MIN_SHADOW_SUN_Z: f64 = 0.06;

/// Packs the point lights into the fixed uniform arrays, stopping at the frame budget.
fn point_uniforms(lighting: &Lighting) -> ([u32; 4], [[f32; 4]; MAX_POINT_LIGHTS], [[f32; 4]; MAX_POINT_LIGHTS]) {
    let mut positions = [[0.0; 4]; MAX_POINT_LIGHTS];
    let mut colors = [[0.0; 4]; MAX_POINT_LIGHTS];
    let mut count = 0usize;
    for (index, point) in lighting.points.iter().take(MAX_POINT_LIGHTS).enumerate() {
        let position = if point.position.is_finite() { point.position } else { DVec3::ZERO };
        let color = if point.color.iter().all(|component| component.is_finite()) { point.color } else { [1.0; 3] };
        let power = if point.power.is_finite() && point.power >= 0.0 { point.power } else { 0.0 };
        positions[index] = [position.x as f32, position.y as f32, position.z as f32, 0.0];
        colors[index] = [color[0], color[1], color[2], power];
        count += 1;
    }
    ([count as u32, 0, 0, 0], positions, colors)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::sync::Arc;

    use super::*;
    use crate::geometry::{EDGE_INK, MAX_GRID_DIVISIONS, MAX_VERTICES, edge_ink};
    use crate::geometry::{floor_indices, floor_vertices};
    use crate::ground::{GroundKey, penumbra_texels};
    use crate::pipelines::{FACE_DEPTH_BIAS, MAX_STENCIL_OWNERS, STENCIL_MASK, face_depth_state, floor_depth_state};
    use geom::{DMat4, DVec2, MeshData};
    use model::Id;
    use scene::{Drawable, Grid, Polyline, ProductEnvelope, ProductEnvelopeState, SceneColors};

    fn triangle() -> MeshData {
        MeshData {
            positions: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            indices: vec![0, 1, 2],
        }
    }

    /// A closed box in world millimetres, flat-shaded with outward normals.
    ///
    /// The shadow tests need a caster with real faces: a triangle cannot stand between the light and
    /// anything, so the shelf, the panel and the flush band are boxes. Every quad runs
    /// counter-clockwise seen from outside, so `front_facing` agrees with the normal the fragment
    /// shader flips back faces with; a mesh wound the other way reads every visible face as a back
    /// face, flips its normal inwards and lets it shadow itself.
    fn box_mesh(min: DVec3, max: DVec3) -> MeshData {
        let corner = |index: usize| {
            DVec3::new(
                if index & 1 == 0 { min.x } else { max.x },
                if index & 2 == 0 { min.y } else { max.y },
                if index & 4 == 0 { min.z } else { max.z },
            )
        };
        let faces: [(DVec3, [usize; 4]); 6] = [
            (-DVec3::X, [0, 4, 6, 2]),
            (DVec3::X, [1, 3, 7, 5]),
            (-DVec3::Y, [0, 1, 5, 4]),
            (DVec3::Y, [2, 6, 7, 3]),
            (-DVec3::Z, [0, 2, 3, 1]),
            (DVec3::Z, [4, 5, 7, 6]),
        ];
        let mut mesh = MeshData::default();
        for (normal, indices) in faces {
            let base = mesh.positions.len() as u32;
            for index in indices {
                let point = corner(index);
                mesh.positions.push([point.x as f32, point.y as f32, point.z as f32]);
                mesh.normals.push([normal.x as f32, normal.y as f32, normal.z as f32]);
            }
            mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        mesh
    }

    /// The pixel rectangle the eight corners of a world box project to, clamped to the target.
    fn projected_rect(camera: Camera, size: [u32; 2], min: DVec3, max: DVec3) -> (u32, u32, u32, u32) {
        let aspect = f64::from(size[0].max(1)) / f64::from(size[1].max(1));
        let matrix = camera.view_projection(aspect);
        let mut low = DVec3::splat(f64::INFINITY);
        let mut high = DVec3::splat(f64::NEG_INFINITY);
        for corner in Aabb::of_points([min, max]).expect("a box of two corners").corners() {
            let clip = matrix * corner.extend(1.0);
            let ndc = clip.truncate() / clip.w.max(1.0e-9);
            low = low.min(ndc);
            high = high.max(ndc);
        }
        let pixel = |ndc: f64, extent: u32| {
            let value = ((ndc * 0.5 + 0.5) * f64::from(extent)).round().max(0.0);
            (value as u32).min(extent.saturating_sub(1))
        };
        let x0 = pixel(low.x, size[0]);
        let x1 = pixel(high.x, size[0]);
        let y0 = pixel(-high.y, size[1]);
        let y1 = pixel(-low.y, size[1]);
        (x0, y0, x1.saturating_sub(x0).max(1), y1.saturating_sub(y0).max(1))
    }

    /// The mean of a pixel rectangle's three channels, in target levels.
    fn mean_luma(pixels: &[u8], size: [u32; 2], rect: (u32, u32, u32, u32)) -> f64 {
        let (x0, y0, width, height) = rect;
        let mut total = 0.0;
        let mut count = 0.0;
        for y in y0..(y0 + height).min(size[1]) {
            for x in x0..(x0 + width).min(size[0]) {
                let start = ((y * size[0] + x) * 4) as usize;
                if let Some(pixel) = pixels.get(start..start + 4) {
                    total += f64::from(pixel[0]) + f64::from(pixel[1]) + f64::from(pixel[2]);
                    count += 3.0;
                }
            }
        }
        if count == 0.0 { 0.0 } else { total / count }
    }

    /// The largest amount any channel of `first` is darker than the same channel of `second`.
    ///
    /// A shadow map that lets a surface shadow itself shows up here and nowhere else: the two
    /// pictures are the same frame at the same camera, one with the map and one without it.
    fn darkest_change(first: &[u8], second: &[u8]) -> i32 {
        first.iter().zip(second.iter()).map(|(first, second)| i32::from(*second) - i32::from(*first)).max().unwrap_or(0)
    }

    /// A frame of the given world boxes, lit by one sun and looked at through one camera.
    fn shaded_scene(
        camera: Camera,
        light: DVec3,
        boxes: &[(u64, DVec3, DVec3)],
        shadows: ContactShadows,
    ) -> SceneFrame {
        let mut frame = SceneFrame {
            camera,
            shading: ShadingStyle::Shaded,
            grid: Grid { visible: false, ..Grid::default() },
            contact_shadows: shadows,
            ..SceneFrame::default()
        };
        frame.lighting.key_direction = light.try_normalize().expect("a sun");
        frame.lighting.key_color = [0.9, 0.88, 0.85];
        frame.lighting.ambient = 0.25;
        for (index, min, max) in boxes {
            let id = Id::new(*index).expect("id");
            let mesh = Arc::new(box_mesh(*min, *max));
            frame.push(Drawable::new(id, mesh, DMat4::IDENTITY, [0.6, 0.58, 0.55, 1.0]));
        }
        frame
    }

    #[test]
    fn clamp_size_never_returns_zero_or_exceeds_the_device_limit() {
        assert_eq!(clamp_size(0, 9999, 4096), [1, 4096]);
        assert_eq!(clamp_size(800, 600, 4096), [800, 600]);
    }

    #[test]
    fn vertices_are_transformed_into_world_space() {
        let mesh = triangle();
        let transform = DMat4::from_translation(DVec3::new(100.0, 20.0, 5.0));
        let (vertices, indices) = world_vertices(0, &mesh, transform, [1.0, 0.5, 0.25, 1.0], None).expect("valid mesh");

        assert_eq!(indices, vec![0, 1, 2]);
        assert_eq!(vertices.len(), 3);
        assert_eq!(vertices[0].position, [100.0, 20.0, 5.0]);
        assert_eq!(vertices[1].position, [110.0, 20.0, 5.0]);
        assert!((vertices[0].normal[2] - 1.0).abs() < 1.0e-6);
        assert_eq!(vertices[0].color, [1.0, 0.5, 0.25, 1.0]);
    }

    #[test]
    fn meshes_without_normals_are_expanded_for_flat_shading() {
        let mesh = MeshData {
            positions: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
            normals: Vec::new(),
            indices: vec![0, 1, 2],
        };
        let (vertices, indices) = world_vertices(0, &mesh, DMat4::IDENTITY, [1.0; 4], None).expect("valid mesh");
        assert_eq!(vertices.len(), 3);
        assert_eq!(indices, vec![0, 1, 2]);
        assert!((vertices[0].normal[2] - 1.0).abs() < 1.0e-6);
    }

    fn line_chain(transform: DMat4) -> Polyline {
        let points: Arc<[DVec3]> =
            Arc::from(vec![DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 10.0, 0.0)]);
        Polyline::new(Id::new(1).expect("id"), points, transform, [1.0, 0.5, 0.25, 1.0])
    }

    #[test]
    fn a_polyline_becomes_one_line_list_segment_per_pair() {
        let transform = DMat4::from_translation(DVec3::new(100.0, 20.0, 5.0));
        let vertices =
            polyline_vertices(&[line_chain(transform)], SceneFrame::default().line_ink).expect("valid polyline");

        assert_eq!(vertices.len(), 4, "two segments, two vertices each");
        assert_eq!(vertices[0].position, [100.0, 20.0, 5.0]);
        assert_eq!(vertices[1].position, [110.0, 20.0, 5.0]);
        assert_eq!(vertices[2].position, [110.0, 20.0, 5.0]);
        assert_eq!(vertices[3].position, [110.0, 30.0, 5.0]);
        assert_eq!(vertices[0].color, [1.0, 0.5, 0.25, 1.0]);
    }

    #[test]
    fn a_polyline_needs_two_finite_points() {
        let id = Id::new(1).expect("id");
        let empty = Polyline::new(id, Arc::from(Vec::<DVec3>::new()), DMat4::IDENTITY, [1.0; 4]);
        assert_eq!(
            polyline_vertices(&[empty], SceneFrame::default().line_ink),
            Err(RenderError::EmptyPolyline { index: 0 })
        );

        let bad_point =
            Polyline::new(id, Arc::from(vec![DVec3::ZERO, DVec3::new(f64::NAN, 0.0, 0.0)]), DMat4::IDENTITY, [1.0; 4]);
        assert_eq!(
            polyline_vertices(&[bad_point], SceneFrame::default().line_ink),
            Err(RenderError::InvalidPolyline { index: 0 })
        );

        let bad_transform = Polyline::new(
            id,
            Arc::from(vec![DVec3::ZERO, DVec3::X]),
            DMat4::from_translation(DVec3::new(f64::NAN, 0.0, 0.0)),
            [1.0; 4],
        );
        assert_eq!(
            polyline_vertices(&[bad_transform], SceneFrame::default().line_ink),
            Err(RenderError::InvalidPolyline { index: 0 })
        );
    }

    fn product_envelope(id: u64, size: DVec3, state: ProductEnvelopeState) -> ProductEnvelope {
        ProductEnvelope::rectangular(Id::new(id).expect("id"), size, DMat4::IDENTITY, state)
    }

    #[test]
    fn a_product_envelope_becomes_dashed_lines_in_its_state_colour() {
        let active = SceneColors::DARK.product_envelope_active;
        let inactive = SceneColors::DARK.product_envelope_inactive;
        let open = product_envelope(1, DVec3::new(600.0, 400.0, 700.0), ProductEnvelopeState::Open);
        let vertices = product_envelope_vertices(&[open], active, inactive).expect("a valid envelope");

        // Twelve edges, each at most one dash cycle per 1/24 of the longest side.
        assert_eq!(vertices.len() % 2, 0, "a line list is drawn in pairs");
        assert!(vertices.len() > 12, "twelve edges need more than one dash each");
        assert!(vertices.len() <= 12 * 2 * 26, "the dash cycle bounds the vertex count: {}", vertices.len());
        assert!(vertices.iter().all(|vertex| vertex.color == active), "an open product draws in the active ink");

        // The first edge runs along X from the zero corner; the first dash is painted and the next
        // one starts after a gap, so the edge is visibly not a solid line.
        assert_eq!(vertices[0].position, [0.0, 0.0, 0.0]);
        let first_dash_end = vertices[1].position[0];
        assert!(first_dash_end > 0.0 && first_dash_end < 600.0, "a dash stops before the corner");
        let second_dash_start = vertices[2].position[0];
        assert!(second_dash_start > first_dash_end, "a gap separates two dashes");
        assert!(first_dash_end < 20.0, "the first dash is short: {first_dash_end}");

        let closed = product_envelope(2, DVec3::splat(500.0), ProductEnvelopeState::ClosedEmpty);
        let vertices = product_envelope_vertices(&[closed], active, inactive).expect("a valid envelope");
        assert!(vertices.iter().all(|vertex| vertex.color == inactive), "a closed empty piece is inactive");
        assert!(!vertices.is_empty());

        let with_parts = product_envelope(3, DVec3::splat(500.0), ProductEnvelopeState::ClosedWithParts);
        assert!(
            product_envelope_vertices(&[with_parts], active, inactive).expect("a valid envelope").is_empty(),
            "a closed piece with parts draws no lines"
        );
    }

    #[test]
    fn a_cut_envelope_draws_its_two_rings_and_one_vertical_at_each_corner() {
        let active = SceneColors::DARK.product_envelope_active;
        let inactive = SceneColors::DARK.product_envelope_inactive;
        let outline = [
            DVec2::new(0.0, 0.0),
            DVec2::new(100.0, 0.0),
            DVec2::new(100.0, 40.0),
            DVec2::new(40.0, 40.0),
            DVec2::new(40.0, 100.0),
            DVec2::new(0.0, 100.0),
        ];
        let envelope = ProductEnvelope {
            id: Id::new(9).expect("id"),
            outline: Arc::from(outline),
            corners: Arc::from(outline),
            height: 100.0,
            world_transform: DMat4::IDENTITY,
            state: ProductEnvelopeState::Open,
        };
        let vertices = product_envelope_vertices(&[envelope], active, inactive).expect("a valid envelope");
        assert!(vertices.len().is_multiple_of(2), "a line list is drawn in pairs");
        assert!(
            vertices.iter().all(|vertex| (0.0..=100.0).contains(&vertex.position[2])),
            "the wireframe stays between the base and the top"
        );
        let mut verticals: Vec<(i64, i64)> = vertices
            .chunks(2)
            .filter_map(|pair| {
                let [from, to] = pair else {
                    return None;
                };
                ((from.position[2] - to.position[2]).abs() > 1.0e-6)
                    .then(|| (i64::from(from.position[0].round() as i32), i64::from(from.position[1].round() as i32)))
            })
            .collect();
        verticals.sort_unstable();
        verticals.dedup();
        assert_eq!(verticals.len(), 6, "one vertical per corner of the cut shape: {verticals:?}");
        assert!(!verticals.iter().any(|(x, y)| *x > 40 && *y > 40), "the cut corner stands no vertical: {verticals:?}");
    }

    #[test]
    fn a_product_envelope_follows_its_transform_and_refuses_a_malformed_box() {
        let active = SceneColors::DARK.product_envelope_active;
        let inactive = SceneColors::DARK.product_envelope_inactive;
        let mut moved = product_envelope(1, DVec3::splat(100.0), ProductEnvelopeState::Open);
        moved.world_transform = DMat4::from_translation(DVec3::new(1000.0, 0.0, 0.0));
        let vertices = product_envelope_vertices(&[moved], active, inactive).expect("a valid envelope");
        assert!(vertices.iter().all(|vertex| vertex.position[0] >= 1000.0), "the box is drawn in world space");

        let zero = product_envelope(2, DVec3::ZERO, ProductEnvelopeState::Open);
        assert_eq!(
            product_envelope_vertices(&[zero], active, inactive),
            Err(RenderError::InvalidProductEnvelope { index: 0 })
        );
        let nan = product_envelope(3, DVec3::new(f64::NAN, 100.0, 100.0), ProductEnvelopeState::Open);
        assert_eq!(
            product_envelope_vertices(&[nan], active, inactive),
            Err(RenderError::InvalidProductEnvelope { index: 0 })
        );
        assert!(
            product_envelope_vertices(&[], active, inactive).is_ok_and(|vertices| vertices.is_empty()),
            "no envelopes cost no vertices"
        );
    }

    #[test]
    fn a_closed_envelope_with_parts_is_not_sent_to_the_gpu() {
        let active = SceneColors::LIGHT.product_envelope_active;
        let inactive = SceneColors::LIGHT.product_envelope_inactive;
        let hidden = product_envelope(1, DVec3::splat(100.0), ProductEnvelopeState::ClosedWithParts);
        let open = product_envelope(2, DVec3::splat(100.0), ProductEnvelopeState::Open);
        let vertices = product_envelope_vertices(&[hidden, open], active, inactive).expect("valid envelopes");
        assert!(vertices.iter().all(|vertex| vertex.color == active), "only the open box is drawn");
    }

    #[test]
    fn a_polyline_without_a_colour_draws_in_the_frame_ink() {
        let points: Arc<[DVec3]> = Arc::from(vec![DVec3::ZERO, DVec3::X]);
        let id = Id::new(1).expect("id");
        let chain = Polyline::in_ink(id, points, DMat4::IDENTITY);

        let mut frame = SceneFrame::default();
        frame.push_polyline(chain);
        frame.apply_colors(scene::SceneColors::LIGHT);
        let vertices = polyline_vertices(frame.polylines(), frame.line_ink).expect("valid polyline");
        assert_eq!(vertices[0].color, scene::SceneColors::LIGHT.line_ink);
        assert_ne!(vertices[0].color, scene::SceneColors::DARK.line_ink, "the ink followed the theme");
    }

    #[test]
    fn malformed_meshes_are_rejected_before_upload() {
        let mut mesh = triangle();
        mesh.indices[2] = 3;
        assert_eq!(
            world_vertices(4, &mesh, DMat4::IDENTITY, [1.0; 4], None),
            Err(RenderError::InvalidMesh { index: 4 })
        );

        let mut mesh = triangle();
        mesh.normals.pop();
        assert_eq!(
            world_vertices(0, &mesh, DMat4::IDENTITY, [1.0; 4], None),
            Err(RenderError::InvalidMesh { index: 0 })
        );

        let empty = MeshData::default();
        assert_eq!(
            world_vertices(0, &empty, DMat4::IDENTITY, [1.0; 4], None),
            Err(RenderError::EmptyMesh { index: 0 })
        );
    }

    #[test]
    fn grid_geometry_has_axes_and_major_lines() {
        let grid = Grid { spacing_mm: 100.0, extent_mm: 200.0, major_every: 2, ..Grid::default() };
        let vertices = grid_vertices(&grid);

        assert_eq!(vertices.len(), 5 * 4, "five lines in each direction");
        // Five offsets from -200 to 200; at offset zero the X line starts at vertex 8 and the
        // Y line at vertex 10.
        assert_eq!(vertices[8].color, grid.axis_x_color);
        assert_eq!(vertices[10].color, grid.axis_y_color);
        assert_eq!(vertices[0].color, grid.major_color);
        assert_eq!(vertices[4].color, grid.minor_color);
        assert!(vertices.iter().all(|vertex| vertex.position[2] > 0.0));
    }

    #[test]
    fn the_grid_strength_is_the_ink_s_opacity() {
        // The display strength fades the grid instead of recolouring it: the hue is the theme's and
        // the opacity is the workspace's, so a zero strength hides the lines (U64).
        let grid = Grid { spacing_mm: 100.0, extent_mm: 200.0, major_every: 2, ..Grid::default() };
        let faint = grid_vertices(&Grid { ink_strength: 0.25, ..grid });
        let hidden = grid_vertices(&Grid { ink_strength: 0.0, ..grid });
        assert_eq!(faint.len(), grid_vertices(&grid).len(), "a strength never changes the geometry");
        for (faint, full) in faint.iter().zip(grid_vertices(&grid).iter()) {
            assert_eq!(faint.color[..3], full.color[..3], "the ink keeps its hue");
            assert!((faint.color[3] - 0.25).abs() < 1.0e-6, "the ink fades by the strength");
        }
        assert!(hidden.iter().all(|vertex| vertex.color[3] == 0.0), "a zero strength hides the grid");
        let hostile = grid_vertices(&Grid { ink_strength: f32::NAN, ..grid });
        assert!(hostile.iter().all(|vertex| vertex.color[3] > 0.0), "a hostile strength stays visible");
    }

    #[test]
    fn grid_values_are_clamped_to_a_bounded_line_count() {
        let grid = Grid { spacing_mm: 1.0, extent_mm: f64::MAX, major_every: 1, ..Grid::default() };
        let vertices = grid_vertices(&grid);
        assert_eq!(vertices.len(), (2 * MAX_GRID_DIVISIONS as usize + 1) * 4);
    }

    #[test]
    fn a_hidden_grid_costs_no_vertices() {
        let grid = Grid { visible: false, ..Grid::default() };
        assert!(grid_vertices(&grid).is_empty(), "a hidden grid must not build line geometry");
    }

    #[test]
    fn the_floor_plate_is_one_quad_the_camera_sees_from_above() {
        let vertices = floor_vertices(5000.0, SceneFrame::default().floor, 0.0);
        assert_eq!(vertices.len(), 4, "four corners, one quad");
        assert!(vertices.iter().all(|vertex| vertex.position[2] == 0.0), "the plate is the floor");
        assert!(vertices.iter().all(|vertex| vertex.color == SceneFrame::default().floor));
        let indices = floor_indices();
        assert_eq!(indices.len(), 6, "two triangles");
        // Seen from `+Z` the corners run counter-clockwise, so the plate's own side is the top: the
        // pipeline culls a view from below, which would otherwise hide the cabinet behind the ground.
        let corners = [vertices[0].position, vertices[1].position, vertices[2].position];
        let cross = (corners[1][0] - corners[0][0]) * (corners[2][1] - corners[0][1])
            - (corners[1][1] - corners[0][1]) * (corners[2][0] - corners[0][0]);
        assert!(cross > 0.0, "the top of the plate faces `+Z`, which is the side a view from above sees");
        assert!(floor_vertices(0.0, [1.0; 4], 0.0).is_empty(), "a ground with no area draws nothing");
        assert!(floor_vertices(f64::NAN, [1.0; 4], 0.0).is_empty(), "a hostile size draws nothing");
    }

    #[test]
    fn the_floor_plate_tests_depth_without_writing_it() {
        let state = floor_depth_state();
        assert_eq!(
            state.depth_write_enabled,
            Some(false),
            "a body under the floor draws over the plate instead of being culled behind it (U108)"
        );
        assert_eq!(state.depth_compare, Some(wgpu::CompareFunction::Less));
        assert_eq!(state.bias, FACE_DEPTH_BIAS, "a bottom outline still wins against the plate it lies on");
        assert_eq!(state.stencil, wgpu::StencilState::default(), "the ground owns no stencil slot");
    }

    #[test]
    fn the_floor_plate_blends_so_it_can_tint_what_stands_under_it() {
        // U109: the plate is one blended surface whatever the workspace's opacity is, so the slider
        // changes a uniform and never a pipeline. Without a blend the paint would be opaque at any
        // value, which is the floor U108 drew.
        let target = crate::pipelines::floor_target();
        assert_eq!(target.as_ref().map(|target| target.format), Some(COLOR_FORMAT));
        assert_eq!(target.and_then(|target| target.blend), Some(wgpu::BlendState::ALPHA_BLENDING));
    }

    #[test]
    fn the_floor_opacity_reaches_the_uniform_clamped_and_never_costs_a_buffer() {
        // The plate reads one number from the camera slot, so a drag of the slider writes a uniform.
        // A hostile frame value is clamped where the slot is written, not trusted.
        let uniform = |opacity| {
            camera_uniform(Camera::default(), &Lighting::default(), 1.0, &ContactShadows::default(), 5000.0, opacity)
                .floor_opacity[0]
        };
        assert_eq!(uniform(1.0), 1.0, "an opaque floor is the value a fresh workspace carries");
        assert_eq!(uniform(0.4), 0.4, "and forty percent is the plate a session asks for");
        assert_eq!(uniform(0.0), 0.0, "an empty floor reaches the fragment as an empty floor");
        assert_eq!(uniform(-3.0), 0.0, "a negative opacity is clamped, never blended backwards");
        assert_eq!(uniform(9.0), 1.0);
        assert_eq!(uniform(f32::NAN), 1.0, "a NaN falls back to the floor that hides nothing");
        let frame = SceneFrame { floor_opacity: 0.4, ..SceneFrame::default() };
        assert_eq!(
            camera_uniform(frame.camera, &frame.lighting, 1.0, &frame.contact_shadows, 5000.0, frame.floor_opacity)
                .floor_opacity[0],
            0.4,
            "the frame's own value is what the plate draws with"
        );
    }

    #[test]
    fn the_shadow_switch_collapses_the_uniform_to_zero() {
        let shadows = ContactShadows::default();
        let light = DVec3::new(0.0, 0.0, 1.0);
        let on = shadow_uniform(&shadows, light);
        assert!(on[3] > 0.0, "the default draws a shadow");
        let off = shadow_uniform(&ContactShadows { visible: false, ..shadows }, light);
        assert_eq!(off[3], 0.0, "the switch draws nothing");
        assert_eq!(off[..3], on[..3], "the colour survives the switch, so turning it back on costs no upload");
        let faint = shadow_uniform(&ContactShadows { strength: 0.0, ..shadows }, light);
        assert_eq!(faint[3], 0.0);
    }

    #[test]
    fn a_sun_below_the_floor_casts_no_shadow() {
        let shadows = ContactShadows::default();
        assert_eq!(shadow_uniform(&shadows, DVec3::new(0.0, 0.0, -1.0))[3], 0.0, "a lamp under the floor lights it");
        assert_eq!(shadow_uniform(&shadows, DVec3::new(0.9, 0.0, 0.02))[3], 0.0, "a sun on the horizon never sets");
        assert!(shadow_uniform(&shadows, DVec3::new(0.0, 0.0, 1.0))[3] > 0.0);
    }

    #[test]
    fn a_mask_is_stale_when_the_scene_or_the_light_moves_and_not_when_the_camera_does() {
        let light = DVec3::new(-0.35, 0.55, 0.75);
        let first = GroundKey::of(1, light, 5000.0);
        assert_eq!(first, GroundKey::of(1, light, 5000.0), "the same frame keeps its mask");
        assert_ne!(first, GroundKey::of(2, light, 5000.0), "an edit rebuilds it");
        assert_ne!(first, GroundKey::of(1, DVec3::new(0.35, 0.55, 0.75), 5000.0), "a turned sun rebuilds it");
        assert_ne!(first, GroundKey::of(1, light, 2500.0), "a resized ground rebuilds it");
        // A direction that differs in the fifth decimal is the same light: a slider does not thrash.
        assert_eq!(first, GroundKey::of(1, light * 1.00005, 5000.0));
    }

    #[test]
    fn the_penumbra_is_a_world_size_and_not_a_zoom() {
        let wide = penumbra_texels(5000.0);
        let close = penumbra_texels(2500.0);
        assert!((wide - 6.1).abs() < 0.3, "a ten-metre ground carries a sixty-millimetre penumbra, not a texel");
        assert!(close > wide, "half the ground is twice as many texels for the same penumbra");
        assert!(penumbra_texels(0.0).is_finite() && penumbra_texels(f64::NAN).is_finite());
    }

    #[test]
    fn point_lights_fill_the_uniform_and_stop_at_the_budget() {
        let mut lighting = Lighting::default();
        for index in 0..MAX_POINT_LIGHTS + 2 {
            lighting.points.push(scene::PointLight {
                position: DVec3::new(index as f64, 0.0, 0.0),
                color: [0.25, 0.5, 0.75],
                power: 1000.0,
            });
        }

        let (count, positions, colors) = point_uniforms(&lighting);
        assert_eq!(count[0] as usize, MAX_POINT_LIGHTS);
        assert_eq!(positions[0], [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(positions[MAX_POINT_LIGHTS - 1], [(MAX_POINT_LIGHTS - 1) as f32, 0.0, 0.0, 0.0]);
        assert_eq!(colors[0], [0.25, 0.5, 0.75, 1000.0]);
    }

    #[test]
    fn each_display_style_maps_to_the_passes_it_promises() {
        assert_eq!(
            StylePasses::new(ShadingStyle::ShadedWithEdges),
            StylePasses { opaque_faces: true, translucent_faces: false, visible_edges: true, through_edges: false }
        );
        assert_eq!(
            StylePasses::new(ShadingStyle::Shaded),
            StylePasses { opaque_faces: true, translucent_faces: false, visible_edges: false, through_edges: false }
        );
        let wireframe = StylePasses::new(ShadingStyle::Wireframe);
        assert!(!wireframe.opaque_faces, "wireframe draws no triangle");
        assert!(!wireframe.translucent_faces);
        assert!(wireframe.visible_edges, "it draws the edge pass alone");
        assert!(!wireframe.through_edges);
        let hidden = StylePasses::new(ShadingStyle::ShadedWithHiddenEdges);
        assert!(hidden.opaque_faces, "hidden edges keep the shaded faces");
        assert!(hidden.through_edges, "and draw the edges through them");
        assert!(!hidden.visible_edges);
        let ghost = StylePasses::new(ShadingStyle::Translucent);
        assert!(ghost.translucent_faces);
        assert!(!ghost.opaque_faces, "one face pass, blended, not two");
        assert!(!ghost.visible_edges && !ghost.through_edges);
    }

    #[test]
    fn each_view_derives_its_own_passes_from_its_style() {
        // The pass loop asks each view for its own style, so two neighbours in one target may draw
        // different passes from the same uploaded geometry and edge buffer (U63).
        let region = ViewRegion { x: 0, y: 0, width: 100, height: 100 };
        let views = [
            ViewSpec {
                camera: Camera::default(),
                region,
                shading: ShadingStyle::Wireframe,
                shadows: ContactShadows::default(),
            },
            ViewSpec {
                camera: Camera::default(),
                region,
                shading: ShadingStyle::Shaded,
                shadows: ContactShadows::default(),
            },
        ];
        assert_eq!(views[0].passes(), StylePasses::new(ShadingStyle::Wireframe));
        assert_eq!(views[1].passes(), StylePasses::new(ShadingStyle::Shaded));
        assert_ne!(views[0].passes(), views[1].passes(), "different styles turn on different passes");
    }

    #[test]
    fn the_shadow_switch_is_per_view_and_its_strength_is_the_frame_s() {
        // One cell of a split draws a shadow while its neighbour does not, from one workspace
        // strength and one mask (U107): the difference is the `visible` flag alone.
        let workspace = ContactShadows { strength: 0.55, ..ContactShadows::default() };
        let on = ContactShadows { visible: true, ..workspace };
        let off = ContactShadows { visible: false, ..workspace };
        assert_eq!(shadow_uniform(&on, DVec3::Z)[3], 0.55, "the workspace strength reaches the plate");
        assert_eq!(shadow_uniform(&off, DVec3::Z)[3], 0.0, "the other view keeps its own floor");
    }

    #[test]
    fn a_translucent_face_pass_writes_no_depth() {
        let ghost = face_depth_state(ShadingStyle::Translucent);
        assert_eq!(ghost.depth_write_enabled, Some(false), "a ghost must not occlude the next object");
        assert_eq!(ghost.depth_compare, Some(wgpu::CompareFunction::LessEqual));

        let opaque = face_depth_state(ShadingStyle::Shaded);
        assert_eq!(opaque.depth_write_enabled, Some(true));
        assert_eq!(opaque.depth_compare, Some(wgpu::CompareFunction::Less));
    }

    #[test]
    fn hidden_edges_ignore_the_depth_test_and_visible_ones_are_only_compared() {
        assert_eq!(edge_depth_state(true).depth_compare, Some(wgpu::CompareFunction::Always));
        assert_eq!(edge_depth_state(false).depth_compare, Some(wgpu::CompareFunction::LessEqual));
        assert_eq!(edge_depth_state(true).depth_write_enabled, Some(false));
        assert_eq!(edge_depth_state(false).depth_write_enabled, Some(false));
        // wgpu rejects DepthBiasState on a line topology, so the unfavourable comparison is the whole
        // mechanism on the edge side; the bias lives on the faces instead.
        assert_eq!(edge_depth_state(false).bias, wgpu::DepthBiasState::default());
        assert_eq!(edge_depth_state(true).bias, wgpu::DepthBiasState::default());
    }

    #[test]
    fn the_face_pass_owns_the_depth_bias_and_stamps_its_drawable() {
        let opaque = face_depth_state(ShadingStyle::ShadedWithEdges);
        assert_eq!(opaque.bias, FACE_DEPTH_BIAS, "the faces are pushed back so an unbias line wins");
        assert_ne!(opaque.bias, wgpu::DepthBiasState::default());
        assert_eq!(opaque.stencil.front.pass_op, wgpu::StencilOperation::Replace, "an opaque face claims the pixel");
        assert_eq!(opaque.stencil.write_mask, STENCIL_MASK);

        let ghost = face_depth_state(ShadingStyle::Translucent);
        assert_eq!(ghost.bias, wgpu::DepthBiasState::default(), "a blended face competes with nothing");
        assert_eq!(ghost.stencil, wgpu::StencilState::default(), "a blended face owns no pixel");
    }

    #[test]
    fn the_hidden_edge_pass_only_accepts_its_own_stencil_slot() {
        let hidden = edge_depth_state(true);
        assert_eq!(hidden.stencil.front.compare, wgpu::CompareFunction::Equal);
        assert_eq!(hidden.stencil.front.pass_op, wgpu::StencilOperation::Keep);
        assert_eq!(hidden.stencil.write_mask, 0, "the pass never rewrites the ownership it tests");
        assert_eq!(hidden.stencil.read_mask, STENCIL_MASK);

        // The visible pass tests nothing in the stencil buffer; the depth test is its only filter.
        assert_eq!(edge_depth_state(false).stencil, wgpu::StencilState::default());
    }

    #[test]
    fn a_stencil_slot_is_one_based_and_never_zero() {
        assert_eq!(stencil_slot(0), 1, "zero is reserved for 'no surface here'");
        assert_eq!(stencil_slot(4), 5);
        // The count wraps instead of overflowing past the byte the buffer stores.
        assert_eq!(stencil_slot(MAX_STENCIL_OWNERS), 1);
        assert_eq!(stencil_slot(MAX_STENCIL_OWNERS * 2 + 3), 4);
    }

    #[test]
    fn translucent_drawables_are_ordered_far_to_near() {
        let centres = [DVec3::new(0.0, 0.0, 1.0), DVec3::new(0.0, 0.0, 10.0), DVec3::new(0.0, 0.0, 5.0)];
        let mut order = Vec::new();
        sort_back_to_front(&centres, DVec3::ZERO, &mut order);
        assert_eq!(order, vec![1, 2, 0]);

        // A second view reuses the vector instead of allocating, and a garbage eye sorts at the origin.
        sort_back_to_front(&centres, DVec3::new(f64::NAN, 0.0, 0.0), &mut order);
        assert_eq!(order.len(), centres.len());
    }

    #[test]
    fn a_drawable_centre_is_the_world_space_centre_of_its_bounds() {
        let transform = DMat4::from_translation(DVec3::new(100.0, 20.0, 5.0));
        let (vertices, _) = world_vertices(0, &triangle(), transform, [1.0; 4], None).expect("a valid mesh");
        let centre = faces_centre(&vertices);
        assert!((centre - DVec3::new(105.0, 25.0, 5.0)).length() < 1.0e-9);
        assert_eq!(faces_centre(&[]), DVec3::ZERO, "a drawable with no vertex sorts at the origin");
    }

    #[test]
    fn edges_become_world_space_lines_and_skip_malformed_entries() {
        let mut mesh = triangle();
        mesh.normals.clear();
        let edges = [0, 1, 0, 9];
        let transform = DMat4::from_translation(DVec3::new(100.0, 20.0, 5.0));
        let ink = edge_ink(1.0);

        let vertices = edge_vertices(0, &mesh, &edges, transform, ink).expect("a drawable's lines");
        assert_eq!(vertices.len(), 2, "the edge naming a missing vertex is skipped");
        assert_eq!(vertices[0].position, [100.0, 20.0, 5.0]);
        assert_eq!(vertices[1].position, [110.0, 20.0, 5.0]);
        assert_eq!(vertices[0].color, EDGE_INK, "one black ink serves every variant");

        // A half edge and a non-finite transform draw nothing, and neither is an error.
        assert_eq!(edge_vertices(0, &mesh, &[0, 1, 2], DMat4::IDENTITY, ink).expect("a half edge").len(), 2);
        let moved = DMat4::from_translation(DVec3::new(f64::NAN, 0.0, 0.0));
        assert!(edge_vertices(0, &mesh, &edges, moved, ink).expect("a hostile transform").is_empty());
        assert!(edge_vertices(0, &mesh, &[], DMat4::IDENTITY, ink).expect("no lines").is_empty());
    }

    #[test]
    fn an_edge_list_past_the_budget_is_refused_by_name() {
        // One buffer holds `MAX_VERTICES` line vertices; a longer list would have to be cut, and a
        // wireframe that quietly stops is a wrong picture (X60).
        let edges = vec![0u32; MAX_VERTICES + 2];
        assert_eq!(
            edge_vertices(3, &triangle(), &edges, DMat4::IDENTITY, EDGE_INK),
            Err(RenderError::EdgeListTooLarge { index: 3 })
        );
    }

    #[test]
    fn every_drawable_of_a_dense_frame_keeps_all_of_its_edges() {
        // X60: one flattened list over the whole frame cut the wireframe at 4 M vertices, so a
        // 5 000-sphere scene drew edges on about 1 400 balls. A buffer per drawable makes the budget
        // a per-buffer limit, so a frame past it keeps every drawable's lines.
        let mesh = triangle();
        let edges: Vec<u32> = (0..2_880).map(|index| (index % 3) as u32).collect();
        let transform = DMat4::IDENTITY;
        let drawables = 1_500;
        let mut total = 0usize;
        for _ in 0..drawables {
            let vertices = edge_vertices(0, &mesh, &edges, transform, EDGE_INK).expect("a drawable's lines");
            assert_eq!(vertices.len(), 2_880, "the list is never cut short");
            total += vertices.len();
        }
        assert!(
            total > MAX_VERTICES,
            "the frame holds {total} line vertices, past the {MAX_VERTICES} one buffer takes"
        );
    }

    #[test]
    fn the_all_edges_style_reads_the_full_triangulation_list() {
        let mesh = triangle();
        let feature: Arc<[u32]> = Arc::from(vec![0, 1]);
        let all: Arc<[u32]> = Arc::from(vec![0, 1, 1, 2, 2, 0]);
        let ink = EDGE_INK;

        let form = edge_vertices(0, &mesh, &feature, DMat4::IDENTITY, ink).expect("the form edges");
        let every = edge_vertices(0, &mesh, &all, DMat4::IDENTITY, ink).expect("every triangle edge");
        assert_eq!(form.len(), 2);
        assert_eq!(every.len(), 6);
    }

    #[test]
    fn the_outline_ink_is_black_under_every_theme() {
        // U108: the outline is a drawing line over a material face, so one black ink serves both
        // variants instead of following the background.
        const { assert!(EDGE_INK[0] < 0.1 && EDGE_INK[1] < 0.1 && EDGE_INK[2] < 0.1, "the ink is black") };
        // The strength is the ink's opacity, so zero leaves the fill alone and a hostile value is
        // opaque rather than invisible (U107).
        assert_eq!(edge_ink(0.4)[3], 0.4);
        assert_eq!(edge_ink(0.0)[3], 0.0);
        assert_eq!(edge_ink(f32::NAN)[3], 1.0);
        assert_eq!(edge_ink(0.4)[..3], EDGE_INK[..3], "a strength never recolours the line");
    }

    /// A device and a queue with no surface behind them, or the sentence that says why there is
    /// none; the ignored pixel test below uses it.
    fn gpu_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    }

    /// A device with no adapter and no surface behind it.
    ///
    /// The noop backend builds real wgpu buffers without a GPU, so the upload cache is asserted in
    /// a plain `cargo test` run: what the test compares is the buffer objects themselves, not a
    /// counter this crate keeps about them.
    fn noop_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::NOOP,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    }

    #[test]
    fn an_upload_keeps_the_buffers_of_the_drawables_it_did_not_touch() {
        let Some((device, queue)) = noop_device() else {
            println!("this build has no noop backend to upload geometry with");
            return;
        };
        let id = |value| Id::new(value).expect("id");
        let mesh = Arc::new(triangle());
        let edges: Arc<[u32]> = Arc::from(vec![0, 1, 1, 2]);
        let drawable =
            |id, transform| Drawable::new(id, Arc::clone(&mesh), transform, [1.0; 4]).with_edges(Arc::clone(&edges));
        let mut frame = SceneFrame::default();
        frame.push(drawable(id(1), DMat4::IDENTITY));
        frame.push(drawable(id(2), DMat4::from_translation(DVec3::new(100.0, 0.0, 0.0))));

        let mut renderer = ViewportRenderer::new(&device, 320, 240);
        renderer.upload(&device, &queue, &frame).expect("the first upload");
        assert_eq!(renderer.faces.len(), 1, "both drawables share one mesh, so one vertex and index buffer pair");
        assert_eq!(renderer.edge_slots.len(), 2, "the lines are baked with their transform");
        let standing_faces = Arc::clone(&renderer.drawables[0].faces);
        let standing_edges = Arc::clone(renderer.drawables[0].edges.as_ref().expect("the first drawable's lines"));
        let moved_faces = Arc::clone(&renderer.drawables[1].faces);
        let grid = Arc::clone(renderer.grid.value().expect("the default frame has a grid"));

        // The same frame again, which is what a camera move or a theme change produces: no buffer
        // is created and every drawable still draws the ones the first upload built.
        renderer.upload(&device, &queue, &frame).expect("a second upload of the same frame");
        assert!(Arc::ptr_eq(&standing_faces, &renderer.drawables[0].faces), "the faces keep their buffers");
        assert!(
            Arc::ptr_eq(&standing_edges, renderer.drawables[0].edges.as_ref().expect("the lines")),
            "and so do the edge lines"
        );
        assert!(Arc::ptr_eq(&grid, renderer.grid.value().expect("the grid")), "the grid is not rebuilt either");

        // One drawable moves: it gets new buffers, its neighbour keeps both of its own.
        frame.drawables[1].world_transform = DMat4::from_translation(DVec3::new(0.0, 50.0, 0.0));
        renderer.upload(&device, &queue, &frame).expect("the edited frame");
        assert!(
            Arc::ptr_eq(&standing_faces, &renderer.drawables[0].faces),
            "an untouched drawable keeps the vertex and index buffer it was uploaded with"
        );
        assert!(
            Arc::ptr_eq(&standing_edges, renderer.drawables[0].edges.as_ref().expect("the lines")),
            "its edge list is not rebuilt either"
        );
        assert!(!Arc::ptr_eq(&moved_faces, &renderer.drawables[1].faces), "the moved drawable gets new buffers");
        assert!(
            Arc::ptr_eq(&grid, renderer.grid.value().expect("the grid")),
            "an edit that moves a body rebuilds no frame-level buffer"
        );

        // A drawable that leaves the frame takes its buffers and its pinned mesh with it.
        frame.drawables.pop();
        renderer.upload(&device, &queue, &frame).expect("the frame without the moved drawable");
        assert_eq!(renderer.drawables.len(), 1);
        assert_eq!(renderer.faces.len(), 1, "the dropped drawable's buffers are freed");
        assert_eq!(renderer.edge_slots.len(), 1);
    }

    /// Copies the renderer's colour target back, which is what the preview will do (J10).
    fn read_pixels(renderer: &ViewportRenderer, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
        renderer.read_color(device, queue).expect("the picture can be read back")
    }

    /// How many pixels carry `color`, within a rounding step per channel.
    fn pixels_of(pixels: &[u8], color: [f32; 4]) -> usize {
        let expected = color.map(|channel| (channel * 255.0).round() as i32);
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| {
                pixel.iter().enumerate().all(|(channel, value)| (i32::from(*value) - expected[channel]).abs() <= 2)
            })
            .count()
    }

    #[test]
    #[ignore = "needs a GPU adapter; the edge pass draws one buffer per drawable"]
    fn the_edge_pass_draws_every_drawable_own_lines() {
        let Some((device, queue)) = gpu_device() else {
            println!("this machine has no GPU adapter to draw the edge pass with");
            return;
        };
        let id = |value| Id::new(value).expect("id");
        // Two equal boxes far apart, each with the four edges of its front triangle, so a pixel
        // count per half of the target says whether both drawables' own buffers were drawn.
        let mut mesh = triangle();
        mesh.positions = vec![[0.0, 0.0, 0.0], [500.0, 0.0, 0.0], [0.0, 500.0, 0.0]];
        let mesh = Arc::new(mesh);
        let edges: Arc<[u32]> = Arc::from(vec![0, 1, 1, 2, 2, 0]);
        let drawable = |id, y| {
            Drawable::new(id, Arc::clone(&mesh), DMat4::from_translation(DVec3::new(0.0, y, 0.0)), [0.3; 4])
                .with_edges(Arc::clone(&edges))
        };
        // Looking straight down at the XY plane, so screen x is world x and each half of the target
        // holds exactly one drawable.
        let camera = Camera::default().looked_from(DVec3::new(0.0, 0.0, 2000.0), DVec3::X, DVec3::Y, -DVec3::Z);
        let mut frame = SceneFrame {
            camera,
            shading: ShadingStyle::Wireframe,
            grid: Grid { visible: false, ..Grid::default() },
            ..SceneFrame::default()
        };
        frame.push(drawable(id(1), -700.0));
        frame.push(drawable(id(2), 200.0));
        // The readback is row-major, top row first, so the first half of the picture is the upper
        // half of the view and holds the drawable at y = 200.

        let mut renderer = ViewportRenderer::new(&device, 800, 400);
        renderer.upload(&device, &queue, &frame).expect("the frame uploads");
        renderer.render(&device, &queue, &frame);
        let pixels = read_pixels(&renderer, &device, &queue);
        let ink = crate::geometry::edge_ink(frame.edge_strength);
        let half = pixels.len() / 2;
        let upper = pixels_of(&pixels[..half], ink);
        let lower = pixels_of(&pixels[half..], ink);
        assert_eq!(renderer.edge_slots.len(), 2, "each drawable owns one edge buffer");
        assert!(upper > 20, "the upper drawable's own line buffer is drawn, {upper} pixels");
        assert!(lower > 20, "and the lower drawable's is drawn too, from its own buffer: {lower} pixels");
    }

    /// A square board `side` millimetres across, facing `+Z`, with its zero corner at the minimum.
    ///
    /// That is the panel frame `ADR 0004` describes, and it is what the decor's projection starts
    /// from: a print begins at the board's own corner, not at its centre.
    fn board(side: f64) -> MeshData {
        let side = side as f32;
        MeshData {
            positions: vec![[0.0, 0.0, 0.0], [side, 0.0, 0.0], [side, side, 0.0], [0.0, side, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }

    /// The board's placement: its zero corner moved to the middle of the view.
    fn centred(side: f64) -> DMat4 {
        DMat4::from_translation(DVec3::new(-side * 0.5, -side * 0.5, 0.0))
    }

    /// A two-tone decor: the left half of the picture red, its right half blue, covering `covers`.
    fn striped(covers: i64, turns: scene::Turns) -> Decor {
        let texels = vec![[255, 0, 0, 255], [255, 0, 0, 255], [0, 0, 255, 255], [0, 0, 255, 255]];
        let image = Arc::new(scene::DecorImage::new(4, 1, texels).expect("four texels"));
        let id = DecorId::new(1).expect("one decor");
        let covers = units::Length::from_millimetres(covers).expect("a length");
        Decor::new(id, image, covers, turns).expect("a decor")
    }

    /// Counts the lit pixels of one rectangle whose red channel leads their blue one, and the other
    /// way round: `(red, blue)`.
    ///
    /// The readback is row-major from the top, so a rectangle is named by its columns and its rows.
    /// A pixel has to be brighter than `lit` in the channel it leads with, so the viewport's own
    /// background — a dark grey of neither colour — is not counted as either half of a picture.
    fn red_minus_blue(
        pixels: &[u8],
        width: u32,
        columns: std::ops::Range<u32>,
        rows: std::ops::Range<u32>,
        lit: u8,
        lead: i32,
    ) -> (usize, usize) {
        let mut red = 0;
        let mut blue = 0;
        for y in rows {
            for x in columns.clone() {
                let index = (y as usize * width as usize + x as usize) * 4;
                let Some(pixel) = pixels.get(index..index + 4) else {
                    continue;
                };
                let difference = i32::from(pixel[0]) - i32::from(pixel[2]);
                if difference > lead && pixel[0] > lit {
                    red += 1;
                } else if difference < -lead && pixel[2] > lit {
                    blue += 1;
                }
            }
        }
        (red, blue)
    }

    #[test]
    fn one_decor_is_one_texture_however_many_panels_are_cut_from_it() {
        let Some((device, queue)) = noop_device() else {
            println!("this build has no noop backend to upload a decor with");
            return;
        };
        let id = |value| Id::new(value).expect("id");
        let mesh = Arc::new(board(600.0));
        let mut frame = SceneFrame::default();
        for value in 1..=3 {
            let drawable = Drawable::new(id(value), Arc::clone(&mesh), DMat4::IDENTITY, [1.0; 4])
                .with_decor(striped(600, scene::Turns::ALONG));
            frame.push(drawable);
        }
        // A fourth drawable cut from a plain board: it is bound to the white texel, not to a decor.
        frame.push(Drawable::new(id(4), Arc::clone(&mesh), DMat4::IDENTITY, [1.0; 4]));

        let mut renderer = ViewportRenderer::new(&device, 200, 200);
        renderer.upload(&device, &queue, &frame).expect("the frame uploads");
        assert_eq!(renderer.decors.len(), 1, "three panels of one board upload one texture");
        assert!(renderer.white_decor.is_some(), "and the plain board is bound to the white texel");
        assert_eq!(renderer.faces.len(), 1, "the three decorated boards share their vertices");

        // A board whose decor is turned is a different picture at a different coordinate, so its
        // vertices are built again while the picture itself stays one texture per id.
        frame.drawables[0] = Drawable::new(id(1), Arc::clone(&mesh), DMat4::IDENTITY, [1.0; 4])
            .with_decor(striped(600, scene::Turns::ACROSS));
        renderer.upload(&device, &queue, &frame).expect("the turned decor uploads");
        assert_eq!(renderer.decors.len(), 1, "a turn changes the coordinate, not the picture");
        assert_eq!(renderer.faces.len(), 2, "the turned board is built again, its two copies are not");
    }

    #[test]
    #[ignore = "needs a GPU adapter; draws a striped board and reads the pixels back"]
    fn a_decor_is_drawn_where_the_board_puts_it_and_a_turn_rotates_it() {
        let Some((device, queue)) = gpu_device() else {
            println!("this machine has no GPU adapter to draw a decor with");
            return;
        };
        let id = Id::new(1).expect("id");
        let mesh = Arc::new(board(600.0));
        // Looking straight down at the board from above, so screen x is the board's x and screen y
        // is its y: the readback's left half is the board's negative x, its top half the positive y.
        let camera = Camera::default().looked_from(DVec3::new(0.0, 0.0, 2000.0), DVec3::X, DVec3::Y, -DVec3::Z);
        let frame_with = |turns| {
            let mut frame = SceneFrame {
                camera,
                shading: ShadingStyle::Shaded,
                grid: Grid { visible: false, ..Grid::default() },
                ..SceneFrame::default()
            };
            frame.lighting.key_direction = DVec3::new(0.0, 0.0, 1.0);
            frame.lighting.key_color = [1.0, 1.0, 1.0];
            frame.lighting.ambient = 0.0;
            frame.push(
                Drawable::new(id, Arc::clone(&mesh), centred(600.0), [1.0, 1.0, 1.0, 1.0])
                    .with_decor(striped(600, turns)),
            );
            frame
        };

        let (_, straight) = shot(&device, &queue, &frame_with(scene::Turns::ALONG), 200);
        let all_rows = 0..200;
        let (left_red, left_blue) = red_minus_blue(&straight, 200, 0..100, all_rows.clone(), 128, 24);
        let (right_red, right_blue) = red_minus_blue(&straight, 200, 100..200, all_rows.clone(), 128, 24);
        assert!(left_red > 200, "the board's left half is the picture's red half: {left_red} pixels");
        assert!(right_blue > 200, "and its right half is the blue one: {right_blue} pixels");
        assert!(left_blue < 20 && right_red < 20, "{left_blue} and {right_red} are the wrong way round");

        // A quarter turn: the picture's own width — its grain — now runs along the board's y, so the
        // stripes cross the board instead of running along it.
        let (_, turned) = shot(&device, &queue, &frame_with(scene::Turns::ACROSS), 200);
        let all_columns = 0..200;
        let (top_red, top_blue) = red_minus_blue(&turned, 200, all_columns.clone(), 0..100, 128, 24);
        let (bottom_red, bottom_blue) = red_minus_blue(&turned, 200, all_columns, 100..200, 128, 24);
        assert!(top_blue > 200 && bottom_red > 200, "the stripes cross the board: {top_blue} and {bottom_red}");
        assert!(top_red < 20 && bottom_blue < 20, "and they are the other way round: {top_red} and {bottom_blue}");
    }

    #[test]
    #[ignore = "needs a GPU adapter; writes target/ui-shots/product-envelope-*.png"]
    fn the_product_envelope_draws_both_inks_and_nothing_when_it_has_parts() {
        let Some((device, queue)) = gpu_device() else {
            println!("this machine has no GPU adapter to render the product envelope with");
            return;
        };
        let camera = Camera::new(DVec3::new(0.0, 0.0, 350.0), 4500.0).expect("camera");
        let mut frame = SceneFrame { camera, ..SceneFrame::default() };
        let box_size = DVec3::new(500.0, 400.0, 700.0);
        for (id, x, state) in [
            (1, -800.0, ProductEnvelopeState::Open),
            (2, 0.0, ProductEnvelopeState::ClosedEmpty),
            (3, 800.0, ProductEnvelopeState::ClosedWithParts),
        ] {
            frame.push_product_envelope(ProductEnvelope::rectangular(
                Id::new(id).expect("id"),
                box_size,
                DMat4::from_translation(DVec3::new(x, 0.0, 0.0)),
                state,
            ));
        }
        let mut renderer = ViewportRenderer::new(&device, 1280, 800);
        let directory = testkit::ui_shots_dir().expect("the shots directory is created");

        for (name, colors) in [("dark", SceneColors::DARK), ("light", SceneColors::LIGHT)] {
            frame.apply_colors(colors);
            renderer.upload(&device, &queue, &frame).expect("the envelopes upload");
            renderer.render(&device, &queue, &frame);
            let pixels = read_pixels(&renderer, &device, &queue);
            let path = directory.join(format!("product-envelope-{name}.png"));
            testkit::write_png(&path, 1280, 800, &pixels).expect("the shot is written");
            let active = pixels_of(&pixels, colors.product_envelope_active);
            let inactive = pixels_of(&pixels, colors.product_envelope_inactive);
            assert!(active > 20, "{name}: the open envelope is drawn, {active} pixels");
            assert!(inactive > 20, "{name}: the closed empty envelope is drawn, {inactive} pixels");
        }

        // A closed product with parts keeps no line at all: the same frame without the other two
        // boxes paints neither ink.
        let mut hidden = frame.clone();
        hidden.product_envelopes.retain(|envelope| envelope.state == ProductEnvelopeState::ClosedWithParts);
        renderer.upload(&device, &queue, &hidden).expect("the hidden envelope uploads");
        renderer.render(&device, &queue, &hidden);
        let pixels = read_pixels(&renderer, &device, &queue);
        let colors = SceneColors::DARK;
        assert_eq!(pixels_of(&pixels, colors.product_envelope_active), 0);
        assert_eq!(pixels_of(&pixels, colors.product_envelope_inactive), 0, "a piece with parts draws no box");
    }

    /// Uploads one frame into a fresh target, renders it and reads the pixels back.
    fn shot(device: &wgpu::Device, queue: &wgpu::Queue, frame: &SceneFrame, side: u32) -> (ViewportRenderer, Vec<u8>) {
        let mut renderer = ViewportRenderer::new(device, side, side);
        renderer.upload(device, queue, frame).expect("the frame uploads");
        renderer.render(device, queue, frame);
        let pixels = read_pixels(&renderer, device, queue);
        (renderer, pixels)
    }

    #[test]
    fn the_ground_mask_and_the_shadow_map_turn_on_one_light() {
        // ADR 0142 and 0143: the floor's silhouette and a panel's occlusion are cast by one sun, so
        // the two keys must quantise the same direction and turn on the same input.
        let light = DVec3::new(-0.35, 0.55, 0.75);
        let directions = [
            light,
            light * 1.00005,
            DVec3::new(0.35, 0.55, 0.75),
            DVec3::new(0.0, 0.0, 1.0),
            DVec3::new(0.2, -0.4, 0.9),
        ];
        for left in directions {
            for right in directions {
                let ground = GroundKey::of(1, left, 5000.0) == GroundKey::of(1, right, 5000.0);
                let shadow = ShadowKey::of(1, left) == ShadowKey::of(1, right);
                assert_eq!(ground, shadow, "the two masks disagree about {left:?} and {right:?}");
            }
        }
    }

    #[test]
    fn a_shadow_map_is_drawn_on_an_upload_and_a_turned_sun_and_never_on_an_orbit() {
        // The probe the task asks for: a world-space map is redrawn when the scene or the sun moves
        // and costs nothing while the camera orbits, which is most frames of a session.
        let Some((device, queue)) = noop_device() else {
            println!("this build has no noop backend to draw a shadow map with");
            return;
        };
        // A right-handed basis: looking down `-Y` with `+Z` up puts `-X` on the right.
        let camera = Camera::default().looked_from(DVec3::new(0.0, 2000.0, 350.0), -DVec3::X, DVec3::Z, -DVec3::Y);
        let box_ = (1, DVec3::ZERO, DVec3::new(100.0, 100.0, 100.0));
        let mut frame = shaded_scene(camera, DVec3::new(0.0, 0.8, 0.6), &[box_], ContactShadows::default());
        let mut renderer = ViewportRenderer::new(&device, 200, 200);
        renderer.upload(&device, &queue, &frame).expect("the frame uploads");
        renderer.render(&device, &queue, &frame);
        assert_eq!(renderer.shadow_rebuilds(), 1, "an upload draws the map once");

        frame.camera.orbit(0.3, -0.2);
        renderer.render(&device, &queue, &frame);
        renderer.render(&device, &queue, &frame);
        assert_eq!(renderer.shadow_rebuilds(), 1, "orbiting redraws nothing");

        frame.lighting.key_direction = DVec3::new(0.3, -0.5, 0.8);
        renderer.render(&device, &queue, &frame);
        assert_eq!(renderer.shadow_rebuilds(), 2, "a turned sun redraws it");

        renderer.upload(&device, &queue, &frame).expect("the edited frame uploads");
        renderer.render(&device, &queue, &frame);
        assert_eq!(renderer.shadow_rebuilds(), 3, "an edit redraws it");
    }

    #[test]
    #[ignore = "needs a GPU adapter; a shelf darkens the panel behind it"]
    fn a_shelf_darkens_the_panel_behind_it_and_the_open_cabinet_does_not() {
        // The task's picture, measured: a shelf 300 mm inside an open cabinet stands between the sun
        // and the back panel, so the band under it darkens; remove the shelf — or the shadow — and
        // the same band is lit again.
        let Some((device, queue)) = gpu_device() else {
            println!("this machine has no GPU adapter to draw a shadow map with");
            return;
        };
        const SIDE: u32 = 640;
        // A right-handed basis: looking down `-Y` with `+Z` up puts `-X` on the right.
        let camera = Camera::default().looked_from(DVec3::new(0.0, 2000.0, 350.0), -DVec3::X, DVec3::Z, -DVec3::Y);
        let light = DVec3::new(0.0, 0.8, 0.6);
        let on = ContactShadows { visible: true, strength: 0.65, color: [0.0, 0.0, 0.0] };
        let off = ContactShadows { visible: false, ..on };
        let panel = (1, DVec3::new(-300.0, -18.0, 0.0), DVec3::new(300.0, 0.0, 700.0));
        let shelf = (2, DVec3::new(-250.0, 0.0, 300.0), DVec3::new(250.0, 300.0, 318.0));
        let size = [SIDE, SIDE];

        let hidden = projected_rect(camera, size, DVec3::new(-100.0, 0.0, 150.0), DVec3::new(100.0, 0.0, 280.0));
        let lit_above = projected_rect(camera, size, DVec3::new(-100.0, 0.0, 400.0), DVec3::new(100.0, 0.0, 600.0));
        assert!(hidden.2 > 20 && hidden.3 > 20, "the measured band is on screen: {hidden:?}");

        let (_, with_shelf) = shot(&device, &queue, &shaded_scene(camera, light, &[panel, shelf], on), SIDE);
        let (_, with_shelf_no_shadow) = shot(&device, &queue, &shaded_scene(camera, light, &[panel, shelf], off), SIDE);
        let (_, open_cabinet) = shot(&device, &queue, &shaded_scene(camera, light, &[panel], on), SIDE);

        // The picture beside the numbers, for the eye that has to accept it: the shelf inside an open
        // cabinet, and the same cabinet with the shelf taken out.
        if let Ok(directory) = testkit::ui_shots_dir() {
            for (name, pixels) in [("shelf", &with_shelf), ("open", &open_cabinet)] {
                let path = directory.join(format!("shadow-map-{name}.png"));
                testkit::write_png(&path, SIDE, SIDE, pixels).expect("the picture is written");
                println!("the shadow map at {name} is in {}", path.display());
            }
        }

        let shadowed = mean_luma(&with_shelf, size, hidden);
        let unshadowed = mean_luma(&with_shelf_no_shadow, size, hidden);
        assert!(
            shadowed < unshadowed - 20.0,
            "the band under the shelf darkens: {shadowed:.1} against {unshadowed:.1}"
        );

        let opened = mean_luma(&open_cabinet, size, hidden);
        assert!(
            (opened - unshadowed).abs() < 3.0,
            "opened to the sky the band is lit again: {opened:.1} against {unshadowed:.1}"
        );

        // The shadow is the shelf's, not a global dimming: above the shelf the panel is unchanged.
        let above_with = mean_luma(&with_shelf, size, lit_above);
        let above_open = mean_luma(&open_cabinet, size, lit_above);
        assert!(
            (above_with - above_open).abs() < 3.0,
            "the panel above the shelf keeps its light: {above_with:.1} against {above_open:.1}"
        );
    }

    #[test]
    #[ignore = "needs a GPU adapter; the floor tints what stands under it and keeps its shadow"]
    fn a_see_through_floor_tints_the_part_below_it_and_keeps_its_shadow() {
        // U109: the plate is blended now, so one scene drawn at 100 %, 40 % and 0 % is three
        // pictures of one rule — an opaque floor hides what stands under it, a translucent one tints
        // it, an empty one draws neither plate nor shadow. Two patches of floor carry the shadow: one
        // the standing box shades, one it does not, both away from every body so no geometry but
        // plate, mask and background decides the pixel.
        let Some((device, queue)) = gpu_device() else {
            println!("this machine has no GPU adapter to draw a see-through floor with");
            return;
        };
        const SIDE: u32 = 512;
        // A right-handed basis: looking from `-Y` and above, so screen right is `+X` and screen up is
        // `+Z`. The mirror `(X, up, forward)` would flip the winding and make every face a back face.
        let eye = DVec3::new(0.0, -3000.0, 2500.0);
        let forward = (DVec3::ZERO - eye).try_normalize().expect("a view direction");
        let right = forward.cross(DVec3::Z).try_normalize().expect("a right vector");
        let up = right.cross(forward);
        let camera = Camera::default().looked_from(eye, right, up, forward);
        let light = DVec3::new(-0.4, 0.3, 0.85);
        // A strength above the plate's opacity: this is the case the `max()` rule exists for.
        let shadows = ContactShadows { visible: true, strength: 0.9, color: [0.0, 0.0, 0.0] };
        let standing = (1, DVec3::new(0.0, -200.0, 0.0), DVec3::new(400.0, 200.0, 600.0));
        let below = (2, DVec3::new(-700.0, -500.0, -800.0), DVec3::new(-300.0, -100.0, -500.0));
        let size = [SIDE, SIDE];
        let frame = |opacity: f32| {
            let mut frame = shaded_scene(camera, light, &[standing, below], shadows);
            frame.floor_opacity = opacity;
            frame
        };
        let (_, full) = shot(&device, &queue, &frame(1.0), SIDE);
        let (_, tinted) = shot(&device, &queue, &frame(0.4), SIDE);
        let (_, empty) = shot(&device, &queue, &frame(0.0), SIDE);

        let patch = |min: DVec3, max: DVec3| projected_rect(camera, size, min, max);
        // Up-sun of everything, so the mask is zero there.
        let lit = patch(DVec3::new(-1100.0, 300.0, 0.0), DVec3::new(-600.0, 700.0, 0.0));
        // Down-sun of the standing box and clear of the box's own screen footprint.
        let shaded = patch(DVec3::new(500.0, -330.0, 0.0), DVec3::new(650.0, -160.0, 0.0));
        let part = patch(DVec3::new(-700.0, -500.0, -800.0), DVec3::new(-300.0, -100.0, -500.0));
        for (name, rect) in [("the lit floor", lit), ("the shaded floor", shaded), ("the part below", part)] {
            assert!(rect.2 > 20 && rect.3 > 12, "{name} is on screen: {rect:?}");
        }

        // An opaque floor looks as the floor always did: its own colour, darkened under the shadow.
        let full_lit = mean_luma(&full, size, lit);
        let full_shaded = mean_luma(&full, size, shaded);
        assert!(
            full_lit > full_shaded + 15.0,
            "the shadow darkens the opaque floor: {full_shaded:.1} against {full_lit:.1}"
        );

        // The part under the floor is hidden by an opaque plate, seen through a translucent one.
        let part_full = mean_luma(&full, size, part);
        let part_tinted = mean_luma(&tinted, size, part);
        let part_none = mean_luma(&empty, size, part);
        assert!(
            part_tinted > part_full + 10.0,
            "the part below is seen through the tint: {part_tinted:.1} against the opaque floor's {part_full:.1}"
        );
        assert!(part_tinted < part_none - 10.0, "and it is tinted, not drawn plainly: {part_none:.1} without a plate");

        // The shadow survives the transparency, and keeps its weight rather than fading with the
        // plate: the `max()` rule puts it above the plate's own opacity.
        let tinted_lit = mean_luma(&tinted, size, lit);
        let tinted_shaded = mean_luma(&tinted, size, shaded);
        assert!(
            tinted_lit > tinted_shaded + 15.0,
            "the shadow is still on the see-through floor: {tinted_shaded:.1} against {tinted_lit:.1}"
        );
        assert!(
            tinted_shaded < tinted_lit * 0.4,
            "a shadow stronger than the opacity keeps its weight: {tinted_shaded:.1} of {tinted_lit:.1}"
        );
        assert!(
            (tinted_lit - full_lit).abs() > 5.0,
            "and the plate is genuinely see-through: {tinted_lit:.1} against {full_lit:.1}"
        );

        // An empty floor draws no plate and no shadow, because a shadow needs a surface to darken.
        let background = frame(0.0).background;
        let expected = f64::from(background[0]) + f64::from(background[1]) + f64::from(background[2]);
        let expected = expected / 3.0 * 255.0;
        let empty_lit = mean_luma(&empty, size, lit);
        let empty_shaded = mean_luma(&empty, size, shaded);
        assert!(
            (empty_lit - expected).abs() < 3.0,
            "an empty floor clears to the background: {empty_lit:.1} against {expected:.1}"
        );
        assert!((empty_lit - empty_shaded).abs() < 3.0, "and it carries no shadow either");
    }

    #[test]
    #[ignore = "needs a GPU adapter; a lit box shows no acne and no gap under a band"]
    fn a_box_never_shadows_itself_and_a_flush_band_casts_nothing() {
        // The bias decides this picture: with the map on, a lone convex box must look exactly like
        // the same box with the shadow switched off — any darker pixel is the box comparing against
        // itself — and a band lying flush on its top face must add no shadow of its own.
        let Some((device, queue)) = gpu_device() else {
            println!("this machine has no GPU adapter to draw a shadow map with");
            return;
        };
        const SIDE: u32 = 512;
        // Above the box, so its lit top face — the one the band lies on — fills the picture.
        let eye = DVec3::new(1200.0, -1900.0, 2100.0);
        let forward = (DVec3::new(0.0, 0.0, 1200.0) - eye).try_normalize().expect("a view direction");
        let right = forward.cross(DVec3::Z).try_normalize().expect("a right vector");
        let up = right.cross(forward);
        let camera = Camera::default().looked_from(eye, right, up, forward);
        let light = DVec3::new(-0.3, 0.4, 0.87);
        let on = ContactShadows { visible: true, strength: 0.65, color: [0.0, 0.0, 0.0] };
        let off = ContactShadows { visible: false, ..on };
        let body = (1, DVec3::new(-200.0, -200.0, 1000.0), DVec3::new(200.0, 200.0, 1400.0));
        let band = (2, DVec3::new(-120.0, -120.0, 1400.0), DVec3::new(120.0, 120.0, 1400.0));
        // A ground of no size draws no plate at all, so the whole picture is the box: with the plate
        // in frame its own silhouette would change between the two renders and hide the acne.
        let frame = |boxes: &[(u64, DVec3, DVec3)], shadows: ContactShadows| {
            let mut frame = shaded_scene(camera, light, boxes, shadows);
            frame.grid.extent_mm = 0.0;
            frame
        };

        let (_, alone) = shot(&device, &queue, &frame(&[body], on), SIDE);
        let (_, alone_off) = shot(&device, &queue, &frame(&[body], off), SIDE);
        let (_, banded) = shot(&device, &queue, &frame(&[body, band], on), SIDE);
        let (_, banded_off) = shot(&device, &queue, &frame(&[body, band], off), SIDE);

        // A comparison only means something if there is light to lose: the box's top face is lit.
        let brightest = alone
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| (u32::from(pixel[0]) + u32::from(pixel[1]) + u32::from(pixel[2])) / 3)
            .max()
            .unwrap_or(0);
        assert!(brightest > 190, "the picture holds a lit face, brightest {brightest}");

        let self_shadow = darkest_change(&alone, &alone_off);
        assert!(self_shadow <= 3, "the box never shadows itself, worst pixel {self_shadow} levels darker");
        let band_gap = darkest_change(&banded, &banded_off);
        assert!(band_gap <= 3, "the flush band casts nothing on the face it lies on, worst pixel {band_gap}");
        let band_itself = darkest_change(&banded, &alone);
        assert!(band_itself <= 3, "a band at the face's own depth changes no pixel, worst {band_itself}");
    }

    #[test]
    #[ignore = "needs a GPU adapter; a shelf flush with a side shadows nothing at the joint"]
    fn a_shelf_flush_with_a_side_leaves_the_panel_above_it_lit() {
        // The second coplanar case: a shelf whose end meets a side panel's inner face exactly. The
        // panel just above the joint must keep its light — a bias that leaks here draws a dark seam
        // along every shelf in every cabinet.
        let Some((device, queue)) = gpu_device() else {
            println!("this machine has no GPU adapter to draw a shadow map with");
            return;
        };
        const SIDE: u32 = 512;
        let eye = DVec3::new(-1400.0, -400.0, 550.0);
        let forward = (DVec3::new(200.0, 50.0, 380.0) - eye).try_normalize().expect("a view direction");
        let right = forward.cross(DVec3::Z).try_normalize().expect("a right vector");
        let up = right.cross(forward);
        let camera = Camera::default().looked_from(eye, right, up, forward);
        let light = DVec3::new(-0.45, 0.3, 0.84);
        let on = ContactShadows { visible: true, strength: 0.65, color: [0.0, 0.0, 0.0] };
        let side = (1, DVec3::new(200.0, -250.0, 0.0), DVec3::new(218.0, 250.0, 700.0));
        let shelf = (2, DVec3::new(-200.0, -250.0, 300.0), DVec3::new(200.0, 250.0, 318.0));
        let size = [SIDE, SIDE];
        // The inner face of the side panel, just above the shelf's top face.
        let joint = projected_rect(camera, size, DVec3::new(200.0, -100.0, 322.0), DVec3::new(200.0, 100.0, 345.0));
        assert!(joint.2 > 10 && joint.3 > 10, "the joint is on screen: {joint:?}");

        let (_, with_shelf) = shot(&device, &queue, &shaded_scene(camera, light, &[side, shelf], on), SIDE);
        let (_, without_shelf) = shot(&device, &queue, &shaded_scene(camera, light, &[side], on), SIDE);
        let banded = mean_luma(&with_shelf, size, joint);
        let clean = mean_luma(&without_shelf, size, joint);
        assert!(clean > 120.0, "the measured strip is lit by the sun: {clean:.1}");
        assert!(
            (banded - clean).abs() < 3.0,
            "the shelf leaves the panel above it lit: {banded:.1} against {clean:.1}"
        );
    }

    #[test]
    fn a_view_draws_only_the_drawables_its_volume_meets() {
        // X58: the passes walked the whole frame whatever the camera saw. The view volume is now
        // tested once per drawable before a pass names a buffer, and the list serves faces and
        // edges alike, so a drawable off screen costs neither.
        let Some((device, queue)) = noop_device() else {
            println!("this build has no noop backend to render the culling test with");
            return;
        };
        let id = |value| Id::new(value).expect("id");
        let mesh = Arc::new(triangle());
        let edges: Arc<[u32]> = Arc::from(vec![0, 1, 1, 2, 2, 0]);
        let mut frame = SceneFrame {
            camera: Camera::default().looked_from(DVec3::new(0.0, 0.0, 1_000.0), DVec3::X, DVec3::Y, -DVec3::Z),
            grid: Grid { visible: false, ..Grid::default() },
            ..SceneFrame::default()
        };
        // Five balls in a row three metres apart: a 45-degree view from the middle sees one at that
        // distance, and only the camera's own pull-back sees all five.
        for (index, x) in [-3_000.0, -1_500.0, 0.0, 1_500.0, 3_000.0].into_iter().enumerate() {
            let transform = DMat4::from_translation(DVec3::new(x, 0.0, 0.0));
            let drawable = Drawable::new(id(index as u64 + 1), Arc::clone(&mesh), transform, [0.6; 4])
                .with_edges(Arc::clone(&edges));
            frame.push(drawable);
        }

        let mut renderer = ViewportRenderer::new(&device, 200, 200);
        renderer.upload(&device, &queue, &frame).expect("the field uploads");
        renderer.render(&device, &queue, &frame);
        assert_eq!(renderer.drawn_drawables(), 1, "the camera inside the row draws the ball in front of it");
        assert_eq!(renderer.drawables.len(), 5, "the culling drops drawables from the passes, not from the upload");
        let drawn = &renderer.visible[0];
        assert!(renderer.drawables[*drawn].edges.is_some(), "a drawn drawable keeps its edge list");

        // Framing the whole row draws every drawable again.
        frame.camera = Camera::new(DVec3::ZERO, 20_000.0).expect("camera");
        renderer.render(&device, &queue, &frame);
        assert_eq!(renderer.drawn_drawables(), 5, "a camera far enough draws the whole row");
        assert!(renderer.drawables.iter().all(|drawn| drawn.edges.is_some()), "and every edge list with it");
    }

    #[test]
    fn the_scene_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(include_str!("scene.wgsl")).expect("the scene shader parses");
        let mut validator =
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
        validator.validate(&module).expect("the scene shader validates");
    }

    #[test]
    fn four_view_regions_tile_the_target_in_framebuffer_coordinates() {
        // A view rect and a scissor share the framebuffer origin, which is the top-left corner, so
        // the region is used unchanged. Flipping it would draw the top view's camera in the bottom
        // half of the texture while the shell still reads the pointer against the top rect.
        let target_height = 600;
        let regions = [
            ViewRegion { x: 0, y: 0, width: 400, height: 300 },
            ViewRegion { x: 400, y: 0, width: 400, height: 300 },
            ViewRegion { x: 0, y: 300, width: 400, height: 300 },
            ViewRegion { x: 400, y: 300, width: 400, height: 300 },
        ];
        let scissors: Vec<_> = regions.iter().map(|region| region.scissor(800, target_height)).collect();
        assert_eq!(scissors, vec![(0, 0, 400, 300), (400, 0, 400, 300), (0, 300, 400, 300), (400, 300, 400, 300)]);
        for (region, scissor) in regions.iter().zip(&scissors) {
            assert_eq!((scissor.0, scissor.1), (region.x, region.y), "the scissor keeps the top-left origin");
        }

        // A region reaching past the target clamps to it rather than making an invalid scissor.
        assert_eq!(ViewRegion { x: 0, y: 0, width: 10, height: 10 }.scissor(5, 5), (0, 0, 5, 5));
        // An offset past the far edge still yields a drawable one-pixel scissor inside the target.
        assert_eq!(ViewRegion { x: 900, y: 700, width: 10, height: 10 }.scissor(800, 600), (799, 599, 1, 1));
    }

    #[test]
    fn a_camera_slot_stride_is_aligned_for_a_dynamic_offset() {
        let uniform = std::mem::size_of::<CameraUniform>() as u64;
        // The default device alignment is 256; a slot must start on a multiple of it.
        let stride = uniform.next_multiple_of(256);
        assert!(stride >= uniform, "the stride never overlaps the next slot");
        assert_eq!(stride % 256, 0, "and keeps the dynamic offset aligned");
        assert!(stride * MAX_VIEWS as u64 > uniform, "four views need four distinct slots");
    }

    #[test]
    fn an_aspect_ratio_is_per_view_not_per_target() {
        // Looking straight down at the XY plane, so screen x is world x and each half of the target
        // holds exactly one drawable.
        let camera = Camera::default().looked_from(DVec3::new(0.0, 0.0, 2000.0), DVec3::X, DVec3::Y, -DVec3::Z);
        let whole = camera.view_projection(2.0);
        let half = camera.view_projection(1.0);
        assert_ne!(whole, half, "a narrow view must not borrow the target's aspect");
    }
}
