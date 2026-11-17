//! The vertex builders: frame data in, GPU-ready vertices out.
//!
//! A builder reads a frame (`Drawable`, `Polyline`, `Grid`) and returns a
//! [`MeshVertex`] list — or a triangle list for the contact shadows — without a device, a queue or
//! a buffer. `gpu.rs` owns the device objects and the renderer decides when to call a builder, so
//! this module holds no wgpu type and can be tested without one.
//!
//! Inputs are scene-space data: world transforms and positions in millimetres, colours in
//! gamma-space `f32` RGB. The one conversion this module performs is millimetres to `f32` at the
//! vertex boundary, which is where physical sizes legitimately become floats (AGENTS.md §4.3). A
//! vertex's alpha is a weight the fragment shader reads: full at a shadow's centre and none at its
//! rim.
//!
//! Refuses: a malformed mesh, polyline or contact shadow is a [`RenderError`] naming the index of
//! the offender, and every builder stays inside [`MAX_VERTICES`] instead of allocating what a
//! hostile frame asks for. An edge that cannot be computed is skipped, because an edge is decoration
//! and a frame must not fail for one; an edge list past the budget is refused by name instead of
//! truncated, because a wireframe that quietly stops is a wrong picture (X60).
//!
//! Does not create GPU objects, choose which view or style draws what, or store anything: the
//! back-to-front order of translucent drawables and the stencil slot of each one are derived per
//! frame and handed to the renderer, never kept.
//!
//! Changes when: the CPU-side shape of a frame changes — a new primitive, a new vertex attribute,
//! or a new reason to refuse a mesh.

use bytemuck::{Pod, Zeroable};
use geom::{DMat4, DVec3, MeshData};
use scene::{Grid, Polyline, ProductEnvelope, ProductEnvelopeState};

use crate::RenderError;
use crate::pipelines::MAX_STENCIL_OWNERS;

/// Largest vertex count the renderer accepts for one uploaded primitive: a drawable's mesh or the
/// whole frame's line list.
pub(crate) const MAX_VERTICES: usize = 4_000_000;

/// Largest number of grid divisions on each side of the origin.
pub(crate) const MAX_GRID_DIVISIONS: i64 = 512;

/// The floor plate's four corners, in the order its two triangles read them.
///
/// Seen from `+Z` the winding runs counter-clockwise, which is what lets the plate's pipeline draw
/// it from above and cull it from below: a view from under the floor keeps the model instead of
/// losing it behind the ground.
const FLOOR_CORNERS: [(f32, f32); 4] = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
/// The plate's two triangles, as indices into [`FLOOR_CORNERS`].
const FLOOR_INDICES: [u32; 6] = [0, 1, 2, 0, 2, 3];

/// One dash cycle of a product envelope, as a fraction of its longest side.
///
/// A cycle tied to the piece instead of the zoom keeps every envelope about two dozen dashes per
/// side: a 600 mm cabinet and a 3 m wardrobe both read as a dashed box and neither turns into a
/// solid line at the far end of the room. The world-space pattern is the price; the camera never
/// re-uploads geometry, so a screen-space pattern would need a second shader entry.
const ENVELOPE_CYCLES_PER_SIDE: f64 = 24.0;
/// The shortest dash cycle an envelope may use, in millimetres; a tiny box still reads as dashes.
const MIN_ENVELOPE_DASH_MM: f64 = 6.0;
/// The painted fraction of one cycle: a dash and a gap about the same length.
const ENVELOPE_DASH_DUTY: f64 = 0.55;

/// Gamma-space ink for a light viewport.
pub(crate) const EDGE_COLOR_DARK: [f32; 4] = [0.06, 0.07, 0.08, 1.0];

/// Gamma-space ink for a dark viewport: the dark ink disappears on the default background.
pub(crate) const EDGE_COLOR_LIGHT: [f32; 4] = [0.86, 0.88, 0.91, 1.0];

/// A shaded vertex in world space.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct MeshVertex {
    pub(crate) position: [f32; 3],
    pub(crate) normal: [f32; 3],
    pub(crate) color: [f32; 4],
}

/// The world-space centre of a built drawable's bounds, used to order translucent objects.
///
/// The box is measured from the vertices the upload has already built, so a drawable never
/// transforms its positions a second time to learn where it is; a drawable with no vertex has no
/// centre to sort by and sorts at the origin.
pub(crate) fn vertices_centre(vertices: &[MeshVertex]) -> DVec3 {
    let mut min = DVec3::splat(f64::INFINITY);
    let mut max = DVec3::splat(f64::NEG_INFINITY);
    for vertex in vertices {
        let position = vector(vertex.position);
        min = min.min(position);
        max = max.max(position);
    }
    if min.is_finite() && max.is_finite() { (min + max) * 0.5 } else { DVec3::ZERO }
}

/// Flattens one drawable's edge list into world-space line-list vertices.
///
/// The edge list is derived data that `crates/mesh` cached beside the mesh (ADR 0053), so this is a
/// transform and a copy, never a search. A malformed edge or a non-finite vertex is skipped: an edge
/// is decoration and must never turn a whole frame into an error.
///
/// One list per drawable, so an unchanged drawable's lines are never transformed again and the
/// buffer belongs to the mesh instead of the frame (X57). A list that does not fit one buffer is
/// refused by name: a truncated wireframe would silently hide part of an object (X60).
///
/// The colour is the ink the caller derived from the background, so a theme switch rebuilds the
/// lines and leaves the faces alone.
pub(crate) fn edge_vertices(
    index: usize,
    mesh: &MeshData,
    edges: &[u32],
    transform: DMat4,
    color: [f32; 4],
) -> Result<Vec<MeshVertex>, RenderError> {
    // Two vertices per edge, checked against the budget before anything is allocated.
    let pairs = edges.len() / 2;
    if pairs.saturating_mul(2) > MAX_VERTICES {
        return Err(RenderError::EdgeListTooLarge { index });
    }
    let mut vertices = Vec::with_capacity(pairs.saturating_mul(2));
    if !transform.is_finite() {
        return Ok(vertices);
    }
    for edge in edges.as_chunks::<2>().0 {
        let (Some(a), Some(b)) = (position_at(mesh, edge[0]), position_at(mesh, edge[1])) else {
            continue;
        };
        let a = transform.transform_point3(a);
        let b = transform.transform_point3(b);
        if !a.is_finite() || !b.is_finite() {
            continue;
        }
        push_line(&mut vertices, [a.x, a.y, a.z], [b.x, b.y, b.z], color);
    }
    Ok(vertices)
}

/// The stencil value one drawable stamps, counted from one so zero keeps meaning "no surface".
///
/// The count wraps at [`MAX_STENCIL_OWNERS`], so a scene with more drawables than slots reuses the
/// low values. Two objects sharing a slot can show one inside the other at a pixel; no object can
/// appear where it is not, which is the property that matters.
pub(crate) fn stencil_slot(index: usize) -> u32 {
    (index % MAX_STENCIL_OWNERS) as u32 + 1
}

/// Light ink for a dark viewport and dark ink for a light one.
pub(crate) fn edge_color(background: [f32; 4], strength: f32) -> [f32; 4] {
    let background = sanitize_color(background);
    let luminance = 0.2126 * background[0] + 0.7152 * background[1] + 0.0722 * background[2];
    let mut ink = if luminance < 0.45 { EDGE_COLOR_LIGHT } else { EDGE_COLOR_DARK };
    // The strength is the ink's opacity: the line blends into the face it traces, so a strength of
    // zero leaves the fill alone instead of painting a second outline (U107).
    ink[3] = if strength.is_finite() { strength.clamp(0.0, 1.0) } else { 1.0 };
    ink
}

/// Rewrites `order` so far objects come first, which is the order a blended pass needs.
///
/// The comparison is on squared distance to the eye, so no square root is taken per pair. A
/// non-finite eye or centre sorts as the origin, which keeps the order total instead of panicking.
pub(crate) fn sort_back_to_front(centres: &[DVec3], eye: DVec3, order: &mut Vec<usize>) {
    order.clear();
    order.extend(0..centres.len());
    let eye = if eye.is_finite() { eye } else { DVec3::ZERO };
    order.sort_by(|&a, &b| {
        let far = centres.get(a).map_or(0.0, |centre| (centre - eye).length_squared());
        let near = centres.get(b).map_or(0.0, |centre| (centre - eye).length_squared());
        near.total_cmp(&far)
    });
}

/// Builds the floor plate: one quad on `z = 0` across the square the grid draws.
///
/// The plate is the surface the ground shadow falls on, so it is the one piece of the frame that is
/// neither lit nor a body: its colour arrives in the vertex colour, and the fragment shader mixes
/// the shadow into it. It writes depth, because a body standing on the ground has to be nearer than
/// the ground for the frame to read.
///
/// The corners are the grid's own square so the two never disagree, and the winding runs
/// counter-clockwise seen from `+Z` so a view from below culls the plate.
pub(crate) fn floor_vertices(extent_mm: f64, color: [f32; 4], z: f32) -> Vec<MeshVertex> {
    if !extent_mm.is_finite() || extent_mm <= 0.0 || !color.iter().all(|channel| channel.is_finite()) {
        return Vec::new();
    }
    let extent = extent_mm as f32;
    FLOOR_CORNERS
        .iter()
        .map(|(x, y)| MeshVertex { position: [x * extent, y * extent, z], normal: [0.0, 0.0, 1.0], color })
        .collect()
}

/// The floor plate's index list, in the winding [`floor_vertices`] builds.
pub(crate) fn floor_indices() -> [u32; 6] {
    FLOOR_INDICES
}

/// Flattens every frame polyline into one line-list vertex buffer.
///
/// Points are transformed into world space here, the same way a drawable's mesh is. A chain that
/// names no colour of its own is drawn in `line_ink`, the frame's own ink, so a theme switch
/// repaints it without rebuilding the frame. A malformed chain is an error instead of a silently
/// missing path, so the shell can report it.
pub(crate) fn polyline_vertices(polylines: &[Polyline], line_ink: [f32; 4]) -> Result<Vec<MeshVertex>, RenderError> {
    let mut vertices = Vec::new();
    for (index, polyline) in polylines.iter().enumerate() {
        push_polyline_vertices(index, polyline, line_ink, &mut vertices)?;
    }
    Ok(vertices)
}

/// Flattens the product envelopes into one dashed line list in world space.
///
/// Each envelope is the twelve edges of a box in the product's own frame. A line is not a body, so
/// the chain is dashed and the dashes are the renderer's promise that this box is reserved space,
/// never a solid (ADR 0100). The state chooses the ink: active for the open product, inactive for a
/// closed empty one, and nothing at all for a closed product whose parts speak for themselves.
///
/// A malformed size or transform is a [`RenderError`] naming the offender, so the shell can report
/// it; the vertex budget is checked before the dashes are pushed. The list is one flat buffer drawn
/// with the line pipeline, which writes no depth.
pub(crate) fn product_envelope_vertices(
    envelopes: &[ProductEnvelope],
    active: [f32; 4],
    inactive: [f32; 4],
) -> Result<Vec<MeshVertex>, RenderError> {
    let mut vertices = Vec::new();
    for (index, envelope) in envelopes.iter().enumerate() {
        push_product_envelope_vertices(index, envelope, active, inactive, &mut vertices)?;
    }
    Ok(vertices)
}

/// Appends one product envelope as dashed line-list vertices in world space.
fn push_product_envelope_vertices(
    index: usize,
    envelope: &ProductEnvelope,
    active: [f32; 4],
    inactive: [f32; 4],
    vertices: &mut Vec<MeshVertex>,
) -> Result<(), RenderError> {
    let color = match envelope.state {
        ProductEnvelopeState::Open => active,
        ProductEnvelopeState::ClosedEmpty => inactive,
        ProductEnvelopeState::ClosedWithParts => return Ok(()),
    };
    let size = envelope.size;
    if !size.is_finite() || size.min_element() <= 0.0 || !envelope.world_transform.is_finite() {
        return Err(RenderError::InvalidProductEnvelope { index });
    }
    let color = sanitize_color(color);
    let longest = size.max_element();
    let cycle = (longest / ENVELOPE_CYCLES_PER_SIDE).max(MIN_ENVELOPE_DASH_MM);
    let dash = cycle * ENVELOPE_DASH_DUTY;
    let capacity = envelope_vertex_count(size, cycle);
    if vertices.len().saturating_add(capacity) > MAX_VERTICES {
        return Err(RenderError::ProductEnvelopeTooLarge { index });
    }
    for (start, end) in ENVELOPE_EDGES {
        let start = envelope.world_transform.transform_point3(corner(start, size));
        let end = envelope.world_transform.transform_point3(corner(end, size));
        if !start.is_finite() || !end.is_finite() {
            return Err(RenderError::InvalidProductEnvelope { index });
        }
        push_dashes(vertices, start, end, cycle, dash, color);
    }
    Ok(())
}

/// One corner of the local box; a bit in `bits` is the high side of one axis.
fn corner(bits: usize, size: DVec3) -> DVec3 {
    DVec3::new(
        if bits & 1 == 0 { 0.0 } else { size.x },
        if bits & 2 == 0 { 0.0 } else { size.y },
        if bits & 4 == 0 { 0.0 } else { size.z },
    )
}

/// The twelve edges of a box as corner-index pairs; a pair differs in one bit.
const ENVELOPE_EDGES: [(usize, usize); 12] =
    [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];

/// How many line-list vertices the dashes of one box need, so the budget is checked before the walk.
fn envelope_vertex_count(size: DVec3, cycle: f64) -> usize {
    let mut total = 0usize;
    for (start, end) in ENVELOPE_EDGES {
        let length = corner(start, size).distance(corner(end, size));
        let dashes = (length / cycle).ceil().max(1.0) as usize;
        total = total.saturating_add(dashes.saturating_mul(2));
    }
    total
}

/// Pushes one edge as dashes: a painted run, a gap, and so on to the end of the edge.
fn push_dashes(vertices: &mut Vec<MeshVertex>, start: DVec3, end: DVec3, cycle: f64, dash: f64, color: [f32; 4]) {
    let length = start.distance(end);
    if !length.is_finite() || length <= 0.0 {
        return;
    }
    let direction = (end - start) / length;
    let mut along = 0.0;
    while along < length && vertices.len().saturating_add(2) <= MAX_VERTICES {
        let dash_end = (along + dash).min(length);
        let a = start + direction * along;
        let b = start + direction * dash_end;
        if a.is_finite() && b.is_finite() {
            push_line(vertices, [a.x, a.y, a.z], [b.x, b.y, b.z], color);
        }
        along += cycle;
    }
}

/// Appends one polyline as pairs of line-list vertices in world space.
fn push_polyline_vertices(
    index: usize,
    polyline: &Polyline,
    line_ink: [f32; 4],
    vertices: &mut Vec<MeshVertex>,
) -> Result<(), RenderError> {
    let point_count = polyline.points.len();
    if point_count < 2 {
        return Err(RenderError::EmptyPolyline { index });
    }
    let needed = (point_count - 1).checked_mul(2).ok_or(RenderError::PolylineTooLarge { index })?;
    let total = vertices.len().checked_add(needed).ok_or(RenderError::PolylineTooLarge { index })?;
    if total > MAX_VERTICES {
        return Err(RenderError::PolylineTooLarge { index });
    }
    if !polyline.world_transform.is_finite() || polyline.points.iter().any(|point| !point.is_finite()) {
        return Err(RenderError::InvalidPolyline { index });
    }
    let color = sanitize_color(polyline.drawn_color(line_ink));
    let mut previous = None;
    for point in polyline.points.iter() {
        let world = polyline.world_transform.transform_point3(*point);
        if !world.is_finite() {
            return Err(RenderError::InvalidPolyline { index });
        }
        let position = to_array(world);
        if let Some(previous) = previous {
            vertices.push(MeshVertex { position: previous, normal: [0.0, 0.0, 1.0], color });
            vertices.push(MeshVertex { position, normal: [0.0, 0.0, 1.0], color });
        }
        previous = Some(position);
    }
    Ok(())
}

/// Converts a local mesh into world-space vertices and indices.
///
/// A mesh without normals is expanded into flat-shaded triangles. The fallback matters because
/// the geometry port allows normals to be absent; panels normally arrive with per-face normals.
pub(crate) fn world_vertices(
    index: usize,
    mesh: &MeshData,
    transform: DMat4,
    color: [f32; 4],
) -> Result<(Vec<MeshVertex>, Vec<u32>), RenderError> {
    if mesh.positions.is_empty() || mesh.indices.is_empty() || !mesh.indices.len().is_multiple_of(3) {
        return Err(RenderError::EmptyMesh { index });
    }
    if mesh.positions.len() > MAX_VERTICES {
        return Err(RenderError::MeshTooLarge { index });
    }
    if !transform.is_finite()
        || mesh.positions.iter().flatten().any(|value| !value.is_finite())
        || mesh.normals.iter().flatten().any(|value| !value.is_finite())
        || (!mesh.normals.is_empty() && mesh.normals.len() != mesh.positions.len())
    {
        return Err(RenderError::InvalidMesh { index });
    }
    let vertex_count = u32::try_from(mesh.positions.len()).map_err(|_| RenderError::MeshTooLarge { index })?;
    if mesh.indices.iter().any(|vertex| *vertex >= vertex_count) {
        return Err(RenderError::InvalidMesh { index });
    }
    let color = sanitize_color(color);

    if mesh.normals.is_empty() {
        return flat_vertices(index, mesh, transform, color);
    }

    let mut vertices = Vec::with_capacity(mesh.positions.len());
    for (position, normal) in mesh.positions.iter().zip(&mesh.normals) {
        let position = transform.transform_point3(vector(*position));
        if !position.is_finite() {
            return Err(RenderError::InvalidMesh { index });
        }
        let normal = transform.transform_vector3(vector(*normal)).try_normalize().unwrap_or(DVec3::Z);
        vertices.push(MeshVertex { position: to_array(position), normal: to_array(normal), color });
    }
    Ok((vertices, mesh.indices.clone()))
}

fn flat_vertices(
    index: usize,
    mesh: &MeshData,
    transform: DMat4,
    color: [f32; 4],
) -> Result<(Vec<MeshVertex>, Vec<u32>), RenderError> {
    let triangle_count = mesh.indices.len() / 3;
    let capacity = triangle_count.checked_mul(3).ok_or(RenderError::MeshTooLarge { index })?;
    if capacity > MAX_VERTICES {
        return Err(RenderError::MeshTooLarge { index });
    }
    let mut vertices = Vec::with_capacity(capacity);
    let mut indices = Vec::with_capacity(capacity);
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let a = position_at(mesh, triangle[0]);
        let b = position_at(mesh, triangle[1]);
        let c = position_at(mesh, triangle[2]);
        let (Some(a), Some(b), Some(c)) = (a, b, c) else {
            return Err(RenderError::InvalidMesh { index });
        };
        let a = transform.transform_point3(a);
        let b = transform.transform_point3(b);
        let c = transform.transform_point3(c);
        if !a.is_finite() || !b.is_finite() || !c.is_finite() {
            return Err(RenderError::InvalidMesh { index });
        }
        let normal = (b - a).cross(c - a).try_normalize().unwrap_or(DVec3::Z);
        let base = u32::try_from(vertices.len()).map_err(|_| RenderError::MeshTooLarge { index })?;
        for position in [a, b, c] {
            vertices.push(MeshVertex { position: to_array(position), normal: to_array(normal), color });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2]);
    }
    Ok((vertices, indices))
}

fn position_at(mesh: &MeshData, index: u32) -> Option<DVec3> {
    let position = mesh.positions.get(index as usize)?;
    Some(vector(*position))
}

pub(crate) fn grid_vertices(grid: &Grid) -> Vec<MeshVertex> {
    if !grid.visible {
        return Vec::new();
    }
    let spacing = if grid.spacing_mm.is_finite() && grid.spacing_mm >= 1.0 {
        grid.spacing_mm.min(scene::MAX_DISTANCE_MM)
    } else {
        100.0
    };
    let requested_extent =
        if grid.extent_mm.is_finite() { grid.extent_mm.clamp(spacing, scene::MAX_DISTANCE_MM) } else { 5000.0 };
    let divisions = ((requested_extent / spacing).floor() as i64).clamp(0, MAX_GRID_DIVISIONS);
    let extent = divisions as f64 * spacing;
    let major_every = i64::from(grid.major_every.max(1));
    let mut vertices = Vec::with_capacity((2 * divisions as usize + 1) * 4);

    // The inks keep their hue and lose opacity, so the grid fades into the floor plate instead of
    // changing colour (the display strength, U107).
    let strength = if grid.ink_strength.is_finite() { grid.ink_strength.clamp(0.0, 1.0) } else { 1.0 };
    let ink = |color: [f32; 4]| {
        let mut color = sanitize_color(color);
        color[3] *= strength;
        color
    };
    for step in -divisions..=divisions {
        let offset = step as f64 * spacing;
        let x_color = if step == 0 {
            ink(grid.axis_x_color)
        } else if step % major_every == 0 {
            ink(grid.major_color)
        } else {
            ink(grid.minor_color)
        };
        let y_color = if step == 0 {
            ink(grid.axis_y_color)
        } else if step % major_every == 0 {
            ink(grid.major_color)
        } else {
            ink(grid.minor_color)
        };
        // Lift the grid a fraction of a millimetre above the floor so a panel resting at z = 0
        // does not z-fight with it.
        push_line(&mut vertices, [-extent, offset, 0.02], [extent, offset, 0.02], x_color);
        push_line(&mut vertices, [offset, -extent, 0.02], [offset, extent, 0.02], y_color);
    }

    vertices
}

fn push_line(vertices: &mut Vec<MeshVertex>, start: [f64; 3], end: [f64; 3], color: [f32; 4]) {
    for point in [start, end] {
        vertices.push(MeshVertex {
            position: [point[0] as f32, point[1] as f32, point[2] as f32],
            normal: [0.0, 0.0, 1.0],
            color,
        });
    }
}

fn vector(value: [f32; 3]) -> DVec3 {
    DVec3::new(f64::from(value[0]), f64::from(value[1]), f64::from(value[2]))
}

fn to_array(value: DVec3) -> [f32; 3] {
    [value.x as f32, value.y as f32, value.z as f32]
}

pub(crate) fn sanitize_color(color: [f32; 4]) -> [f32; 4] {
    if color.iter().all(|component| component.is_finite()) { color } else { [0.8, 0.8, 0.8, 1.0] }
}
