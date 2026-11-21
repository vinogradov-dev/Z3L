//! Triangle meshes derived from the document model.
//!
//! Turns a panel body into indexed geometry with normals. Coordinates stay in the body's local
//! frame; a placement is applied later by the scene. This crate owns no GPU state.
//!
//! [`EvaluationCache`] is the one evaluator the viewport and `object.collapse` share. A node is
//! re-evaluated only when the inputs its mesh was derived from change, so a move, a rename or an
//! unrelated edit rebuilds no mesh. The cache keeps the derivation inputs beside the mesh and
//! compares them; a global [`Revision`] alone would invalidate every node on every edit.
//!
//! The same entry also holds the **holes a panel receives** from the drilling objects that reach it
//! ([`model::derive_holes`]). They are derived from the panel, its world placement and those
//! objects, and compared on those three alone: a hinge that moves re-derives the holes and leaves
//! the panel's mesh alone, because a hole is not part of the body the mesh draws yet.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use geom::{Aabb, DVec3, MeshData, triangulate_face};
use kernel::{BodyId, KernelError, SolidKernel, TessOptions};
use model::{
    BandingSpec, Constructive, CornerBanding, DerivedHoles, Drilling, EdgeBanding, EdgeBevel, HoleError, Id, Machining,
    Mesh, Object, Panel, PanelGeometry, PanelNominalSize, PanelOutline, Path, Placement, Primitive, Revision,
    RingBanding, SegmentSource, derive_holes,
};
use param::{EdgeBandProfile, ParamError, build_constructive_mesh, build_panel_parts_with};
use thiserror::Error;

mod primitive;

use primitive::build_primitive_mesh;

/// The viewport's chord tolerance when a stored path becomes a line chain, in millimetres.
///
/// It matches the kernel profile tolerance, so the line the user sees follows the curve a later
/// extrusion is built from.
pub const PATH_SAMPLE_TOLERANCE_MM: f64 = 0.05;

/// An error raised while building a render mesh.
#[derive(Debug, Error)]
pub enum MeshError {
    /// The geometry kernel rejected or failed tessellation.
    #[error(transparent)]
    Kernel(#[from] KernelError),
    /// The parametric evaluation of a body failed.
    #[error(transparent)]
    Param(#[from] ParamError),
    /// The document rejected the derived polygon mesh.
    #[error("invalid primitive mesh: {0}")]
    Model(#[from] model::MeshError),
    /// The holes a panel receives could not be derived.
    #[error(transparent)]
    Holes(#[from] HoleError),
    /// The adapter returned a mesh that is not safe to render.
    #[error("invalid panel mesh: {0}")]
    Invalid(String),
}

/// Tessellates a panel body and checks the mesh before it reaches the renderer.
///
/// The returned positions and normals are in the body's local millimetres. The caller must keep
/// the body alive in the kernel for the duration of the call; the mesh itself owns no kernel
/// handle.
pub fn tessellate_panel(kernel: &dyn SolidKernel, body: BodyId, options: &TessOptions) -> Result<MeshData, MeshError> {
    let mesh = kernel.tessellate(body, options)?;
    validate_mesh(&mesh)?;
    Ok(mesh)
}

/// Evaluates a constructive object into its document mesh.
///
/// This is the one definition of the mesh a feature history produces: the viewport draws it and
/// `object.collapse` bakes it, so a collapsed object keeps exactly the shape the user saw.
pub fn evaluate_constructive(constructive: &Constructive) -> Result<Mesh, MeshError> {
    Ok(build_constructive_mesh(constructive)?)
}

/// Evaluates a constructive object into flat-shaded render triangles.
pub fn constructive_mesh(constructive: &Constructive) -> Result<MeshData, MeshError> {
    object_mesh(&evaluate_constructive(constructive)?)
}

/// Evaluates a parametric primitive into its document mesh.
///
/// This is the one definition of the mesh a primitive's parameters produce: the viewport draws it
/// and `object.collapse` bakes it, so a collapsed primitive keeps exactly the shape the user saw
/// ([ADR 0054]). The parameters are document data, so the trigonometry happens here, at the
/// geometry boundary.
///
/// [ADR 0054]: ../docs/decisions/0054-a-primitive-keeps-the-parameters-that-built-it.md
pub fn evaluate_primitive(primitive: &Primitive) -> Result<Mesh, MeshError> {
    build_primitive_mesh(primitive)
}

/// Evaluates a parametric primitive into flat-shaded render triangles.
pub fn primitive_mesh(primitive: &Primitive) -> Result<MeshData, MeshError> {
    object_mesh(&evaluate_primitive(primitive)?)
}

/// Triangulates a document mesh into flat-shaded renderable triangles.
///
/// Every face is ear-clipped in its own plane, so a concave cap keeps its triangles inside the
/// contour; a face that ear clipping rejects — a self-crossing one, which dragging a vertex can
/// make — falls back to a fan so the edited mesh still draws. Each face gets one normal, so the
/// object renders faceted without smoothing. Positions are converted from micrometres to local
/// millimetres here, at the render boundary, and a degenerate face is skipped rather than producing
/// a NaN normal.
pub fn object_mesh(mesh: &Mesh) -> Result<MeshData, MeshError> {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    for face in mesh.faces() {
        let points = face_points(mesh, face)?;
        let Some(normal) = face_normal(&points) else {
            continue;
        };
        let normal = [normal.x as f32, normal.y as f32, normal.z as f32];
        let triangles = triangulate_face(&points).unwrap_or_else(|_| fan_triangles(points.len()));
        for triangle in triangles {
            let [Some(first), Some(second), Some(third)] = triangle.map(|index| points.get(index as usize).copied())
            else {
                return Err(MeshError::Invalid("a triangle refers to a missing vertex".into()));
            };
            if positions.len() > (u32::MAX as usize).saturating_sub(3) {
                return Err(MeshError::Invalid("the object has too many vertices to index".into()));
            }
            let Ok(base) = u32::try_from(positions.len()) else {
                return Err(MeshError::Invalid("the object has too many vertices to index".into()));
            };
            for point in [first, second, third] {
                positions.push([point.x as f32, point.y as f32, point.z as f32]);
                normals.push(normal);
            }
            indices.extend([base, base + 1, base + 2]);
        }
    }
    let data = MeshData { positions, normals, indices };
    if data.is_empty() {
        // Every face was degenerate, so there is nothing to draw. An object must not fail the
        // whole frame because one of its faces has zero area.
        return Ok(data);
    }
    validate_mesh(&data)?;
    Ok(data)
}

/// Two triangles sharing an edge lie on one surface when their normals differ by less than about
/// 0.08 degrees. A quad's diagonal is exactly coplanar; a cube's edge turns by 90 degrees.
///
/// The margin guards rounding noise, not a display radius: a face normal is computed from positions
/// that are micrometres rounded to `f32` millimetres, so two triangles of one flat face disagree by
/// a few millionths of a degree rather than by zero. It must stay below the smallest fold a mesher
/// produces on a body the user calls curved, so that a finely tessellated ball keeps its
/// graticule: at 256 sides the meridian fold between two adjacent strips falls to about 0.001
/// degrees, and an opaque margin of 2.6 degrees erased every meridian of a fine ball and left its
/// poles bald while the latitude rings survived.
///
/// The comparison is on the absolute dot product: a tessellator may emit a coplanar pair with
/// opposite winding, and a fold of 180 degrees has no thickness to draw an edge on either.
const COPLANAR_COS: f64 = 0.999_999;

/// The display edges of a mesh: boundary and crease edges, two vertex indices per edge.
///
/// Vertices are matched by their position quantised to one micrometre, so a flat-shaded mesh that
/// stores one vertex per face still finds the edge a neighbouring triangle shares. An edge used by
/// one triangle is a boundary and is kept. An edge shared by two triangles is kept only when the
/// surface turns across it by more than the coplanar margin allows: a quad's diagonal and every
/// other coplanar triangulation edge is dropped, while a cube's twelve edges and a smooth body's
/// finer graticule survive. A degenerate triangle, a missing index and a
/// non-finite position contribute nothing, so the list is always safe to draw as a line list.
///
/// The list is derived beside the mesh and cached with it under the same inputs (ADR 0053), so the
/// edge pass never searches the mesh per frame. [`mesh_all_edges`] keeps the full triangulation for
/// the optional diagnostic style.
pub fn mesh_edges(mesh: &MeshData) -> Vec<u32> {
    struct Use {
        a: u32,
        b: u32,
        normal: DVec3,
        shared: bool,
        crease: bool,
    }

    let mut uses: Vec<Use> = Vec::with_capacity(mesh.indices.len());
    let mut lookup: HashMap<([i64; 3], [i64; 3]), usize> = HashMap::new();
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let (Some(a), Some(b), Some(c)) = (
            vertex_position(mesh, triangle[0]),
            vertex_position(mesh, triangle[1]),
            vertex_position(mesh, triangle[2]),
        ) else {
            continue;
        };
        let Some(normal) = (b - a).cross(c - a).try_normalize() else {
            continue;
        };
        for (start, end) in [(triangle[0], triangle[1]), (triangle[1], triangle[2]), (triangle[2], triangle[0])] {
            if start == end {
                continue;
            }
            let (Some(first), Some(second)) = (edge_key(mesh, start), edge_key(mesh, end)) else {
                continue;
            };
            if first == second {
                continue;
            }
            let key = if first <= second { (first, second) } else { (second, first) };
            match lookup.get(&key) {
                Some(index) => {
                    if let Some(record) = uses.get_mut(*index) {
                        record.shared = true;
                        if record.normal.dot(normal).abs() < COPLANAR_COS {
                            record.crease = true;
                        }
                    }
                }
                None => {
                    lookup.insert(key, uses.len());
                    uses.push(Use { a: start, b: end, normal, shared: false, crease: false });
                }
            }
        }
    }

    let mut edges = Vec::with_capacity(uses.len() * 2);
    for record in &uses {
        if !record.shared || record.crease {
            edges.extend_from_slice(&[record.a, record.b]);
        }
    }
    edges
}

/// Every unique triangle edge of a mesh, triangulation diagonals included.
///
/// This is the raw wireframe list behind the optional *all surface edges* setting. The default
/// [`mesh_edges`] is what a shaded view wants; this one is honest about the triangles the mesher
/// produced. It follows the same deduplication, skip and budget rules.
pub fn mesh_all_edges(mesh: &MeshData) -> Vec<u32> {
    let mut edges = Vec::with_capacity(mesh.indices.len());
    let mut seen = HashSet::new();
    for triangle in mesh.indices.as_chunks::<3>().0 {
        for (a, b) in [(triangle[0], triangle[1]), (triangle[1], triangle[2]), (triangle[2], triangle[0])] {
            if a == b {
                continue;
            }
            let (Some(first), Some(second)) = (edge_key(mesh, a), edge_key(mesh, b)) else {
                continue;
            };
            if first == second {
                continue;
            }
            let key = if first <= second { (first, second) } else { (second, first) };
            if seen.insert(key) {
                edges.extend_from_slice(&[a, b]);
            }
        }
    }
    edges
}

/// The finite local position of one vertex, or `None` for a missing or hostile one.
fn vertex_position(mesh: &MeshData, index: u32) -> Option<DVec3> {
    let position = mesh.positions.get(index as usize)?;
    let value = DVec3::new(f64::from(position[0]), f64::from(position[1]), f64::from(position[2]));
    value.is_finite().then_some(value)
}

/// The quantised position of one vertex, or `None` for a missing or non-finite one.
fn edge_key(mesh: &MeshData, index: u32) -> Option<[i64; 3]> {
    let position = mesh.positions.get(index as usize)?;
    let mut key = [0i64; 3];
    for (slot, value) in key.iter_mut().zip(position) {
        let scaled = f64::from(*value) * 1000.0;
        if !scaled.is_finite() || !(-9.0e18..=9.0e18).contains(&scaled) {
            return None;
        }
        *slot = scaled.round() as i64;
    }
    Some(key)
}

/// Samples a stored path into the local points the viewport draws as a line chain.
///
/// The chain is sampled as an open contour even when the path is closed, so the closing segment is
/// drawn; the joint rule is the document's own, so a large arc whose rounded end misses the next
/// point is still one chain (ADR 0072).
pub fn path_points(path: &Path) -> Result<Vec<DVec3>, MeshError> {
    let segments = path
        .segments()
        .map(|segment| segment.to_geom())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| MeshError::Invalid(error.to_string()))?;
    let chain = geom::Contour::new_with_joint_tolerance(segments, false, |first, next| {
        model::joint_tolerance_mm(*first, *next)
    })
    .map_err(|error| MeshError::Invalid(error.to_string()))?;
    let samples = chain.sample(PATH_SAMPLE_TOLERANCE_MM).map_err(|error| MeshError::Invalid(error.to_string()))?;
    Ok(samples.into_iter().map(|point| DVec3::new(point.x, point.y, 0.0)).collect())
}

/// The derived geometry of one node, in its local frame.
///
/// A panel contributes its body and one mesh per band, a geometry object contributes one mesh, and
/// a path contributes a line chain instead. The meshes are shared handles, so a cache hit and a
/// scene rebuild copy an [`Arc`], never vertex data. Each solid's local [`Aabb`] is measured once
/// here, which is what lets a scene walk turn eight corners into a world box instead of reading
/// every vertex (X56).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeGeometry {
    body: Option<Arc<MeshData>>,
    body_bounds: Option<Aabb>,
    body_edges: Option<Arc<[u32]>>,
    body_all_edges: Option<Arc<[u32]>>,
    bands: Vec<Arc<MeshData>>,
    band_bounds: Vec<Option<Aabb>>,
    band_edges: Vec<Arc<[u32]>>,
    band_all_edges: Vec<Arc<[u32]>>,
    polylines: Vec<Arc<[DVec3]>>,
}

impl NodeGeometry {
    /// The panel body or the object mesh, when the node has a solid.
    pub const fn body(&self) -> Option<&Arc<MeshData>> {
        self.body.as_ref()
    }

    /// The box around [`NodeGeometry::body`] in the node's local millimetres, or `None` when the
    /// body has no finite vertex.
    pub const fn body_bounds(&self) -> Option<Aabb> {
        self.body_bounds
    }

    /// The display edges of [`NodeGeometry::body`]: boundary and crease edges, two indices per edge.
    pub const fn body_edges(&self) -> Option<&Arc<[u32]>> {
        self.body_edges.as_ref()
    }

    /// Every triangle edge of [`NodeGeometry::body`], diagonals included.
    pub const fn body_all_edges(&self) -> Option<&Arc<[u32]>> {
        self.body_all_edges.as_ref()
    }

    /// One mesh per evaluated band, in application order.
    pub fn bands(&self) -> &[Arc<MeshData>] {
        &self.bands
    }

    /// The box around each band's mesh, in the same order as [`NodeGeometry::bands`].
    ///
    /// A band mesh is validated before it reaches here, so its box is always measured; the `Option`
    /// keeps this list aligned with the meshes rather than dropping an entry.
    pub fn band_bounds(&self) -> &[Option<Aabb>] {
        &self.band_bounds
    }

    /// The display edges of each evaluated band, in the same order as [`NodeGeometry::bands`].
    pub fn band_edges(&self) -> &[Arc<[u32]>] {
        &self.band_edges
    }

    /// Every triangle edge of each evaluated band, in the same order as [`NodeGeometry::bands`].
    pub fn band_all_edges(&self) -> &[Arc<[u32]>] {
        &self.band_all_edges
    }

    /// The line chains of a drawing object, when the node has no solid.
    ///
    /// A path is one chain and a contour set is one chain per ring, in the order the rings are
    /// stored. A node with a solid has none.
    pub fn polylines(&self) -> &[Arc<[DVec3]>] {
        &self.polylines
    }

    /// Whether the node has nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.body.is_none() && self.bands.is_empty() && self.polylines.is_empty()
    }
}

/// The inputs a panel mesh was derived from.
///
/// The document keeps features, not geometry, so the cache records every feature the mesh depends
/// on and compares it on the next pass. The catalogue reaches the mesh through the resolved
/// thickness and the resolved band profiles, which are recorded by asking the same closure the
/// evaluation used, so a material swap rebuilds the panels that use it.
#[derive(Clone, Debug)]
struct PanelInputs {
    outline: PanelOutline,
    nominal_size: PanelNominalSize,
    machining: Vec<Machining>,
    edge_banding: Vec<EdgeBanding>,
    corner_banding: Vec<CornerBanding>,
    ring_banding: Vec<RingBanding>,
    band_order: Vec<SegmentSource>,
    bevels: Vec<EdgeBevel>,
    thickness: units::Length,
    profiles: Vec<(BandingSpec, EdgeBandProfile)>,
}

impl PanelInputs {
    /// Captures the derivation inputs of a panel, resolving every band profile through the same
    /// closure the evaluation uses.
    fn capture(panel: &Panel, geometry: &PanelGeometry, profile: &dyn Fn(&BandingSpec) -> EdgeBandProfile) -> Self {
        let profiles = panel
            .edge_banding()
            .iter()
            .map(|band| band.spec())
            .chain(panel.corner_banding().iter().map(|band| band.spec()))
            .chain(panel.ring_banding().iter().map(|band| band.spec()))
            .map(|spec| (spec, profile(&spec)))
            .collect();
        Self {
            outline: panel.outline().clone(),
            nominal_size: panel.nominal_size(),
            machining: panel.machining().to_vec(),
            edge_banding: panel.edge_banding().to_vec(),
            corner_banding: panel.corner_banding().to_vec(),
            ring_banding: panel.ring_banding().to_vec(),
            band_order: panel.band_order().to_vec(),
            bevels: panel.bevels().to_vec(),
            thickness: geometry.thickness(),
            profiles,
        }
    }

    /// Whether the panel still derives the same mesh as this capture.
    fn matches(
        &self,
        panel: &Panel,
        geometry: &PanelGeometry,
        profile: &dyn Fn(&BandingSpec) -> EdgeBandProfile,
    ) -> bool {
        self.outline == *panel.outline()
            && self.nominal_size == panel.nominal_size()
            && self.machining.as_slice() == panel.machining()
            && self.edge_banding.as_slice() == panel.edge_banding()
            && self.corner_banding.as_slice() == panel.corner_banding()
            && self.ring_banding.as_slice() == panel.ring_banding()
            && self.band_order.as_slice() == panel.band_order()
            && self.bevels.as_slice() == panel.bevels()
            && self.thickness == geometry.thickness()
            && self.profiles.iter().all(|(spec, known)| profile(spec) == *known)
    }
}

/// The document payload a derived mesh came from.
#[derive(Clone, Debug)]
enum Payload {
    Panel(PanelInputs),
    Mesh(Mesh),
    Constructive(Constructive),
    Primitive(Primitive),
    Path(Path),
    Contours(model::Contours),
}

/// Evaluates a constructive body into flat-shaded render triangles.
///
/// A history with one extrusion is answered analytically and exactly, without touching the kernel;
/// a history with a boolean step is compiled to a plan and built by the kernel, then tessellated by
/// the same call the panel path uses.
fn constructive_mesh_with(
    body: &Constructive,
    kernel: &mut dyn SolidKernel,
    options: &TessOptions,
) -> Result<MeshData, MeshError> {
    if body.features().len() == 1 {
        return object_mesh(&build_constructive_mesh(body)?);
    }
    let handle = param::evaluate_constructive_body(kernel, body)?;
    // The body is released even when tessellation refuses it: a node that cannot be built must not
    // leave a solid behind in the kernel for the rest of the session (W1).
    let data = tessellate_panel(&*kernel, handle, options);
    kernel.clear();
    data
}

/// One panel to evaluate: the node, its resolved geometry and the inputs its derivation reads.
///
/// A panel's mesh needs its own features and the catalogue's band profiles; its holes need the
/// panel's world placement and the drilling objects that reach it. The request carries all four, so
/// a caller states what a panel depends on once and the cache decides what to rebuild.
pub struct PanelEval<'a> {
    panel: &'a Panel,
    geometry: &'a PanelGeometry,
    placement: Placement,
    drillings: &'a [Drilling],
    profile: &'a dyn Fn(&BandingSpec) -> EdgeBandProfile,
}

impl<'a> PanelEval<'a> {
    /// Builds one request from a resolved panel.
    pub const fn new(
        panel: &'a Panel,
        geometry: &'a PanelGeometry,
        placement: Placement,
        drillings: &'a [Drilling],
        profile: &'a dyn Fn(&BandingSpec) -> EdgeBandProfile,
    ) -> Self {
        Self { panel, geometry, placement, drillings, profile }
    }
}

/// The inputs a panel's holes were derived from.
///
/// A hole depends on the material the panel has, on where the panel sits in the world and on the
/// drilling objects that reach it — not on its banding. The catalogue reaches the holes through the
/// resolved drilling objects themselves, so a directory revision that moves a hinge's part document
/// re-derives the holes of the panels that hinge reaches and no others.
#[derive(Clone, Debug, PartialEq)]
struct HoleInputs {
    outline: PanelOutline,
    nominal_size: PanelNominalSize,
    machining: Vec<Machining>,
    bevels: Vec<EdgeBevel>,
    thickness: units::Length,
    placement: Placement,
    drillings: Vec<Drilling>,
}

impl HoleInputs {
    /// Captures the derivation inputs of one panel, which a miss stores beside the holes.
    fn capture(request: &PanelEval<'_>) -> Self {
        Self {
            outline: request.panel.outline().clone(),
            nominal_size: request.panel.nominal_size(),
            machining: request.panel.machining().to_vec(),
            bevels: request.panel.bevels().to_vec(),
            thickness: request.geometry.thickness(),
            placement: request.placement,
            drillings: request.drillings.to_vec(),
        }
    }

    /// Whether the panel still receives the same holes as this capture.
    ///
    /// It compares without copying, so a frame that derives nothing allocates nothing.
    fn matches(&self, request: &PanelEval<'_>) -> bool {
        self.outline == *request.panel.outline()
            && self.nominal_size == request.panel.nominal_size()
            && self.machining.as_slice() == request.panel.machining()
            && self.bevels.as_slice() == request.panel.bevels()
            && self.thickness == request.geometry.thickness()
            && self.placement.same_transform(request.placement)
            && self.drillings.as_slice() == request.drillings
    }
}

/// One cached node: the payload it was derived from, the mesh, the holes and the pass that last saw it.
#[derive(Clone, Debug)]
struct Entry {
    revision: Revision,
    payload: Payload,
    geometry: NodeGeometry,
    holes: DerivedHoles,
    hole_inputs: Option<HoleInputs>,
    holes_revision: Revision,
    pass: u64,
}

/// A revision-tagged cache of evaluated node geometry.
///
/// The cache is a pure derivation store: it never mutates the document and never owns a kernel
/// body. Each entry remembers the revision that built it and the inputs it was built from, so a
/// rebuild walks the tree once, reuses every node whose inputs are unchanged, and drops the entries
/// of nodes that no longer exist.
///
/// A caller brackets a walk with [`EvaluationCache::start`] and [`EvaluationCache::finish`]; the
/// second call prunes every entry the walk did not visit, which is how a deleted node releases its
/// mesh.
#[derive(Debug, Default)]
pub struct EvaluationCache {
    nodes: HashMap<Id, Entry>,
    pass: u64,
}

impl EvaluationCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts one evaluation pass and marks every existing entry unseen.
    pub fn start(&mut self) {
        self.pass = match self.pass.checked_add(1) {
            Some(next) => next,
            None => {
                self.nodes.clear();
                1
            }
        };
    }

    /// Ends the pass, dropping every node the walk did not visit.
    pub fn finish(&mut self) {
        let pass = self.pass;
        self.nodes.retain(|_, entry| entry.pass == pass);
    }

    /// Drops every entry, for a new session or a failed evaluation that invalidated the kernel.
    pub fn clear(&mut self) {
        self.nodes.clear();
    }

    /// Number of cached nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether no node is cached.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The revision that produced the cached mesh of one node.
    ///
    /// A node whose inputs did not change keeps the earlier revision, which is what "a rebuild
    /// touches only changed nodes" means in observable terms.
    pub fn built_at(&self, id: Id) -> Option<Revision> {
        self.nodes.get(&id).map(|entry| entry.revision)
    }

    /// The cached geometry of one node, without evaluating it.
    pub fn cached(&self, id: Id) -> Option<&NodeGeometry> {
        self.nodes.get(&id).map(|entry| &entry.geometry)
    }

    /// The holes one panel received in its last evaluation.
    ///
    /// A node that is not a panel answers an empty set. The value is read by the panel card, the
    /// report, the drawing and the CNC program — one list, so they cannot disagree (ADR 0150).
    pub fn holes(&self, id: Id) -> Option<&DerivedHoles> {
        self.nodes.get(&id).map(|entry| &entry.holes)
    }

    /// The revision that last derived one panel's holes.
    ///
    /// A panel whose mesh was reused but whose holes were re-derived — a hinge that moved — keeps
    /// its old mesh revision and reports this one. A frame that derives nothing advances neither.
    pub fn holes_built_at(&self, id: Id) -> Option<Revision> {
        self.nodes.get(&id).map(|entry| entry.holes_revision)
    }

    /// Evaluates one panel, reusing the cached mesh and holes when their inputs are unchanged.
    ///
    /// The holes are derived first, because they are cheap and independent of the mesh: a hinge that
    /// moved re-derives them and leaves the body alone. The profile closure resolves band dimensions
    /// from the session catalogue; it is called again only to compare banding with the cached
    /// inputs. The kernel is touched only on a mesh miss, and every body it builds is released
    /// before the call returns.
    pub fn panel(
        &mut self,
        revision: Revision,
        request: &PanelEval<'_>,
        kernel: &mut dyn SolidKernel,
        options: &TessOptions,
    ) -> Result<NodeGeometry, MeshError> {
        let panel = request.panel;
        let geometry = request.geometry;
        let id = panel.id();
        let pass = self.pass;

        // The holes are a second derivation with a second key, so a moved hinge costs a clip and no
        // tessellation. `None` means the stored holes are still the right ones and nothing is built.
        let holes = match self.nodes.get(&id) {
            Some(entry) if entry.hole_inputs.as_ref().is_some_and(|inputs| inputs.matches(request)) => None,
            _ => Some(derive_holes(panel, geometry, request.placement, request.drillings)?),
        };

        if let Some(entry) = self.nodes.get_mut(&id)
            && let Payload::Panel(inputs) = &entry.payload
            && inputs.matches(panel, geometry, request.profile)
        {
            entry.pass = pass;
            if let Some(holes) = holes {
                entry.holes = holes;
                entry.hole_inputs = Some(HoleInputs::capture(request));
                entry.holes_revision = revision;
            }
            return Ok(entry.geometry.clone());
        }

        let holes = match holes {
            Some(holes) => holes,
            None => self.nodes.get(&id).map(|entry| entry.holes.clone()).unwrap_or_default(),
        };
        let inputs = PanelInputs::capture(panel, geometry, request.profile);
        let parts = build_panel_parts_with(kernel, panel, geometry, request.profile)?;
        let body = tessellate_panel(&*kernel, parts.body(), options)?;
        let body_bounds = body.local_bounds();
        let body_edges: Arc<[u32]> = mesh_edges(&body).into();
        let body_all_edges: Arc<[u32]> = mesh_all_edges(&body).into();
        let mut bands = Vec::with_capacity(parts.bands().len());
        let mut band_bounds = Vec::with_capacity(parts.bands().len());
        let mut band_edges = Vec::with_capacity(parts.bands().len());
        let mut band_all_edges = Vec::with_capacity(parts.bands().len());
        for part in parts.bands() {
            let mesh = tessellate_panel(&*kernel, part.body(), options)?;
            band_edges.push(Arc::from(mesh_edges(&mesh)));
            band_all_edges.push(Arc::from(mesh_all_edges(&mesh)));
            if let Some(bounds) = mesh.local_bounds() {
                band_bounds.push(Some(bounds));
            } else {
                band_bounds.push(None);
            }
            bands.push(Arc::new(mesh));
        }
        kernel.clear();
        let geometry = NodeGeometry {
            body: Some(Arc::new(body)),
            body_bounds,
            body_edges: Some(body_edges),
            body_all_edges: Some(body_all_edges),
            bands,
            band_bounds,
            band_edges,
            band_all_edges,
            polylines: Vec::new(),
        };
        self.store(id, revision, Payload::Panel(inputs), geometry.clone(), holes, Some(HoleInputs::capture(request)));
        Ok(geometry)
    }

    /// Evaluates one geometry object, reusing the cached mesh when its payload is unchanged.
    ///
    /// A baked mesh is triangulated, a constructive payload is evaluated through the same function
    /// `object.collapse` bakes, a primitive is built from its parameters by the same function the
    /// viewport draws and the collapse bakes, and a path becomes a line chain. A constructive body
    /// that names a boolean step leaves the analytic path and is built by the kernel.
    pub fn object(
        &mut self,
        revision: Revision,
        object: &Object,
        kernel: &mut dyn SolidKernel,
        options: &TessOptions,
    ) -> Result<NodeGeometry, MeshError> {
        self.object_with(revision, object.id(), object.geometry(), kernel, options)
    }

    /// Evaluates one object's geometry, which the caller may have substituted.
    ///
    /// A previewing operation draws a node the document does not hold yet — the body at the height
    /// the pull shows — so the evaluator takes the geometry it is handed rather than reading it off
    /// the node. The entry is keyed by the node id and compared with the payload it was built from,
    /// so a preview and the document cannot both be cached under one id: whichever is evaluated
    /// next re-derives the node once.
    ///
    /// A boolean step carries the shape it cuts with, so an object's payload is everything its mesh
    /// depends on: no second node can change the result (ADR 0197).
    pub fn object_with(
        &mut self,
        revision: Revision,
        id: Id,
        geometry: &model::Geometry,
        kernel: &mut dyn SolidKernel,
        options: &TessOptions,
    ) -> Result<NodeGeometry, MeshError> {
        let pass = self.pass;
        if let Some(entry) = self.nodes.get_mut(&id) {
            let unchanged = match (&entry.payload, geometry) {
                (Payload::Mesh(cached), model::Geometry::Mesh(current)) => cached == current,
                (Payload::Constructive(cached), model::Geometry::Constructive(current)) => cached == current,
                (Payload::Primitive(cached), model::Geometry::Primitive(current)) => cached == current,
                (Payload::Path(cached), model::Geometry::Path(current)) => cached == current,
                (Payload::Contours(cached), model::Geometry::Contours(current)) => cached == current,
                _ => false,
            };
            if unchanged {
                entry.pass = pass;
                return Ok(entry.geometry.clone());
            }
        }

        let (payload, evaluated) = match geometry {
            model::Geometry::Mesh(mesh) => {
                let data = object_mesh(mesh)?;
                (Payload::Mesh(mesh.clone()), solid_geometry(data))
            }
            model::Geometry::Constructive(constructive) => {
                let data = constructive_mesh_with(constructive, kernel, options)?;
                (Payload::Constructive(constructive.clone()), solid_geometry(data))
            }
            model::Geometry::Primitive(primitive) => {
                let data = primitive_mesh(primitive)?;
                (Payload::Primitive(primitive.clone()), solid_geometry(data))
            }
            model::Geometry::Path(path) => (
                Payload::Path(path.clone()),
                NodeGeometry { polylines: vec![Arc::from(path_points(path)?)], ..NodeGeometry::default() },
            ),
            model::Geometry::Contours(contours) => {
                let mut polylines = Vec::with_capacity(contours.paths().len());
                for path in contours.paths() {
                    polylines.push(Arc::from(path_points(path)?));
                }
                (Payload::Contours(contours.clone()), NodeGeometry { polylines, ..NodeGeometry::default() })
            }
        };
        self.store(id, revision, payload, evaluated.clone(), DerivedHoles::default(), None);
        Ok(evaluated)
    }

    /// Stores one freshly derived node, tagged with the current pass.
    fn store(
        &mut self,
        id: Id,
        revision: Revision,
        payload: Payload,
        geometry: NodeGeometry,
        holes: DerivedHoles,
        hole_inputs: Option<HoleInputs>,
    ) {
        self.nodes.insert(
            id,
            Entry { revision, payload, geometry, holes, hole_inputs, holes_revision: revision, pass: self.pass },
        );
    }
}

/// The derived geometry of one solid: the render mesh, its local box and its feature and full edge
/// lists.
fn solid_geometry(data: MeshData) -> NodeGeometry {
    let bounds = data.local_bounds();
    let edges: Arc<[u32]> = mesh_edges(&data).into();
    let all_edges: Arc<[u32]> = mesh_all_edges(&data).into();
    NodeGeometry {
        body: Some(Arc::new(data)),
        body_bounds: bounds,
        body_edges: Some(edges),
        body_all_edges: Some(all_edges),
        ..NodeGeometry::default()
    }
}

/// The face's vertices in local millimetres, in face order.
fn face_points(mesh: &Mesh, face: &[u32]) -> Result<Vec<DVec3>, MeshError> {
    let mut points = Vec::with_capacity(face.len());
    for index in face {
        let Some(position) = mesh.positions().get(*index as usize) else {
            return Err(MeshError::Invalid(format!("face refers to missing vertex {index}")));
        };
        let point = DVec3::new(
            position[0].micrometres() as f64 / 1000.0,
            position[1].micrometres() as f64 / 1000.0,
            position[2].micrometres() as f64 / 1000.0,
        );
        if !point.is_finite() {
            return Err(MeshError::Invalid("a vertex is not finite".into()));
        }
        points.push(point);
    }
    Ok(points)
}

/// The fan fallback: triangles from the first vertex, valid only while the face is convex.
fn fan_triangles(len: usize) -> Vec<[u32; 3]> {
    let mut triangles = Vec::with_capacity(len.saturating_sub(2));
    for index in 1..len.saturating_sub(1) {
        let (Ok(current), Ok(next)) = (u32::try_from(index), u32::try_from(index + 1)) else {
            break;
        };
        triangles.push([0, current, next]);
    }
    triangles
}

/// Newell's normal of a polygon, or `None` when it is degenerate.
fn face_normal(points: &[DVec3]) -> Option<DVec3> {
    let mut normal = DVec3::ZERO;
    for (index, current) in points.iter().enumerate() {
        let next = points.get((index + 1) % points.len())?;
        normal.x += (current.y - next.y) * (current.z + next.z);
        normal.y += (current.z - next.z) * (current.x + next.x);
        normal.z += (current.x - next.x) * (current.y + next.y);
    }
    let length = normal.length();
    (length.is_finite() && length > f64::EPSILON).then(|| normal / length)
}

fn validate_mesh(mesh: &MeshData) -> Result<(), MeshError> {
    if mesh.positions.is_empty() {
        return Err(MeshError::Invalid("the mesh has no vertices".into()));
    }
    if mesh.indices.is_empty() {
        return Err(MeshError::Invalid("the mesh has no triangles".into()));
    }
    if !mesh.indices.len().is_multiple_of(3) {
        return Err(MeshError::Invalid("the index buffer is not a whole number of triangles".into()));
    }
    if !mesh.normals.is_empty() && mesh.normals.len() != mesh.positions.len() {
        return Err(MeshError::Invalid("the normal buffer does not match the vertex buffer".into()));
    }
    if mesh.positions.iter().flatten().any(|value| !value.is_finite()) {
        return Err(MeshError::Invalid("the mesh has a non-finite vertex position".into()));
    }
    if mesh.normals.iter().flatten().any(|value| !value.is_finite()) {
        return Err(MeshError::Invalid("the mesh has a non-finite normal".into()));
    }

    let vertex_count = u32::try_from(mesh.positions.len())
        .map_err(|_| MeshError::Invalid("the mesh has more vertices than an index can address".into()))?;
    if mesh.indices.iter().any(|index| *index >= vertex_count) {
        return Err(MeshError::Invalid("the mesh has an index outside the vertex buffer".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    /// Evaluates one object through the cache, with a kernel of its own.
    fn evaluated(cache: &mut EvaluationCache, revision: Revision, object: &Object) -> Result<NodeGeometry, MeshError> {
        let mut kernel = TruckKernel::new();
        cache.object(revision, object, &mut kernel, &TessOptions::default())
    }

    use kernel::{BooleanOp, Measure, Profile};
    use kernel_truck::TruckKernel;
    use model::{Contour, EdgeBevel, EdgeId, EdgeRef, Id, Panel, PanelNominalSize, Placement, RectEdge};
    use param::build_panel;
    use testkit::millimetres;
    use units::{Degrees, Length};

    use super::*;

    struct StaticKernel {
        mesh: Option<MeshData>,
    }

    impl SolidKernel for StaticKernel {
        fn extrude(&mut self, _profile: &Profile, _frame: &geom::Frame, _height: f64) -> kernel::Result<BodyId> {
            Err(KernelError::Unsupported("static kernel does not implement extrusion"))
        }

        fn boolean(&mut self, _op: BooleanOp, _target: BodyId, _tool: BodyId) -> kernel::Result<BodyId> {
            Err(KernelError::Unsupported("static kernel does not implement booleans"))
        }

        fn mesh(&mut self, _body: &kernel::MeshBody) -> kernel::Result<BodyId> {
            Err(KernelError::Unsupported("static kernel does not build mesh bodies"))
        }

        fn tessellate(&self, _body: BodyId, _options: &TessOptions) -> kernel::Result<MeshData> {
            self.mesh.clone().ok_or(KernelError::Unsupported("static kernel has no mesh"))
        }

        fn measure(&self, _body: BodyId) -> kernel::Result<Measure> {
            Err(KernelError::Unsupported("static kernel does not implement measurement"))
        }

        fn clear(&mut self) {
            self.mesh = None;
        }
    }

    fn triangle() -> MeshData {
        MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            indices: vec![0, 1, 2],
        }
    }

    #[test]
    fn tessellate_panel_returns_a_valid_mesh() {
        let expected = triangle();
        let kernel = StaticKernel { mesh: Some(expected.clone()) };
        let mesh = tessellate_panel(&kernel, BodyId::from_raw(1), &TessOptions::default()).expect("valid mesh");
        assert_eq!(mesh, expected);
    }

    #[test]
    fn tessellate_panel_propagates_kernel_errors() {
        let kernel = StaticKernel { mesh: None };
        let error = tessellate_panel(&kernel, BodyId::from_raw(1), &TessOptions::default()).expect_err("kernel error");
        assert!(matches!(error, MeshError::Kernel(KernelError::Unsupported(_))));
    }

    #[test]
    fn invalid_meshes_are_rejected() {
        let mut mesh = triangle();
        mesh.positions.clear();
        assert!(matches!(validate_mesh(&mesh), Err(MeshError::Invalid(_))));

        let mut mesh = triangle();
        mesh.indices.clear();
        assert!(matches!(validate_mesh(&mesh), Err(MeshError::Invalid(_))));

        let mut mesh = triangle();
        mesh.indices.pop();
        assert!(matches!(validate_mesh(&mesh), Err(MeshError::Invalid(_))));

        let mut mesh = triangle();
        mesh.normals.pop();
        assert!(matches!(validate_mesh(&mesh), Err(MeshError::Invalid(_))));

        let mut mesh = triangle();
        mesh.positions[0][0] = f32::NAN;
        assert!(matches!(validate_mesh(&mesh), Err(MeshError::Invalid(_))));

        let mut mesh = triangle();
        mesh.indices[2] = 3;
        assert!(matches!(validate_mesh(&mesh), Err(MeshError::Invalid(_))));

        let mut mesh = triangle();
        mesh.normals.clear();
        assert!(validate_mesh(&mesh).is_ok());
    }

    #[test]
    fn object_mesh_triangulates_a_cube_into_twelve_flat_triangles() {
        let cube = testkit::cube_mesh(millimetres(100));
        let mesh = object_mesh(&cube).expect("render mesh");
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.vertex_count(), 36, "flat shading duplicates vertices per face");
        for normal in &mesh.normals {
            let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            assert!((length - 1.0).abs() < 1.0e-5, "normal was {normal:?}");
            let axes = normal.iter().filter(|value| value.abs() > 0.5).count();
            assert_eq!(axes, 1, "a cube normal points along one axis, was {normal:?}");
        }
        let half = 50.0f32;
        for position in &mesh.positions {
            assert_eq!(position[0].abs(), half);
            assert_eq!(position[1].abs(), half);
            assert_eq!(position[2].abs(), half);
        }
    }

    #[test]
    fn object_mesh_skips_degenerate_faces_without_failing() {
        let collinear = model::Mesh::new(
            vec![
                [millimetres(0), millimetres(0), millimetres(0)],
                [millimetres(10), millimetres(0), millimetres(0)],
                [millimetres(20), millimetres(0), millimetres(0)],
                [millimetres(0), millimetres(10), millimetres(0)],
            ],
            vec![vec![0, 1, 2], vec![0, 1, 3]],
        )
        .expect("the model accepts a geometrically degenerate face");
        let mesh = object_mesh(&collinear).expect("render mesh");
        assert_eq!(mesh.triangle_count(), 1, "only the triangle with area survives");
        assert!(mesh.positions.iter().flatten().all(|value| value.is_finite()));
    }

    #[test]
    fn object_mesh_with_only_degenerate_faces_is_empty() {
        let collinear = model::Mesh::new(
            vec![
                [millimetres(0), millimetres(0), millimetres(0)],
                [millimetres(10), millimetres(0), millimetres(0)],
                [millimetres(20), millimetres(0), millimetres(0)],
            ],
            vec![vec![0, 1, 2]],
        )
        .expect("valid mesh");
        let mesh = object_mesh(&collinear).expect("render mesh");
        assert!(mesh.is_empty());
    }

    /// A 100 x 100 contour with a 20 x 60 notch cut into its top edge, in millimetres.
    fn notched_contour() -> Contour {
        let points = [[0, 0], [100, 0], [100, 100], [60, 100], [60, 40], [40, 40], [40, 100], [0, 100]]
            .map(|[x, y]| [millimetres(x), millimetres(y)]);
        Contour::from_polygon(points.to_vec()).expect("a simple contour")
    }

    fn inside_notched_contour(point: [f32; 2]) -> bool {
        let ring = notched_contour();
        let points = ring.sampled_points(1.0).expect("sampled contour");
        let mut inside = false;
        for (index, current) in points.iter().enumerate() {
            let Some(next) = points.get((index + 1) % points.len()) else {
                continue;
            };
            let (ax, ay) = (current[0].millimetres_f64() as f32, current[1].millimetres_f64() as f32);
            let (bx, by) = (next[0].millimetres_f64() as f32, next[1].millimetres_f64() as f32);
            if (ay > point[1]) != (by > point[1]) {
                let at = (bx - ax) * (point[1] - ay) / (by - ay) + ax;
                if point[0] < at {
                    inside = !inside;
                }
            }
        }
        inside
    }

    #[test]
    fn a_coplanar_quad_drops_its_diagonal_and_keeps_the_boundary() {
        // Two triangles of a square, each with its own copy of the shared corners, exactly as a
        // flat-shaded mesh stores them. The diagonal is a triangulation artefact, not a form edge.
        let mesh = MeshData {
            positions: vec![
                [0.0, 0.0, 0.0],
                [10.0, 0.0, 0.0],
                [10.0, 10.0, 0.0],
                [0.0, 0.0, 0.0],
                [10.0, 10.0, 0.0],
                [0.0, 10.0, 0.0],
            ],
            normals: Vec::new(),
            indices: vec![0, 1, 2, 3, 4, 5],
        };
        let edges = mesh_edges(&mesh);
        assert_eq!(edges.len() / 2, 4, "a flat quad shows four boundary edges");
        assert!(edges.len().is_multiple_of(2));

        let pairs = edges.as_chunks::<2>().0;
        let same = |pair: &[u32; 2], a: [f32; 3], b: [f32; 3]| {
            let first = mesh.positions[pair[0] as usize];
            let second = mesh.positions[pair[1] as usize];
            (first == a && second == b) || (first == b && second == a)
        };
        let diagonal = pairs.iter().filter(|pair| same(pair, [0.0, 0.0, 0.0], [10.0, 10.0, 0.0])).count();
        assert_eq!(diagonal, 0, "the coplanar diagonal is not a display edge");
        let boundary = pairs.iter().filter(|pair| same(pair, [0.0, 0.0, 0.0], [10.0, 0.0, 0.0])).count();
        assert_eq!(boundary, 1, "a boundary edge is kept");

        let all = mesh_all_edges(&mesh);
        assert_eq!(all.len() / 2, 5, "the diagnostic list still carries the diagonal");
    }

    /// A flat cap cut into a grid of coplanar triangles: the form edges are the outline only.
    #[test]
    fn a_subdivided_flat_cap_shows_no_inner_edge() {
        let side = 4usize;
        let step = 10.0_f32;
        let mut positions = Vec::with_capacity(side * side);
        for row in 0..side {
            for column in 0..side {
                positions.push([column as f32 * step, row as f32 * step, 0.0]);
            }
        }
        let vertex = |row: usize, column: usize| (row * side + column) as u32;
        let mut indices = Vec::new();
        for row in 0..side - 1 {
            for column in 0..side - 1 {
                let (a, b) = (vertex(row, column), vertex(row, column + 1));
                let (c, d) = (vertex(row + 1, column + 1), vertex(row + 1, column));
                indices.extend([a, b, c, a, c, d]);
            }
        }
        let mesh = MeshData { positions, normals: Vec::new(), indices };
        let outline = 4 * (side as u32 - 1);
        assert_eq!(mesh_edges(&mesh).len() / 2, outline as usize, "a flat cap shows its outline and no inner edge");
        // The diagnostic list keeps every triangulation edge: one diagonal per cell, plus the grid.
        let grid = 2 * (side as u32 - 1) * side as u32 + (side as u32 - 1) * (side as u32 - 1);
        assert_eq!(mesh_all_edges(&mesh).len() / 2, grid as usize);
    }

    /// The display edges of a sphere mesh, sorted the way a person reads the graticule: latitude
    /// rings by height, the two pole-cap fans, and the meridian strips between two neighbouring
    /// rings.
    struct Graticule {
        rings: std::collections::BTreeMap<i64, usize>,
        poles: usize,
        strips: std::collections::BTreeMap<(i64, i64), usize>,
    }

    fn graticule(mesh: &MeshData) -> Graticule {
        let key = |value: f32| (f64::from(value) * 1000.0).round() as i64;
        let is_pole = |point: &[f32; 3]| key(point[0]) == 0 && key(point[1]) == 0;
        let mut graticule = Graticule { rings: Default::default(), poles: 0, strips: Default::default() };
        for pair in mesh_edges(mesh).as_chunks::<2>().0 {
            let (Some(a), Some(b)) = (mesh.positions.get(pair[0] as usize), mesh.positions.get(pair[1] as usize))
            else {
                continue;
            };
            if key(a[2]) == key(b[2]) {
                *graticule.rings.entry(key(a[2])).or_default() += 1;
            } else if is_pole(a) || is_pole(b) {
                graticule.poles += 1;
            } else {
                let low = key(a[2]).min(key(b[2]));
                let high = key(a[2]).max(key(b[2]));
                *graticule.strips.entry((low, high)).or_default() += 1;
            }
        }
        graticule
    }

    #[test]
    fn a_fine_sphere_keeps_a_meridian_at_every_band_and_a_ring_at_every_latitude() {
        // The observed defect: raising the side count removed meridians instead of adding them, so
        // a 50 mm ball read bald at the poles from about 160 sides. The margin used to be 2.6
        // degrees, which is wider than the meridian fold of a finely tessellated smooth surface.
        let segments = 256usize;
        let bands = 16usize;
        let primitive = Primitive::sphere(millimetres(50), segments as u32, bands as u32).expect("a 50 mm ball");
        let mesh = primitive_mesh(&primitive).expect("render mesh");
        let graticule = graticule(&mesh);
        assert_eq!(graticule.rings.len(), bands - 1, "one latitude ring at every band boundary");
        assert!(
            graticule.rings.values().all(|count| *count == segments),
            "every ring runs all the way round: {:?}",
            graticule.rings
        );
        assert_eq!(graticule.strips.len(), bands - 2, "a meridian strip between every pair of rings");
        assert!(
            graticule.strips.values().all(|count| *count == segments),
            "every meridian strip runs pole to pole: {:?}",
            graticule.strips
        );
        assert_eq!(graticule.poles, 2 * segments, "each pole keeps the fan that draws its meridians");
    }

    #[test]
    fn the_default_sphere_wireframe_is_still_the_whole_graticule() {
        let segments = model::DEFAULT_SEGMENTS as usize;
        let bands = model::DEFAULT_BANDS as usize;
        let primitive = Primitive::sphere(millimetres(50), model::DEFAULT_SEGMENTS, model::DEFAULT_BANDS)
            .expect("the default ball");
        let mesh = primitive_mesh(&primitive).expect("render mesh");
        let graticule = graticule(&mesh);
        assert_eq!(graticule.rings.len(), bands - 1);
        assert!(graticule.rings.values().all(|count| *count == segments));
        assert_eq!(graticule.strips.len(), bands - 2);
        assert!(graticule.strips.values().all(|count| *count == segments));
        assert_eq!(graticule.poles, 2 * segments);
        // 15 rings, 14 strips and 2 pole fans of 32 edges each; the ring and strip edges are what
        // the old margin already drew, the pole fan is the part it erased.
        assert_eq!(mesh_edges(&mesh).len() / 2, (2 * bands - 1) * segments);
    }

    #[test]
    fn a_crease_edge_survives_between_two_faces() {
        // Two triangles folded by 90 degrees around the edge (0,0,0)-(10,0,0). The shared edge is
        // a real form edge and must survive, unlike a coplanar diagonal.
        let mesh = MeshData {
            positions: vec![
                [0.0, 0.0, 0.0],
                [10.0, 0.0, 0.0],
                [0.0, 10.0, 0.0],
                [0.0, 0.0, 0.0],
                [10.0, 0.0, 0.0],
                [0.0, 0.0, 10.0],
            ],
            normals: Vec::new(),
            indices: vec![0, 1, 2, 3, 4, 5],
        };
        let edges = mesh_edges(&mesh);
        assert_eq!(edges.len() / 2, 5, "four boundary edges and the crease");
        let pairs = edges.as_chunks::<2>().0;
        let shared = pairs
            .iter()
            .filter(|pair| {
                let first = mesh.positions[pair[0] as usize];
                let second = mesh.positions[pair[1] as usize];
                (first == [0.0, 0.0, 0.0] && second == [10.0, 0.0, 0.0])
                    || (first == [10.0, 0.0, 0.0] && second == [0.0, 0.0, 0.0])
            })
            .count();
        assert_eq!(shared, 1, "the fold edge is drawn once");
    }

    #[test]
    fn a_cube_edge_list_is_the_twelve_box_edges_without_diagonals() {
        let cube = testkit::cube_mesh(millimetres(100));
        let mesh = object_mesh(&cube).expect("render mesh");
        assert_eq!(mesh_edges(&mesh).len() / 2, 12, "a cube shows its twelve box edges");
        assert_eq!(mesh_all_edges(&mesh).len() / 2, 18, "the diagnostic list adds six face diagonals");
    }

    #[test]
    fn a_triangular_prism_keeps_its_three_cap_edges_and_side_seams() {
        // A three-sided prism: two triangular caps and three rectangular sides. Every cap edge is a
        // real form edge, and each short side face shares one with the next, so a triangular body
        // shows nine edges — not the triangulation of its caps.
        let positions = vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [5.0, 8.0, 0.0],
            [0.0, 0.0, 10.0],
            [10.0, 0.0, 10.0],
            [5.0, 8.0, 10.0],
        ];
        let mesh = MeshData {
            positions,
            normals: Vec::new(),
            // Two caps and three sides, each quad split into two coplanar triangles.
            indices: vec![
                0, 2, 1, // bottom cap (one triangle, no diagonal)
                3, 4, 5, // top cap
                0, 1, 4, 0, 4, 3, // front side
                1, 2, 5, 1, 5, 4, // right side
                2, 0, 3, 2, 3, 5, // left side
            ],
        };
        let edges = mesh_edges(&mesh);
        assert_eq!(edges.len() / 2, 9, "three cap edges on each cap and three vertical seams");
        let all = mesh_all_edges(&mesh);
        assert_eq!(all.len() / 2, 9 + 3, "the diagnostic list adds one diagonal per quad side");
    }

    #[test]
    fn mesh_edges_skip_degenerate_and_malformed_edges() {
        let mesh = MeshData {
            positions: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
            normals: Vec::new(),
            indices: vec![0, 1, 1, 0, 2, 9],
        };
        assert!(mesh_edges(&mesh).is_empty(), "a degenerate and a missing triangle produce no form edge");
        assert_eq!(mesh_all_edges(&mesh), vec![0, 1, 0, 2], "a repeated vertex and a missing one produce no line");
    }

    #[test]
    fn a_concave_extrusion_cap_keeps_its_triangles_inside_the_contour() {
        let constructive =
            Constructive::extrude(notched_contour().into(), millimetres(18)).expect("a constructive object");
        let mesh = evaluate_constructive(&constructive).expect("document mesh");
        let data = object_mesh(&mesh).expect("render mesh");
        let mut caps = 0;
        for triangle in data.indices.as_chunks::<3>().0 {
            let corners: Vec<[f32; 3]> =
                triangle.iter().filter_map(|index| data.positions.get(*index as usize).copied()).collect();
            let [Some(a), Some(b), Some(c)] = [corners.first(), corners.get(1), corners.get(2)] else {
                continue;
            };
            if a[2] != b[2] || b[2] != c[2] {
                continue;
            }
            caps += 1;
            let centroid = [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0];
            assert!(
                inside_notched_contour(centroid),
                "a cap triangle at {centroid:?} left the contour: {triangle:?} of {corners:?}"
            );
        }
        assert_eq!(caps, 12, "each cap of an eight-point ring has six triangles");
    }

    #[test]
    fn truck_kernel_tessellates_a_panel_in_local_coordinates() {
        let mut kernel = TruckKernel::new();
        let size = PanelNominalSize::new(millimetres(700), millimetres(500)).expect("valid panel size");
        let panel = Panel::new(Id::new(1).expect("id"), "Panel", size, None, millimetres(18), Placement::IDENTITY)
            .expect("valid panel");
        let geometry = panel.geometry_with_fallbacks().expect("resolved geometry");
        let body = build_panel(&mut kernel, &panel, &geometry).expect("panel body");
        let mesh = tessellate_panel(&kernel, body, &TessOptions::default()).expect("panel mesh");

        assert!(mesh.triangle_count() >= 12, "triangles: {}", mesh.triangle_count());
        assert_eq!(mesh.positions.len(), mesh.normals.len());
        assert_eq!(mesh.indices.len(), mesh.triangle_count() * 3);
        for position in &mesh.positions {
            assert!((-1.0e-3_f32..=700.0_f32 + 1.0e-3_f32).contains(&position[0]));
            assert!((-1.0e-3_f32..=500.0_f32 + 1.0e-3_f32).contains(&position[1]));
            assert!((-1.0e-3_f32..=18.0_f32 + 1.0e-3_f32).contains(&position[2]));
        }
    }

    fn sample_panel() -> Panel {
        let size = PanelNominalSize::new(millimetres(700), millimetres(500)).expect("valid panel size");
        Panel::new(Id::new(1).expect("id"), "Panel", size, None, millimetres(18), Placement::IDENTITY)
            .expect("valid panel")
    }

    /// The four support-joint bands of a rectangle, resolved from the stored fallbacks.
    fn panel_profile(spec: &BandingSpec) -> EdgeBandProfile {
        EdgeBandProfile::from_spec(spec)
    }

    /// A request for one panel at the world origin, with no drilling object reaching it.
    fn plain<'a>(panel: &'a Panel, geometry: &'a PanelGeometry) -> PanelEval<'a> {
        PanelEval::new(panel, geometry, Placement::IDENTITY, &[], &panel_profile)
    }

    fn band(material: u64, thickness: i64, width: i64) -> EdgeBanding {
        EdgeBanding::new(
            EdgeId::new(1).expect("edge"),
            model::ItemId::new(1, u32::try_from(material).unwrap_or(u32::MAX)).expect("material"),
            false,
            millimetres(thickness),
            millimetres(width),
        )
        .expect("band")
    }

    #[test]
    fn a_panel_is_evaluated_once_and_reused_while_its_inputs_hold() {
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let panel = sample_panel();
        let geometry = panel.geometry_with_fallbacks().expect("geometry");

        cache.start();
        let first = cache
            .panel(Revision::ZERO, &plain(&panel, &geometry), &mut kernel, &TessOptions::default())
            .expect("panel mesh");
        cache.finish();
        assert!(!first.is_empty());
        assert_eq!(cache.built_at(panel.id()), Some(Revision::ZERO));
        assert_eq!(kernel.body_count(), 0, "a cached panel releases every kernel body");

        // A later revision with the same panel reuses the mesh instead of tessellating again.
        cache.start();
        let second = cache
            .panel(
                Revision::ZERO.checked_next().expect("revision"),
                &plain(&panel, &geometry),
                &mut kernel,
                &TessOptions::default(),
            )
            .expect("panel mesh");
        cache.finish();
        assert!(Arc::ptr_eq(first.body().expect("body"), second.body().expect("body")), "the mesh is shared");
        assert_eq!(cache.built_at(panel.id()), Some(Revision::ZERO), "an unchanged node keeps its revision");
    }

    #[test]
    fn a_hinge_on_a_door_is_derived_once_and_an_untouched_panel_is_not_rebuilt() {
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let panel = sample_panel();
        let geometry = panel.geometry_with_fallbacks().expect("geometry");
        let placement =
            Placement::new(DVec3::new(1_200.0, 400.0, 300.0), geom::DQuat::IDENTITY).expect("a placed door");
        let cup = |x: f64| {
            Drilling::new(
                Id::new(11).expect("id"),
                placement.transform_point(DVec3::new(x, 22.0, 18.0)),
                placement.transform_vector(-DVec3::Z),
                millimetres(35),
                millimetres(13),
            )
            .expect("a cup bore")
        };
        let first_drillings = [cup(350.0)];

        cache.start();
        let first = cache
            .panel(
                Revision::ZERO,
                &PanelEval::new(&panel, &geometry, placement, &first_drillings, &panel_profile),
                &mut kernel,
                &TessOptions::default(),
            )
            .expect("panel");
        cache.finish();
        let holes = cache.holes(panel.id()).expect("holes").holes();
        assert_eq!(holes.len(), 1);
        assert_eq!(holes[0].entry(), [millimetres(350), millimetres(22), millimetres(18)]);
        assert_eq!(cache.built_at(panel.id()), Some(Revision::ZERO));
        assert_eq!(cache.holes_built_at(panel.id()), Some(Revision::ZERO));

        // The next frame moves the document but touches neither the panel nor the hinge: the mesh
        // keeps its revision and the holes are not derived again.
        let second_revision = Revision::ZERO.checked_next().expect("revision");
        cache.start();
        let second = cache
            .panel(
                second_revision,
                &PanelEval::new(&panel, &geometry, placement, &first_drillings, &panel_profile),
                &mut kernel,
                &TessOptions::default(),
            )
            .expect("panel");
        cache.finish();
        assert_eq!(cache.built_at(panel.id()), Some(Revision::ZERO), "an untouched panel is not rebuilt");
        assert_eq!(cache.holes_built_at(panel.id()), Some(Revision::ZERO), "and its holes are not derived again");
        assert_eq!(cache.holes(panel.id()).expect("holes").holes()[0].entry()[0], millimetres(350));
        assert!(Arc::ptr_eq(first.body().expect("body"), second.body().expect("body")));

        // A hinge that moved re-derives the holes by itself: the body is unchanged, so its mesh keeps
        // both the Arc and the revision the first pass gave it.
        let third_revision = second_revision.checked_next().expect("revision");
        let moved_drillings = [cup(400.0)];
        cache.start();
        let third = cache
            .panel(
                third_revision,
                &PanelEval::new(&panel, &geometry, placement, &moved_drillings, &panel_profile),
                &mut kernel,
                &TessOptions::default(),
            )
            .expect("panel");
        cache.finish();
        assert_eq!(cache.holes(panel.id()).expect("holes").holes()[0].entry()[0], millimetres(400));
        assert_eq!(cache.holes_built_at(panel.id()), Some(third_revision), "the moved hinge is re-derived");
        assert_eq!(cache.built_at(panel.id()), Some(Revision::ZERO), "but the body it drills is not rebuilt");
        assert!(Arc::ptr_eq(first.body().expect("body"), third.body().expect("body")));
    }

    #[test]
    fn a_panel_that_no_drilling_object_reaches_has_no_holes() {
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let panel = sample_panel();
        let geometry = panel.geometry_with_fallbacks().expect("geometry");
        cache.start();
        cache.panel(Revision::ZERO, &plain(&panel, &geometry), &mut kernel, &TessOptions::default()).expect("panel");
        cache.finish();
        assert!(cache.holes(panel.id()).expect("a panel has a hole set").is_empty());
    }

    #[test]
    fn a_changed_band_rebuilds_only_that_panel() {
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let mut project = crate::tests::project_with_two_panels();
        let first = Id::new(1).expect("id");
        let second = Id::new(2).expect("id");
        let first_panel = project.panel(first).expect("panel").clone();
        let second_panel = project.panel(second).expect("panel").clone();
        let first_geometry = first_panel.geometry_with_fallbacks().expect("geometry");
        let second_geometry = second_panel.geometry_with_fallbacks().expect("geometry");

        let revision = Revision::ZERO;
        cache.start();
        let before = cache
            .panel(revision, &plain(&first_panel, &first_geometry), &mut kernel, &TessOptions::default())
            .expect("first");
        let untouched = cache
            .panel(revision, &plain(&second_panel, &second_geometry), &mut kernel, &TessOptions::default())
            .expect("second");
        cache.finish();

        project.set_edge_banding(first, band(1, 2, 22)).expect("banding");
        let revision = project.revision();
        let first_panel = project.panel(first).expect("panel").clone();
        let second_panel = project.panel(second).expect("panel").clone();
        let first_geometry = first_panel.geometry_with_fallbacks().expect("geometry");
        let second_geometry = second_panel.geometry_with_fallbacks().expect("geometry");

        cache.start();
        let after = cache
            .panel(revision, &plain(&first_panel, &first_geometry), &mut kernel, &TessOptions::default())
            .expect("first");
        let untouched_after = cache
            .panel(revision, &plain(&second_panel, &second_geometry), &mut kernel, &TessOptions::default())
            .expect("second");
        cache.finish();

        assert!(!Arc::ptr_eq(before.body().expect("body"), after.body().expect("body")));
        assert!(
            Arc::ptr_eq(untouched.body().expect("body"), untouched_after.body().expect("body")),
            "a band on one panel leaves the other panel's mesh alone"
        );
        assert_eq!(cache.built_at(first), Some(revision));
        assert_eq!(cache.built_at(second), Some(Revision::ZERO));
    }

    #[test]
    fn a_changed_bevel_angle_rebuilds_the_panel() {
        // A bevel is a body feature folded after the extrusion, so the cache must compare it like
        // the outline and the banding. Without it the document revision moved but the old mesh was
        // reused, and the card's angle changed nothing on screen.
        use model::Project;

        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let mut project: Project = crate::tests::project_with_two_panels();
        let id = Id::new(1).expect("id");
        let source = EdgeRef::Rect { edge: RectEdge::Bottom.id() };
        let bevel = |millidegrees| EdgeBevel::new(source, Degrees::from_millidegrees(millidegrees)).expect("a bevel");
        project.set_panel_bevel(id, bevel(20_000)).expect("the first bevel");

        let panel = project.panel(id).expect("panel").clone();
        let geometry = panel.geometry_with_fallbacks().expect("geometry");
        cache.start();
        let before = cache
            .panel(project.revision(), &plain(&panel, &geometry), &mut kernel, &TessOptions::default())
            .expect("the first body");
        cache.finish();

        project.set_panel_bevel(id, bevel(40_000)).expect("the second angle");
        let panel = project.panel(id).expect("panel").clone();
        let geometry = panel.geometry_with_fallbacks().expect("geometry");
        cache.start();
        let after = cache
            .panel(project.revision(), &plain(&panel, &geometry), &mut kernel, &TessOptions::default())
            .expect("the second body");
        cache.finish();

        assert!(
            !Arc::ptr_eq(before.body().expect("first"), after.body().expect("second")),
            "the new angle is a new body"
        );
    }

    #[test]
    fn a_deleted_node_releases_its_cached_mesh() {
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let panel = sample_panel();
        let geometry = panel.geometry_with_fallbacks().expect("geometry");

        cache.start();
        cache
            .panel(Revision::ZERO, &plain(&panel, &geometry), &mut kernel, &TessOptions::default())
            .expect("panel mesh");
        cache.finish();
        assert_eq!(cache.len(), 1);

        cache.start();
        cache.finish();
        assert!(cache.is_empty(), "a pass that visits no node drops every entry");
    }

    #[test]
    fn an_object_is_reused_until_its_payload_changes() {
        let cube = testkit::cube_mesh(millimetres(100));
        let object =
            Object::new(Id::new(3).expect("id"), "Cube", Placement::IDENTITY, model::Geometry::Mesh(cube.clone()));
        let mut cache = EvaluationCache::new();
        cache.start();
        let first = evaluated(&mut cache, Revision::ZERO, &object).expect("cube mesh");
        cache.finish();
        assert_eq!(first.body().expect("body").triangle_count(), 12);

        cache.start();
        let second =
            evaluated(&mut cache, Revision::ZERO.checked_next().expect("revision"), &object).expect("cube mesh");
        cache.finish();
        assert!(Arc::ptr_eq(first.body().expect("body"), second.body().expect("body")));

        let scaled = testkit::cube_mesh(millimetres(200));
        let changed = Object::new(object.id(), "Cube", Placement::IDENTITY, model::Geometry::Mesh(scaled));
        cache.start();
        let third =
            evaluated(&mut cache, Revision::ZERO.checked_next().expect("revision"), &changed).expect("cube mesh");
        cache.finish();
        assert!(!Arc::ptr_eq(first.body().expect("body"), third.body().expect("body")));
    }

    /// A scene with a 100 x 100 x 18 slab at the origin and a 50 x 50 x 200 cutter that passes
    /// through it from end to end, both constructive. The cutter overshoots the slab on every side
    /// and stands strictly inside its outline, so the difference has no coincident faces to resolve
    /// and leaves one body with a square hole.
    fn project_with_a_cutter() -> model::Project {
        use model::{Boolean, BooleanOp, BooleanTool, Constructive, Container, Path, Project};

        let slab = Constructive::extrude(
            Path::rectangle([Length::ZERO, Length::ZERO], [millimetres(100), millimetres(100)]).expect("a slab"),
            millimetres(18),
        )
        .expect("a slab");
        let cutter = Constructive::extrude(
            Path::rectangle([Length::ZERO, Length::ZERO], [millimetres(50), millimetres(50)]).expect("a cutter"),
            millimetres(200),
        )
        .expect("a cutter");
        let cutter_at =
            Placement::new(geom::DVec3::new(25.0, 25.0, -10.0), geom::DQuat::IDENTITY).expect("a cutter placement");
        let step = BooleanTool::new("Cutter", cutter.clone(), cutter_at).expect("a tool");
        let root = Container::new(
            Id::new(1).expect("id"),
            "Root",
            Placement::IDENTITY,
            vec![
                Object::new(Id::new(3).expect("id"), "Slab", Placement::IDENTITY, model::Geometry::Constructive(slab))
                    .into(),
                // The cutter the maker kept: its shape is already frozen in the step, so what happens
                // to this node afterwards changes nothing.
                Object::new(Id::new(4).expect("id"), "Cutter", cutter_at, model::Geometry::Constructive(cutter)).into(),
            ],
        );
        let mut project = Project::new(root).expect("a project");
        project.apply_boolean(id(3), Boolean::new(BooleanOp::Difference, step), None).expect("the slab is cut");
        project
    }

    fn id(value: u64) -> Id {
        Id::new(value).expect("id")
    }

    /// The volume a render mesh encloses, through the divergence theorem.
    fn mesh_volume(mesh: &MeshData) -> f64 {
        let point = |index: u32| {
            let p = mesh.positions[index as usize];
            geom::DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
        };
        let mut total = 0.0;
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let (a, b, c) = (point(triangle[0]), point(triangle[1]), point(triangle[2]));
            total += a.dot(b.cross(c)) / 6.0;
        }
        total
    }

    /// Evaluates one node of a project through the cache.
    fn evaluate_bodies(cache: &mut EvaluationCache, project: &model::Project, node: Id) -> NodeGeometry {
        let object = project.object(node).expect("the node is an object");
        let mut kernel = TruckKernel::new();
        cache
            .object_with(project.revision(), node, object.geometry(), &mut kernel, &TessOptions::default())
            .expect("the body draws")
    }

    #[test]
    fn a_boolean_body_draws_the_cut_its_tool_makes() {
        let project = project_with_a_cutter();
        let mut cache = EvaluationCache::new();
        cache.start();
        let geometry = evaluate_bodies(&mut cache, &project, id(3));
        cache.finish();

        // A 50 x 50 square is taken through the slab: 100 x 100 x 18 less 50 x 50 x 18.
        let volume = mesh_volume(geometry.body().expect("a body"));
        assert!((volume - 135_000.0).abs() < 1.0, "volume was {volume}, expected 135000");
    }

    #[test]
    fn a_boolean_body_is_reused_until_its_own_history_changes_and_ignores_the_source() {
        let mut project = project_with_a_cutter();
        let mut cache = EvaluationCache::new();
        cache.start();
        let first = evaluate_bodies(&mut cache, &project, id(3));
        cache.finish();

        // Nothing changed: the entry is reused even though the revision moved.
        project.bump_revision().expect("a revision");
        cache.start();
        let again = evaluate_bodies(&mut cache, &project, id(3));
        cache.finish();
        assert!(Arc::ptr_eq(first.body().expect("body"), again.body().expect("body")));

        // The cutter node moves and is even reshaped: the step froze the shape it cut with, so the
        // body it cut is untouched (ADR 0197).
        let moved = Placement::new(geom::DVec3::new(35.0, 35.0, -10.0), geom::DQuat::IDENTITY).expect("a placement");
        project.set_node_placement(id(4), moved).expect("move the cutter");
        project.set_object_extrusion_height(id(4), millimetres(300)).expect("a taller cutter");
        cache.start();
        let after_source = evaluate_bodies(&mut cache, &project, id(3));
        cache.finish();
        assert!(
            Arc::ptr_eq(first.body().expect("body"), after_source.body().expect("body")),
            "the frozen step does not follow the source"
        );
        let volume = mesh_volume(after_source.body().expect("a body"));
        assert!((volume - 135_000.0).abs() < 1.0, "the cut is unchanged: {volume}");

        // The body's own history is the key: dropping the step re-derives it into a solid slab.
        project.remove_object_boolean(id(3), 1).expect("the step goes");
        cache.start();
        let after_history = evaluate_bodies(&mut cache, &project, id(3));
        cache.finish();
        assert!(!Arc::ptr_eq(first.body().expect("body"), after_history.body().expect("body")));
        let solid = mesh_volume(after_history.body().expect("a body"));
        assert!((solid - 180_000.0).abs() < 1.0, "the slab is whole again: {solid}");
    }

    #[test]
    fn one_cutter_shape_cuts_two_bodies_and_the_source_edits_neither() {
        use model::{Boolean, BooleanOp, BooleanTool, Constructive, Geometry, Path};

        let mut project = project_with_a_cutter();
        let slab = project.object(id(3)).expect("the slab").geometry().clone();
        let Geometry::Constructive(body) = slab else {
            panic!("the slab is constructive");
        };
        let second = project
            .add_object(
                id(1),
                "Second slab",
                Placement::new(geom::DVec3::new(0.0, 0.0, 100.0), geom::DQuat::IDENTITY).expect("a placement"),
                Geometry::Constructive(body),
            )
            .expect("a second slab");
        assert_eq!(
            project.object(second).expect("the copy").geometry(),
            project.object(id(3)).expect("the slab").geometry(),
            "a copied body carries the same frozen step"
        );

        let mut cache = EvaluationCache::new();
        cache.start();
        let first_slab = evaluate_bodies(&mut cache, &project, id(3));
        let second_slab = evaluate_bodies(&mut cache, &project, second);
        cache.finish();
        for body in [&first_slab, &second_slab] {
            let volume = mesh_volume(body.body().expect("a body"));
            assert!((volume - 135_000.0).abs() < 1.0, "both are cut once: {volume}");
        }

        // Editing the source of the frozen step changes neither body; a fresh step would.
        project.set_object_extrusion_height(id(4), millimetres(300)).expect("a taller cutter");
        cache.start();
        let again = evaluate_bodies(&mut cache, &project, id(3));
        cache.finish();
        assert!(Arc::ptr_eq(first_slab.body().expect("body"), again.body().expect("body")));

        // A body made now from the taller source carries the taller shape, so the two results are
        // one decision apart: the step is a snapshot, taken when it was applied.
        let mut fresh = Constructive::extrude(
            Path::rectangle([Length::ZERO, Length::ZERO], [millimetres(100), millimetres(100)]).expect("a ring"),
            millimetres(18),
        )
        .expect("a slab");
        let source = project.object(id(4)).expect("the source").geometry().as_constructive().expect("a body");
        let taller = BooleanTool::new(
            "Taller",
            source.clone(),
            Placement::new(geom::DVec3::new(25.0, 25.0, -10.0), geom::DQuat::IDENTITY).expect("a placement"),
        )
        .expect("a tool");
        fresh.push_boolean(Boolean::new(BooleanOp::Difference, taller)).expect("a step");
        let third = project
            .add_object(
                id(1),
                "Third slab",
                Placement::new(geom::DVec3::new(0.0, 0.0, 200.0), geom::DQuat::IDENTITY).expect("a placement"),
                Geometry::Constructive(fresh),
            )
            .expect("a third slab");
        cache.start();
        let taller_cut = evaluate_bodies(&mut cache, &project, third);
        cache.finish();
        let volume = mesh_volume(taller_cut.body().expect("a body"));
        assert!(
            (volume - 135_000.0).abs() < 1.0,
            "a 300 mm cutter through an 18 mm slab cuts the same square: {volume}"
        );
    }

    /// The box a scene walk reads: measured once with the mesh, kept beside it, and copied on every
    /// cache hit, so a rebuild costs eight corners per drawable instead of one transform per vertex
    /// (X56).
    #[test]
    fn a_cached_mesh_carries_the_box_it_was_measured_with() {
        let cube = testkit::cube_mesh(millimetres(100));
        let object = Object::new(Id::new(3).expect("id"), "Cube", Placement::IDENTITY, model::Geometry::Mesh(cube));
        let mut cache = EvaluationCache::new();
        cache.start();
        let first = evaluated(&mut cache, Revision::ZERO, &object).expect("cube mesh");
        cache.finish();

        let body = first.body().expect("body");
        let expected = body.local_bounds().expect("the cube has vertices");
        let measured = first.body_bounds().expect("the entry keeps the box of its mesh");
        assert_eq!(measured, expected, "the box is the one the mesh itself measures");
        assert!((measured.max() - measured.min() - geom::DVec3::splat(100.0)).length() < 1.0e-6, "a 100 mm cube");

        // A hit copies the entry, so the box arrives without touching the vertices again.
        cache.start();
        let second =
            evaluated(&mut cache, Revision::ZERO.checked_next().expect("revision"), &object).expect("cube mesh");
        cache.finish();
        assert_eq!(second.body_bounds(), first.body_bounds());
        assert!(Arc::ptr_eq(first.body().expect("body"), second.body().expect("body")));
    }

    #[test]
    fn a_body_is_rebuilt_when_the_path_inside_it_changes() {
        let body = Constructive::extrude(notched_contour().into(), millimetres(18)).expect("a body");
        let object = Object::new(
            Id::new(9).expect("id"),
            "Body",
            Placement::IDENTITY,
            model::Geometry::Constructive(body.clone()),
        );
        let mut cache = EvaluationCache::new();
        cache.start();
        let first = evaluated(&mut cache, Revision::ZERO, &object).expect("body mesh");
        cache.finish();

        cache.start();
        let second =
            evaluated(&mut cache, Revision::ZERO.checked_next().expect("revision"), &object).expect("body mesh");
        cache.finish();
        assert!(Arc::ptr_eq(first.body().expect("body"), second.body().expect("body")), "an unchanged path is reused");

        let mut edited = body.clone();
        edited
            .set_path(
                Path::rectangle([millimetres(0), millimetres(0)], [millimetres(200), millimetres(100)])
                    .expect("rectangle"),
            )
            .expect("replace the path");
        let changed = Object::new(object.id(), "Body", Placement::IDENTITY, model::Geometry::Constructive(edited));
        cache.start();
        let third =
            evaluated(&mut cache, Revision::ZERO.checked_next().expect("revision"), &changed).expect("body mesh");
        cache.finish();
        assert!(
            !Arc::ptr_eq(first.body().expect("body"), third.body().expect("body")),
            "the body keys on the path it owns"
        );
    }

    #[test]
    fn a_primitive_is_drawn_from_its_parameters_and_rebuilt_when_they_change() {
        let primitive = Primitive::sphere(millimetres(50), 16, 8).expect("sphere");
        let object = Object::new(
            Id::new(5).expect("id"),
            "Ball",
            Placement::IDENTITY,
            model::Geometry::Primitive(primitive.clone()),
        );
        let mut cache = EvaluationCache::new();
        cache.start();
        let first = evaluated(&mut cache, Revision::ZERO, &object).expect("sphere mesh");
        cache.finish();
        let body = first.body().expect("a solid");
        assert_eq!(body.triangle_count(), 2 * 16 + 16 * 6 * 2, "two pole rings and two triangles per band quad");

        cache.start();
        let second =
            evaluated(&mut cache, Revision::ZERO.checked_next().expect("revision"), &object).expect("sphere mesh");
        cache.finish();
        assert!(
            Arc::ptr_eq(first.body().expect("body"), second.body().expect("body")),
            "an unchanged primitive is reused"
        );

        let smoother = Primitive::sphere(millimetres(50), 32, 16).expect("sphere");
        let changed = Object::new(object.id(), "Ball", Placement::IDENTITY, model::Geometry::Primitive(smoother));
        cache.start();
        let third =
            evaluated(&mut cache, Revision::ZERO.checked_next().expect("revision"), &changed).expect("sphere mesh");
        cache.finish();
        assert!(!Arc::ptr_eq(first.body().expect("body"), third.body().expect("body")), "more sides rebuild the mesh");
    }

    #[test]
    fn a_path_is_drawn_as_a_line_chain_without_a_solid() {
        use model::ContourSegment;

        let path = Path::new(
            vec![
                ContourSegment::line([millimetres(0), millimetres(0)], [millimetres(600), millimetres(0)])
                    .expect("line"),
                ContourSegment::line([millimetres(600), millimetres(0)], [millimetres(600), millimetres(400)])
                    .expect("line"),
            ],
            false,
        )
        .expect("path");
        let object = Object::new(Id::new(4).expect("id"), "Sketch", Placement::IDENTITY, model::Geometry::Path(path));
        let mut cache = EvaluationCache::new();
        cache.start();
        let geometry = evaluated(&mut cache, Revision::ZERO, &object).expect("path chain");
        cache.finish();
        assert!(geometry.body().is_none());
        assert_eq!(geometry.polylines().first().expect("chain").len(), 3);
    }

    #[test]
    fn a_contour_set_is_drawn_as_one_chain_per_ring() {
        let ring = |x: i64| {
            Path::rectangle([millimetres(x), millimetres(0)], [millimetres(x + 100), millimetres(60)]).expect("a ring")
        };
        let set = model::Contours::new(vec![ring(0), ring(200)]).expect("two rings");
        let object = Object::new(Id::new(4).expect("id"), "Union", Placement::IDENTITY, model::Geometry::Contours(set));
        let mut cache = EvaluationCache::new();
        cache.start();
        let geometry = evaluated(&mut cache, Revision::ZERO, &object).expect("the rings draw");
        cache.finish();
        assert!(geometry.body().is_none(), "a set has no solid");
        assert_eq!(geometry.polylines().len(), 2, "one chain per ring");
        assert_eq!(
            geometry.polylines().first().expect("chain").len(),
            5,
            "a closed chain repeats its first point, so a rectangle samples five"
        );

        // The set is a payload like any other: an unchanged one is reused, a changed one re-derives.
        let revision = Revision::ZERO.checked_next().expect("revision");
        cache.start();
        let again = evaluated(&mut cache, revision, &object).expect("the same set");
        cache.finish();
        assert!(Arc::ptr_eq(&geometry.polylines()[0], &again.polylines()[0]), "an unchanged set is reused");
    }

    /// A project whose root holds two panels with distinct ids, for the invalidation tests.
    fn project_with_two_panels() -> model::Project {
        use model::{Container, Project};

        let size = PanelNominalSize::new(millimetres(700), millimetres(500)).expect("valid panel size");
        let first = Panel::new(Id::new(1).expect("id"), "First", size, None, millimetres(18), Placement::IDENTITY)
            .expect("first panel");
        let second = Panel::new(Id::new(2).expect("id"), "Second", size, None, millimetres(18), Placement::IDENTITY)
            .expect("second panel");
        let root = Container::new(
            Id::new(3).expect("id"),
            "Root",
            Placement::IDENTITY,
            vec![model::Node::Panel(first), model::Node::Panel(second)],
        );
        Project::new(root).expect("project")
    }

    #[test]
    fn a_path_closed_by_a_rounded_arc_samples_as_one_chain() {
        // The document accepts a 5 µm miss at a 600 mm arc end (ADR 0072); the preview and the
        // scene must judge the same joint the document did, or the ring is not drawn at all.
        use model::{ContourSegment, Path};
        use units::Degrees;

        let arc = ContourSegment::arc(
            [Length::ZERO, Length::ZERO],
            millimetres(600),
            Degrees::ZERO,
            Degrees::from_millidegrees(60_000),
        )
        .expect("a 60 degree arc of 600 mm");
        let end = arc.end().expect("a derived end");
        let start = [Length::from_micrometres(end[0].micrometres() + 5), end[1]];
        let path = Path::new(
            vec![
                arc,
                ContourSegment::line(start, [Length::ZERO, Length::ZERO]).expect("the chord"),
                ContourSegment::line([Length::ZERO, Length::ZERO], [millimetres(600), Length::ZERO]).expect("the base"),
            ],
            true,
        )
        .expect("the rounded arc end closes the ring");
        let points = path_points(&path).expect("one chain despite the rounded end");
        assert!(points.len() > 3, "the arc is sampled into the chain");
    }

    #[test]
    fn the_cache_reports_an_empty_state() {
        let cache = EvaluationCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.built_at(Id::new(1).expect("id")), None);
        assert!(cache.cached(Id::new(1).expect("id")).is_none());
    }
}
