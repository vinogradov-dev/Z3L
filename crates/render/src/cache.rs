//! What an upload keys its GPU buffers by, and the tables that keep them between uploads.
//!
//! An upload compares keys instead of rebuilding geometry: a drawable whose mesh, transform and
//! colour are the same as last time keeps its buffers, and only the drawable that changed is
//! built again (X57). A key that identifies a mesh or an edge list by address holds the `Arc`
//! itself, which is what makes the address safe to compare — while a slot exists the allocation
//! cannot be reused by an unrelated mesh, so an address never answers for two different meshes.
//!
//! Layer: L5, beside the renderer.
//!
//! Refuses: nothing here reads geometry or allocates a device object, and a failed build leaves
//! the table exactly as it was — a refused upload can never keep a slot for a frame it did not
//! upload.
//!
//! Does not build vertices, create buffers or know what a frame is: it compares keys, keeps values
//! and drops the ones an upload did not name.
//!
//! Changes when: what an upload must compare to decide it can reuse a buffer changes.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use geom::{DMat4, MeshData};
use scene::{DecorId, Faced};

use crate::RenderError;

/// A table of slots keyed by the data they were built from.
///
/// An upload opens the table with [`SlotCache::slot`] for every key the frame names and closes it
/// with [`SlotCache::sweep`], which drops the slots the frame no longer uses. A key that is
/// already present hands back the slot built for it the first time, so an edit that leaves a
/// drawable alone uploads nothing for it.
///
/// A slot that is currently named is never evicted, and a slot that is not named is dropped at the
/// end of the upload, so a deleted object leaves neither a buffer nor a pinned mesh behind.
pub(crate) struct SlotCache<K, V> {
    /// Every slot of the last upload, and the upload that last named it.
    entries: HashMap<K, Entry<V>>,
    /// The upload in progress; a key whose entry was not named in it is stale.
    generation: u64,
}

/// One slot and the upload that last named its key.
struct Entry<V> {
    value: Arc<V>,
    seen: u64,
}

impl<K: Eq + Hash, V> Default for SlotCache<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash, V> SlotCache<K, V> {
    /// An empty table.
    pub(crate) fn new() -> Self {
        Self { entries: HashMap::new(), generation: 0 }
    }

    /// Starts an upload: every slot is stale until [`Self::slot`] names it again.
    ///
    /// The frame names its keys in its own order, so the table cannot tell "first time this upload"
    /// from "last upload" without a mark; this is that mark.
    pub(crate) fn begin(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    /// The slot `key` names, building it with `build` when the key is new, and marks it as used.
    ///
    /// The key is kept in the table, so a key that holds an `Arc` keeps its allocation alive and
    /// the address it compares stays unique. A refused build inserts nothing.
    pub(crate) fn slot(
        &mut self,
        key: K,
        build: impl FnOnce() -> Result<V, RenderError>,
    ) -> Result<Arc<V>, RenderError> {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.seen = self.generation;
            return Ok(Arc::clone(&entry.value));
        }
        // The build runs before the table is touched, so an error leaves no half-inserted slot.
        let value = Arc::new(build()?);
        self.entries.insert(key, Entry { value: Arc::clone(&value), seen: self.generation });
        Ok(value)
    }

    /// Drops every slot this upload did not name.
    ///
    /// A wrapping generation would in theory let an ancient entry look current after 2^64 uploads;
    /// it is a counter, not a security boundary, and the alternative is a wider key.
    pub(crate) fn sweep(&mut self) {
        let seen = self.generation;
        self.entries.retain(|_, entry| entry.seen == seen);
    }

    /// How many slots the table keeps, for the tests.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

/// The mesh a buffer was built from and the rigid transform its vertices were baked with.
#[derive(Clone, Debug)]
struct PlacementKey {
    /// The mesh, kept alive so its address identifies it and can never answer for another mesh.
    mesh: Arc<MeshData>,
    /// The local-to-world transform, compared bit for bit so a moved drawable is a new key.
    transform: [u64; 16],
}

impl PlacementKey {
    fn new(mesh: &Arc<MeshData>, transform: DMat4) -> Self {
        Self { mesh: Arc::clone(mesh), transform: transform.to_cols_array().map(f64::to_bits) }
    }

    fn mesh(&self) -> &Arc<MeshData> {
        &self.mesh
    }
}

impl PartialEq for PlacementKey {
    fn eq(&self, other: &Self) -> bool {
        Arc::as_ptr(&self.mesh) == Arc::as_ptr(&other.mesh) && self.transform == other.transform
    }
}

impl Eq for PlacementKey {}

impl Hash for PlacementKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.mesh).hash(state);
        self.transform.hash(state);
    }
}

/// What one drawable's face buffers were built from: its vertices and the colour baked into them.
///
/// The colour is part of the key because a selected drawable's vertices carry the highlight ink
/// (ADR 0105), so selecting an object rebuilds that object and no other.
#[derive(Clone)]
pub(crate) struct FaceKey {
    placement: PlacementKey,
    color: [u32; 4],
    decor: Option<DecorKey>,
}

/// What a drawable's decor contributes to its vertices: which picture, how large, how turned, and
/// where the node put it.
///
/// The coordinate is baked into the vertex, so a decor that changes size or turns the grain changes
/// every coordinate of the drawable and the buffers are built again. The picture's texels are not
/// part of it: they are a texture, not a vertex, and the device keeps one copy of them per id.
/// A node's own mapping is part of the key for the same reason as the record's numbers: two boards
/// of one mesh, one placement and one picture, placed differently, carry different coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DecorKey {
    id: DecorId,
    covers: i64,
    turns: u8,
    mapping: Option<MappingKey>,
}

/// What a node's own placement of its picture contributes to the vertices (Y15).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct MappingKey {
    scale: i64,
    offset: [i64; 2],
    turns: u8,
}

impl MappingKey {
    fn new(mapping: scene::DecorMapping) -> Self {
        let offset = mapping.offset();
        Self {
            scale: mapping.scale().micrometres(),
            offset: [offset[0].micrometres(), offset[1].micrometres()],
            turns: mapping.turns().get(),
        }
    }
}

impl DecorKey {
    fn new(faced: &Faced<'_>) -> Self {
        let decor = faced.decor();
        Self {
            id: decor.id(),
            covers: decor.covers().micrometres(),
            turns: decor.turns().get(),
            mapping: faced.mapping().map(MappingKey::new),
        }
    }
}

impl FaceKey {
    /// The key of the buffers `mesh`, `transform`, `color` and `faced` would be built into.
    pub(crate) fn new(mesh: &Arc<MeshData>, transform: DMat4, color: [f32; 4], faced: Option<Faced<'_>>) -> Self {
        Self {
            placement: PlacementKey::new(mesh, transform),
            color: color.map(f32::to_bits),
            decor: faced.as_ref().map(DecorKey::new),
        }
    }
}

impl PartialEq for FaceKey {
    fn eq(&self, other: &Self) -> bool {
        self.placement == other.placement && self.color == other.color && self.decor == other.decor
    }
}

impl std::fmt::Debug for FaceKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The mesh is named by half of its address, which is the part a test compares.
        formatter
            .debug_struct("FaceKey")
            .field("mesh", &(Arc::as_ptr(&self.placement.mesh) as usize))
            .field("color", &self.color)
            .field("decor", &self.decor)
            .finish()
    }
}

impl Eq for FaceKey {}

impl Hash for FaceKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.placement.hash(state);
        self.color.hash(state);
        self.decor.hash(state);
    }
}

/// What one drawable's edge buffers were built from: the edge list, its transform and its ink.
///
/// The ink is part of the key because an edge line carries the viewport's colour, not the object's,
/// so a theme switch rebuilds the lines and leaves the faces alone.
#[derive(Clone)]
pub(crate) struct EdgeKey {
    placement: PlacementKey,
    edges: Arc<[u32]>,
    ink: [u32; 4],
}

impl EdgeKey {
    /// The key of the lines `edges`, `transform` and `ink` would be built into.
    pub(crate) fn new(mesh: &Arc<MeshData>, edges: &Arc<[u32]>, transform: DMat4, ink: [f32; 4]) -> Self {
        Self { placement: PlacementKey::new(mesh, transform), edges: Arc::clone(edges), ink: ink.map(f32::to_bits) }
    }

    /// The mesh the edge vertices are read from.
    pub(crate) fn mesh(&self) -> &Arc<MeshData> {
        self.placement.mesh()
    }

    /// The edge list the lines were built from.
    pub(crate) fn edges(&self) -> &Arc<[u32]> {
        &self.edges
    }
}

impl PartialEq for EdgeKey {
    fn eq(&self, other: &Self) -> bool {
        self.placement == other.placement
            && std::ptr::addr_eq(Arc::as_ptr(&self.edges), Arc::as_ptr(&other.edges))
            && self.ink == other.ink
    }
}

impl Eq for EdgeKey {}

impl Hash for EdgeKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.placement.hash(state);
        Arc::as_ptr(&self.edges).hash(state);
        self.ink.hash(state);
    }
}

/// A frame-level buffer group, rebuilt only when the data it was built from changes.
///
/// The grid, the line chains, the product envelopes and the contact shadows are one buffer each
/// over the whole frame, so they are keyed by their whole input: a revision that changes one
/// panel's thickness leaves all four alone.
pub(crate) struct Rebuilt<V, I> {
    /// The input the stored value was built from, or nothing before the first upload.
    input: Option<I>,
    /// The built buffer, or nothing when the input draws no geometry.
    value: Option<Arc<V>>,
}

impl<V, I> Default for Rebuilt<V, I> {
    fn default() -> Self {
        Self { input: None, value: None }
    }
}

impl<V, I: PartialEq + Clone> Rebuilt<V, I> {
    /// An empty group.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The buffer `input` draws, rebuilt only when `input` differs from what the buffer holds.
    ///
    /// The input is cloned only when it is stored, which is the rebuild path; an upload that reuses
    /// the buffer pays the comparison and no copy.
    pub(crate) fn get(
        &mut self,
        input: &I,
        build: impl FnOnce() -> Result<Option<V>, RenderError>,
    ) -> Result<Option<Arc<V>>, RenderError> {
        if self.input.as_ref() == Some(input) {
            return Ok(self.value.clone());
        }
        // The build runs before the stored input moves, so a refused build keeps the old pair.
        let value = build()?.map(Arc::new);
        self.input = Some(input.clone());
        self.value = value.clone();
        Ok(value)
    }

    /// The buffer the last upload built, or nothing when the input drew no geometry.
    ///
    /// The draw passes read the group rather than a copy on the renderer, so an upload that keeps a
    /// buffer and the pass that draws it can never disagree.
    pub(crate) fn value(&self) -> Option<&Arc<V>> {
        self.value.as_ref()
    }

    /// Whether the group currently holds a buffer, for the tests.
    #[cfg(test)]
    pub(crate) fn holds(&self) -> bool {
        self.value.is_some()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::cell::Cell;

    use geom::DVec3;

    use super::*;

    /// A mesh whose vertices are easy to compare, so only its identity separates two of them.
    fn mesh() -> Arc<MeshData> {
        Arc::new(MeshData {
            positions: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            indices: vec![0, 1, 2],
        })
    }

    #[test]
    fn an_unchanged_drawable_keeps_its_slot_and_a_changed_one_builds() {
        let mut cache: SlotCache<FaceKey, usize> = SlotCache::new();
        let builds = Cell::new(0usize);
        let mesh = mesh();
        let solid = [0.8, 0.8, 0.8, 1.0];
        let selected = [1.0, 0.6, 0.1, 1.0];
        let upload = |cache: &mut SlotCache<FaceKey, usize>, keys: [FaceKey; 2]| -> [Arc<usize>; 2] {
            cache.begin();
            let slots = keys.map(|key| {
                cache
                    .slot(key, || {
                        let next = builds.get() + 1;
                        builds.set(next);
                        Ok(next)
                    })
                    .expect("a slot")
            });
            cache.sweep();
            slots
        };

        let standing =
            [FaceKey::new(&mesh, DMat4::IDENTITY, solid, None), FaceKey::new(&mesh, DMat4::IDENTITY, selected, None)];
        let first = upload(&mut cache, standing.clone());
        assert_eq!(builds.get(), 2, "two colours are two slots");
        assert_eq!(cache.len(), 2);
        assert_ne!(Arc::as_ptr(&first[0]), Arc::as_ptr(&first[1]), "each drawable owns its buffers");

        // The same frame again, which is what a camera move or an edit elsewhere produces for
        // every drawable it did not touch: nothing is built.
        let again = upload(&mut cache, standing.clone());
        assert_eq!(builds.get(), 2, "an unchanged frame builds nothing");
        assert!(Arc::ptr_eq(&first[0], &again[0]), "the untouched drawable keeps its buffers");
        assert!(Arc::ptr_eq(&first[1], &again[1]));

        // One drawable gains the selection ink and the other stands still.
        let highlighted = [
            FaceKey::new(&mesh, DMat4::IDENTITY, solid, None),
            FaceKey::new(&mesh, DMat4::IDENTITY, [1.0, 1.0, 1.0, 1.0], None),
        ];
        let after = upload(&mut cache, highlighted);
        assert_eq!(builds.get(), 3, "only the recoloured drawable is built again");
        assert!(Arc::ptr_eq(&again[0], &after[0]), "the standing drawable keeps its buffers");
        assert!(!Arc::ptr_eq(&again[1], &after[1]), "the recoloured one gets new ones");

        // A moved drawable is a new key as well; its neighbour still keeps its own.
        let moved = [
            FaceKey::new(&mesh, DMat4::IDENTITY, solid, None),
            FaceKey::new(&mesh, DMat4::from_translation(DVec3::X), [1.0, 1.0, 1.0, 1.0], None),
        ];
        let translated = upload(&mut cache, moved);
        assert_eq!(builds.get(), 4, "the move is one new slot");
        assert!(Arc::ptr_eq(&after[0], &translated[0]));
        assert!(!Arc::ptr_eq(&after[1], &translated[1]));
    }

    #[test]
    fn a_key_that_leaves_the_frame_is_swept_with_its_mesh() {
        let mut cache: SlotCache<FaceKey, usize> = SlotCache::new();
        let mesh = mesh();
        cache.begin();
        cache.slot(FaceKey::new(&mesh, DMat4::IDENTITY, [1.0; 4], None), || Ok(7)).expect("a slot");
        cache.sweep();
        assert_eq!(cache.len(), 1);
        assert_eq!(Arc::strong_count(&mesh), 2, "the cache pins the mesh it compares by address");

        // A frame that no longer names the key drops it; its mesh reference goes with it.
        cache.begin();
        cache.sweep();
        assert_eq!(cache.len(), 0, "an upload that draws nothing keeps nothing");
        assert_eq!(Arc::strong_count(&mesh), 1, "the cache no longer pins the mesh");
    }

    #[test]
    fn a_mesh_is_identified_by_its_allocation_not_by_its_vertices() {
        let first = mesh();
        let second = mesh();
        assert_eq!(first, second, "the two meshes hold equal vertices");
        assert_ne!(
            FaceKey::new(&first, DMat4::IDENTITY, [1.0; 4], None),
            FaceKey::new(&second, DMat4::IDENTITY, [1.0; 4], None),
            "two allocations are two buffers"
        );
    }

    #[test]
    fn a_refused_build_inserts_no_slot() {
        let mut cache: SlotCache<FaceKey, usize> = SlotCache::new();
        let mesh = mesh();
        let key = FaceKey::new(&mesh, DMat4::IDENTITY, [1.0; 4], None);
        cache.begin();
        assert!(cache.slot(key.clone(), || Err(RenderError::EmptyMesh { index: 0 })).is_err());
        assert_eq!(cache.len(), 0, "a refused upload keeps no slot");
        assert!(cache.slot(key, || Ok(3)).is_ok(), "the same key can be built later");
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn a_frame_group_is_rebuilt_only_when_its_input_changes() {
        let mut grid: Rebuilt<u32, [u32; 2]> = Rebuilt::new();
        let builds = Cell::new(0u32);
        let mut draw = |input: &[u32; 2]| {
            grid.get(input, || {
                let next = builds.get() + 1;
                builds.set(next);
                Ok(Some(next))
            })
            .expect("a slot")
        };

        let first = draw(&[1, 2]).expect("the first build draws");
        assert_eq!(*first, 1);
        let same = draw(&[1, 2]).expect("the buffer is reused");
        assert!(Arc::ptr_eq(&first, &same), "an unchanged input keeps its buffer");
        assert_eq!(builds.get(), 1);

        let changed = draw(&[1, 3]).expect("a changed input rebuilds");
        assert!(!Arc::ptr_eq(&first, &changed));
        assert_eq!(builds.get(), 2);
        assert!(grid.holds());
    }

    #[test]
    fn a_frame_group_that_draws_nothing_is_remembered_as_empty() {
        let mut group: Rebuilt<u32, [u32; 2]> = Rebuilt::new();
        let builds = Cell::new(0u32);
        let mut draw = |input: &[u32; 2]| {
            group
                .get(input, || {
                    builds.set(builds.get() + 1);
                    Ok(None)
                })
                .expect("no buffer is not an error")
        };
        assert!(draw(&[0, 0]).is_none());
        assert!(draw(&[0, 0]).is_none());
        assert_eq!(builds.get(), 1, "an input that drew nothing is not rebuilt");
        assert!(!group.holds());
    }
}
