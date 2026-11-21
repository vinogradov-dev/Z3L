//! The frame's meshes, flattened into world-space faces for the tracer.
//!
//! The viewport uploads what a frame describes as vertex buffers; the tracer needs a different shape:
//! one flat list of world-space faces, because the BVH knows nothing about drawables, buffers or a
//! document. This module is the one conversion between the two, so the camera, the lights and the
//! triangles of a final image all come from the frame a person was looking at.
//!
//! A [`Face`] is a triangle and the linear colour of the surface it belongs to, because a tracer that
//! lost the colour would trace the right geometry in the wrong material. The colour travels with the
//! triangle through the BVH's reorder, so a leaf's triangles and their colours are one run.
//!
//! The local-to-world transform is applied here, once, in double precision: a placement is a rigid
//! transform in millimetres, and the mesh itself is `f32` because the renderer's buffers are — the
//! widen-and-transform is the boundary where a physical size legitimately becomes a float and back
//! (AGENTS.md §4.3).
//!
//! Refuses: a frame whose triangles pass [`MAX_TRIANGLES`] is an error naming both numbers, because
//! the tracers's buffers and BVH are sized from it and a hostile scene must not be answered with an
//! allocation; a mesh that indexes a vertex it does not have, or whose index list is not a whole
//! number of triangles, is an error naming the index rather than a silently missing face.

use geom::{DVec3, MeshData};
use scene::{Drawable, SceneFrame, Surface};

use crate::FinalError;
use crate::ray::Triangle;

/// The most triangles one final render accepts.
///
/// A whole kitchen is tens of thousands; a million is a ceiling that still leaves the BVH, the node
/// buffers and the triangle buffer inside a few hundred megabytes, and past it a request is a
/// mistake rather than a big scene.
pub const MAX_TRIANGLES: usize = 1_000_000;

/// One triangle, the linear colour of the surface it came from and what that surface is made of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    /// The triangle, in world millimetres.
    pub triangle: Triangle,
    /// The linear RGB albedo of the surface, every channel inside `0..=1`.
    pub colour: [f32; 3],
    /// The finish the surface is made of: paint, conductor or pane.
    pub surface: Surface,
}

impl Face {
    /// A matte face of one colour, which is what a frame that names no finish describes.
    pub fn matte(triangle: Triangle, colour: [f32; 3]) -> Self {
        Self { triangle, colour, surface: Surface::MATTE }
    }
}

/// The frame's drawables as world-space faces, in frame order.
pub fn faces_of(frame: &SceneFrame) -> Result<Vec<Face>, FinalError> {
    flatten(frame, MAX_TRIANGLES)
}

/// The same flattening under a caller's ceiling, which is how the cap is tested without a million
/// triangles.
fn flatten(frame: &SceneFrame, max: usize) -> Result<Vec<Face>, FinalError> {
    let mut count = 0usize;
    for drawable in frame.drawables() {
        count = count.saturating_add(drawable.mesh.triangle_count());
    }
    if count > max {
        return Err(FinalError::TooManyTriangles { count, max });
    }
    let mut triangles = Vec::with_capacity(count);
    for drawable in frame.drawables() {
        let mesh: &MeshData = &drawable.mesh;
        let transform = drawable.world_transform;
        let remainder = mesh.indices.len() % 3;
        if remainder != 0 {
            return Err(FinalError::MalformedMesh { index: mesh.indices.len() - remainder });
        }
        for corner in (0..mesh.indices.len()).step_by(3) {
            let mut points = [DVec3::ZERO; 3];
            for (offset, point) in points.iter_mut().enumerate() {
                let Some(index) = mesh.indices.get(corner.saturating_add(offset)).copied() else {
                    return Err(FinalError::MalformedMesh { index: corner.saturating_add(offset) });
                };
                let Ok(index) = usize::try_from(index) else {
                    return Err(FinalError::MalformedMesh { index: corner.saturating_add(offset) });
                };
                let Some(position) = mesh.positions.get(index) else {
                    return Err(FinalError::MalformedMesh { index });
                };
                // The one place a mesh's `f32` millimetres become the tracer's `f64` ones.
                *point = transform.transform_point3(DVec3::new(
                    f64::from(position[0]),
                    f64::from(position[1]),
                    f64::from(position[2]),
                ));
            }
            triangles.push(Face {
                triangle: Triangle::new(points[0], points[1], points[2]),
                colour: albedo(drawable),
                surface: drawable.surface,
            });
        }
    }
    Ok(triangles)
}

/// The linear albedo a drawable's triangles are traced with.
///
/// A drawable's colour is `f32` data a test or a scene builder wrote, so a channel that is not finite
/// or outside `0..=1` is clamped rather than carried into the image: a negative albedo would emit
/// light and a NaN one would poison every sample it touches. Opacity is not read: a final render is
/// opaque until the material task gives it a transparent surface (`plan/BACKLOG.md`, Y3).
fn albedo(drawable: &Drawable) -> [f32; 3] {
    let mut colour = [0.0f32; 3];
    for (channel, value) in colour.iter_mut().zip(drawable.color) {
        *channel = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
    }
    colour
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::sync::Arc;

    use geom::DMat4;
    use scene::Drawable;

    use super::*;

    /// One quad on the XY plane, `side` millimetres square, two triangles.
    fn quad(side: f32) -> Arc<MeshData> {
        Arc::new(MeshData {
            positions: vec![[0.0, 0.0, 0.0], [side, 0.0, 0.0], [side, side, 0.0], [0.0, side, 0.0]],
            normals: Vec::new(),
            indices: vec![0, 1, 2, 0, 2, 3],
        })
    }

    fn id(value: u64) -> model::Id {
        model::Id::new(value).expect("a non-zero id")
    }

    fn frame(meshes: &[(Arc<MeshData>, DMat4)]) -> SceneFrame {
        let mut frame = SceneFrame::default();
        for (index, (mesh, transform)) in meshes.iter().enumerate() {
            let id = id(index as u64 + 1);
            frame.push(Drawable::new(id, Arc::clone(mesh), *transform, [0.5, 0.5, 0.5, 1.0]));
        }
        frame
    }

    #[test]
    fn a_mesh_becomes_world_faces_through_its_placement() {
        let mesh = quad(200.0);
        let placed = DMat4::from_translation(DVec3::new(1000.0, 500.0, 250.0));
        let faces = faces_of(&frame(&[(Arc::clone(&mesh), placed)])).expect("the frame flattens");
        assert_eq!(faces.len(), 2);
        assert_eq!(faces[0].triangle.v0, DVec3::new(1000.0, 500.0, 250.0), "the local origin moved with the placement");
        assert_eq!(faces[0].triangle.v1, DVec3::new(1200.0, 500.0, 250.0));
        assert_eq!(faces[1].triangle.v2, DVec3::new(1000.0, 700.0, 250.0));
        assert_eq!(faces[0].colour, [0.5, 0.5, 0.5], "the surface's colour travels with its triangle");
        // The mesh itself is untouched: the placement is applied to the output, never stored back.
        assert_eq!(mesh.positions[1], [200.0, 0.0, 0.0]);
    }

    #[test]
    fn a_hostile_colour_is_clamped_and_never_reaches_the_image() {
        let mesh = quad(10.0);
        let mut frame = SceneFrame::default();
        frame.push(Drawable::new(id(1), Arc::clone(&mesh), DMat4::IDENTITY, [f32::NAN, -2.0, 1.5, 1.0]));
        let faces = faces_of(&frame).expect("a hostile colour is not a broken mesh");
        assert_eq!(
            faces[0].colour,
            [0.0, 0.0, 1.0],
            "a NaN channel is black, a negative one zero and a bright one white"
        );
    }

    #[test]
    fn every_drawable_keeps_its_own_placement_and_an_empty_frame_holds_nothing() {
        let mesh = quad(100.0);
        let first = DMat4::from_translation(DVec3::new(0.0, 0.0, 0.0));
        let second = DMat4::from_translation(DVec3::new(0.0, 0.0, 900.0));
        let faces = faces_of(&frame(&[(Arc::clone(&mesh), first), (Arc::clone(&mesh), second)]))
            .expect("two drawables flatten");
        assert_eq!(faces.len(), 4);
        assert!((faces[0].triangle.v0.z - 0.0).abs() < 1e-9);
        assert!((faces[2].triangle.v0.z - 900.0).abs() < 1e-9, "the second drawable keeps its height");
        assert!(faces_of(&SceneFrame::default()).expect("an empty frame").is_empty());
    }

    #[test]
    fn the_triangle_cap_is_a_sentence_naming_both_numbers() {
        let frame = frame(&[(quad(100.0), DMat4::IDENTITY)]);
        let error = flatten(&frame, 1).expect_err("two triangles do not fit under a cap of one");
        assert!(matches!(error, FinalError::TooManyTriangles { count: 2, max: 1 }), "{error}");
        assert!(error.to_string().contains('2') && error.to_string().contains('1'), "{error}");
    }

    #[test]
    fn a_mesh_that_names_a_vertex_it_does_not_have_is_a_sentence() {
        let broken = Arc::new(MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            normals: Vec::new(),
            indices: vec![0, 1, 99],
        });
        let error = faces_of(&frame(&[(broken, DMat4::IDENTITY)])).expect_err("index 99 is past the mesh");
        assert!(matches!(error, FinalError::MalformedMesh { index: 99 }), "{error}");
        assert!(error.to_string().contains("99"), "{error}");

        let partial = Arc::new(MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]],
            normals: Vec::new(),
            indices: vec![0, 1, 2, 0, 1],
        });
        let error = faces_of(&frame(&[(partial, DMat4::IDENTITY)])).expect_err("half a triangle is refused");
        assert!(matches!(error, FinalError::MalformedMesh { index: 3 }), "{error}");
    }
}
