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
