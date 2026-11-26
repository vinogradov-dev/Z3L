//! The device objects the renderer owns, one wrapper per buffer shape.
//!
//! Holds the uploaded shapes — a drawable's indexed triangles, a drawable's edge lines, a frame's
//! line list, and the contact shadow pass's indexed triangles — plus the two helpers that create a
//! buffer from vertices and the size clamp the offscreen target uses. What each shape is keyed by,
//! and which of them an upload may keep, is `cache.rs`.
//!
//! Contract: each wrapper owns buffers that live as long as the object and reports the counts the
//! draw call needs; a wrapper is either present and drawable or absent, never half-uploaded, so the
//! empty case is `None` instead of a zero-count buffer. Every vertex count is converted to `u32`
//! once, at upload, and a count that does not fit is [`RenderError::MeshTooLarge`].
//!
//! Refuses: an index list with no vertices, a vertex count past `u32`, and a target size of zero
//! (clamped to one pixel rather than rejected).
//!
//! Does not own frame data: a `GpuMesh` is the uploaded result of one drawable and holds no
//! document id, no colour and no transform, and this module knows neither the scene nor the draw
//! order. It also creates no pipeline: what a pass may do to depth and stencil is `pipelines.rs`.
//!
//! Changes when: what a buffer holds or how one frame's geometry reaches the device changes.

use geom::DVec3;
use scene::Decor;
use wgpu::util::DeviceExt as _;

use crate::RenderError;
use crate::geometry::MeshVertex;

/// One drawable's uploaded triangles and the values the draw calls need.
///
/// The centre travels with the buffers because it is derived from the same world vertices; the
/// stencil slot a drawable stamps is not here, because it is the drawable's place in the frame and
/// a kept buffer may move.
pub(crate) struct GpuMesh {
    pub(crate) vertex_buffer: wgpu::Buffer,
    pub(crate) index_buffer: wgpu::Buffer,
    pub(crate) index_count: u32,
    /// World-space centre, used to order translucent objects back to front.
    pub(crate) centre: DVec3,
}

/// One drawable's edge lines.
///
/// A buffer belongs to the mesh, not to the frame, so an edit that leaves the drawable alone keeps
/// its lines and a dense scene never rewrites one flattened list (X57).
pub(crate) struct GpuEdges {
    /// The line buffer, absent when no edge of the list could be drawn.
    pub(crate) vertex_buffer: Option<wgpu::Buffer>,
    pub(crate) vertex_count: u32,
}

impl GpuEdges {
    /// Uploads `vertices` as one line list, or nothing when the list draws no line.
    pub(crate) fn new(device: &wgpu::Device, label: &str, vertices: Vec<MeshVertex>) -> Result<Self, RenderError> {
        Ok(match line_list(device, label, vertices)? {
            Some(list) => Self { vertex_buffer: Some(list.vertex_buffer), vertex_count: list.vertex_count },
            None => Self { vertex_buffer: None, vertex_count: 0 },
        })
    }
}

/// One line-list vertex buffer: the grid and the frame's polylines each own one.
pub(crate) struct GpuLineList {
    pub(crate) vertex_buffer: wgpu::Buffer,
    pub(crate) vertex_count: u32,
}

/// One indexed triangle buffer: the contact-shadow pass owns one.
pub(crate) struct GpuTriangles {
    pub(crate) vertex_buffer: wgpu::Buffer,
    pub(crate) index_buffer: wgpu::Buffer,
    pub(crate) index_count: u32,
}

/// One decor as the face passes read it: the picture, its view and the group it is bound in.
///
/// The picture is `Rgba8UnormSrgb`, so the sampler hands the shader linear light and the fragment
/// multiplies the surface's own linear colour by it: one arithmetic in the viewport and the same one
/// in the final render, where a decor is read the same way. It repeats and it filters linearly,
/// because a decor is a print seen from any distance and 300 mm of board is one picture.
pub(crate) struct DecorGroup {
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    _layer: wgpu::Buffer,
    group: wgpu::BindGroup,
}

impl DecorGroup {
    /// The layout both the pipeline and the group are built against: one sampled picture, one
    /// sampling rule, and the one number that says whether the picture is a decor at all.
    pub(crate) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Z3L decor layout"),
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
                        min_binding_size: wgpu::BufferSize::new(DECOR_LAYER_BYTES),
                    },
                    count: None,
                },
            ],
        })
    }

    /// The sampler every decor is read with: repeat, linear, no mip chain.
    pub(crate) fn sampler(device: &wgpu::Device) -> wgpu::Sampler {
        device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Z3L decor sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        })
    }

    /// A one-texel white picture, which the drawables with no decor are bound to.
    ///
    /// The fragment multiplies its colour by the sampled texel whatever the surface is, so a white
    /// texel is the value that changes nothing and a surface without a decor needs no branch in the
    /// shader — a branch that would have to be uniform to keep the sampler's derivatives defined.
    /// The white stand-in is the one group whose layer says it is no picture, which is what keeps the
    /// tiling guide off a surface nobody can tile (Y16).
    pub(crate) fn white(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Self {
        Self::new(
            device,
            queue,
            layout,
            sampler,
            Texels { width: 1, height: 1, rows: &[[255, 255, 255, 255]], picture: false },
        )
    }

    /// The decor's own picture on the device, at the size the record scanned it.
    pub(crate) fn of(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        decor: &Decor,
    ) -> Self {
        let image = decor.image();
        Self::new(
            device,
            queue,
            layout,
            sampler,
            Texels { width: image.width(), height: image.height(), rows: image.texels(), picture: true },
        )
    }

    /// The picture `texels` on the device, or the actor's refusal when it cannot be uploaded.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        texels: Texels<'_>,
    ) -> Self {
        let Texels { width, height, rows, picture } = texels;
        let bytes: Vec<u8> = rows.iter().flatten().copied().collect();
        let size = wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Z3L decor"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size.width * 4),
                rows_per_image: Some(size.height),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        // One number for the tiling guide: a real picture is a board a shop can tile, and the white
        // stand-in is a surface with nothing to tile (Y16).
        let layer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Z3L decor layer"),
            contents: bytemuck::bytes_of(&[if picture { 1.0f32 } else { 0.0 }, 0.0, 0.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Z3L decor bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: layer.as_entire_binding() },
            ],
        });
        Self { _texture: texture, _view: view, _layer: layer, group }
    }

    /// The group a face pass binds.
    pub(crate) fn group(&self) -> &wgpu::BindGroup {
        &self.group
    }
}

/// The size of the one uniform the decor group carries, in bytes: `x` is the whole number.
pub(crate) const DECOR_LAYER_BYTES: u64 = 16;

/// The texels one picture uploads: its size and its eight-bit RGBA rows, one texel per four bytes.
///
/// The four travel together because a texture without its size is not a picture: the upload writes
/// the rows at the width the record scanned them, and says whether they are a board's picture or the
/// stand-in a plain surface is bound to.
pub(crate) struct Texels<'a> {
    /// The picture's width in texels.
    pub(crate) width: u32,
    /// The picture's height in texels.
    pub(crate) height: u32,
    /// Every texel, row-major from the top.
    pub(crate) rows: &'a [[u8; 4]],
    /// Whether these rows are a decor, or the white stand-in for a surface that has none.
    pub(crate) picture: bool,
}

/// Clamps a requested target size to at least one pixel and to the device's texture limit.
pub(crate) fn clamp_size(width: u32, height: u32, max_dimension: u32) -> [u32; 2] {
    let max_dimension = max_dimension.max(1);
    [width.clamp(1, max_dimension), height.clamp(1, max_dimension)]
}

/// Uploads a line-list vertex buffer, or nothing when there are no vertices.
pub(crate) fn line_list(
    device: &wgpu::Device,
    label: &str,
    vertices: Vec<MeshVertex>,
) -> Result<Option<GpuLineList>, RenderError> {
    if vertices.is_empty() {
        return Ok(None);
    }
    let vertex_count = u32::try_from(vertices.len()).map_err(|_| RenderError::MeshTooLarge { index: 0 })?;
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    Ok(Some(GpuLineList { vertex_buffer, vertex_count }))
}

/// Uploads an indexed triangle buffer, or nothing when there is no geometry.
pub(crate) fn triangle_list(
    device: &wgpu::Device,
    label: &str,
    vertices: &[MeshVertex],
    indices: &[u32],
) -> Option<GpuTriangles> {
    if vertices.is_empty() || indices.is_empty() {
        return None;
    }
    let index_count = u32::try_from(indices.len()).ok()?;
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    Some(GpuTriangles { vertex_buffer, index_buffer, index_count })
}
