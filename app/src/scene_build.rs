//! The document the editor opens with, and evaluation of the container tree into a render scene.
//!
//! This is composition-root code: it owns the geometry adapter, evaluates panels through the
//! parametric layer and composes placements into world transforms. It does not mutate the
//! document; editing commands arrive in a later milestone.

use std::sync::Arc;

use engine::Session;
use geom::{Aabb, DMat4, DVec2, DVec3};
use kernel::TessOptions;
use kernel_truck::TruckKernel;
use mesh::{EvaluationCache, PanelEval};
use model::{
    CameraProjection, Container, Envelope, Id, LightKind, Node, NodeQuery, Panel, Placement, Project, StructuralKind,
};
// The demonstration fixture is the only caller of these: a test build evaluates the same document
// a maker builds, and nothing else in the shell creates cutouts, bands or a named panel size.
#[cfg(test)]
use geom::{DQuat, MeshData};
#[cfg(test)]
use model::ItemId;
#[cfg(test)]
use model::{CutoutDepth, CutoutShape, EdgeBanding, EdgeId, FaceSide, PanelNominalSize, PanelRect};
use scene::{
    Camera, DEFAULT_FOV_Y, Drawable, Lighting, MAX_POINT_LIGHTS, PointLight, Polyline, ProductEnvelope,
    ProductEnvelopeState, Projection, SceneFrame,
};
#[cfg(test)]
use std::f64::consts::FRAC_PI_2;
use ui::ViewFlags;
#[cfg(test)]
use units::Length;

/// A panel's surface colour, taken in tree order, in linear RGB: v1 has no per-panel material
/// colour and the three tones keep neighbouring parts readable.
const PANEL_COLORS: [[f32; 4]; 3] = [[0.62, 0.42, 0.22, 1.0], [0.50, 0.34, 0.20, 1.0], [0.45, 0.47, 0.50, 1.0]];

/// A common debug colour for every band on a panel.
const EDGE_BAND_COLOR: [f32; 4] = [0.18, 0.20, 0.24, 1.0];

/// The default surface colour of a geometry object; v1 has no per-object material.
const OBJECT_COLOR: [f32; 4] = [0.58, 0.60, 0.64, 1.0];

/// The demonstration fixture's item ids: the starter package's own rows.
///
/// A scene is not the directory and never writes it, so these numbers label one fixture.
#[cfg(test)]
const DEFAULT_OAK_MATERIAL: u32 = 1;
#[cfg(test)]
const DEFAULT_WHITE_MATERIAL: u32 = 2;
#[cfg(test)]
const DEFAULT_OAK_EDGE: u32 = 101;
#[cfg(test)]
const DEFAULT_WHITE_EDGE: u32 = 102;
/// The starter package's 4 mm HDF sheet: what an applied or a grooved back is cut from.
#[cfg(test)]
const DEFAULT_HDF_BACK: u32 = 3;

/// A world-space axis-aligned box built from evaluated meshes.
///
/// The value grows one drawable at a time; the box of a drawable arrives as its cached local
/// [`Aabb`] and eight corner transforms, so no rebuild reads a vertex (X56).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    /// `None` until a finite point is included.
    extent: Option<Aabb>,
}

impl Bounds {
    fn empty() -> Self {
        Self { extent: None }
    }

    /// Includes one drawable's local box turned into the world.
    ///
    /// This is the one path a scene walk takes: the box is the mesh's own, measured when the mesh
    /// was built, so the walk costs eight corners however many vertices the mesh has.
    fn include_aabb(&mut self, local: Aabb, transform: DMat4) {
        self.include(local.transformed_by(transform));
    }

    /// Includes every vertex of a mesh, transformed one by one.
    ///
    /// The reference implementation [`Bounds::include_aabb`] is checked against: the tests compose
    /// the per-vertex box of a frame and compare it with the evaluated one. Nothing in the scene
    /// walk may call it — the probe below fails when a rebuild reaches a vertex.
    #[cfg(test)]
    fn include_mesh(&mut self, mesh: &MeshData, transform: DMat4) {
        for position in &mesh.positions {
            VERTICES_TRANSFORMED.with(|count| count.set(count.get().saturating_add(1)));
            self.include_point(transform.transform_point3(DVec3::new(
                f64::from(position[0]),
                f64::from(position[1]),
                f64::from(position[2]),
            )));
        }
    }

    fn include_point(&mut self, point: DVec3) {
        if let Some(point_box) = Aabb::of_points(std::iter::once(point)) {
            self.include(point_box);
        }
    }

    fn include(&mut self, other: Aabb) {
        self.extent = Some(match self.extent {
            Some(extent) => extent.union(other),
            None => other,
        });
    }

    fn include_bounds(&mut self, other: Self) {
        if let Some(extent) = other.extent {
            self.include(extent);
        }
    }

    /// Whether the box holds no finite point.
    fn is_empty(self) -> bool {
        self.extent.is_none()
    }

    /// The minimum corner of the box, or `None` when no finite vertex was included.
    ///
    /// No production path measures a corner any more: the ground shadow is cast by the light from
    /// the geometry itself and needs no bound, so only a test reads the corners of an evaluation.
    #[cfg(test)]
    pub fn min(self) -> Option<DVec3> {
        self.extent.map(Aabb::min)
    }

    /// The maximum corner of the box, or `None` when no finite vertex was included.
    #[cfg(test)]
    pub fn max(self) -> Option<DVec3> {
        self.extent.map(Aabb::max)
    }

    /// The centre of the box, or `None` when no finite vertex was included.
    pub fn center(self) -> Option<DVec3> {
        self.extent.map(Aabb::center)
    }

    /// The radius of the sphere around the centre that contains the box.
    pub fn radius(self) -> Option<f64> {
        self.extent.map(Aabb::radius)
    }

    /// The eight corners of the box in world space, or `None` when it holds no finite vertex.
    ///
    /// The bit order is [`Aabb::corners`]: bit 0 of the index picks the `x` maximum, bit 1 the `y`
    /// and bit 2 the `z`, so the two corners that differ in one bit share one edge of the box —
    /// which is what a bracket needs to draw its three arms along.
    pub fn corners(self) -> Option<[DVec3; 8]> {
        self.extent.map(Aabb::corners)
    }
}

// How many mesh vertices the per-vertex world box has transformed on this thread in a test build.
//
// A rebuild must leave this at zero: a drawable's world box comes from the corners of the box the
// mesh cache measured, not from its vertices. A session that puts the vertex loop back into the
// walk fails the probe even on a machine fast enough to hide it in the timing (X56).
#[cfg(test)]
thread_local! {
    static VERTICES_TRANSFORMED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The world box around one container's whole subtree, as its eight corners.
///
/// The shell keeps one per container from the last rebuild and paints the brackets of the levels
/// that are open now, so opening or closing a level costs no evaluation at all: openness is view
/// state, never a document revision (ADR 0098).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelBounds {
    /// The container whose subtree the box surrounds.
    pub id: Id,
    /// Everything the subtree draws — a hidden child excluded, as the frame excludes it — or `None`
    /// when it draws nothing at all.
    pub corners: Option<[DVec3; 8]>,
}

/// The evaluated frame plus the bounds used to frame the initial camera.
pub struct EvaluatedScene {
    /// Drawables and default scene settings in local-to-world space.
    pub frame: SceneFrame,
    /// Bounds of the evaluated meshes.
    pub bounds: Bounds,
    /// One box per container, in tree order: what the shell draws the bracket of an open level on.
    pub levels: Vec<LevelBounds>,
    /// The nodes this pass could not build, in tree order, each with its sentence.
    ///
    /// A node whose geometry the kernel refuses — a boolean that leaves no solid, a contour it
    /// cannot tessellate — is a diagnostic on that node, never a reason to lose the picture: the
    /// rest of the scene still draws and still follows the pointer (`geometry.md` §8).
    pub failures: Vec<SceneFailure>,
}

/// One node the evaluation could not build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneFailure {
    /// The node whose geometry failed.
    pub id: Id,
    /// The sentence the status bar prints, already naming the node.
    pub message: String,
}

/// Builds the demonstration project the interface tests draw.
///
/// Three panels exercise face shading, through and blind cutouts, and both container kinds: an
/// assembly is a furniture component, a group is an organisational container. It is a test fixture
/// and nothing else: the editor opens [`initial_session`], which carries no object at all, because a
/// sample piece is in the way of the first real one.
#[cfg(test)]
pub fn demo_session() -> Result<Session, String> {
    let root_id = id(1)?;
    let root = Container::new(root_id, "Demo", Placement::IDENTITY, Vec::new());
    let mut project = Project::new(root).map_err(describe)?;
    let oak = material_id(DEFAULT_OAK_MATERIAL)?;
    let white = material_id(DEFAULT_WHITE_MATERIAL)?;
    let oak_edge = material_id(DEFAULT_OAK_EDGE)?;
    let white_edge = material_id(DEFAULT_WHITE_EDGE)?;

    let standing = Placement::new(DVec3::ZERO, DQuat::from_rotation_x(FRAC_PI_2)).map_err(describe)?;
    let thickness = millimetres(18)?;
    let back = project
        .add_panel(root_id, "Back panel", panel_nominal_size(900, 600)?, Some(oak), thickness, standing)
        .map_err(describe)?;
    project.add_cutout(back, cutout(60, 70, 180, 140)?, CutoutDepth::Through, FaceSide::A).map_err(describe)?;
    project
        .add_cutout(back, cutout(330, 70, 180, 140)?, CutoutDepth::Blind { depth: millimetres(12)? }, FaceSide::A)
        .map_err(describe)?;
    add_band(&mut project, back, 1, true, oak_edge)?;
    add_band(&mut project, back, 2, false, oak_edge)?;

    let floor = Placement::new(DVec3::new(150.0, -500.0, 0.0), DQuat::IDENTITY).map_err(describe)?;
    let shelf = project
        .add_panel(root_id, "Loose shelf", panel_nominal_size(600, 400)?, Some(white), thickness, floor)
        .map_err(describe)?;
    project.add_cutout(shelf, cutout(80, 60, 150, 100)?, CutoutDepth::Through, FaceSide::B).map_err(describe)?;
    add_band(&mut project, shelf, 4, true, white_edge)?;

    project.group_nodes(&[back, shelf], "Loose parts").map_err(describe)?;

    let drawer = project.add_component(root_id, "Drawer", Placement::IDENTITY).map_err(describe)?;
    let front_offset = Placement::new(DVec3::new(0.0, 120.0, 40.0), DQuat::IDENTITY).map_err(describe)?;
    project
        .add_panel(drawer, "Drawer front", panel_nominal_size(500, 180)?, Some(white), thickness, front_offset)
        .map_err(describe)?;

    project.validate().map_err(describe)?;
    Ok(Session::with_directory(project, default_catalog()?).with_contact(contact_engine()))
}

/// Builds an empty scene around the default catalogue: a root container with no children.
///
/// The window opens its first tab through `App::compose` and a new tab through the
/// snapshot the workspace is on, so this is the fixture path — a test or a shot that wants an empty
/// document and the starter rows without a workspace around it. The catalogue is not scene data and
/// is never written to the file, so the rows only decide what the inspector can offer.
#[cfg(test)]
pub fn empty_session() -> Result<Session, String> {
    empty_session_with(default_catalog()?)
}

/// Builds an empty scene around a directory the window already holds.
///
/// A new tab takes the snapshot the workspace is on instead of reading the folder again: the folder
/// is watched off the frame path, and a second read here would give the tab rows the window has
/// already replaced.
pub fn empty_session_with(directory: impl Into<std::sync::Arc<catalog::Directory>>) -> Result<Session, String> {
    let root = Container::new(id(1)?, "Scene", Placement::IDENTITY, Vec::new());
    let project = Project::new(root).map_err(describe)?;
    Ok(Session::with_directory(project, directory).with_contact(contact_engine()))
}

/// The contact adapter every session of this shell measures a body with.
///
/// One adapter for the window: the drag's rule and the write's guard ask the same one, so a release
/// cannot refuse a place the preview showed (ADR 0166, ADR 0179). It is a value, not a service: the
/// engine holds it behind an `Arc` and no state changes under it.
pub fn contact_engine() -> Arc<dyn geom::Contact> {
    Arc::new(contact_parry::Parry)
}

/// Builds the directory a new session starts from: the workshop's packages, or the starter one.
///
/// The shell reads `nomenclature/` beside `settings.json`, so a workshop's directory replaces the
/// starter rows without a rebuild, and a failure is reported rather than swallowed: the editor keeps
/// working on the starter package and says which file was refused (`docs/domain/nomenclature.md`
/// §13). A caller that watches the folder — the window — reads it through `directory_watch`
/// instead and keeps the refusal for the status bar; this one is the command line's and the
/// fixtures' way in.
pub fn default_catalog() -> Result<catalog::Directory, String> {
    // A test never reads the machine's folder: one `nomenclature/` beside `settings.json` would change
    // what a fixture builds, so the starter rows are the only directory a test opens.
    if cfg!(test) {
        return catalog::default_directory().map_err(describe);
    }
    let root = crate::settings_store::config_dir().map(|folder| folder.join(crate::directory_watch::DIRECTORY_DIR));
    let start = crate::directory_watch::open(root.as_deref());
    if let Some(refusal) = &start.refusal {
        log::warn!("the workshop directory could not be read; the starter rows are in use: {refusal}");
    }
    Ok(start.directory)
}

/// Evaluates a constructive object into world-space drawables and product envelopes.
///
/// The cache is the only evaluator: a node whose derivation inputs did not change is reused, so a
/// move, a rename or an unrelated edit rebuilds no mesh.
///
/// `open_product` is the session's one open product (ADR 0098), which only decides the ink of its
/// envelope and whether the frame shows while its parts are closed (ADR 0100). The shell passes
/// `Session::active_product`; it is view state, never document data.
pub fn evaluate(
    cache: &mut EvaluationCache,
    kernel: &mut TruckKernel,
    session: &Session,
    flags: &ViewFlags,
    open_product: Option<Id>,
) -> Result<EvaluatedScene, String> {
    let project = session.project();
    let root_world = Placement::compose(Placement::IDENTITY, project.root().placement()).map_err(describe)?;
    cache.start();
    let mut evaluation = Evaluation::new(cache, kernel, session, flags, open_product);
    let mut scene_bounds = Bounds::empty();
    for child in project.root().children() {
        scene_bounds.include_bounds(evaluation.collect(std::slice::from_ref(child), root_world));
    }
    let Evaluation { cache, frame, levels, failures, .. } = evaluation;
    cache.finish();
    let mut frame = frame;
    if let Some(lighting) = authored_lighting_with(project, &[], flags) {
        frame.lighting = lighting;
    }
    Ok(EvaluatedScene { frame, bounds: scene_bounds, levels, failures })
}

/// The frame lighting from the authored lights.
///
/// The first sun becomes the key light: it emits along its local `-Z`, so the direction from a
/// surface towards it is the node's world rotation applied to `+Z`, and its colour is multiplied by
/// the intensity. Every point light contributes its world position and power, up to
/// [`MAX_POINT_LIGHTS`] in tree order. A scene with no authored light keeps the built-in key light;
/// so does a scene with point lights but no sun, because the point lights only add to the key.
#[cfg(test)]
pub fn authored_lighting(project: &Project) -> Option<Lighting> {
    authored_lighting_with(project, &[], &ViewFlags::default())
}

/// The frame lighting with previewed world placements for moved light nodes.
///
/// A gizmo drag never mutates the document, so the frame it renders must take the same previewed
/// placements the drawables use. An entry in `previewed` wins over the document's world placement;
/// it lets a sun direction and a point light position follow the pointer before the command that
/// commits the drag.
pub fn authored_lighting_with(project: &Project, previewed: &[(Id, Placement)], flags: &ViewFlags) -> Option<Lighting> {
    let mut lighting = Lighting::default();
    let mut any_authored = false;
    let mut key_set = false;
    for id in project.query(&NodeQuery::any().with_kind(StructuralKind::Light)) {
        let Some(light) = project.light(id) else {
            continue;
        };
        // A hidden light contributes nothing, exactly like a hidden body: the tree says it is off.
        if flags.hides(project, id) {
            continue;
        }
        let Some(world) = previewed_world(project, id, previewed) else {
            continue;
        };
        match light.kind() {
            LightKind::Sun if !key_set => {
                let Some(direction) = (world.rotation() * DVec3::Z).try_normalize() else {
                    continue;
                };
                let intensity = light.intensity();
                let color = light.color();
                lighting.key_direction = direction;
                lighting.key_color = [color[0] * intensity, color[1] * intensity, color[2] * intensity];
                key_set = true;
                any_authored = true;
            }
            LightKind::Sun => {}
            LightKind::Point => {
                if lighting.points.len() < MAX_POINT_LIGHTS {
                    lighting.points.push(PointLight {
                        position: world.translation(),
                        color: light.color(),
                        power: light.intensity(),
                    });
                }
                any_authored = true;
            }
        }
    }
    any_authored.then_some(lighting)
}

/// The tolerance a footprint's arcs are flattened with, in millimetres.
///
/// A quarter of a millimetre is far finer than a dashed wireframe can show and far coarser than the
/// half-micrometre a panel profile samples with, so a rounded corner costs a handful of points rather
/// than a curve drawn as code.
const FOOTPRINT_SAMPLE_MM: f64 = 0.25;

/// The base outline of a product envelope and the points where its vertical edges stand.
///
/// The footprint draws itself through the panel's own outline generator (ADR 0159), so a frame and a
/// board cut to the same profile agree to the micrometre. The ring is flattened here because the scene
/// carries points, not curves; a smooth joint — a fillet's tangency — draws no vertical edge, so only
/// the shape's real corners are named.
fn envelope_outline(envelope: &Envelope) -> Result<(Vec<DVec2>, Vec<DVec2>), String> {
    let ring = envelope.footprint().outline(envelope.x(), envelope.y()).map_err(describe)?;
    let points = ring.sample(FOOTPRINT_SAMPLE_MM).map_err(describe)?;
    let mut corners = Vec::new();
    for (index, segment) in ring.segments().iter().enumerate() {
        if ring.joint_is_smooth(index, geom::DEFAULT_TANGENT_TOLERANCE) == Some(true) {
            continue;
        }
        corners.push(segment.start());
    }
    Ok((points, corners))
}

/// A node's world placement, preferring a previewed one during a drag.
pub fn previewed_world(project: &Project, id: Id, previewed: &[(Id, Placement)]) -> Option<Placement> {
    previewed
        .iter()
        .find(|(node, _)| *node == id)
        .map(|(_, placement)| *placement)
        .or_else(|| project.world_placement(id))
}

/// Adds one solid drawable to a subtree's bounds from the local box the mesh cache measured.
///
/// The box is the eight corners of the cached local box turned once, so this costs nothing per
/// vertex (X56). A frame's own extent is what *Frame all* reads and what a view culls by; the
/// ground shadow is cast by the light from the geometry itself and needs no box (X62).
fn include_body(bounds: &mut Bounds, local: Option<Aabb>, transform: DMat4) {
    if let Some(local) = local {
        bounds.include_aabb(local, transform);
    }
}

/// Mutable state shared by one scene evaluation.
struct Evaluation<'a> {
    cache: &'a mut EvaluationCache,
    kernel: &'a mut TruckKernel,
    session: &'a Session,
    /// The session's hidden set: a hidden node and its whole subtree are skipped.
    flags: &'a ViewFlags,
    /// The one open product, whose envelope takes the active ink, or `None` while none is open.
    open_product: Option<Id>,
    options: TessOptions,
    frame: SceneFrame,
    /// The world box of every container met, so the shell can bracket the open ones without a
    /// second walk of the tree.
    levels: Vec<LevelBounds>,
    /// The nodes this pass could not build, with the sentence each one reports.
    failures: Vec<SceneFailure>,
    palette_index: usize,
}

impl<'a> Evaluation<'a> {
    fn new(
        cache: &'a mut EvaluationCache,
        kernel: &'a mut TruckKernel,
        session: &'a Session,
        flags: &'a ViewFlags,
        open_product: Option<Id>,
    ) -> Self {
        Self {
            cache,
            kernel,
            session,
            flags,
            open_product,
            options: TessOptions::default(),
            frame: SceneFrame::default(),
            levels: Vec::new(),
            failures: Vec::new(),
            palette_index: 0,
        }
    }

    fn collect(&mut self, children: &[Node], parent: Placement) -> Bounds {
        let mut bounds = Bounds::empty();
        for child in children {
            // A hidden node takes its whole subtree with it, so the frame, its bounds, its
            // contact shadows and picking all forget the branch in one place.
            if self.flags.is_hidden(child.id()) {
                continue;
            }
            match self.collect_child(child, parent) {
                Ok(child_bounds) => bounds.include_bounds(child_bounds),
                // One node the geometry kernel refuses is a diagnostic on that node: the rest of
                // the scene draws, so a person can see and fix the part that failed instead of a
                // picture frozen at the last frame that happened to build (`geometry.md` §8).
                Err(message) => self.failures.push(SceneFailure { id: child.id(), message }),
            }
        }
        bounds
    }

    /// One child of the tree, or the sentence saying why it could not be built.
    fn collect_child(&mut self, child: &Node, parent: Placement) -> Result<Bounds, String> {
        Ok(match child {
            Node::Container(container) => {
                let world = Placement::compose(parent, container.placement()).map_err(describe)?;
                let subtree = self.collect(container.children(), world);
                let subtree = match container.envelope() {
                    Some(envelope) => self.collect_product(container.id(), envelope, world, subtree)?,
                    None => subtree,
                };
                // Recorded for every container, open or not: which of them are open is view
                // state that moves without a rebuild, so the filtering happens at paint time.
                self.levels.push(LevelBounds { id: container.id(), corners: subtree.corners() });
                subtree
            }
            Node::Panel(panel) => self.collect_panel(panel, parent)?,
            Node::Object(object) => self.collect_object(object, parent)?,
            Node::Camera(_) | Node::Light(_) => Bounds::empty(),
        })
    }

    /// Records one product's authored box as a scene item and returns the subtree bounds with it.
    ///
    /// The three states of ADR 0100 are decided here and nowhere else: the open product is the one
    /// the session names, a closed product whose subtree drew something keeps its lines hidden, and
    /// a closed product with nothing in it still shows its box. The envelope joins the drawn bounds
    /// so *Frame all* sees an empty piece; it never joins the solid bounds, because a reserved box is
    /// not a body and casts no contact shadow.
    fn collect_product(
        &mut self,
        id: Id,
        envelope: &Envelope,
        world: Placement,
        mut bounds: Bounds,
    ) -> Result<Bounds, String> {
        let state = if self.open_product == Some(id) {
            ProductEnvelopeState::Open
        } else if !bounds.is_empty() {
            ProductEnvelopeState::ClosedWithParts
        } else {
            ProductEnvelopeState::ClosedEmpty
        };
        let (outline, corners) = envelope_outline(envelope)?;
        let height = envelope.z().millimetres_f64();
        let transform = DMat4::from_rotation_translation(world.rotation(), world.translation());
        for point in &outline {
            for z in [0.0, height] {
                bounds.include_point(transform.transform_point3(DVec3::new(point.x, point.y, z)));
            }
        }
        self.frame.push_product_envelope(ProductEnvelope {
            id,
            outline: outline.into(),
            corners: corners.into(),
            height,
            world_transform: transform,
            state,
        });
        Ok(bounds)
    }

    fn collect_panel(&mut self, panel: &Panel, parent: Placement) -> Result<Bounds, String> {
        let world = Placement::compose(parent, panel.placement()).map_err(describe)?;
        let session = self.session;
        let geometry = session.panel_geometry(panel.id()).map_err(describe)?;
        let revision = session.project().revision();
        // The drilling objects that reach a panel come from the hardware instances that drill it;
        // until the instance node lands, a panel is drawn and reported undrilled (task O13).
        let band_profile = |spec: &model::BandingSpec| session.band_spec_profile(spec);
        let request = PanelEval::new(panel, &geometry, world, &[], &band_profile);
        let evaluated = self.cache.panel(revision, &request, self.kernel, &self.options).map_err(describe)?;
        let transform = DMat4::from_rotation_translation(world.rotation(), world.translation());
        let color = PANEL_COLORS[self.palette_index % PANEL_COLORS.len()];
        self.palette_index = self.palette_index.saturating_add(1);
        let mut bounds = Bounds::empty();
        if let Some(body) = evaluated.body() {
            include_body(&mut bounds, evaluated.body_bounds(), transform);
            let drawable = Drawable::with_box(panel.id(), Arc::clone(body), transform, color, evaluated.body_bounds());
            self.frame.push(with_cached_edges(drawable, evaluated.body_edges(), evaluated.body_all_edges()));
        }
        for (index, mesh) in evaluated.bands().iter().enumerate() {
            let band_bounds = evaluated.band_bounds().get(index).copied().flatten();
            include_body(&mut bounds, band_bounds, transform);
            let drawable = Drawable::with_box(panel.id(), Arc::clone(mesh), transform, EDGE_BAND_COLOR, band_bounds);
            self.frame.push(with_cached_edges(
                drawable,
                evaluated.band_edges().get(index),
                evaluated.band_all_edges().get(index),
            ));
        }
        Ok(bounds)
    }

    /// Turns a geometry object into one flat-shaded drawable or one line chain in world space.
    ///
    /// A mesh payload is drawn as stored. A constructive or primitive payload is evaluated through
    /// the cache's one evaluator — the same function `object.collapse` bakes — so the collapsed
    /// object keeps the shape the viewport showed. A body with a boolean step is built by the
    /// kernel from the tool bodies the history names, resolved in the body's own frame.
    fn collect_object(&mut self, object: &model::Object, parent: Placement) -> Result<Bounds, String> {
        let world = Placement::compose(parent, object.placement()).map_err(describe)?;
        let revision = self.session.project().revision();
        // A previewing operation hands the viewport the geometry the node would have, so the frame
        // draws the result the command bar is about to commit instead of the stored one.
        let geometry =
            self.session.drawn_geometry(object.id()).ok_or_else(|| format!("{}: no geometry", object.name()))?;
        // The tools come from the history that is about to be drawn, not from the stored payload: a
        // preview that adds a step names a tool the document does not hold yet.
        let tools = match geometry {
            model::Geometry::Constructive(body) => {
                self.session.project().boolean_tools_of(object.id(), body).map_err(describe)?
            }
            _ => Vec::new(),
        };
        let evaluated = self
            .cache
            .object_with(revision, object.id(), geometry, &tools, self.kernel, &self.options)
            .map_err(|error| format!("{}: {error}", object.name()))?;
        let transform = DMat4::from_rotation_translation(world.rotation(), world.translation());
        let mut bounds = Bounds::empty();
        // A drawing has no solid: it is drawn as unlit chains of line segments, one chain per ring,
        // and their points are sampled from the same analytic segments a later extrusion uses. A
        // chain leaves its colour to the frame, so a theme switch repaints it without a rebuild.
        if !evaluated.polylines().is_empty() {
            for points in evaluated.polylines() {
                for point in points.iter() {
                    bounds.include_point(transform.transform_point3(*point));
                }
                self.frame.push_polyline(Polyline::in_ink(object.id(), Arc::clone(points), transform));
            }
            return Ok(bounds);
        }
        if let Some(mesh) = evaluated.body() {
            include_body(&mut bounds, evaluated.body_bounds(), transform);
            let drawable =
                Drawable::with_box(object.id(), Arc::clone(mesh), transform, OBJECT_COLOR, evaluated.body_bounds());
            self.frame.push(with_cached_edges(drawable, evaluated.body_edges(), evaluated.body_all_edges()));
            return Ok(bounds);
        }
        Err(format!("{}: the object has no drawable geometry", object.name()))
    }
}

/// Attaches the cached form edges and the full triangulation edges to a drawable.
///
/// The two lists come from the same cached mesh, so this copies two `Arc`s and never searches the
/// mesh. The renderer picks one from the frame's style; the other is shared with the cache.
fn with_cached_edges(drawable: Drawable, edges: Option<&Arc<[u32]>>, all_edges: Option<&Arc<[u32]>>) -> Drawable {
    let drawable = match edges {
        Some(edges) => drawable.with_edges(Arc::clone(edges)),
        None => drawable,
    };
    match all_edges {
        Some(all_edges) => drawable.with_all_edges(Arc::clone(all_edges)),
        None => drawable,
    }
}

/// Places the initial camera so the whole scene fits in the viewport.
pub fn home_camera(bounds: Bounds) -> Result<Camera, String> {
    let Some(center) = bounds.center() else {
        return Ok(Camera::default());
    };
    let radius = bounds.radius().unwrap_or(0.0).max(50.0);
    let half_fov = DEFAULT_FOV_Y * 0.5;
    let distance = radius / half_fov.tan() * 1.25;
    Camera::new(center, distance).map_err(describe)
}

/// Distance from an authored camera's eye to its orbit target, used as a pivot placeholder.
const AUTHORED_CAMERA_DISTANCE_MM: f64 = 1_000.0;

/// The authored camera the scene renders from, or `None` for the built-in camera.
///
/// A camera node looks down its local `-Z` with `+Y` up. The projection is copied from the
/// document; near and far stay at the scene defaults because the model does not store them yet.
/// An authored camera pins an explicit basis, so viewport navigation leaves it alone.
pub fn authored_camera(project: &Project) -> Option<Camera> {
    let id = project.active_camera()?;
    let world = project.world_placement(id)?;
    let projection = match project.camera(id)?.projection() {
        CameraProjection::Perspective { fov_y } => Projection::Perspective { fov_y: fov_y.radians_f64() },
        CameraProjection::Orthographic { height } => Projection::Orthographic { height: height.millimetres_f64() },
    };
    let eye = world.translation();
    let rotation = world.rotation();
    let right = rotation * DVec3::X;
    let up = rotation * DVec3::Y;
    let forward = rotation * -DVec3::Z;
    let target = eye + forward * AUTHORED_CAMERA_DISTANCE_MM;
    Camera::new(target, AUTHORED_CAMERA_DISTANCE_MM)
        .ok()
        .map(|camera| camera.looked_from(eye, right, up, forward).with_projection(projection))
}

/// The camera a viewport draws with: the active authored camera, or the navigable view fallback.
pub fn scene_camera(project: &Project, view: Camera) -> Camera {
    authored_camera(project).unwrap_or(view)
}

fn id(value: u64) -> Result<Id, String> {
    Id::new(value).ok_or_else(|| "ids start at one".to_owned())
}

#[cfg(test)]
fn material_id(value: u32) -> Result<ItemId, String> {
    ItemId::new(1, value).ok_or_else(|| "material ids start at one".to_owned())
}

#[cfg(test)]
fn millimetres(value: i64) -> Result<Length, String> {
    Length::from_millimetres(value).ok_or_else(|| format!("{value} mm does not fit a length"))
}

#[cfg(test)]
fn add_band(
    project: &mut Project,
    panel: Id,
    edge: u32,
    included_in_size: bool,
    material: ItemId,
) -> Result<(), String> {
    let edge = EdgeId::new(edge).ok_or_else(|| "edge ids start at one".to_owned())?;
    let band =
        EdgeBanding::new(edge, material, included_in_size, millimetres(2)?, millimetres(22)?).map_err(describe)?;
    project.set_edge_banding(panel, band).map_err(describe)?;
    Ok(())
}

#[cfg(test)]
fn panel_nominal_size(length: i64, width: i64) -> Result<PanelNominalSize, String> {
    PanelNominalSize::new(millimetres(length)?, millimetres(width)?).map_err(describe)
}

#[cfg(test)]
fn rectangle(x: i64, y: i64, width: i64, height: i64) -> Result<PanelRect, String> {
    PanelRect::new(millimetres(x)?, millimetres(y)?, millimetres(width)?, millimetres(height)?).map_err(describe)
}

#[cfg(test)]
fn cutout(x: i64, y: i64, width: i64, height: i64) -> Result<CutoutShape, String> {
    Ok(CutoutShape::Rectangle(rectangle(x, y, width, height)?))
}

fn describe(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use model::{ClassCode, ContourSegment, Geometry, Path, Revision};
    use testkit::sample_project;

    fn panels_in<'a>(children: &'a [Node], panels: &mut Vec<&'a Panel>) {
        for child in children {
            match child {
                Node::Panel(panel) => panels.push(panel),
                Node::Container(container) => panels_in(container.children(), panels),
                Node::Object(_) | Node::Camera(_) | Node::Light(_) => {}
            }
        }
    }

    #[test]
    fn a_back_panel_resolves_its_thickness_from_the_catalogue_row_it_names() {
        // The two back constructions are two rows and not two numbers in code: an applied or grooved
        // back is cut from the 4 mm sheet, an inset one from the 18 mm carcass board, and both panels
        // store the same fallback — so the resolved planes come from the catalogue alone.
        let directory = catalog::default_directory().expect("the starter package");
        let root = Container::new(id(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("project");
        let root = project.root().id();
        let envelope = Envelope::new(
            millimetres(600).expect("width"),
            millimetres(560).expect("depth"),
            millimetres(720).expect("height"),
        )
        .expect("an envelope");
        let product = project
            .add_product(root, "Case", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a product");
        let size = panel_nominal_size(600, 400).expect("a back");
        let fallback = millimetres(18).expect("18 mm");
        let row = |serial: u32| material_id(serial).expect("an item id");
        let thin = project
            .add_panel(product, "Back (applied)", size, Some(row(DEFAULT_HDF_BACK)), fallback, Placement::IDENTITY)
            .expect("the thin back");
        let inset = project
            .add_panel(product, "Back (inset)", size, Some(row(DEFAULT_OAK_MATERIAL)), fallback, Placement::IDENTITY)
            .expect("the inset back");

        let session = Session::with_directory(project, directory);
        let resolved = |panel| session.panel_geometry(panel).expect("resolved geometry").thickness();
        assert_eq!(resolved(thin), millimetres(4).expect("4 mm"), "the 4 mm sheet answers the applied back");
        assert_eq!(resolved(inset), millimetres(18).expect("18 mm"), "the inset back keeps the carcass board");
    }

    #[test]
    fn an_authored_camera_maps_its_placement_and_projection() {
        let root = Container::new(id(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("project");
        let root = project.root().id();
        let rotation = DQuat::from_rotation_z(FRAC_PI_2);
        let placement = Placement::new(DVec3::new(100.0, 200.0, 300.0), rotation).expect("placement");
        let fov = units::Degrees::from_millidegrees(60_000);
        let camera_id = project
            .add_camera(root, "Presentation", placement, CameraProjection::Perspective { fov_y: fov })
            .expect("camera");
        assert!(authored_camera(&project).is_none(), "the built-in camera is used while none is active");
        let view = Camera::default();
        assert_eq!(scene_camera(&project, view), view);

        project.set_active_camera(Some(camera_id)).expect("active camera");
        let camera = authored_camera(&project).expect("the authored camera");
        assert!((camera.eye() - DVec3::new(100.0, 200.0, 300.0)).length() < 1.0e-9);
        assert!(camera.forward().dot(rotation * -DVec3::Z) > 1.0 - 1.0e-9);
        assert!(camera.right().dot(rotation * DVec3::X) > 1.0 - 1.0e-9);
        assert!(camera.up().dot(rotation * DVec3::Y) > 1.0 - 1.0e-9);
        assert_eq!(camera.projection(), Projection::Perspective { fov_y: fov.radians_f64() });
        assert!(camera.project_ndc(camera.eye() + camera.forward() * 10.0, 1.0).is_some());

        // An orthographic camera keeps its visible height.
        project
            .set_camera_projection(
                camera_id,
                CameraProjection::Orthographic { height: Length::from_millimetres(500).expect("height") },
            )
            .expect("orthographic");
        let camera = authored_camera(&project).expect("camera");
        assert_eq!(
            camera.projection(),
            Projection::Orthographic { height: Length::from_millimetres(500).expect("height").millimetres_f64() }
        );
    }

    #[test]
    fn an_authored_sun_lights_the_frame_through_its_world_rotation() {
        // A scene without a light keeps the built-in key light; a point light adds to that key.
        let mut project = sample_project();
        assert_eq!(authored_lighting(&project), None);
        let root = project.root().id();
        project
            .add_light(root, "Fill", Placement::IDENTITY, LightKind::Point, [0.2, 0.3, 0.4], 1000.0)
            .expect("point light");
        let lighting = authored_lighting(&project).expect("a point light lights the frame");
        assert_eq!(lighting.key_color, Lighting::default().key_color, "a point-only scene keeps the built-in key");
        assert_eq!(lighting.points.len(), 1);
        assert_eq!(lighting.points[0].power, 1000.0);
        assert_eq!(lighting.points[0].color, [0.2, 0.3, 0.4]);

        let spin = DQuat::from_rotation_y(FRAC_PI_2);
        let placement = Placement::new(DVec3::new(10.0, 20.0, 30.0), spin).expect("placement");
        project.add_light(root, "Sun", placement, LightKind::Sun, [1.0, 0.5, 0.25], 2.0).expect("sun");
        let lighting = authored_lighting(&project).expect("the sun lights the frame");
        assert!(
            lighting.key_direction.dot(DVec3::X) > 1.0 - 1.0e-9,
            "a sun emits along local -Z, so the direction towards it is +Z turned into +X: {lighting:?}"
        );
        assert_eq!(lighting.key_color, [2.0, 1.0, 0.5]);
        assert_eq!(lighting.ambient, Lighting::default().ambient);

        // The direction follows the whole parent chain, not only the light's own placement.
        let group = Container::new(
            id(2).expect("id"),
            "Group",
            Placement::new(DVec3::ZERO, DQuat::from_rotation_z(FRAC_PI_2)).expect("placement"),
            Vec::new(),
        );
        let root = Container::new(id(1).expect("id"), "Root", Placement::IDENTITY, vec![Node::Container(group)]);
        let mut nested = Project::new(root).expect("project");
        nested
            .add_light(id(2).expect("id"), "Sun", placement, LightKind::Sun, [1.0, 1.0, 1.0], 1.0)
            .expect("nested sun");
        let lighting = authored_lighting(&nested).expect("the nested sun lights the frame");
        assert!(
            lighting.key_direction.dot(DVec3::Y) > 1.0 - 1.0e-9,
            "the parent turn and the light turn compose: {lighting:?}"
        );
    }

    #[test]
    fn point_lights_use_their_world_position_and_a_frame_shades_at_most_eight() {
        let mut project = sample_project();
        let root = project.root().id();
        for index in 1..=10 {
            let placement =
                Placement::new(DVec3::new(index as f64 * 100.0, 0.0, 0.0), DQuat::IDENTITY).expect("placement");
            project
                .add_light(root, format!("Point {index}"), placement, LightKind::Point, [1.0; 3], 1000.0)
                .expect("point light");
        }

        let lighting = authored_lighting(&project).expect("the point lights light the frame");
        assert_eq!(lighting.points.len(), MAX_POINT_LIGHTS, "the fragment loop has a fixed budget");
        for (index, point) in lighting.points.iter().enumerate() {
            assert_eq!(point.position, DVec3::new((index + 1) as f64 * 100.0, 0.0, 0.0));
            assert_eq!(point.power, 1000.0);
        }
    }

    #[test]
    fn a_point_light_adds_its_parent_placement() {
        let group = Container::new(
            id(2).expect("id"),
            "Group",
            Placement::new(DVec3::new(100.0, 0.0, 0.0), DQuat::IDENTITY).expect("placement"),
            Vec::new(),
        );
        let root = Container::new(id(1).expect("id"), "Root", Placement::IDENTITY, vec![Node::Container(group)]);
        let mut project = Project::new(root).expect("project");
        project
            .add_light(
                id(2).expect("id"),
                "Fill",
                Placement::new(DVec3::new(10.0, 0.0, 0.0), DQuat::IDENTITY).expect("placement"),
                LightKind::Point,
                [1.0; 3],
                1000.0,
            )
            .expect("nested point light");

        let lighting = authored_lighting(&project).expect("the point light lights the frame");
        assert_eq!(lighting.points[0].position, DVec3::new(110.0, 0.0, 0.0));
    }

    #[test]
    fn a_previewed_placement_lights_the_frame_before_the_command_lands() {
        let mut project = sample_project();
        let root = project.root().id();
        let sun = project
            .add_light(
                root,
                "Sun",
                Placement::new(DVec3::ZERO, DQuat::IDENTITY).expect("placement"),
                LightKind::Sun,
                [1.0; 3],
                1.0,
            )
            .expect("sun");
        let point = project
            .add_light(
                root,
                "Fill",
                Placement::new(DVec3::new(100.0, 0.0, 0.0), DQuat::IDENTITY).expect("placement"),
                LightKind::Point,
                [1.0; 3],
                1000.0,
            )
            .expect("point light");

        let base = authored_lighting_with(&project, &[], &ViewFlags::default()).expect("authored lighting");
        assert_eq!(base.points[0].position, DVec3::new(100.0, 0.0, 0.0));

        // A quarter turn about X aims the sun's emission along -Y, so the light comes from +Y.
        let turned = Placement::new(DVec3::ZERO, DQuat::from_rotation_x(-FRAC_PI_2)).expect("placement");
        let moved = Placement::new(DVec3::new(300.0, 40.0, 0.0), DQuat::IDENTITY).expect("placement");
        let previewed = authored_lighting_with(&project, &[(sun, turned), (point, moved)], &ViewFlags::default())
            .expect("previewed lighting");
        assert!(previewed.key_direction.dot(DVec3::Y) > 1.0 - 1.0e-9, "the previewed sun lights from +Y");
        assert_eq!(previewed.points[0].position, DVec3::new(300.0, 40.0, 0.0));
        assert_eq!(previewed.points[0].power, 1000.0, "a preview does not change the power");
    }

    #[test]
    fn the_demo_document_has_three_panels_and_three_cutouts() {
        let session = demo_session().expect("the demo session");
        let project = session.project();
        project.validate().expect("a valid demo document");

        let mut panels = Vec::new();
        panels_in(project.root().children(), &mut panels);
        let cutouts: usize = panels
            .iter()
            .map(|panel| panel.machining().iter().filter(|machining| machining.cutout().is_some()).count())
            .sum();
        let bands: usize = panels.iter().map(|panel| panel.edge_banding().len()).sum();
        assert_eq!(panels.len(), 3);
        assert_eq!(cutouts, 3);
        assert_eq!(bands, 3);
    }

    #[test]
    fn the_demo_document_has_one_group_and_one_component() {
        let session = demo_session().expect("the demo session");
        let project = session.project();
        let root = project.root();
        assert_eq!(root.children().len(), 2);
        let mut groups = 0;
        let mut components = 0;
        for child in root.children() {
            if let Node::Container(container) = child {
                if container.is_component() {
                    components += 1;
                } else {
                    groups += 1;
                }
            }
        }
        assert_eq!(groups, 1);
        assert_eq!(components, 1);
    }

    #[test]
    fn the_demo_materials_resolve_from_the_catalogue() {
        let session = demo_session().expect("the demo session");
        let catalog = session.directory();
        let mut panel_materials = Vec::new();
        let mut panels = Vec::new();
        panels_in(session.project().root().children(), &mut panels);
        for panel in &panels {
            let material = panel.material().expect("a demo panel has a material");
            let resolved = catalog.board(material).expect("a demo panel material resolves");
            assert!(!resolved.name.is_empty());
            panel_materials.push(material);
            for band in panel.edge_banding() {
                let resolved = catalog.banding(band.material()).expect("a demo edge material resolves");
                assert!(!resolved.name.is_empty());
            }
        }
        assert_eq!(panel_materials.len(), 3);
        assert_ne!(panel_materials[0], panel_materials[1], "the two panels demonstrate different materials");
    }

    #[test]
    fn an_empty_session_has_a_root_and_nothing_to_draw() {
        let session = empty_session().expect("empty session");
        session.project().validate().expect("valid empty document");
        assert!(session.project().root().children().is_empty());
        assert!(!session.is_dirty());

        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let evaluated =
            evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("evaluated scene");
        assert!(evaluated.frame.drawables().is_empty());
        assert_eq!(evaluated.bounds.center(), None);
        assert!(home_camera(evaluated.bounds).is_ok(), "an empty scene still has a usable camera");
    }

    #[test]
    fn a_new_directory_rebuilds_the_panels_of_the_board_it_changed() {
        // A directory revision is not a document revision, so the cache is the only thing that
        // decides what to rebuild: a panel is keyed by the thickness it resolved, which is exactly
        // what a changed board row moves (ADR 0053, ADR 0148).
        let root = Container::new(id(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("project");
        let root = project.root().id();
        let size = panel_nominal_size(600, 400).expect("a panel");
        let fallback = millimetres(18).expect("18 mm");
        let cut_from = |project: &mut Project, name: &str, serial: u32| {
            let material = material_id(serial).expect("an item id");
            project.add_panel(root, name, size, Some(material), fallback, Placement::IDENTITY).expect("a panel")
        };
        let changed = cut_from(&mut project, "Side", 1);
        let untouched = cut_from(&mut project, "Shelf", 2);
        let mut session = Session::with_directory(project, testkit::sample_catalog());

        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let before = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("the frame");
        assert_eq!(top_of(&before, changed), 18.0, "the fixture board is 18 mm");

        session.set_directory(std::sync::Arc::new(testkit::sample_catalog_with_thickness(
            millimetres(25).expect("25 mm"),
        )));
        let after = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("the frame");

        assert_eq!(top_of(&after, changed), 25.0, "the panel of the changed board is cut again, from the new row");
        assert!(!Arc::ptr_eq(mesh_of(&before, changed), mesh_of(&after, changed)), "as a new mesh");
        assert!(
            Arc::ptr_eq(mesh_of(&before, untouched), mesh_of(&after, untouched)),
            "and a panel nothing in the directory touched keeps the mesh it had"
        );
    }

    #[test]
    fn the_editor_opens_a_document_with_nothing_in_it() {
        // The window opens its first tab from an empty session; a demonstration scene there would
        // put three panels, a group and a drawer in front of the maker's first piece.
        let session = empty_session().expect("the initial session");
        assert!(session.project().root().children().is_empty(), "a new document has no object");
        assert!(!session.is_dirty(), "opening a document is not an edit");
        assert!(
            !session.directory().boards().collect::<Vec<_>>().is_empty(),
            "the default materials stay: a panel needs one to be created"
        );
    }

    #[test]
    fn a_larger_scene_fits_farther_out() {
        let bounds = |half_extent: f64| {
            let mut bounds = Bounds::empty();
            bounds.include_point(DVec3::splat(-half_extent));
            bounds.include_point(DVec3::splat(half_extent));
            bounds
        };
        let small = home_camera(bounds(100.0)).expect("small fit");
        let large = home_camera(bounds(1_000.0)).expect("large fit");
        assert!(
            large.distance() > small.distance(),
            "a larger scene frames from farther away: {} vs {}",
            large.distance(),
            small.distance()
        );
        assert_eq!(small.target(), DVec3::ZERO, "the fit looks at the scene centre");
        assert_eq!(large.target(), DVec3::ZERO);
    }

    #[test]
    fn a_drawing_on_the_floor_casts_no_contact_shadow() {
        let millimetres = |value: i64| Length::from_millimetres(value).expect("length");
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

        let root = Container::new(id(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("project");
        let root = project.root().id();
        project.add_object(root, "Sketch", Placement::IDENTITY, Geometry::Path(path)).expect("object");
        let session = Session::with_directory(project, default_catalog().expect("catalogue"));

        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let evaluated =
            evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("evaluated scene");
        assert_eq!(evaluated.frame.polylines().len(), 1);
        assert!(evaluated.frame.drawables().is_empty(), "a flat chain is not a body, so the light casts nothing");
    }

    #[test]
    fn the_demo_scene_stands_on_the_floor() {
        let session = demo_session().expect("the demo session");
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let evaluated =
            evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("evaluated scene");

        let floor = evaluated.bounds.min().expect("bounds");
        assert!(floor.z.abs() < 1.0e-9, "the demo scene stands on the floor");
    }

    #[test]
    fn bounds_report_their_centre_and_radius() {
        let mut bounds = Bounds::empty();
        let mesh = MeshData {
            positions: vec![[0.0, 0.0, 0.0], [100.0, 200.0, 40.0]],
            normals: Vec::new(),
            indices: Vec::new(),
        };
        bounds.include_mesh(&mesh, DMat4::IDENTITY);
        assert_eq!(bounds.center(), Some(DVec3::new(50.0, 100.0, 20.0)));
        assert!(bounds.radius().is_some_and(|radius| radius > 0.0));
    }

    /// The world box of a drawable is the eight corners of the local box the mesh cache measured,
    /// and it agrees with the per-vertex box the scene walk used to compute (X56).
    #[test]
    fn the_scene_box_is_the_box_of_the_vertices_it_holds() {
        let session = demo_session().expect("the demo session");
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let evaluated = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("the scene");

        // The reference: every vertex of every drawable, transformed one at a time, exactly as the
        // walk did while `Bounds::include_mesh` fed both of the subtree's boxes.
        let mut vertices = Bounds::empty();
        for drawable in evaluated.frame.drawables() {
            vertices.include_mesh(&drawable.mesh, drawable.world_transform);
        }
        for polyline in evaluated.frame.polylines() {
            for point in polyline.points.iter() {
                vertices.include_point(polyline.world_transform.transform_point3(*point));
            }
        }
        let (Some(vertex_min), Some(vertex_max)) = (vertices.min(), vertices.max()) else {
            panic!("the demo scene draws something");
        };
        let scene_min = evaluated.bounds.min().expect("the scene has a box");
        let scene_max = evaluated.bounds.max().expect("the scene has a box");
        assert!(
            (scene_min - vertex_min).length() < 1.0e-6 && (scene_max - vertex_max).length() < 1.0e-6,
            "the corner box is the vertex box at the demo scene: {scene_min:?}..{scene_max:?} against \
             {vertex_min:?}..{vertex_max:?}"
        );

        // And every drawable stays inside the box, which is what framing and the open brackets read.
        for drawable in evaluated.frame.drawables() {
            let local = drawable.mesh.local_bounds().expect("a drawable mesh has vertices");
            let world = local.transformed_by(drawable.world_transform);
            assert!(
                world.min().cmpge(scene_min).all() && world.max().cmple(scene_max).all(),
                "the drawable of {} stays inside the scene box",
                drawable.id
            );
        }
    }

    /// A flat root of `count` 32 x 16 spheres: 896 vertices each, so the whole field is about 4.5 M
    /// vertices and 4.8 M triangles. It is the stress scene the `[perf]` entries are measured on.
    fn sphere_field(count: u64) -> Session {
        let root = Container::new(id(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("project");
        let root = project.root().id();
        let primitive = model::Primitive::sphere(millimetres(25).expect("radius"), 32, 16).expect("sphere");
        let side = (count as f64).sqrt().ceil() as u64;
        for index in 0..count {
            let column = index % side.max(1);
            let row = index / side.max(1);
            let placement = Placement::new(DVec3::new(column as f64 * 60.0, row as f64 * 60.0, 0.0), DQuat::IDENTITY)
                .expect("placement");
            project
                .add_object(root, "Ball", placement, Geometry::Primitive(primitive.clone()))
                .expect("a ball in the field");
        }
        Session::with_directory(project, default_catalog().expect("catalogue"))
    }

    /// Measures what one edit costs the uploader: one drawable rebuilt, the rest kept (X57).
    ///
    /// The device is a real adapter, because creating a buffer is the driver's work; the frame comes
    /// from the same fixture the evaluation probe uses. Ignored by default: it needs a GPU and
    /// thousands of spheres, and `cargo test` touches neither.
    #[test]
    #[ignore = "needs a GPU adapter; measures the incremental upload"]
    fn an_edit_uploads_one_drawable_and_not_the_scene() {
        let Ok(render_state) = crate::ui_shots::render_state() else {
            println!("this machine has no GPU adapter to upload the scene with");
            return;
        };
        for count in [500, 2_000] {
            let (cold, warm, edit, whole) = upload_costs(&render_state.device, count);
            println!(
                "upload at {count} drawables: cold {cold:?}, unchanged {warm:?}, one edit {edit:?}, whole {whole:?}"
            );
            assert!(edit * 20 < whole, "{count}: an edit uploads a fraction of the frame: {edit:?} of {whole:?}");
            assert!(warm < edit, "{count}: an unchanged frame costs less than an edit: {warm:?} against {edit:?}");
        }
    }

    /// One cold upload of a field of `count` spheres, an unchanged one, one edit, and the edit's
    /// whole-frame upload, each measured on the real adapter.
    fn upload_costs(
        device: &eframe::wgpu::Device,
        count: u64,
    ) -> (std::time::Duration, std::time::Duration, std::time::Duration, std::time::Duration) {
        use std::time::{Duration, Instant};

        use render::ViewportRenderer;

        let mut session = sphere_field(count);
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let flags = ViewFlags::default();
        let mut scene = evaluate(&mut cache, &mut kernel, &session, &flags, None).expect("the first scene").frame;
        let moved = session.project().root().children().first().map(Node::id).expect("the field has a ball");
        let mut renderer = ViewportRenderer::new(device, 1280, 800);

        // The frame the last editor built, uploaded into an empty renderer, is what every edit cost
        // before the cache: one vertex and index buffer per drawable and one flattened edge list.
        let started = Instant::now();
        renderer.upload(device, &scene).expect("the first upload");
        let cold = started.elapsed();

        // A second upload of the same frame, which is what a camera move or a theme change asks for.
        let mut warm = Duration::MAX;
        for _ in 0..5 {
            let started = Instant::now();
            renderer.upload(device, &scene).expect("an unchanged frame uploads");
            warm = warm.min(started.elapsed());
        }

        // Five moves of one ball: each is one new vertex and index buffer pair and one edge buffer,
        // and every other drawable keeps what it already had.
        let mut edit = Duration::MAX;
        for step in 0..5 {
            let placement =
                Placement::new(DVec3::new(0.0, 0.0, step as f64 * 10.0), DQuat::IDENTITY).expect("placement");
            let params = serde_json::json!({ "node": moved, "placement": placement });
            session.dispatch("node.placement.set", &params).expect("move one ball");
            scene = evaluate(&mut cache, &mut kernel, &session, &flags, None).expect("the edited scene").frame;
            let started = Instant::now();
            renderer.upload(device, &scene).expect("the edited frame uploads");
            edit = edit.min(started.elapsed());
        }

        // The same edited frame into an empty renderer: the cost the cache removed.
        let mut fresh = ViewportRenderer::new(device, 1280, 800);
        let started = Instant::now();
        fresh.upload(device, &scene).expect("the whole frame uploads");
        let whole = started.elapsed();
        (cold, warm, edit, whole)
    }

    /// A click used to cost every triangle of every object: 16 / 64 / 161 ms at 500 / 2 000 /
    /// 5 000 spheres (0.48 M / 1.9 M / 4.8 M triangles), because the cast walked the whole index
    /// buffer of each drawable whether or not the ray could reach it. It now rejects a drawable by
    /// the box its mesh cache measured, so a click costs a small multiple of the objects (X59).
    #[test]
    fn a_click_costs_the_objects_and_not_their_triangles() {
        for count in [500, 2_000, 5_000] {
            let session = sphere_field(count);
            let mut kernel = TruckKernel::new();
            let mut cache = EvaluationCache::new();
            let frame = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None)
                .expect("the stress scene")
                .frame;
            let triangles: usize = frame.drawables().iter().map(|drawable| drawable.mesh.triangle_count()).sum();

            // A click through empty space is the case that used to walk everything: no box of the
            // field is near the ray, so the whole frame answers without reading a vertex.
            let empty = scene::Ray::new(DVec3::new(-1.0e6, -1.0e6, 1_000.0), -DVec3::Z).expect("ray");
            // And a click into the field, which does cast the object it points at.
            let model = frame.camera.ray_ndc(0.0, 0.0, 16.0 / 9.0);
            let empty_cost = fastest_pick(&frame, empty);
            let model_cost = fastest_pick(&frame, model);
            println!(
                "pick at {count} spheres, {triangles} triangles: empty {empty_cost:?}, on the model {model_cost:?}"
            );
            assert_eq!(frame.pick(empty, 600.0), None, "the empty click hits nothing");
            assert!(frame.pick(model, 600.0).is_some(), "the model click hits a ball");
            assert!(
                empty_cost < std::time::Duration::from_millis(20),
                "{count} objects answer an empty click in {empty_cost:?}"
            );
            assert!(
                model_cost < std::time::Duration::from_millis(20),
                "{count} objects answer a click on the model in {model_cost:?}"
            );
        }
    }

    /// The fastest of five picks of one frame with `ray`, so a busy machine reports its best case.
    fn fastest_pick(frame: &SceneFrame, ray: scene::Ray) -> std::time::Duration {
        let mut best = std::time::Duration::MAX;
        for _ in 0..5 {
            let started = std::time::Instant::now();
            let _ = frame.pick(ray, 600.0);
            best = best.min(started.elapsed());
        }
        best
    }

    /// The fastest of `runs` evaluations of one document, so a busy machine reports its best case
    /// instead of nothing.
    fn fastest_evaluation(
        runs: usize,
        cache: &mut EvaluationCache,
        kernel: &mut TruckKernel,
        session: &Session,
    ) -> std::time::Duration {
        let mut best = std::time::Duration::MAX;
        for _ in 0..runs {
            let started = std::time::Instant::now();
            evaluate(cache, kernel, session, &ViewFlags::default(), None).expect("a scene");
            best = best.min(started.elapsed());
        }
        best
    }

    /// One edit anywhere cost the whole scene: the walk transformed every vertex of every mesh twice
    /// to find the world box of its drawable, so 5 000 spheres were 9 M transforms per rebuild (X56).
    /// An edit now costs the walk of the tree and the corners of what moved.
    #[test]
    fn an_edit_costs_the_walk_of_the_tree_and_not_the_vertices() {
        const COUNT: u64 = 5_000;
        let mut session = sphere_field(COUNT);
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let flags = ViewFlags::default();
        evaluate(&mut cache, &mut kernel, &session, &flags, None).expect("the first scene");
        let moved = session.project().root().children().first().map(Node::id).expect("the field has a ball");

        // A rebuild that changes nothing is the walk alone: 5 000 nodes, every mesh a cache hit.
        let walk = fastest_evaluation(5, &mut cache, &mut kernel, &session);

        // The edit: one ball moves, which is the most expensive one-node change there is — its world
        // box changes, so the scene box is composed again from every drawable.
        VERTICES_TRANSFORMED.with(|count| count.set(0));
        let mut edit = std::time::Duration::MAX;
        for step in 0..5 {
            let placement =
                Placement::new(DVec3::new(0.0, 0.0, step as f64 * 10.0), DQuat::IDENTITY).expect("placement");
            let params = serde_json::json!({ "node": moved, "placement": placement });
            session.dispatch("node.placement.set", &params).expect("move one ball");
            let started = std::time::Instant::now();
            evaluate(&mut cache, &mut kernel, &session, &flags, None).expect("the edited scene");
            edit = edit.min(started.elapsed());
        }
        assert_eq!(VERTICES_TRANSFORMED.with(|count| count.get()), 0, "the walk reads boxes, never vertices");
        assert!(
            edit < std::time::Duration::from_millis(50),
            "5 000 objects rebuild in {edit:?}; the walk alone costs {walk:?}"
        );
        assert!(
            edit < walk * 2 + std::time::Duration::from_millis(5),
            "an edit costs the walk of the tree, not the scene: edit {edit:?}, walk {walk:?}"
        );

        // The box still holds everything, and it followed the ball that moved.
        let scene = evaluate(&mut cache, &mut kernel, &session, &flags, None).expect("the settled scene");
        assert_eq!(scene.frame.drawables().len(), COUNT as usize, "every ball is still drawn");
        let top = scene.bounds.max().expect("the field has a box").z;
        assert!((top - 65.0).abs() < 1.0e-6, "the last move lifted a 25 mm ball onto a 40 mm offset: {top}");
    }

    #[test]
    fn a_constructive_object_is_evaluated_into_a_drawable() {
        use model::{Constructive, Contour};

        let millimetres = |value: i64| Length::from_millimetres(value).expect("length");
        let contour = Contour::from_polygon(vec![
            [millimetres(0), millimetres(0)],
            [millimetres(600), millimetres(0)],
            [millimetres(600), millimetres(400)],
            [millimetres(0), millimetres(400)],
        ])
        .expect("contour");
        let constructive = Constructive::extrude(contour.into(), millimetres(18)).expect("constructive");

        let root = Container::new(id(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("project");
        let root = project.root().id();
        let object =
            project.add_object(root, "Top", Placement::IDENTITY, Geometry::Constructive(constructive)).expect("object");
        let session = Session::with_directory(project, default_catalog().expect("catalogue"));

        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let evaluated =
            evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("evaluated scene");
        assert_eq!(evaluated.frame.drawables().len(), 1, "the derived mesh is one drawable");
        let drawable = &evaluated.frame.drawables()[0];
        assert_eq!(drawable.id, object, "the drawable carries the object id, so picking and selection work");
        assert!(!drawable.mesh.positions.is_empty(), "the extrusion is tessellated from its contour");
        let center = evaluated.bounds.center().expect("the object contributes bounds");
        assert!(center.y > 0.0 && center.z > 0.0, "bounds follow the contour and the extrusion height");
    }

    #[test]
    fn a_primitive_is_drawn_from_its_parameters_and_picks_by_id() {
        let millimetres = |value: i64| Length::from_millimetres(value).expect("length");
        let primitive = model::Primitive::sphere(millimetres(40), 16, 8).expect("sphere");
        let root = Container::new(id(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("project");
        let root = project.root().id();
        let object =
            project.add_object(root, "Ball", Placement::IDENTITY, Geometry::Primitive(primitive)).expect("object");
        let session = Session::with_directory(project, default_catalog().expect("catalogue"));

        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let evaluated =
            evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("evaluated scene");
        assert_eq!(evaluated.frame.drawables().len(), 1, "a primitive is one drawable");
        let drawable = &evaluated.frame.drawables()[0];
        assert_eq!(drawable.id, object, "the drawable carries the object id");
        assert!(!drawable.mesh.positions.is_empty(), "the mesh is derived from the parameters");
        let center = evaluated.bounds.center().expect("the object contributes bounds");
        assert!(center.x.abs() < 1.0e-3 && center.z.abs() < 1.0e-3, "a sphere is centred on its origin");
    }

    #[test]
    fn an_open_path_is_drawn_as_a_polyline() {
        let millimetres = |value: i64| Length::from_millimetres(value).expect("length");
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

        let root = Container::new(id(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("project");
        let root = project.root().id();
        let placement = Placement::new(DVec3::new(10.0, 20.0, 30.0), DQuat::IDENTITY).expect("placement");
        let object = project.add_object(root, "Sketch", placement, Geometry::Path(path)).expect("object");
        let session = Session::with_directory(project, default_catalog().expect("catalogue"));

        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let evaluated =
            evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("evaluated scene");
        assert!(evaluated.frame.drawables().is_empty(), "a path has no solid");
        assert_eq!(evaluated.frame.polylines().len(), 1);
        let line = &evaluated.frame.polylines()[0];
        assert_eq!(line.id, object);
        assert_eq!(line.points.len(), 3, "two segments share one joint");
        assert_eq!(line.points[0], DVec3::ZERO);
        assert_eq!(line.points[1], DVec3::new(600.0, 0.0, 0.0));
        assert_eq!(line.points[2], DVec3::new(600.0, 400.0, 0.0));
        let origin = line.world_transform.transform_point3(line.points[0]);
        assert_eq!(origin, DVec3::new(10.0, 20.0, 30.0), "the object placement moves the chain");
        let center = evaluated.bounds.center().expect("the path contributes bounds");
        assert!(center.x > 10.0 && center.y > 20.0);
    }

    #[test]
    fn a_closed_path_draws_its_closing_segment() {
        let millimetres = |value: i64| Length::from_millimetres(value).expect("length");
        let corner = |x: i64, y: i64| [millimetres(x), millimetres(y)];
        let path = Path::new(
            vec![
                ContourSegment::line(corner(0, 0), corner(600, 0)).expect("line"),
                ContourSegment::line(corner(600, 0), corner(0, 400)).expect("line"),
                ContourSegment::line(corner(0, 400), corner(0, 0)).expect("line"),
            ],
            true,
        )
        .expect("closed path");

        let root = Container::new(id(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("project");
        let root = project.root().id();
        project.add_object(root, "Sketch", Placement::IDENTITY, Geometry::Path(path)).expect("object");
        let session = Session::with_directory(project, default_catalog().expect("catalogue"));

        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let evaluated =
            evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("evaluated scene");
        let line = &evaluated.frame.polylines()[0];
        assert_eq!(line.points.len(), 4, "the closing segment keeps its end point");
        assert_eq!(line.points[3], line.points[0], "the chain closes on its first point");
    }

    #[test]
    fn the_demo_scene_evaluates_panels_and_banding() {
        let session = demo_session().expect("the demo session");
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let evaluated =
            evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("evaluated scene");

        assert_eq!(evaluated.frame.drawables().len(), 6);
        let mut groups = std::collections::HashMap::new();
        for drawable in evaluated.frame.drawables() {
            assert!(!drawable.mesh.positions.is_empty());
            *groups.entry(drawable.id).or_insert(0) += 1;
        }
        assert_eq!(groups.len(), 3, "three panels");
        let mut counts: Vec<_> = groups.values().copied().collect();
        counts.sort_unstable();
        assert_eq!(counts, vec![1, 2, 3], "body plus bands per panel");
        assert!(evaluated.bounds.center().is_some());
        assert!(home_camera(evaluated.bounds).is_ok());
        assert_eq!(kernel.body_count(), 0, "evaluation releases every kernel body");
    }

    /// The mesh handle of the first drawable carrying `id`.
    fn mesh_of(scene: &EvaluatedScene, id: Id) -> &Arc<MeshData> {
        scene
            .frame
            .drawables()
            .iter()
            .find(|drawable| drawable.id == id)
            .map(|drawable| &drawable.mesh)
            .expect("drawable")
    }

    fn panel_id(session: &Session, name: &str) -> Id {
        let mut panels = Vec::new();
        panels_in(session.project().root().children(), &mut panels);
        panels.iter().find(|panel| panel.name() == name).map(|panel| panel.id()).expect("panel by name")
    }

    #[test]
    fn a_rebuild_at_a_new_revision_reuses_every_unchanged_mesh() {
        let mut session = demo_session().expect("the demo session");
        let renamed = panel_id(&session, "Loose shelf");
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let first = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("first scene");

        // A rename bumps the document revision but none of the derived geometry depends on it.
        let params = serde_json::json!({ "node": renamed, "name": "Shelf" });
        session.dispatch("node.rename", &params).expect("rename");

        let second = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("second scene");
        assert!(session.project().revision() > Revision::ZERO);
        assert_eq!(first.frame.drawables().len(), second.frame.drawables().len());
        for (before, after) in first.frame.drawables().iter().zip(second.frame.drawables()) {
            assert_eq!(before.id, after.id);
            assert!(Arc::ptr_eq(&before.mesh, &after.mesh), "an unchanged node keeps its mesh across a rebuild");
        }
        assert_eq!(kernel.body_count(), 0, "a hit touches no kernel body");
    }

    /// The highest vertex of the mesh drawn for `id`, in the node's own millimetres.
    fn top_of(scene: &EvaluatedScene, id: Id) -> f32 {
        mesh_of(scene, id).positions.iter().map(|position| position[2]).fold(f32::MIN, f32::max)
    }

    #[test]
    fn a_preview_draws_the_body_the_command_would_store() {
        // A previewing operation shows a node the document does not hold yet. The frame must follow
        // the overlay — the taller body, not the stored one — while the document keeps its height,
        // its revision and its history (M71).
        let mut project = sample_project();
        let root = project.root().id();
        let path = Path::rectangle(
            [millimetres(20).expect("x"), millimetres(10).expect("y")],
            [millimetres(120).expect("width"), millimetres(60).expect("height")],
        )
        .expect("a rectangle");
        let body = model::Constructive::extrude(path, millimetres(100).expect("height")).expect("a body");
        let object = project
            .add_object(root, "Body", Placement::IDENTITY, Geometry::Constructive(body.clone()))
            .expect("object");
        let mut session = Session::new(project);
        let revision = session.project().revision();
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let stored = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("stored scene");
        assert!((top_of(&stored, object) - 100.0).abs() < 0.5, "the document draws its own height");

        let mut previewed = body;
        previewed.set_height(millimetres(250).expect("height")).expect("a preview height");
        session.set_preview_geometry(object, Geometry::Constructive(previewed));
        let preview = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("preview scene");

        assert_eq!(session.preview().len(), 1, "one node is previewed");
        assert_eq!(session.project().revision(), revision, "a preview is not an edit");
        assert!(!session.can_undo(), "a preview leaves no undo entry");
        assert!((top_of(&preview, object) - 250.0).abs() < 0.5, "the frame shows the preview height");

        session.clear_preview();
        let restored =
            evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("restored scene");
        assert!((top_of(&restored, object) - 100.0).abs() < 0.5, "the cross restores the pre-command body");
    }

    #[test]
    fn moving_a_panel_keeps_its_mesh_and_only_moves_its_transform() {
        let mut session = demo_session().expect("the demo session");
        let id = panel_id(&session, "Loose shelf");
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let before = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("before move");

        let placement = Placement::new(DVec3::new(400.0, -500.0, 0.0), DQuat::IDENTITY).expect("placement");
        let params = serde_json::json!({ "node": id, "placement": placement });
        session.dispatch("node.placement.set", &params).expect("move");

        let after = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("after move");
        assert!(
            Arc::ptr_eq(mesh_of(&before, id), mesh_of(&after, id)),
            "a placement is not a derivation input, so the mesh is reused"
        );
        let transform = after.frame.drawables().iter().find(|drawable| drawable.id == id).map(|d| d.world_transform);
        assert!(transform.is_some_and(|transform| transform.w_axis.x > 0.0));
    }

    #[test]
    fn a_resize_rebuilds_only_the_resized_panel() {
        let mut session = demo_session().expect("the demo session");
        let resized = panel_id(&session, "Loose shelf");
        let untouched = panel_id(&session, "Back panel");
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let before = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("before resize");

        let size = model::PanelNominalSize::new(millimetres(650).expect("length"), millimetres(420).expect("width"))
            .expect("size");
        let params = serde_json::json!({ "panel": resized, "nominal_size": size });
        session.dispatch("panel.resize", &params).expect("resize");

        let after = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("after resize");
        assert!(!Arc::ptr_eq(mesh_of(&before, resized), mesh_of(&after, resized)), "the resized panel rebuilds");
        assert!(
            Arc::ptr_eq(mesh_of(&before, untouched), mesh_of(&after, untouched)),
            "an edit elsewhere leaves a panel's mesh alone"
        );
    }

    #[test]
    fn deleting_a_node_drops_its_cache_entry() {
        let mut session = demo_session().expect("the demo session");
        let id = panel_id(&session, "Loose shelf");
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("scene");
        assert!(cache.cached(id).is_some());

        let params = serde_json::json!({ "node": id });
        session.dispatch("node.delete", &params).expect("delete");
        let after =
            evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("scene after delete");
        assert!(cache.cached(id).is_none(), "a deleted node releases its mesh");
        assert!(after.frame.drawables().iter().all(|drawable| drawable.id != id));
    }

    /// The frame forgets a hidden branch in one place: drawables, bounds, contact shadows and the
    /// light rig all read the same set, and the mesh cache keeps the entries for when it is shown
    /// again.
    #[test]
    fn a_hidden_container_takes_its_whole_subtree_out_of_the_frame() {
        let session = Session::with_directory(sample_project(), default_catalog().expect("catalogue"));
        let shelf = Id::new(2).expect("shelf id");
        let drawer = Id::new(3).expect("drawer id");
        let front = Id::new(4).expect("front id");
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();

        let visible = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("the demo scene");
        assert!(visible.frame.drawables().iter().any(|drawable| drawable.id == front));

        let mut flags = ViewFlags::default();
        flags.set_hidden(drawer, true);
        let hidden = evaluate(&mut cache, &mut kernel, &session, &flags, None).expect("the scene without the drawer");
        let drawn: Vec<Id> = hidden.frame.drawables().iter().map(|drawable| drawable.id).collect();
        assert!(!drawn.contains(&front), "a child of a hidden container is not drawn");
        assert!(drawn.contains(&shelf), "a sibling branch stays drawn");
        assert!(
            cache.cached(front).is_none(),
            "a hidden branch releases its mesh like any node the pass did not touch; showing it rebuilds it"
        );
    }

    #[test]
    fn a_hidden_light_does_not_light_the_frame() {
        let mut project = sample_project();
        let root = project.root().id();
        let group = project.add_component(root, "Lamp", Placement::IDENTITY).expect("container");
        let sun = project.add_light(group, "Sun", Placement::IDENTITY, LightKind::Sun, [1.0; 3], 1.0).expect("sun");
        assert!(authored_lighting(&project).is_some(), "an authored sun is the key light");

        let mut flags = ViewFlags::default();
        flags.set_hidden(sun, true);
        assert_eq!(authored_lighting_with(&project, &[], &flags), None, "a hidden sun is switched off");

        let mut flags = ViewFlags::default();
        flags.set_hidden(group, true);
        assert_eq!(
            authored_lighting_with(&project, &[], &flags),
            None,
            "a light under a hidden container is switched off with it"
        );
    }

    /// A hidden node is gone from the frame, so the ray never finds it: picking reads the same
    /// scene the viewport draws and needs no second hidden set.
    #[test]
    fn a_hidden_node_cannot_be_picked() {
        let size =
            PanelNominalSize::new(millimetres(700).expect("length"), millimetres(500).expect("width")).expect("size");
        let hidden_panel = Panel::new(
            id(2).expect("id"),
            "Hidden",
            size,
            None,
            millimetres(18).expect("thickness"),
            Placement::IDENTITY,
        )
        .expect("panel");
        let aside = Placement::new(DVec3::new(3000.0, 0.0, 0.0), DQuat::IDENTITY).expect("placement");
        let shown_panel =
            Panel::new(id(3).expect("id"), "Shown", size, None, millimetres(18).expect("thickness"), aside)
                .expect("panel");
        let root = Container::new(
            id(1).expect("id"),
            "Root",
            Placement::IDENTITY,
            vec![hidden_panel.into(), shown_panel.into()],
        );
        let project = Project::new(root).expect("project");
        let session = Session::with_directory(project, default_catalog().expect("catalogue"));
        let camera = scene::Camera::new(DVec3::ZERO, 1500.0).expect("camera");
        let viewport = eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO, eframe::egui::vec2(800.0, 600.0));
        let pointer = viewport.center();
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();

        let mut visible = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("the scene");
        visible.frame.camera = camera;
        assert_eq!(
            crate::mesh_pick::pick_scene(viewport, &visible.frame, pointer),
            Some(id(2).expect("id")),
            "the camera looks at the panel at the origin"
        );

        let mut flags = ViewFlags::default();
        flags.set_hidden(id(2).expect("id"), true);
        let mut hidden =
            evaluate(&mut cache, &mut kernel, &session, &flags, None).expect("the scene without the panel");
        hidden.frame.camera = camera;
        assert_eq!(
            crate::mesh_pick::pick_scene(viewport, &hidden.frame, pointer),
            None,
            "a hidden panel is not in the frame, so the ray finds nothing"
        );
    }

    /// A root with one product of `1 x 1 x 1 m` at `(100, 200, 0)` and, when asked, one flat panel
    /// inside it. The product is not the root, because a product is always a node of the scene.
    fn product_scene(with_panel: bool) -> (Session, Id) {
        let root = Container::new(id(1).expect("id"), "Scene", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("project");
        let root_id = project.root().id();
        let envelope = Envelope::new(
            millimetres(1000).expect("length"),
            millimetres(1000).expect("length"),
            millimetres(1000).expect("length"),
        )
        .expect("a valid envelope");
        let placement = Placement::new(DVec3::new(100.0, 200.0, 0.0), DQuat::IDENTITY).expect("placement");
        let product =
            project.add_product(root_id, "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, placement).expect("product");
        if with_panel {
            project
                .add_panel(
                    product,
                    "Shelf",
                    panel_nominal_size(600, 400).expect("size"),
                    None,
                    millimetres(18).expect("thickness"),
                    Placement::IDENTITY,
                )
                .expect("panel");
        }
        project.validate().expect("a valid product scene");
        (Session::new(project), product)
    }

    fn ray_down(x: f64, y: f64) -> scene::Ray {
        scene::Ray::new(DVec3::new(x, y, 2000.0), -DVec3::Z).expect("ray")
    }

    #[test]
    fn a_product_envelope_is_drawn_in_the_three_states_of_the_decision() {
        let (empty, product) = product_scene(false);
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let closed = evaluate(&mut cache, &mut kernel, &empty, &ViewFlags::default(), None).expect("scene");
        let envelopes = closed.frame.product_envelopes();
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].id, product);
        assert_eq!(envelopes[0].state, ProductEnvelopeState::ClosedEmpty);
        assert_eq!(envelopes[0].height, 1000.0);
        assert_eq!(envelopes[0].outline.len(), 4, "a plain frame draws its four base sides");
        assert_eq!(envelopes[0].corners.len(), 4, "and one vertical at each corner");
        assert_eq!(
            DVec3::new(
                envelopes[0].world_transform.w_axis.x,
                envelopes[0].world_transform.w_axis.y,
                envelopes[0].world_transform.w_axis.z,
            ),
            DVec3::new(100.0, 200.0, 0.0)
        );
        assert!(
            closed.bounds.radius().is_some_and(|radius| radius > 0.0),
            "an empty piece is visible, so it is framed"
        );

        let open = evaluate(&mut cache, &mut kernel, &empty, &ViewFlags::default(), Some(product)).expect("scene");
        assert_eq!(open.frame.product_envelopes()[0].state, ProductEnvelopeState::Open);

        let (filled, product) = product_scene(true);
        let with_parts = evaluate(&mut cache, &mut kernel, &filled, &ViewFlags::default(), None).expect("scene");
        assert_eq!(with_parts.frame.product_envelopes()[0].state, ProductEnvelopeState::ClosedWithParts);
        let opened = evaluate(&mut cache, &mut kernel, &filled, &ViewFlags::default(), Some(product)).expect("scene");
        assert_eq!(opened.frame.product_envelopes()[0].state, ProductEnvelopeState::Open, "opening reveals the box");
        assert!(
            with_parts.frame.drawables().iter().any(|drawable| drawable.id != product),
            "the panel is drawn inside the closed piece"
        );
        assert!(closed.frame.drawables().iter().all(|drawable| drawable.id != product), "the envelope is never a body");
    }

    #[test]
    fn a_cut_footprint_reaches_the_scene_as_its_cut_outline() {
        let root = Container::new(id(1).expect("id"), "Scene", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("project");
        let footprint = model::Footprint::cut([
            None,
            Some(model::CornerFeatureKind::Chamfer {
                incoming: millimetres(120).expect("length"),
                outgoing: millimetres(120).expect("length"),
            }),
            None,
            None,
        ]);
        let envelope = Envelope::with_footprint(
            millimetres(800).expect("length"),
            millimetres(600).expect("length"),
            millimetres(900).expect("length"),
            footprint,
        )
        .expect("a corner cabinet");
        let root_id = project.root().id();
        project
            .add_product(root_id, "Corner", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("product");
        let session = Session::with_directory(project, default_catalog().expect("catalogue"));
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let evaluated = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("scene");
        let envelope = &evaluated.frame.product_envelopes()[0];
        // A chamfered rectangle: the cut replaces one corner, so five edges and five corners stand.
        assert_eq!(envelope.outline.len(), 5, "the outline is the cut shape: {:?}", envelope.outline);
        assert_eq!(envelope.corners.len(), 5, "two verticals stand at the cut: {:?}", envelope.corners);
        assert!(
            envelope.corners.iter().any(|point| (point.x - 680.0).abs() < 0.01 && point.y.abs() < 0.01)
                && envelope
                    .corners
                    .iter()
                    .any(|point| (point.x - 800.0).abs() < 0.01 && (point.y - 120.0).abs() < 0.01),
            "the cut's two ends are corners: {:?}",
            envelope.corners
        );
        assert!(
            !envelope.outline.iter().any(|point| (point.x - 800.0).abs() < 0.01 && point.y.abs() < 0.01),
            "the square corner is gone: {:?}",
            envelope.outline
        );
        assert!(evaluated.bounds.radius().is_some_and(|radius| radius > 0.0), "the piece is framed");
    }

    #[test]
    fn a_product_envelope_is_picked_inside_its_empty_space() {
        let (session, product) = product_scene(false);
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let scene = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("scene");

        assert_eq!(scene.frame.pick(ray_down(600.0, 700.0), 600.0), Some(product), "a click in the void");
        assert_eq!(scene.frame.pick(ray_down(150.0, 250.0), 600.0), Some(product), "a click on the box edge");
        assert_eq!(scene.frame.pick(ray_down(50.0, 50.0), 600.0), None, "a click outside the piece");

        let mut flags = ViewFlags::default();
        flags.set_hidden(product, true);
        let hidden = evaluate(&mut cache, &mut kernel, &session, &flags, None).expect("scene without the piece");
        assert_eq!(hidden.frame.pick(ray_down(600.0, 700.0), 600.0), None, "a hidden piece is not picked");
    }

    #[test]
    fn a_panel_inside_the_frame_wins_the_click() {
        let (session, product) = product_scene(true);
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let scene = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("scene");
        let panel = scene
            .frame
            .drawables()
            .iter()
            .find(|drawable| drawable.id != product)
            .map(|drawable| drawable.id)
            .expect("the panel is drawn");

        assert_eq!(scene.frame.pick(ray_down(150.0, 250.0), 600.0), Some(panel), "a click on a part picks the part");
        assert_eq!(scene.frame.pick(ray_down(950.0, 950.0), 600.0), Some(product), "the void still picks the frame");
        assert_ne!(panel, product);
    }

    #[test]
    fn the_innermost_product_wins_when_boxes_nest() {
        let root = Container::new(id(1).expect("id"), "Scene", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("project");
        let root_id = project.root().id();
        let box_envelope = |side: i64| {
            Envelope::new(
                millimetres(side).expect("length"),
                millimetres(side).expect("length"),
                millimetres(side).expect("length"),
            )
            .expect("a valid envelope")
        };
        let outer = project
            .add_product(root_id, "Outer", ClassCode::GENERAL_PRODUCT, box_envelope(1000), Placement::IDENTITY)
            .expect("outer product");
        let inner_placement = Placement::new(DVec3::new(400.0, 400.0, 0.0), DQuat::IDENTITY).expect("placement");
        let inner = project
            .add_product(root_id, "Inner", ClassCode::GENERAL_PRODUCT, box_envelope(200), inner_placement)
            .expect("inner product");
        project.validate().expect("valid nested products");
        let session = Session::new(project);
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let scene = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("scene");

        assert_eq!(scene.frame.product_envelopes().len(), 2, "both pieces carry a box");
        assert_eq!(scene.frame.pick(ray_down(500.0, 500.0), 600.0), Some(inner), "the tightest box is the piece meant");
        assert_eq!(
            scene.frame.pick(ray_down(100.0, 100.0), 600.0),
            Some(outer),
            "outside the inner piece is the outer"
        );
        assert_ne!(inner, outer);
    }
}
