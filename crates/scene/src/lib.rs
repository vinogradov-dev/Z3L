//! Renderer-agnostic scene description: what a frame contains.
//!
//! Drawables, world transforms, colours, the grid, contact shadows and lights are plain data, so a
//! scene can be built and tested without a GPU. The camera and its projection live in [`camera`],
//! the rays that pick with it in [`ray`]; both are re-exported here. The renderer owns the GPU
//! resources and interprets this description; the document never stores a camera, a grid or a
//! drawable.
//!
//! Physical lengths are millimetres and angles radians, as in [`camera`]; a colour is gamma space
//! unless it says linear.
//!
//! The surface is over B6's 25 public items on purpose: they are the fields and constructors of the
//! frame's value types (`Drawable`, `Polyline`, `ProductEnvelope`, `ShadingStyle`, `Grid`,
//! `ContactShadows`, `Lighting`, `PointLight`) beside `SceneFrame`'s own builders,
//! and a frame is assembled from all of them at once — splitting them would put one description in
//! nine modules a reader has to join back. [`Frustum`] is the volume a view culls the frame's
//! drawables by, derived from the camera the frame carries. [`decor`] is the picture a surface is
//! faced with, which both the viewport and the final render read as one value.
//!
//! Changes when: what a frame carries, or how a frame describes a drawable, a shadow or a light,
//! changes.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::sync::Arc;

use geom::{Aabb, DMat4, DQuat, DVec2, DVec3, DVec4, MeshData};
use model::Id;

use crate::ray::{ray_hit_drawable, ray_hit_polyline, ray_hit_product_envelope};

pub mod band;
pub mod camera;
pub mod decor;
pub mod ray;

pub use band::{BandCoverage, NdcRect};
pub use camera::{
    Camera, CameraError, DEFAULT_FOV_Y, MAX_DISTANCE_MM, MAX_PITCH, MIN_DISTANCE_MM, MIN_PITCH, POLE_TOLERANCE,
    Projection, ProjectionToggle, canonical_up,
};
pub use decor::{Decor, DecorError, DecorId, DecorImage, MAX_DECOR_SIDE, Turns};
pub use ray::{LINE_PICK_RADIUS_POINTS, Ray, ray_segment, ray_triangle};

/// Gamma-space colour of a selected line chain.
///
/// A tint mixed into the path colour moves too little to be seen, and the line pipeline cannot
/// widen a chain, so the selected chain takes this explicit colour and the shell repeats it as a
/// thick screen-space outline. Cyan stays visible over a brown panel, a light one and the yellow
/// unselected path.
pub const SELECTION_LINE_COLOR: [f32; 4] = [0.15, 0.85, 1.0, 1.0];

/// How the renderer shades the drawables of a frame.
///
/// The value is frame data, not document data: a viewport chooses it and the settings file remembers
/// the default, but a `.z3l` never carries it (ADR 0029, ADR 0070). The renderer turns each style
/// into pipelines; [`ShadingStyle`] only promises which passes are on, so a frame can be tested
/// without a GPU.
///
/// [ADR 0029]: ../docs/decisions/0029-view-state-belongs-to-the-session.md
/// [ADR 0070]: ../docs/decisions/0070-display-styles-are-frame-data.md
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShadingStyle {
    /// Shaded faces with the deduplicated mesh edges drawn over them.
    #[default]
    ShadedWithEdges,
    /// Shaded faces with no edge pass.
    Shaded,
    /// The edge pass alone, with no faces.
    Wireframe,
    /// Edges drawn through the faces, so hidden geometry stays visible.
    ShadedWithHiddenEdges,
    /// Blended faces with no depth write, drawn back to front by object centre.
    Translucent,
}

impl ShadingStyle {
    /// Every style, in the order the viewport cluster draws its buttons.
    pub const ALL: [Self; 5] =
        [Self::ShadedWithEdges, Self::Shaded, Self::Wireframe, Self::ShadedWithHiddenEdges, Self::Translucent];

    /// Whether the style draws filled faces.
    pub const fn draws_faces(self) -> bool {
        !matches!(self, Self::Wireframe)
    }

    /// Whether the style draws the mesh edge pass.
    pub const fn draws_edges(self) -> bool {
        !matches!(self, Self::Shaded | Self::Translucent)
    }

    /// Whether the edge pass ignores the depth test, drawing edges through the faces.
    pub const fn edges_through_faces(self) -> bool {
        matches!(self, Self::ShadedWithHiddenEdges)
    }

    /// Whether the faces are blended and write no depth.
    pub const fn is_translucent(self) -> bool {
        matches!(self, Self::Translucent)
    }
}

/// A floor grid on the world XY plane.
///
/// The renderer derives line geometry from these values; the document knows nothing about it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    /// Whether the grid is drawn at all. A hidden grid costs no vertices.
    pub visible: bool,
    /// Distance between adjacent minor lines, in millimetres.
    pub spacing_mm: f64,
    /// Half-size of the square grid around the world origin, in millimetres.
    pub extent_mm: f64,
    /// Every `major_every`-th line is drawn with `major_color`.
    pub major_every: u32,
    /// Colour of ordinary lines, in gamma space.
    pub minor_color: [f32; 4],
    /// Colour of every major line, in gamma space.
    pub major_color: [f32; 4],
    /// Colour of the world X axis, in gamma space.
    pub axis_x_color: [f32; 4],
    /// Colour of the world Y axis, in gamma space.
    pub axis_y_color: [f32; 4],
    /// How much of the inks above shows, from 0 to 1: the display strength of the grid.
    ///
    /// A fade, not a second colour, so the theme owns the hue and the workspace owns how much of it
    /// appears — a strength of zero hides the grid instead of turning it black. The renderer
    /// multiplies every grid ink's alpha by it (U107).
    pub ink_strength: f32,
}

impl Grid {
    /// The floor grid of the dark variant: light lines on the dark viewport.
    pub const DARK: Self = Self {
        visible: true,
        spacing_mm: 100.0,
        extent_mm: 5000.0,
        major_every: 5,
        minor_color: [0.28, 0.30, 0.34, 1.0],
        major_color: [0.42, 0.45, 0.50, 1.0],
        axis_x_color: [0.72, 0.28, 0.28, 1.0],
        axis_y_color: [0.28, 0.62, 0.34, 1.0],
        ink_strength: 1.0,
    };

    /// The floor grid of the light variant: dark lines on the paper viewport.
    ///
    /// The greys are the mirror of [`Self::DARK`], so the same line reads at the same strength on
    /// either background; the axes keep their hue and lose brightness, which is what makes them
    /// readable on paper.
    pub const LIGHT: Self = Self {
        visible: true,
        spacing_mm: 100.0,
        extent_mm: 5000.0,
        major_every: 5,
        minor_color: [0.66, 0.68, 0.72, 1.0],
        major_color: [0.50, 0.53, 0.58, 1.0],
        axis_x_color: [0.70, 0.26, 0.26, 1.0],
        axis_y_color: [0.20, 0.52, 0.28, 1.0],
        ink_strength: 1.0,
    };
}

impl Default for Grid {
    fn default() -> Self {
        Self::DARK
    }
}

/// The viewport's own colours: what the active theme variant paints the frame with.
///
/// These are the frame's non-material colours — the clear colour, the floor grid, the ink of an
/// unlit line that leaves its colour to the frame (see [`Polyline::in_ink`]), the two inks of a
/// product envelope (see [`ProductEnvelopeState`]) and the ink of a guide (see
/// [`SceneColors::helper`]). A variant never touches a drawable's material
/// colour, so the furniture keeps its own colours in both themes. The renderer derives the edge ink
/// from [`SceneColors::background`], so a light workspace draws dark form edges without a second
/// switch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneColors {
    /// The clear colour, in gamma space.
    pub background: [f32; 4],
    /// The grid ink; its switch, spacing and size stay the viewport's own display state.
    pub grid: Grid,
    /// The colour of the floor plate, in gamma space.
    ///
    /// The plate is the surface the ground shadow falls on, so it has to differ from the clear
    /// colour behind it: a black shadow cannot darken a near-black background, which is why the
    /// dark variant puts the floor a few steps above the background instead of at it.
    pub floor: [f32; 4],
    /// The colour of an unlit line that names no colour of its own, in gamma space.
    pub line_ink: [f32; 4],
    /// The ink of the open product's envelope, in gamma space.
    pub product_envelope_active: [f32; 4],
    /// The ink of a closed, empty product's envelope, in gamma space.
    pub product_envelope_inactive: [f32; 4],
    /// The ink of the base an armed boolean is born in, in gamma space.
    ///
    /// The base is the operand the result replaces, and it is not a picked shape: a plain click on
    /// the tool drops it from the selection while it is still an operand, so it would lose its
    /// highlight and the picture would stop saying which ring the subtraction cuts into (X99). It
    /// takes an ink of its own, drawn beside the selection cyan on every picked operand.
    ///
    /// The hue is the one no other role claims: the selection is cyan, a guide and the unselected
    /// path are amber, a refusal is red and the grid axes are red and green. It is rose, and it is
    /// measured against both backgrounds and against every ink above rather than chosen
    /// (`crates/ui/src/theme.rs` owns the accessor and the floor, ADR 0219).
    pub boolean_base: [f32; 4],
    /// The ink of a guide the frame draws for the person rather than for the form, in gamma space.
    ///
    /// A guide is the program's own geometry — the bracket of an open level, a work plane — so it
    /// is screen paint and never document data. The chrome side of the same ink is
    /// `ui::theme::helper`; the two are one colour, pinned together by a test in `ui`,
    /// because `scene` cannot reach the palette (ADR 0111, sheet U83). The values are written as
    /// eight-bit steps divided by 255 so a reader can compare them with the palette without a
    /// rounding rule in between.
    pub helper: [f32; 4],
}

impl SceneColors {
    /// The dark variant the editor has drawn with since U38.
    pub const DARK: Self = Self {
        background: [0.10, 0.11, 0.13, 1.0],
        grid: Grid::DARK,
        floor: [0.17, 0.18, 0.21, 1.0],
        line_ink: [0.95, 0.72, 0.20, 1.0],
        product_envelope_active: [0.88, 0.64, 0.29, 1.0],
        product_envelope_inactive: [0.50, 0.54, 0.60, 1.0],
        boolean_base: [242.0 / 255.0, 115.0 / 255.0, 168.0 / 255.0, 1.0],
        helper: [226.0 / 255.0, 187.0 / 255.0, 109.0 / 255.0, 1.0],
    };

    /// The light variant: the same scene on paper instead of slate.
    pub const LIGHT: Self = Self {
        background: [0.87, 0.88, 0.90, 1.0],
        grid: Grid::LIGHT,
        floor: [0.93, 0.93, 0.94, 1.0],
        line_ink: [0.70, 0.44, 0.03, 1.0],
        product_envelope_active: [0.55, 0.33, 0.05, 1.0],
        product_envelope_inactive: [0.44, 0.47, 0.51, 1.0],
        boolean_base: [158.0 / 255.0, 46.0 / 255.0, 112.0 / 255.0, 1.0],
        helper: [104.0 / 255.0, 68.0 / 255.0, 11.0 / 255.0, 1.0],
    };
}

impl Default for SceneColors {
    fn default() -> Self {
        Self::DARK
    }
}

/// The shadow's strength and colour, beside the grid in a frame.
///
/// A shadow is the key light's, on two receivers: the floor plate takes the silhouette a body
/// projects onto it — not a blob under the body, so a body at any height casts one and a turned body
/// casts its own shape — and a face darkens where another face stands between it and the sun. A
/// change is read at draw time like [`ShadingStyle`], so turning the shadow off costs no upload.
///
/// The switch is the view's (U107): the shell's view leaf carries it, the renderer resolves it into
/// a per-view [`ContactShadows`], and so one cell of a split draws the shadow while its neighbour
/// does not. The strength and the colour stay workspace data, one value for every view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactShadows {
    /// Whether the pass draws at all.
    pub visible: bool,
    /// How far a shadow darkens what it falls on — the floor plate and the faces of a body — 0 to 1.
    ///
    /// One strength for both: the plate's silhouette and a panel's occlusion are cast by the same
    /// sun, so turning it down draws less of both instead of splitting the light in two (ADR 0143).
    pub strength: f32,
    /// Gamma-space RGB of the shadow, like [`Grid`] colours, because the pass is unlit.
    pub color: [f32; 3],
}

impl ContactShadows {
    /// Whether the shadow draws anything: the switch is on and the strength is above zero.
    pub fn draws(&self) -> bool {
        self.visible && self.strength.is_finite() && self.strength > 0.0
    }
}

impl Default for ContactShadows {
    fn default() -> Self {
        Self { visible: true, strength: 0.4, color: [0.0, 0.0, 0.0] }
    }
}

/// The largest number of point lights one frame can shade.
///
/// A frame has one directional key light and this many point lights, which keeps the uniform small
/// and the fragment loop bounded. Further point lights are stored in the document but do not light
/// the viewport.
pub const MAX_POINT_LIGHTS: usize = 8;

/// A point light in world space.
///
/// The power is in watts, following Blender's point-light unit: a light of 1000 W at one metre
/// shades a white surface at about half intensity under the viewport's fixed exposure. The
/// renderer applies inverse-square falloff and no hard range cutoff ([ADR 0039]).
///
/// [ADR 0039]: ../docs/decisions/0039-point-lights-use-watts-and-inverse-square-falloff.md
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointLight {
    /// World-space origin.
    pub position: DVec3,
    /// Linear RGB colour.
    pub color: [f32; 3],
    /// Radiant power in watts.
    pub power: f32,
}

/// The largest number of rectangular emitters one frame carries.
///
/// A window is one and a softbox is another; four keep the shader's light loop and the uniform small,
/// and a fifth is a scene that does not fit rather than an image that silently loses a source. The
/// document holds as many as a maker authors, and only the first four in tree order reach a frame.
pub const MAX_AREA_LIGHTS: usize = 4;

/// A rectangular emitter in world space: its face, its colour and its watts.
///
/// The face is the light's own `XY` plane centred on its origin, and the light emits from one side of
/// it, along its local `-Z` — the convention a sun already uses ([`Lighting::key_direction`]), so a
/// window lights the room and not the street (ADR 0247).
///
/// The frame carries watts and geometry, never a radiance: the conversion from emitted power to
/// radiance is one function here, [`AreaLight::radiance`], and the tests of it are the frame's own.
///
/// [ADR 0247]: ../docs/decisions/0247-a-rectangular-light-is-a-face-at-watts.md
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaLight {
    /// The centre of the face, in world millimetres.
    pub origin: DVec3,
    /// The half-edge along the light's own `X`, in world millimetres.
    pub half_width: DVec3,
    /// The half-edge along the light's own `Y`, in world millimetres.
    pub half_height: DVec3,
    /// Linear RGB colour.
    pub color: [f32; 3],
    /// Radiant power in watts, the unit a point light already uses (ADR 0039).
    pub power: f32,
}

impl AreaLight {
    /// The unit normal of the face the light emits from.
    ///
    /// It is derived and never stored: the face is the two half-edges, and their cross product in the
    /// ring's own order — the height edge into the width edge — is the side the light emits from, the
    /// same order [`AreaLight::corners`] writes. A second stored normal could disagree with the edge
    /// the shader samples, and did in the test that turned a window around (L2).
    pub fn normal(&self) -> DVec3 {
        self.half_height.cross(self.half_width).try_normalize().unwrap_or(DVec3::Z)
    }

    /// The area of the face in square millimetres, or 0 for a face with no extent.
    ///
    /// It is the length of the cross product of the two half-edges, doubled and doubled again, so a
    /// degenerate rectangle answers zero instead of a number that divides a radiance by nothing.
    pub fn area_mm2(&self) -> f64 {
        let area = 4.0 * self.half_width.cross(self.half_height).length();
        if area.is_finite() && area > 0.0 { area } else { 0.0 }
    }

    /// The radiance of the face: the watts spread over its area and its hemisphere.
    ///
    /// A Lambertian emitter of power `P` and area `A` radiates `P = L * PI * A`, so the radiance a
    /// tracer samples is `P / (PI * A)` in `W/(m^2 sr)`. The document's millimetres become square
    /// metres here, once, because that is the unit watts are defined in.
    pub fn radiance(&self) -> [f32; 3] {
        let area = self.area_mm2() * 1.0e-6;
        if area <= 0.0 || !self.power.is_finite() || self.power <= 0.0 {
            return [0.0; 3];
        }
        let scale = f64::from(self.power) / (std::f64::consts::PI * area);
        self.color.map(|channel| if channel.is_finite() { (f64::from(channel) * scale) as f32 } else { 0.0 })
    }

    /// The four corners of the face, in world millimetres, counter-clockwise seen from the emitting
    /// side.
    ///
    /// The viewport draws them and a picking test meets them, so the two readings of one rectangle
    /// come from one list.
    pub fn corners(&self) -> [DVec3; 4] {
        [
            self.origin - self.half_width - self.half_height,
            self.origin - self.half_width + self.half_height,
            self.origin + self.half_width + self.half_height,
            self.origin + self.half_width - self.half_height,
        ]
    }
}

/// The key light, point lights and ambient term used to shade solid drawables.
#[derive(Clone, Debug, PartialEq)]
pub struct Lighting {
    /// Unit direction from a surface towards the key light, in world space.
    pub key_direction: DVec3,
    /// Linear RGB intensity of the key light.
    pub key_color: [f32; 3],
    /// Fraction of the surface colour visible without direct light.
    pub ambient: f32,
    /// Point lights in tree order, at most [`MAX_POINT_LIGHTS`] of them when built from a scene.
    pub points: Vec<PointLight>,
    /// Rectangular emitters in tree order, at most [`MAX_AREA_LIGHTS`] of them when built from a
    /// scene.
    ///
    /// A viewport shades the model from [`Lighting::points`] and draws the faces; it is the final
    /// render that samples them, by solid angle, and that is why they are data here and not geometry
    /// in the soup.
    pub areas: Vec<AreaLight>,
    /// The radiance of an infinite, uniform environment, linear RGB and zero for none.
    ///
    /// This is the studio the workshop does not build: the light a final render sees where the frame
    /// has no geometry, which is how a shadow gets a value and a white page stays white instead of
    /// black. It is not the viewport's background colour — that is view state (ADR 0029) and never a
    /// radiance — and a frame that says nothing is black, which is what every view drew before.
    pub environment: [f32; 3],
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            key_direction: DVec3::new(-0.35, 0.55, 0.75).try_normalize().unwrap_or(DVec3::Z),
            key_color: [1.0, 0.97, 0.92],
            ambient: 0.32,
            points: Vec::new(),
            areas: Vec::new(),
            environment: [0.0; 3],
        }
    }
}

/// The physical surface a drawable is made of: what a tracer needs and a viewport ignores.
///
/// A viewport shades a drawable by its colour and its display style, and a final render has to know
/// more: a handle is a rough conductor and a glass front is a pane, and neither is a paint. The fields
/// are the tracer's own vocabulary — not the document's material record, which is a catalogue item
/// and is mapped onto this by the render that reads it (`plan/BACKLOG.md`, Y3).
///
/// Every value is clamped by the tracer where it is used, so a frame that carries a hostile number
/// shades as a repaired surface rather than poisoning an image (`crates/final/src/bsdf.rs`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    /// Microfacet roughness of the metal or of the pane, `0..=1`.
    ///
    /// A pane's width is both interfaces': it scatters what it reflects and what it transmits, so a
    /// glass front with a width is a frosted one (`crates/final/src/bsdf.rs`). A roughness of zero is
    /// the smooth interface float glass has, which is the picture every clear pane was before it.
    pub roughness: f32,
    /// `0` for a dielectric, `1` for a metal.
    pub metallic: f32,
    /// Index of refraction of the dielectric.
    pub ior: f32,
    /// How much of the beam crosses the surface, `0..=1`.
    pub transmission: f32,
    /// Beer-Lambert extinction inside the material, per millimetre, linear RGB.
    pub absorption: [f32; 3],
    /// The clear layer over the base, or [`Coat::NONE`] for a surface that is its base alone.
    pub coat: Coat,
}

impl Surface {
    /// Painted or veneered board: diffuse, opaque, dielectric, no finish over it.
    pub const MATTE: Self =
        Self { roughness: 1.0, metallic: 0.0, ior: 1.5, transmission: 0.0, absorption: [0.0; 3], coat: Coat::NONE };
}

/// The clear dielectric layer over a base: a lacquer, a melamine face, a gloss acrylic front.
///
/// A furniture catalogue sells a surface, not a base: a veneer under a 90 % lacquer is a picture of
/// its own, and ADR 0243 decides the shape — one base, at most one coat, never a stack. The frame
/// carries the layer the way the tracer reads it: a coverage, a GGX width and an index of refraction.
///
/// The gloss in percent is the number a workshop says and a catalogue stores; it becomes the width
/// here, once, where a record turns into a frame ([`Coat::glossed`]), so no shader sees a percent
/// and no record carries an alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coat {
    /// How much of the surface the layer covers, `0..=1`; zero is no coat at all.
    pub weight: f32,
    /// GGX alpha of the layer, `0..=1`: `(1 - gloss / 100)²`, so 90 % gloss is `0.01`.
    pub alpha: f32,
    /// The layer's own index of refraction.
    pub ior: f32,
}

impl Coat {
    /// The index of refraction of a clear lacquer: the glass index a finish reads as.
    pub const IOR: f32 = 1.5;

    /// No layer: the surface is its base alone, which is exactly a picture without a coat.
    pub const NONE: Self = Self { weight: 0.0, alpha: 1.0, ior: Self::IOR };

    /// A layer covering `weight` of the surface, glossed `gloss` percent, with its own `ior`.
    ///
    /// The gloss is the number a workshop says — 90 % is a lacquer, 50 % is a satin — and the width
    /// the tracer reads is V-Ray's and Cycles' `alpha = (1 - gloss / 100)²` (ADR 0243). The mapping
    /// lives here because this is the one place a record becomes a frame; a value outside its range
    /// is repaired rather than carried, the way every other value of a surface is.
    pub fn glossed(weight: f32, gloss: f32, ior: f32) -> Self {
        let weight = if weight.is_finite() { weight.clamp(0.0, 1.0) } else { 0.0 };
        let gloss = if gloss.is_finite() { gloss.clamp(0.0, 100.0) } else { 0.0 };
        let ior = if ior.is_finite() { ior.max(1.0) } else { Self::IOR };
        let roughness = 1.0 - gloss / 100.0;
        Self { weight, alpha: roughness * roughness, ior }
    }
}

impl Default for Surface {
    fn default() -> Self {
        Self::MATTE
    }
}

/// How a viewport draws one mesh's faces.
///
/// A body is shaded by the frame's lights; a light's own face is not, because it is the source
/// rather than a surface the source falls on. The two are one enum rather than a `bool` so a third
/// way of drawing a mesh has a name to take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FaceShading {
    /// Shaded by the frame's lights: every body in the model.
    #[default]
    Lit,
    /// The surface's own colour, blended over the scene and never darkened by a light: the face of a
    /// rectangular emitter, which is a source and not a board (L2).
    Source,
}

/// One shaded mesh in world space.
///
/// The mesh is shared so rebuilding a scene does not copy vertex data. `world_transform` is a
/// rigid transform composed from the assembly tree; the renderer applies it before uploading.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawable {
    /// The document object this mesh belongs to.
    pub id: Id,
    /// Local-space triangles.
    pub mesh: Arc<MeshData>,
    /// Boundary and crease edges of `mesh`, two vertex indices per edge.
    ///
    /// The list is derived beside the mesh and cached with it (ADR 0053), so the edge pass costs no
    /// search per frame. `None` means the drawable has no edge pass.
    pub edges: Option<Arc<[u32]>>,
    /// Every triangle edge of `mesh`, triangulation diagonals included.
    ///
    /// Only the optional *all surface edges* style reads it; the default edge pass uses [`Self::edges`].
    pub all_edges: Option<Arc<[u32]>>,
    /// Rigid local-to-world transform.
    pub world_transform: DMat4,
    /// Linear RGB colour and opacity.
    pub color: [f32; 4],
    /// What the surface is made of, which a final render shades by and a viewport ignores.
    ///
    /// A frame that says nothing is [`Surface::MATTE`]: the paint every drawable was before a tracer
    /// could tell a handle from a board.
    pub surface: Surface,
    /// The decor the surface is faced with, or `None` for the flat colour of [`Self::color`].
    ///
    /// The picture multiplies the colour, which is the arithmetic every renderer shares: the colour
    /// tints the print, and a record with no decor is the colour alone. Both consumers read this one
    /// value — the viewport samples it in its fragment and the tracer carries it into its own
    /// shader — because a material the two disagree about is two materials.
    pub decor: Option<Decor>,
    /// The box around the mesh in its own frame, as the mesh cache measured it (X56).
    ///
    /// `None` means the box was never measured, not that the drawable is empty: a view keeps a
    /// drawable whose extent is unknown. [`Drawable::world_bounds`] turns it into the box a view
    /// culls by (X58).
    pub local_bounds: Option<Aabb>,
    /// How the viewport draws the faces: lit like a body, or as a source.
    pub faces: FaceShading,
}

impl Drawable {
    /// Creates a drawable from a document id, a mesh, a rigid world transform and a colour.
    ///
    /// The box around the mesh is measured here, which walks every vertex: a mesh that came from
    /// the evaluation cache carries its box already, and [`Drawable::with_box`] takes it instead.
    /// The drawable starts with no edge lists; [`Drawable::with_edges`] and
    /// [`Drawable::with_all_edges`] add them.
    pub fn new(id: Id, mesh: Arc<MeshData>, world_transform: DMat4, color: [f32; 4]) -> Self {
        let local_bounds = mesh.local_bounds();
        Self::with_box(id, mesh, world_transform, color, local_bounds)
    }

    /// Creates a drawable with the box the mesh cache measured beside the mesh, in the mesh's frame.
    ///
    /// This is the evaluation path: a rebuild copies the box it holds instead of measuring the
    /// mesh again, so a dense frame costs eight corners per drawable and never a vertex (X56).
    pub fn with_box(
        id: Id,
        mesh: Arc<MeshData>,
        world_transform: DMat4,
        color: [f32; 4],
        local_bounds: Option<Aabb>,
    ) -> Self {
        Self {
            id,
            mesh,
            edges: None,
            all_edges: None,
            world_transform,
            color,
            surface: Surface::MATTE,
            decor: None,
            local_bounds,
            faces: FaceShading::Lit,
        }
    }

    /// The same drawable drawn as a source: its colour is the light it gives, not a surface under a
    /// light, and the viewport blends it over the scene instead of shading it.
    pub fn as_source(mut self) -> Self {
        self.faces = FaceShading::Source;
        self
    }

    /// The same drawable made of `surface`, which is how a frame says a front is glass or a handle is
    /// metal without a second constructor for every joint of the two.
    pub fn with_surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    /// The same drawable faced with `decor`, which is the board a panel is cut from.
    pub fn with_decor(mut self, decor: Decor) -> Self {
        self.decor = Some(decor);
        self
    }

    /// The box around the drawable in world space, or `None` when its extent was never measured.
    pub fn world_bounds(&self) -> Option<Aabb> {
        self.local_bounds.map(|local| local.transformed_by(self.world_transform))
    }

    /// Attaches the boundary and crease edge list derived from `mesh`.
    pub fn with_edges(mut self, edges: Arc<[u32]>) -> Self {
        self.edges = Some(edges);
        self
    }

    /// Attaches the full triangulation edge list derived from `mesh`.
    pub fn with_all_edges(mut self, all_edges: Arc<[u32]>) -> Self {
        self.all_edges = Some(all_edges);
        self
    }
}

/// A camera's view volume, as the six half-spaces a view culls the frame against.
///
/// The volume is read from the same view-projection matrix the shader receives, so one rule holds
/// for a perspective and an orthographic view, and the near and far planes are the camera's own (one
/// millimetre in front of the eye, [`MAX_DISTANCE_MM`] away). A box that straddles a plane is kept:
/// a box slightly behind the eye can still reach into the picture, so only a box entirely outside
/// the volume is dropped (X58).
///
/// The value is derived per view and never stored: a camera move changes it, and it is not document
/// data (ADR 0029).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum {
    /// The six planes, or `None` when the matrix was degenerate: a volume that cannot be measured
    /// keeps every drawable rather than hiding the scene.
    planes: Option<[Plane; 6]>,
}

/// One half-space of a view volume: a point is inside when `normal · point + offset >= 0`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Plane {
    normal: DVec3,
    offset: f64,
}

impl Frustum {
    /// The view volume of a camera seen through a viewport of the given aspect ratio.
    pub fn of_camera(camera: Camera, aspect: f64) -> Self {
        Self { planes: planes_of(camera.view_projection(aspect)) }
    }

    /// Whether any part of `bounds` lies inside the volume.
    ///
    /// The test is the box's farthest corner along each plane's normal: when that corner is on the
    /// outside of one plane the whole box is, because no other corner reaches further that way.
    pub fn meets(&self, bounds: Aabb) -> bool {
        let Some(planes) = self.planes.as_ref() else {
            return true;
        };
        let (min, max) = (bounds.min(), bounds.max());
        if !min.is_finite() || !max.is_finite() {
            return true;
        }
        planes.iter().all(|plane| {
            let normal = plane.normal;
            let far = DVec3::new(
                if normal.x >= 0.0 { max.x } else { min.x },
                if normal.y >= 0.0 { max.y } else { min.y },
                if normal.z >= 0.0 { max.z } else { min.z },
            );
            normal.dot(far) + plane.offset >= 0.0
        })
    }

    /// Replaces `out` with the position of every box the volume meets, in input order.
    ///
    /// A box that was never measured (`None`) is kept: an unknown extent is not an empty one. The
    /// caller owns `out` because the passes of one view read the same list twice — once for faces
    /// and once for the edges that belong to them (X58).
    pub fn visible(&self, boxes: impl IntoIterator<Item = Option<Aabb>>, out: &mut Vec<usize>) {
        out.clear();
        for (index, bounds) in boxes.into_iter().enumerate() {
            if bounds.is_none_or(|bounds| self.meets(bounds)) {
                out.push(index);
            }
        }
    }
}

/// The six planes of a view-projection matrix, or `None` when one of them is degenerate.
///
/// Clip depth runs from `0` at the near plane to `1` at the far one, so the near plane is the row of
/// `z` rather than the usual `w + z` of a `-1..1` depth range.
fn planes_of(matrix: DMat4) -> Option<[Plane; 6]> {
    let (w, x, y, z) = (matrix.row(3), matrix.row(0), matrix.row(1), matrix.row(2));
    let rows = [w + x, w - x, w + y, w - y, z, w - z];
    let mut planes = [Plane { normal: DVec3::ZERO, offset: 0.0 }; 6];
    for (plane, row) in planes.iter_mut().zip(rows) {
        *plane = Plane::from_row(row)?;
    }
    Some(planes)
}

impl Plane {
    /// The unit-normal form of one clip row, or `None` when it has no direction to normalize.
    fn from_row(row: DVec4) -> Option<Self> {
        if !row.is_finite() {
            return None;
        }
        let normal = row.truncate();
        let length = normal.length();
        if !length.is_finite() || length <= 0.0 {
            return None;
        }
        Some(Self { normal: normal / length, offset: row.w / length })
    }
}

/// One unlit polyline in world space.
///
/// A path is drawn as a chain of line segments, not a solid: it has no faces and no shading. The
/// points are local-space and `world_transform` is the rigid transform of the node that owns the
/// path, exactly like [`Drawable`], so a drag preview moves the matrix instead of copying the
/// points. The colour is gamma-space, like [`Grid`] colours, because the line pipeline is unlit.
#[derive(Clone, Debug, PartialEq)]
pub struct Polyline {
    /// The document object this chain belongs to.
    pub id: Id,
    /// Local-space points in travel order; at least two.
    pub points: Arc<[DVec3]>,
    /// Rigid local-to-world transform.
    pub world_transform: DMat4,
    /// Gamma-space RGB and opacity, or `None` for the frame's own line ink.
    ///
    /// [`Polyline::in_ink`] leaves the colour to [`SceneFrame::line_ink`], so a line that carries
    /// no colour of its own follows the active theme variant without a rebuild.
    pub color: Option<[f32; 4]>,
}

impl Polyline {
    /// Creates a polyline from a document id, local points, a rigid world transform and a colour.
    pub fn new(id: Id, points: Arc<[DVec3]>, world_transform: DMat4, color: [f32; 4]) -> Self {
        Self { id, points, world_transform, color: Some(color) }
    }

    /// Creates a polyline that the frame paints in its own ink.
    ///
    /// The chain has no colour of its own, so the renderer draws it with [`SceneFrame::line_ink`]
    /// and a theme switch repaints it without touching the frame's geometry.
    pub fn in_ink(id: Id, points: Arc<[DVec3]>, world_transform: DMat4) -> Self {
        Self { id, points, world_transform, color: None }
    }

    /// The colour this chain draws with: its own, or the frame's line ink.
    pub fn drawn_color(&self, line_ink: [f32; 4]) -> [f32; 4] {
        self.color.unwrap_or(line_ink)
    }
}

/// What a product envelope says about its product: the three states ADR 0100 draws differently.
///
/// The state is session data, never document data: opening and closing a product is a view
/// operation (ADR 0098), so the scene carries the answer and the file never does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductEnvelopeState {
    /// The product is the one open piece: its lines take the active ink.
    Open,
    /// The product is closed and draws nothing yet: its lines take the inactive ink.
    ClosedEmpty,
    /// The product is closed and already speaks for itself: no lines at all.
    ClosedWithParts,
}

impl ProductEnvelopeState {
    /// Whether the state draws the envelope.
    pub const fn draws(self) -> bool {
        !matches!(self, Self::ClosedWithParts)
    }
}

/// One product's authored envelope in world space: the space it reserves, never a body.
///
/// The document stores the footprint and the height in the product's own frame (ADR 0097, ADR 0159),
/// so the scene carries the footprint as a **base outline** in local millimetres plus the rigid
/// local-to-world transform: the renderer dashes the two rings and a vertical at each corner, and
/// picking casts the ray into the product frame and tests the outline's prism. The outline is the
/// shape the frame really reserves — a rectangle, a rectangle with a cut corner or a drawn ring —
/// and `x`/`y` are only its bounds. The envelope is picked even when its lines are hidden, which is
/// how a closed product with parts stays reachable (ADR 0100).
///
/// The points are the outline's own polyline in its travel order and are closed implicitly; a caller
/// that has a contour samples it and says which of the samples are corners.
#[derive(Clone, Debug, PartialEq)]
pub struct ProductEnvelope {
    /// The product container this envelope belongs to.
    pub id: Id,
    /// The base outline in the product's own frame, in millimetres, counter-clockwise.
    pub outline: Arc<[DVec2]>,
    /// The outline's corners: the points where a vertical edge stands.
    ///
    /// An arc is sampled into many points but is smooth, so it draws no vertical edge; a corner of the
    /// shape is one line up the side of the reserved space.
    pub corners: Arc<[DVec2]>,
    /// The height of the reserved space, in millimetres.
    pub height: f64,
    /// Rigid local-to-world transform of the product.
    pub world_transform: DMat4,
    /// Open, closed and empty, or closed with parts.
    pub state: ProductEnvelopeState,
}

impl ProductEnvelope {
    /// One product's envelope with the plain rectangular footprint its three sides describe.
    ///
    /// The shape a frame reserves is normally derived from the document's footprint (ADR 0159); this
    /// is the rectangle case, which is also what a caller that has only three sides can state.
    pub fn rectangular(id: Id, size: DVec3, world_transform: DMat4, state: ProductEnvelopeState) -> Self {
        let outline =
            [DVec2::new(0.0, 0.0), DVec2::new(size.x, 0.0), DVec2::new(size.x, size.y), DVec2::new(0.0, size.y)];
        Self { id, outline: Arc::from(outline), corners: Arc::from(outline), height: size.z, world_transform, state }
    }

    /// The volume of the reserved space in cubic millimetres, or `None` when it is not usable.
    ///
    /// Picking uses it to prefer the innermost product when several envelopes contain the point:
    /// the tightest space is the one the maker means. The area is the outline's own shoelace area,
    /// not its bounding rectangle, so a cut corner really is a smaller piece. A non-finite, empty or
    /// flat outline has no volume, so a hostile frame never wins a pick.
    fn volume(&self) -> Option<f64> {
        let volume = self.footprint_area()? * self.height;
        (volume.is_finite() && volume > 0.0).then_some(volume)
    }

    /// The area the base outline encloses, in square millimetres, or `None` when it is not a ring.
    fn footprint_area(&self) -> Option<f64> {
        if !self.height.is_finite() || self.height <= 0.0 || self.outline.len() < 3 {
            return None;
        }
        if self.outline.iter().any(|point| !point.is_finite()) {
            return None;
        }
        let mut twice = 0.0;
        for index in 0..self.outline.len() {
            let start = self.outline.get(index)?;
            let next = self.outline.get((index + 1) % self.outline.len())?;
            twice += start.x * next.y - next.x * start.y;
        }
        let area = twice.abs() / 2.0;
        (area.is_finite() && area > 0.0).then_some(area)
    }

    /// The local box the reserved space lives in: the outline's bounds and `0..height`.
    ///
    /// This is the cheap first test picking and drawing both make; the shape itself is the outline,
    /// so the box is only ever a rejection and never the answer.
    pub fn local_bounds(&self) -> Option<(DVec3, DVec3)> {
        if !self.height.is_finite() || self.height <= 0.0 || self.outline.len() < 3 {
            return None;
        }
        let mut min = DVec2::splat(f64::INFINITY);
        let mut max = DVec2::splat(f64::NEG_INFINITY);
        for point in self.outline.iter() {
            if !point.is_finite() {
                return None;
            }
            min = min.min(*point);
            max = max.max(*point);
        }
        Some((DVec3::new(min.x, min.y, 0.0), DVec3::new(max.x, max.y, self.height)))
    }
}

/// Everything needed to draw and light one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneFrame {
    /// The active orbit camera.
    pub camera: Camera,
    /// How the drawables are shaded.
    pub shading: ShadingStyle,
    /// Whether the edge pass draws every triangle edge instead of only the form edges.
    ///
    /// A diagnostic view state beside [`SceneFrame::shading`], never document data.
    pub all_edges: bool,
    /// How strongly the form-outline pass draws, from 0 to 1.
    ///
    /// The renderer multiplies the outline ink's alpha by it, so the outline fades into the faces it
    /// traces and a strength of zero leaves the fill alone. Workspace data, read at draw time.
    pub edge_strength: f32,
    /// The floor grid.
    pub grid: Grid,
    /// The shadow's strength and colour, under the active view's switch.
    ///
    /// The strength and the colour are workspace data and the two masks the renderer builds — the
    /// ground's coverage and the key light's depth map — are derived from them; the *switch* belongs
    /// to the view, so a split workspace can draw it in one cell and not in the next (U107). This
    /// value is the fallback for a caller that draws the frame whole and follows the active leaf,
    /// exactly like [`SceneFrame::shading`].
    pub contact_shadows: ContactShadows,
    /// The colour of the floor plate, in gamma space: the surface the shadow falls on.
    ///
    /// The plate is the square the grid draws (see [`Grid::extent_mm`]) at `z = 0`, drawn
    /// single-sided so a view from below keeps the model. It is view state, never document data:
    /// the ground of a workspace, like the grid, not a board on the floor.
    pub floor: [f32; 4],
    /// How opaque the floor plate is, from 0 to 1, read at draw time.
    ///
    /// One is the opaque floor of ADR 0142; below one the plate is blended over what stands under
    /// it, so a part below the floor is seen *through* the plate's tint instead of beside it. A
    /// shadow needs a surface to darken, so zero draws neither the plate nor its shadow (U109,
    /// ADR 0144). Workspace data like the display strengths, never document data.
    pub floor_opacity: f32,
    /// The key light.
    pub lighting: Lighting,
    /// The clear colour, in gamma space.
    pub background: [f32; 4],
    /// The ink of a line chain that names no colour of its own, in gamma space.
    pub line_ink: [f32; 4],
    /// The ink of an open product's envelope, in gamma space.
    pub product_envelope_active: [f32; 4],
    /// The ink of a closed, empty product's envelope, in gamma space.
    pub product_envelope_inactive: [f32; 4],
    /// The ink of a guide the frame draws, in gamma space (see [`SceneColors::helper`]).
    pub helper: [f32; 4],
    /// The ink of the base an armed boolean is born in, in gamma space (see
    /// [`SceneColors::boolean_base`]).
    pub boolean_base: [f32; 4],
    /// Shaded meshes in world space.
    pub drawables: Vec<Drawable>,
    /// Unlit line chains in world space.
    pub polylines: Vec<Polyline>,
    /// The authored envelopes of the products, drawn as dashes and picked as boxes.
    pub product_envelopes: Vec<ProductEnvelope>,
}

impl Default for SceneFrame {
    fn default() -> Self {
        let colors = SceneColors::DARK;
        Self {
            camera: Camera::default(),
            shading: ShadingStyle::default(),
            all_edges: false,
            edge_strength: 1.0,
            grid: colors.grid,
            contact_shadows: ContactShadows::default(),
            floor: colors.floor,
            floor_opacity: 1.0,
            lighting: Lighting::default(),
            background: colors.background,
            line_ink: colors.line_ink,
            product_envelope_active: colors.product_envelope_active,
            product_envelope_inactive: colors.product_envelope_inactive,
            helper: colors.helper,
            boolean_base: colors.boolean_base,
            drawables: Vec::new(),
            polylines: Vec::new(),
            product_envelopes: Vec::new(),
        }
    }
}

impl SceneFrame {
    /// Paints the frame's non-material colours from `colors`, leaving every material colour alone.
    ///
    /// The background, the grid inks, the line ink and the guide ink are display state, so this is
    /// how a theme variant reaches a frame that is already built: no geometry is evaluated, no
    /// document revision moves and the furniture keeps its own colours. The grid's switch, spacing, size
    /// and major count stay with the viewport, so a variant change never turns a hidden grid on.
    pub fn apply_colors(&mut self, colors: SceneColors) {
        self.background = colors.background;
        self.grid.minor_color = colors.grid.minor_color;
        self.grid.major_color = colors.grid.major_color;
        self.grid.axis_x_color = colors.grid.axis_x_color;
        self.grid.axis_y_color = colors.grid.axis_y_color;
        self.floor = colors.floor;
        self.line_ink = colors.line_ink;
        self.product_envelope_active = colors.product_envelope_active;
        self.product_envelope_inactive = colors.product_envelope_inactive;
        self.helper = colors.helper;
        self.boolean_base = colors.boolean_base;
    }

    /// Adds a drawable to the frame.
    pub fn push(&mut self, drawable: Drawable) {
        self.drawables.push(drawable);
    }

    /// Adds an unlit line chain to the frame.
    pub fn push_polyline(&mut self, polyline: Polyline) {
        self.polylines.push(polyline);
    }

    /// Adds a product envelope to the frame.
    pub fn push_product_envelope(&mut self, envelope: ProductEnvelope) {
        self.product_envelopes.push(envelope);
    }

    /// The frame's drawables.
    pub fn drawables(&self) -> &[Drawable] {
        &self.drawables
    }

    /// The frame's unlit line chains.
    pub fn polylines(&self) -> &[Polyline] {
        &self.polylines
    }

    /// The frame's product envelopes.
    pub fn product_envelopes(&self) -> &[ProductEnvelope] {
        &self.product_envelopes
    }

    /// The document id of the nearest drawable or line chain hit by `ray`.
    ///
    /// A drawable is hit when one of its triangles crosses the ray. A line chain has no thickness,
    /// so it is hit when the ray passes within [`LINE_PICK_RADIUS_POINTS`] points of a segment at
    /// that segment's depth; `viewport_height` is the viewport's height in points, and a
    /// non-positive or non-finite one accepts only an exact crossing. A path wins a tie with a
    /// solid it lies on, so a contour drawn on a face stays selectable.
    ///
    /// A product envelope is picked by its whole box, not only by its lines: a click on empty space
    /// inside the piece selects the piece. It loses to every solid and every line chain, so a click
    /// through the box still reaches the panel behind it, and the tightest box wins when several
    /// envelopes contain the point, which is how a product standing in the void of another stays
    /// reachable (ADR 0100). Two equally tight boxes tie to the nearer one; a closed product with
    /// parts draws no lines but is still picked by its interior.
    ///
    /// Malformed vertices, indices, transforms and envelopes are skipped. Picking returns a document
    /// id, never a mesh index or a pointer: the caller can select it or send it to a command.
    pub fn pick(&self, ray: Ray, viewport_height: f64) -> Option<Id> {
        if let Some(id) = self.pick_surface(ray, viewport_height) {
            return Some(id);
        }
        // Only an unoccluded click reaches an envelope, and then the innermost product wins: the
        // smallest box containing the point is the piece the maker is pointing at. Two equally
        // tight boxes tie to the nearer one, so the answer never depends on tree order.
        let mut innermost: Option<(f64, f64, Id)> = None;
        for envelope in &self.product_envelopes {
            let Some(volume) = envelope.volume() else {
                continue;
            };
            let Some(entry) = ray_hit_product_envelope(ray, envelope) else {
                continue;
            };
            let nearer = innermost.is_none_or(|(best_volume, best_entry, _)| {
                volume < best_volume || (volume == best_volume && entry < best_entry)
            });
            if nearer {
                innermost = Some((volume, entry, envelope.id));
            }
        }
        innermost.map(|(_, _, id)| id)
    }

    /// The document id of the nearest drawable or line chain hit by `ray`, product envelopes aside.
    ///
    /// This is the shape under the pointer and nothing else, which is what a rubber band asks before
    /// it starts: a piece's envelope is its container's own target, not a shape, so the empty part of
    /// an open piece is still empty space a rectangle can be dragged in (ADR 0100, ADR 0137).
    ///
    /// A line chain has no thickness, so it is hit when the ray passes within
    /// [`LINE_PICK_RADIUS_POINTS`] points of a segment at that segment's depth; `viewport_height` is
    /// the viewport's height in points. A path wins a tie with a solid it lies on. Malformed
    /// vertices, indices and transforms are skipped.
    ///
    /// A drawable is cast only when the ray reaches the box its mesh cache measured, and not at all
    /// when that box begins behind a nearer hit, so a click costs a small multiple of the frame's
    /// objects rather than of its triangles (X59). A drawable whose box was never measured is cast
    /// regardless.
    pub fn pick_surface(&self, ray: Ray, viewport_height: f64) -> Option<Id> {
        let mut nearest: Option<(f64, Id)> = None;
        for drawable in &self.drawables {
            let limit = nearest.map_or(f64::INFINITY, |(best, _)| best);
            let Some(distance) = ray_hit_drawable(ray, drawable, limit) else {
                continue;
            };
            if nearest.is_none_or(|(best, _)| distance < best) {
                nearest = Some((distance, drawable.id));
            }
        }
        for polyline in &self.polylines {
            let Some(distance) = ray_hit_polyline(ray, polyline, self.camera, viewport_height) else {
                continue;
            };
            if nearest.is_none_or(|(best, _)| distance <= best) {
                nearest = Some((distance, polyline.id));
            }
        }
        nearest.map(|(_, id)| id)
    }

    /// A copy of the frame with every selected drawable and line chain highlighted for picking
    /// feedback.
    ///
    /// The base frame keeps its material colours, so clearing the selection restores them without
    /// rebuilding meshes. Passing the panels below a selected container highlights the container as
    /// one object. A line chain takes [`SELECTION_LINE_COLOR`] explicitly: a tint is invisible on a
    /// one-pixel line, and the shell widens it as an overlay.
    pub fn with_selection_highlight(&self, selected: &[Id]) -> Self {
        self.with_picks(selected, None)
    }

    /// A copy of the frame with the picks highlighted and, when a boolean is armed, its base in the
    /// frame's own [base ink](SceneColors::boolean_base).
    ///
    /// A boolean is a mode: the target it is born in is the base, and every other picked node joins
    /// as an operand. The base is often *not* in the selection — a plain click on the tool replaces
    /// it — so it is drawn from `base` rather than from `selected`, and the two roles keep two inks
    /// (X99).
    pub fn with_picks(&self, selected: &[Id], base: Option<Id>) -> Self {
        let mut frame = self.clone();
        if selected.is_empty() && base.is_none() {
            return frame;
        }
        for drawable in &mut frame.drawables {
            if Some(drawable.id) == base {
                tint(&mut drawable.color, frame.boolean_base, SELECTION_MIX);
            } else if selected.contains(&drawable.id) {
                tint(&mut drawable.color, SELECTION_TINT, SELECTION_MIX);
            }
        }
        for polyline in &mut frame.polylines {
            if Some(polyline.id) == base {
                polyline.color = Some(frame.boolean_base);
            } else if selected.contains(&polyline.id) {
                polyline.color = Some(SELECTION_LINE_COLOR);
            }
        }
        frame
    }

    /// A copy of the frame with the given drawables and line chains shifted by a world-space
    /// translation.
    ///
    /// This is a drag preview: it moves the listed primitives without touching the document, so the
    /// renderer can follow the pointer before one command commits the result. A non-finite delta
    /// leaves the frame unchanged.
    pub fn with_world_translation(&self, ids: &[Id], delta: DVec3) -> Self {
        let mut frame = self.clone();
        if ids.is_empty() || !delta.is_finite() {
            return frame;
        }
        let translation = DMat4::from_translation(delta);
        for drawable in &mut frame.drawables {
            if ids.contains(&drawable.id) {
                drawable.world_transform = translation * drawable.world_transform;
            }
        }
        for polyline in &mut frame.polylines {
            if ids.contains(&polyline.id) {
                polyline.world_transform = translation * polyline.world_transform;
            }
        }
        for envelope in &mut frame.product_envelopes {
            if ids.contains(&envelope.id) {
                envelope.world_transform = translation * envelope.world_transform;
            }
        }
        frame
    }

    /// A copy of the frame with the given drawables and line chains rotated about a world-space
    /// pivot.
    ///
    /// This is a drag preview: it turns the listed primitives without touching the document, so the
    /// renderer can follow the pointer before one command commits the result. The rotation must be a
    /// finite unit quaternion and the pivot finite; anything else leaves the frame unchanged.
    pub fn with_world_rotation(&self, ids: &[Id], pivot: DVec3, rotation: DQuat) -> Self {
        let mut frame = self.clone();
        if ids.is_empty() || !pivot.is_finite() || !rotation.is_finite() || (rotation.length() - 1.0).abs() > 1.0e-9 {
            return frame;
        }
        let about_pivot = DMat4::from_translation(pivot) * DMat4::from_quat(rotation) * DMat4::from_translation(-pivot);
        for drawable in &mut frame.drawables {
            if ids.contains(&drawable.id) {
                drawable.world_transform = about_pivot * drawable.world_transform;
            }
        }
        for polyline in &mut frame.polylines {
            if ids.contains(&polyline.id) {
                polyline.world_transform = about_pivot * polyline.world_transform;
            }
        }
        for envelope in &mut frame.product_envelopes {
            if ids.contains(&envelope.id) {
                envelope.world_transform = about_pivot * envelope.world_transform;
            }
        }
        frame
    }

    /// A copy of the frame with one drawable's local mesh replaced.
    ///
    /// This is a drag preview for sub-object editing: the shell builds a mesh with the moved
    /// vertices and swaps it in for one frame, so the document and the renderer cache stay untouched
    /// until the command commits. An unknown id leaves the frame unchanged.
    pub fn with_drawable_mesh(&self, id: Id, mesh: Arc<MeshData>) -> Self {
        let mut frame = self.clone();
        for drawable in &mut frame.drawables {
            if drawable.id == id {
                drawable.mesh = mesh.clone();
            }
        }
        frame
    }
}

/// Linear RGB colour mixed into the selected drawable.
const SELECTION_TINT: [f32; 4] = [1.0, 0.55, 0.15, 1.0];

/// Fraction of a [tint](SELECTION_TINT) or a [base ink](SceneColors::boolean_base) mixed into the
/// base colour of a body.
const SELECTION_MIX: f64 = 0.35;

/// Mixes the RGB of `tint` into an RGB colour, leaving the alpha channel untouched.
fn tint(color: &mut [f32; 4], tint: [f32; 4], mix: f64) {
    for (channel, tint) in color.iter_mut().zip(tint.into_iter().take(3)) {
        let base = f64::from(*channel);
        *channel = (base * (1.0 - mix) + f64::from(tint) * mix) as f32;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use std::f64::consts::{FRAC_PI_2, TAU};

    use geom::DVec4;
    use model::Id;

    fn assert_close(actual: DVec3, expected: DVec3) {
        assert!((actual - expected).length() < 1.0e-9, "actual {actual:?}, expected {expected:?}");
    }

    /// The relative luminance of a gamma-space colour: enough to tell paper from slate.
    fn luminance(color: [f32; 4]) -> f32 {
        0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2]
    }

    #[test]
    fn a_display_style_is_frame_data_with_a_default() {
        assert_eq!(SceneFrame::default().shading, ShadingStyle::ShadedWithEdges);
        assert_eq!(ShadingStyle::ALL.len(), 5, "the cluster offers every style the renderer interprets");
        assert!(ShadingStyle::Wireframe.draws_edges() && !ShadingStyle::Wireframe.draws_faces());
        assert!(!ShadingStyle::Translucent.draws_edges() && ShadingStyle::Translucent.is_translucent());
        assert!(!ShadingStyle::Shaded.draws_edges() && ShadingStyle::Shaded.draws_faces());
        assert!(ShadingStyle::ShadedWithHiddenEdges.edges_through_faces());
        assert!(!ShadingStyle::ShadedWithEdges.edges_through_faces());
    }

    #[test]
    fn the_floor_plate_is_a_surface_a_shadow_can_darken() {
        for colors in [SceneColors::DARK, SceneColors::LIGHT] {
            let background = luminance(colors.background);
            let floor = luminance(colors.floor);
            assert!(
                (floor - background).abs() > 0.02,
                "the floor reads as its own surface: {floor} against {background}"
            );
            let shadow = colors.floor.map(|channel| channel * (1.0 - 0.4));
            assert!(
                (luminance(shadow) - floor).abs() > 0.02,
                "an ordinary strength darkens the plate enough to see: {} against {floor}",
                luminance(shadow)
            );
        }
    }

    #[test]
    fn a_frame_paints_the_floor_with_the_variant_it_is_given() {
        let mut frame = SceneFrame::default();
        assert_eq!(frame.floor, SceneColors::DARK.floor, "the default frame is the dark variant");
        frame.apply_colors(SceneColors::LIGHT);
        assert_eq!(frame.floor, SceneColors::LIGHT.floor, "a theme switch repaints the ground");
        assert!(frame.grid.extent_mm > 0.0, "the plate spans the square the grid draws");
    }

    #[test]
    fn the_floor_is_opaque_until_the_workspace_says_otherwise() {
        let mut frame = SceneFrame::default();
        assert_eq!(frame.floor_opacity, 1.0, "a frame that says nothing draws the floor as it always did");
        frame.floor_opacity = 0.4;
        frame.apply_colors(SceneColors::LIGHT);
        assert_eq!(frame.floor_opacity, 0.4, "a theme repaints the ground without making it solid again");
    }

    #[test]
    fn the_contact_shadow_switch_removes_the_shadow() {
        let mut frame = SceneFrame::default();
        assert!(frame.contact_shadows.draws(), "a frame draws the shadow by default");
        frame.contact_shadows.visible = false;
        assert!(!frame.contact_shadows.draws(), "the switch removes the shadow from the frame");
        frame.contact_shadows.visible = true;
        frame.contact_shadows.strength = 0.0;
        assert!(!frame.contact_shadows.draws(), "a zero strength draws nothing either");
    }

    #[test]
    fn default_camera_orbits_at_a_finite_distance() {
        let camera = Camera::default();
        assert!(camera.eye().is_finite());
        assert!(((camera.eye() - camera.target()).length() - camera.distance()).abs() < 1.0e-9);
        assert!((camera.forward().length() - 1.0).abs() < 1.0e-12);
        assert!((camera.right().dot(camera.up())).abs() < 1.0e-12);
    }

    #[test]
    fn camera_rejects_hostile_constructor_input() {
        assert!(matches!(Camera::new(DVec3::new(f64::NAN, 0.0, 0.0), 1000.0), Err(CameraError::NonFiniteTarget)));
        assert!(matches!(Camera::new(DVec3::ZERO, f64::INFINITY), Err(CameraError::InvalidDistance { .. })));
        assert!(matches!(Camera::new(DVec3::ZERO, MIN_DISTANCE_MM * 0.5), Err(CameraError::InvalidDistance { .. })));
    }

    #[test]
    fn camera_looks_at_the_target() {
        let camera = Camera::new(DVec3::new(10.0, 20.0, 30.0), 1000.0).expect("valid camera");
        let eye = camera.eye();
        let view = camera.view_matrix();
        assert_close(view.transform_point3(eye), DVec3::ZERO);
        assert_close(view.transform_point3(camera.target()), DVec3::new(0.0, 0.0, -1000.0));
    }

    #[test]
    fn orbit_wraps_yaw_and_clamps_pitch() {
        let mut camera = Camera::default();
        camera.orbit(TAU + 0.25, 100.0);
        assert!((0.0..TAU).contains(&camera.yaw()));
        assert_eq!(camera.pitch(), MAX_PITCH);
        camera.orbit(0.0, -10_000.0);
        assert_eq!(camera.pitch(), MIN_PITCH);
        let yaw = camera.yaw();
        camera.orbit(f64::NAN, 0.1);
        assert_eq!(camera.yaw(), yaw);
    }

    #[test]
    fn zoom_clamps_and_ignores_invalid_factors() {
        let mut camera = Camera::default();
        camera.zoom(0.5);
        assert_eq!(camera.distance(), 600.0);
        camera.zoom(1.0e12);
        assert_eq!(camera.distance(), MAX_DISTANCE_MM);
        camera.zoom(1.0e-12);
        assert_eq!(camera.distance(), MIN_DISTANCE_MM);
        let distance = camera.distance();
        camera.zoom(f64::NAN);
        camera.zoom(0.0);
        assert_eq!(camera.distance(), distance);
    }

    #[test]
    fn a_zoom_about_a_point_leaves_that_point_on_screen() {
        // X67: the wheel zooms towards the pointer. A zoom about the target slides everything the
        // maker is looking at out from under the pointer — a carried piece leaves the hand on the
        // first wheel step — so the anchor keeps its own place in both projections.
        for orthographic in [false, true] {
            let mut camera = Camera::default();
            camera.orbit(0.6, 0.35);
            if orthographic {
                camera.set_orthographic();
            }
            let anchor = camera.target() + camera.right() * 400.0 + camera.up() * 220.0;
            let before = camera.project_ndc(anchor, 1.6).expect("the anchor is on screen");
            for factor in [0.5, 0.8, 1.0, 1.4, 2.0] {
                let mut zoomed = camera;
                zoomed.zoom_about(factor, Some(anchor));
                let after = zoomed.project_ndc(anchor, 1.6).expect("the anchor is still on screen");
                assert!(
                    (after - before).length() < 1.0e-9,
                    "a zoom by {factor} keeps the point: {after:?} from {before:?}"
                );
                assert_eq!(zoomed.distance(), camera.distance() * factor, "the wheel still zooms");
            }
            // A point on the view's own axis has nowhere to slide to, and a plain zoom is the answer.
            let mut centred = camera;
            centred.zoom_about(0.5, None);
            assert_eq!(centred.target(), camera.target(), "a zoom with no anchor moves only the distance");
        }
    }

    #[test]
    fn pan_moves_the_target_with_the_pointer() {
        let mut camera = Camera::default();
        let before = camera.target();
        camera.pan_pixels(100.0, 0.0, 800.0);
        let moved = camera.target() - before;
        assert!(moved.length() > 0.0);
        assert!(moved.dot(camera.right()) < 0.0, "dragging right moves the target left");
        camera.pan_pixels(f64::INFINITY, 0.0, 800.0);
        assert!(camera.target().is_finite());
    }

    #[test]
    fn look_from_snaps_to_axis_views_and_keeps_the_target() {
        let default = Camera::default();
        let mut camera = Camera::default();
        camera.look_from(DVec3::X);
        let offset = camera.eye() - camera.target();
        assert!((offset.try_normalize().expect("eye offset") - DVec3::X).length() < 1.0e-9);
        assert!(camera.forward().dot(-DVec3::X) > 1.0 - 1.0e-9);
        assert_eq!(camera.target(), default.target());
        assert_eq!(camera.distance(), default.distance());

        camera.look_from(-DVec3::Y);
        let offset = camera.eye() - camera.target();
        assert!((offset.try_normalize().expect("eye offset") - -DVec3::Y).length() < 1.0e-9);

        camera.look_from(DVec3::Z);
        assert_eq!(camera.pitch(), MAX_PITCH);
        let yaw = camera.yaw();
        camera.look_from(DVec3::ZERO);
        assert_eq!(camera.yaw(), yaw);
        assert_eq!(camera.pitch(), MAX_PITCH);
        camera.look_from(DVec3::new(f64::NAN, 0.0, 0.0));
        assert_eq!(camera.yaw(), yaw);
        assert_eq!(camera.pitch(), MAX_PITCH);
    }

    #[test]
    fn a_pole_view_looks_straight_down_and_projects_its_target() {
        let mut camera = Camera::default();
        camera.look_from(DVec3::Z);
        assert_eq!(camera.pitch(), FRAC_PI_2);
        assert!((camera.forward() - -DVec3::Z).length() < 1.0e-12, "forward was {:?}", camera.forward());
        let up = camera.up();
        assert!(up.is_finite());
        assert!((up.length() - 1.0).abs() < 1.0e-12, "up was {up:?}");
        assert!(up.dot(camera.forward()).abs() < 1.0e-12, "up is perpendicular to forward");

        let aspect = 16.0 / 9.0;
        let centre = camera.ray_ndc(0.0, 0.0, aspect);
        assert!((centre.direction() - -DVec3::Z).length() < 1.0e-12, "the centre ray looks straight down");
        let ndc = camera.project_ndc(camera.target(), aspect).expect("the target is in front");
        assert!(ndc.length() < 1.0e-12, "the target projects to the viewport centre, got {ndc:?}");

        camera.look_from(-DVec3::Z);
        assert_eq!(camera.pitch(), MIN_PITCH);
        assert!((camera.forward() - DVec3::Z).length() < 1.0e-12, "forward was {:?}", camera.forward());
        assert!((camera.up().length() - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn a_pole_view_takes_the_canonical_up() {
        // Start from a yaw the pole must not keep, so a camera that still inherits it fails here.
        let mut camera = Camera::default();
        camera.orbit(0.9, 0.0);
        let target = camera.target();
        let distance = camera.distance();

        camera.look_from_oriented(DVec3::Z, canonical_up(DVec3::Z));
        assert_eq!(camera.pitch(), MAX_PITCH);
        assert!(camera.up().dot(DVec3::Y) > 1.0 - 1.0e-9, "the top view is +Y up, got {:?}", camera.up());
        assert!(camera.right().dot(DVec3::X) > 1.0 - 1.0e-9, "and +X right, got {:?}", camera.right());
        assert_eq!(camera.target(), target, "the target does not move");
        assert_eq!(camera.distance(), distance, "nor the distance");

        camera.look_from_oriented(-DVec3::Z, canonical_up(-DVec3::Z));
        assert_eq!(camera.pitch(), MIN_PITCH);
        assert!(camera.up().dot(DVec3::Y) > 1.0 - 1.0e-9, "the bottom view is +Y up, got {:?}", camera.up());
        assert!(camera.right().dot(-DVec3::X) > 1.0 - 1.0e-9, "and -X right, got {:?}", camera.right());
        assert_eq!(camera.target(), target);
    }

    #[test]
    fn a_non_pole_direction_ignores_the_requested_up() {
        let mut camera = Camera::default();
        camera.look_from_oriented(DVec3::X, DVec3::Y);
        assert!(camera.up().dot(DVec3::Z) > 1.0 - 1.0e-9, "a side view keeps +Z up, got {:?}", camera.up());
        assert!(camera.right().dot(DVec3::Y) > 1.0 - 1.0e-9, "and turns with its yaw, got {:?}", camera.right());
    }

    #[test]
    fn a_pole_without_a_usable_up_keeps_the_yaw() {
        let mut camera = Camera::default();
        camera.look_from(DVec3::X);
        let yaw = camera.yaw();

        // An up parallel to the view direction names no roll, so the pole keeps the yaw it had.
        camera.look_from_oriented(DVec3::Z, DVec3::Z);
        assert_eq!(camera.pitch(), MAX_PITCH);
        assert_eq!(camera.yaw(), yaw);

        // A zero direction is not a view at all and must not move the camera.
        camera.look_from_oriented(DVec3::ZERO, DVec3::Y);
        assert_eq!(camera.pitch(), MAX_PITCH);
        assert_eq!(camera.yaw(), yaw);
    }

    #[test]
    fn the_canonical_up_is_y_at_a_pole_and_z_elsewhere() {
        assert_eq!(canonical_up(DVec3::Z), DVec3::Y);
        assert_eq!(canonical_up(-DVec3::Z), DVec3::Y);
        assert_eq!(canonical_up(DVec3::X), DVec3::Z);
        assert_eq!(canonical_up(-DVec3::Y), DVec3::Z);
        assert_eq!(canonical_up(DVec3::new(0.3, 0.7, 0.5)), DVec3::Z);
    }

    #[test]
    fn orbiting_off_a_pole_moves_the_view() {
        let mut camera = Camera::default();
        camera.look_from(DVec3::Z);
        let before = camera.forward();
        camera.orbit(0.0, -0.1);
        assert!((camera.pitch() - (FRAC_PI_2 - 0.1)).abs() < 1.0e-12);
        assert!((camera.forward() - before).length() > 0.05, "an orbit leaves the pole");
        assert!((camera.up().length() - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn a_roll_turns_the_basis_about_the_view_axis() {
        let mut camera = Camera::default();
        camera.look_from(DVec3::new(0.6, 0.8, 0.3));
        let (eye, target, forward) = (camera.eye(), camera.target(), camera.forward());
        let (right, up) = (camera.right(), camera.up());

        camera.roll_to(FRAC_PI_2);
        assert!((camera.eye() - eye).length() < 1.0e-9, "the eye does not move");
        assert!((camera.target() - target).length() < 1.0e-9, "nor the target");
        assert!((camera.forward() - forward).length() < 1.0e-9, "nor the view direction");
        assert_close(camera.up(), right);
        assert_close(camera.right(), -up);
        assert!((camera.up() - camera.right().cross(camera.forward())).length() < 1.0e-9, "up stays right × forward");
        assert!((camera.right().length() - 1.0).abs() < 1.0e-9);
        assert!((camera.up().length() - 1.0).abs() < 1.0e-9);
    }

    #[test]
    fn a_roll_moves_a_world_point_from_screen_right_to_screen_up() {
        let mut camera = Camera::default();
        let aspect = 16.0 / 9.0;
        let point = camera.target() + camera.right() * 300.0;
        let before = camera.project_ndc(point, aspect).expect("the point is in front");
        assert!(before.x > 0.3 && before.y.abs() < 0.01, "the point starts on screen right: {before:?}");

        camera.roll_to(FRAC_PI_2);
        let after = camera.project_ndc(point, aspect).expect("the point is still in front");
        assert!(after.y > 0.3 && after.x.abs() < 0.01, "the image turned counter-clockwise: {after:?}");
    }

    #[test]
    fn a_ray_follows_the_rolled_basis() {
        let mut camera = Camera::default();
        camera.roll_to(FRAC_PI_2);
        let right_ray = camera.ray_ndc(1.0, 0.0, 1.0);
        assert!(right_ray.direction().dot(camera.right()) > 0.3, "the right edge follows the rolled right vector");
        assert!(right_ray.direction().dot(camera.up()).abs() < 1.0e-9, "and has no up component");
        let up_ray = camera.ray_ndc(0.0, 1.0, 1.0);
        assert!(up_ray.direction().dot(camera.up()) > 0.3, "the top edge follows the rolled up vector");
        assert!(up_ray.direction().dot(camera.right()).abs() < 1.0e-9, "and has no right component");
    }

    #[test]
    fn the_view_matrix_stays_orthonormal_under_a_roll() {
        let mut camera = Camera::default();
        camera.look_from(DVec3::new(0.4, 0.7, 0.5));
        camera.roll_to(0.9);
        let columns = camera.view_matrix().to_cols_array();
        let right = DVec3::new(columns[0], columns[4], columns[8]);
        let up = DVec3::new(columns[1], columns[5], columns[9]);
        let back = DVec3::new(columns[2], columns[6], columns[10]);
        for pair in [(right, up), (right, back), (up, back)] {
            assert!(pair.0.dot(pair.1).abs() < 1.0e-9, "the basis is orthogonal");
            assert!((pair.0.length() - 1.0).abs() < 1.0e-9);
        }
        assert!((right.cross(up).dot(back) - 1.0).abs() < 1.0e-9, "the view basis is right-handed");
    }

    #[test]
    fn a_rolled_pole_view_stays_straight_down_and_rolls() {
        let mut camera = Camera::default();
        camera.look_from(DVec3::Z);
        let (eye, target, forward) = (camera.eye(), camera.target(), camera.forward());
        camera.roll_to(0.7);
        assert!((camera.eye() - eye).length() < 1.0e-9);
        assert!((camera.target() - target).length() < 1.0e-9);
        assert!((camera.forward() - forward).length() < 1.0e-9, "the pole still looks straight down");
        assert!((camera.up().length() - 1.0).abs() < 1.0e-9);
        assert!(camera.up().dot(camera.forward()).abs() < 1.0e-9);
        assert!((camera.up() - DVec3::Y).length() > 0.1, "the roll leaves the canonical up");
    }

    #[test]
    fn look_from_resets_the_roll_and_navigation_keeps_it() {
        let mut camera = Camera::default();
        camera.roll_to(0.8);
        camera.orbit(0.0, 0.0);
        assert!((camera.roll() - 0.8).abs() < 1.0e-12, "orbit keeps the roll");
        camera.pan_pixels(10.0, 0.0, 800.0);
        assert!((camera.roll() - 0.8).abs() < 1.0e-12, "pan keeps the roll");
        camera.zoom(1.1);
        assert!((camera.roll() - 0.8).abs() < 1.0e-12, "zoom keeps the roll");
        assert!(camera.toggle_projection().kept(), "a default view has a distance for its height");
        assert!((camera.roll() - 0.8).abs() < 1.0e-12, "the projection toggle keeps the roll");

        camera.look_from(DVec3::new(0.3, 0.4, 0.5));
        assert_eq!(camera.roll(), 0.0, "a standard direction resets the roll");
        camera.roll_to(0.9);
        camera.look_from_oriented(DVec3::Z, DVec3::Y);
        assert_eq!(camera.roll(), 0.0, "the oriented pole snap resets the roll too");
    }

    #[test]
    fn a_roll_wraps_and_ignores_hostile_input() {
        let mut camera = Camera::default();
        camera.roll_to(-FRAC_PI_2);
        assert!((camera.roll() - (TAU - FRAC_PI_2)).abs() < 1.0e-12, "a negative roll wraps into 0..TAU");
        camera.roll_by(TAU);
        assert!((camera.roll() - (TAU - FRAC_PI_2)).abs() < 1.0e-12, "a full turn changes nothing");
        camera.roll_by(0.1);
        assert!((camera.roll() - (TAU - FRAC_PI_2 + 0.1)).abs() < 1.0e-12);

        let roll = camera.roll();
        camera.roll_to(f64::NAN);
        camera.roll_to(f64::INFINITY);
        camera.roll_by(f64::NEG_INFINITY);
        assert_eq!(camera.roll(), roll, "non-finite input changes nothing");
    }

    #[test]
    fn a_pinned_camera_ignores_the_roll() {
        let mut rolled = Camera::default().looked_from(DVec3::new(0.0, 0.0, 100.0), DVec3::X, DVec3::Y, -DVec3::Z);
        let pinned = rolled;
        rolled.roll_to(FRAC_PI_2);
        rolled.roll_by(0.4);
        assert_eq!(rolled, pinned, "a pinned basis ignores the roll and keeps every field");
        assert_close(rolled.right(), DVec3::X);
        assert_close(rolled.up(), DVec3::Y);
    }

    #[test]
    fn panning_follows_the_rolled_axes() {
        let mut camera = Camera::default();
        camera.look_from(DVec3::new(0.5, 0.5, 0.4));
        let unrolled_right = camera.right();
        camera.roll_to(FRAC_PI_2);
        let before = camera.target();
        camera.pan_pixels(100.0, 0.0, 800.0);
        let moved = camera.target() - before;
        assert!(moved.dot(camera.right()) < 0.0, "a rightward drag moves along the rolled right");
        assert!(moved.dot(unrolled_right).abs() < 1.0e-9, "and not along the unrolled right");
    }

    #[test]
    fn the_view_matrix_keeps_an_authored_up_vector() {
        let up = DVec3::new(0.0, 1.0, 0.005).try_normalize().expect("a finite up vector");
        let camera = Camera::default().looked_from(DVec3::new(0.0, 0.0, 100.0), DVec3::X, up, -DVec3::Z);
        assert_close(camera.up(), up);
        let columns = camera.view_matrix().to_cols_array();
        assert_close(DVec3::new(columns[1], columns[5], columns[9]), up);
    }

    #[test]
    fn projection_is_finite_at_both_depth_planes() {
        let camera = Camera::default();
        let projection = camera.projection_matrix(16.0 / 9.0);
        assert!(projection.is_finite());

        let near = projection * DVec4::new(0.0, 0.0, -camera.near, 1.0);
        let far = projection * DVec4::new(0.0, 0.0, -camera.far, 1.0);
        assert!((near.z / near.w - 0.0).abs() < 1.0e-9);
        assert!((far.z / far.w - 1.0).abs() < 1.0e-9);
    }

    #[test]
    fn toggling_the_projection_keeps_the_visible_height() {
        let mut camera = Camera::default();
        let perspective = camera;
        assert!(camera.toggle_projection().kept());
        let Projection::Orthographic { height } = camera.projection() else {
            panic!("a perspective view becomes orthographic");
        };
        let expected = 2.0 * perspective.distance() * (DEFAULT_FOV_Y * 0.5).tan();
        assert!((height - expected).abs() < 1.0e-9, "the orthographic height is the frame at the target");
        assert_eq!(camera.target(), perspective.target());
        assert_eq!(camera.yaw(), perspective.yaw());
        assert_eq!(camera.pitch(), perspective.pitch());
        assert_eq!(camera.distance(), perspective.distance());

        assert!(camera.toggle_projection().kept());
        assert!(matches!(camera.projection(), Projection::Perspective { .. }));
        assert!((camera.distance() - perspective.distance()).abs() < 1.0e-9, "the frame comes back");
        assert_eq!(camera.target(), perspective.target());
        assert_eq!(camera.yaw(), perspective.yaw());
        assert_eq!(camera.pitch(), perspective.pitch());
    }

    #[test]
    fn set_orthographic_keeps_the_visible_height_and_the_framing() {
        let mut camera = Camera::default();
        camera.orbit(0.3, 0.1);
        camera.zoom(1.4);
        let perspective = camera;
        camera.set_orthographic();
        let Projection::Orthographic { height } = camera.projection() else {
            panic!("a perspective view becomes orthographic");
        };
        let expected = 2.0 * perspective.distance() * (DEFAULT_FOV_Y * 0.5).tan();
        assert!((height - expected).abs() < 1.0e-9, "the height is the frame at the target");
        assert_eq!(camera.target(), perspective.target(), "the target stays");
        assert_eq!(camera.distance(), perspective.distance(), "the distance stays");
        assert_eq!(camera.yaw(), perspective.yaw());
        assert_eq!(camera.pitch(), perspective.pitch());

        // A second call is a no-op: a standard view that is already orthographic keeps its height.
        camera.zoom(0.5);
        let Projection::Orthographic { height: zoomed } = camera.projection() else {
            panic!("zooming keeps the projection");
        };
        camera.set_orthographic();
        let Projection::Orthographic { height: kept } = camera.projection() else {
            panic!("the view is still orthographic");
        };
        assert!((kept - zoomed).abs() < 1.0e-9, "an orthographic view keeps its visible height");

        // An authored camera is pinned to its document projection.
        let authored = Camera::default()
            .looked_from(DVec3::new(0.0, 0.0, 100.0), DVec3::X, DVec3::Y, -DVec3::Z)
            .with_projection(Projection::Perspective { fov_y: DEFAULT_FOV_Y });
        let mut pinned = authored;
        pinned.set_orthographic();
        assert_eq!(pinned, authored, "a pinned camera is not view state the viewport may change");
    }

    #[test]
    fn zooming_an_orthographic_view_scales_the_visible_height() {
        let mut camera = Camera::default();
        assert!(camera.toggle_projection().kept());
        let Projection::Orthographic { height } = camera.projection() else {
            panic!("the view is orthographic");
        };
        camera.zoom(0.5);
        let Projection::Orthographic { height: zoomed } = camera.projection() else {
            panic!("zooming keeps the projection");
        };
        assert!((zoomed - height * 0.5).abs() < 1.0e-9, "zooming in halves the visible height");

        assert!(camera.toggle_projection().kept());
        assert!(camera.toggle_projection().kept());
        let Projection::Orthographic { height: returned } = camera.projection() else {
            panic!("the round trip returns an orthographic view");
        };
        assert!((returned - zoomed).abs() < 1.0e-9, "a toggle pair keeps the zoomed frame");
    }

    #[test]
    fn an_authored_camera_keeps_its_projection() {
        let authored = Camera::default()
            .looked_from(DVec3::new(0.0, 0.0, 100.0), DVec3::X, DVec3::Y, -DVec3::Z)
            .with_projection(Projection::Orthographic { height: 500.0 });
        let mut camera = authored;
        assert_eq!(camera.toggle_projection(), ProjectionToggle::Pinned, "a pinned camera is not switched");
        camera.zoom(4.0);
        assert_eq!(camera, authored, "a pinned camera is not view state the viewport may change");
    }

    #[test]
    fn a_projection_switch_keeps_the_height_or_moves_nothing() {
        // X120: the switch promised the height at the target and clamped the distance instead, so a
        // large orthographic view landed a kilometre from its model without a word. Every height is
        // either shown at the same size or refused with the reason; no switch leaves the camera on a
        // limit it did not ask for.
        let heights = [
            8.3,
            2_000.0,
            828_427.0,
            MIN_DISTANCE_MM / (DEFAULT_FOV_Y * 0.5).tan(),
            MAX_DISTANCE_MM * 2.0 * (DEFAULT_FOV_Y * 0.5).tan(),
            2_200_000.0,
            5.0,
            1.0e9,
        ];
        for height in heights {
            let mut camera = Camera::default().with_projection(Projection::Orthographic { height });
            assert!(camera.projection().is_valid(), "the height {height} is a view a camera can hold");
            let before = camera;
            let outcome = camera.toggle_projection();
            if outcome.kept() {
                assert!(matches!(camera.projection(), Projection::Perspective { .. }), "{height} switched");
                assert!(
                    ((camera.visible_height() - height) / height).abs() < 1.0e-9,
                    "the frame at the target is the one the view had: {} against {height}",
                    camera.visible_height()
                );
                assert_ne!(camera.distance(), MIN_DISTANCE_MM, "a switched view is not clamped short");
                assert!(
                    camera.distance() > MIN_DISTANCE_MM && camera.distance() <= MAX_DISTANCE_MM,
                    "and stands inside the distances the camera supports: {}",
                    camera.distance()
                );
            } else {
                assert_eq!(outcome, ProjectionToggle::NoDistance { height }, "the reason names the height");
                assert_eq!(camera, before, "a switch with no distance to stand at moves nothing");
            }
        }
    }

    #[test]
    fn a_view_wound_out_to_the_distance_cap_still_switches() {
        // The app reaches the widest frame it can hold by zooming out until `zoom` stops at the cap,
        // so the height is a multiplication away from the distance that shows it. The round trip is
        // off by a rounding step, and a view that sat at the cap must still switch (X120).
        let mut camera = Camera::default();
        assert!(camera.toggle_projection().kept());
        camera.zoom(1.0e12);
        assert_eq!(camera.distance(), MAX_DISTANCE_MM, "the wheel stops at the cap");
        let height = camera.visible_height();

        assert!(camera.toggle_projection().kept(), "a view at the cap has a distance to stand at");
        assert!(matches!(camera.projection(), Projection::Perspective { .. }));
        assert!(
            ((camera.visible_height() - height) / height).abs() < 1.0e-9,
            "and keeps the frame it had: {} against {height}",
            camera.visible_height()
        );
    }

    #[test]
    fn the_visible_height_is_the_same_number_in_both_projections() {
        let mut camera = Camera::default();
        camera.orbit(0.4, -0.2);
        camera.zoom(1.7);
        let perspective = camera.visible_height();
        assert!((perspective - 2.0 * camera.distance() * (DEFAULT_FOV_Y * 0.5).tan()).abs() < 1.0e-9);

        camera.set_orthographic();
        assert!((camera.visible_height() - perspective).abs() < 1.0e-9, "the switch keeps the height it names");
        let Projection::Orthographic { height } = camera.projection() else {
            panic!("the view is orthographic");
        };
        assert!((height - perspective).abs() < 1.0e-9, "and the projection holds that same number");
    }

    #[test]
    fn an_orthographic_camera_projects_without_perspective() {
        let camera = Camera::default().with_projection(Projection::Orthographic { height: 100.0 });
        let aspect = 2.0;
        let eye = camera.eye();
        let point = eye + camera.forward() * 500.0 + camera.right() * 100.0;
        let ndc = camera.project_ndc(point, aspect).expect("a point in front of the camera");
        assert!((ndc.x - 1.0).abs() < 1.0e-9, "ndc was {ndc:?}");
        assert!(ndc.y.abs() < 1.0e-9);

        let centre = camera.ray_ndc(0.0, 0.0, aspect);
        let right = camera.ray_ndc(1.0, 0.0, aspect);
        assert_close(centre.direction(), camera.forward());
        assert_close(right.direction(), camera.forward());
        assert!((right.origin() - centre.origin()).dot(camera.right()) > 99.0, "rays are parallel and offset");

        assert!(camera.project_ndc(eye - camera.forward() * 10.0, aspect).is_none(), "behind is rejected");

        let projection = camera.projection_matrix(aspect);
        let near = projection * DVec4::new(0.0, 0.0, -camera.near, 1.0);
        let far = projection * DVec4::new(0.0, 0.0, -camera.far, 1.0);
        assert!((near.z / near.w - 0.0).abs() < 1.0e-9);
        assert!((far.z / far.w - 1.0).abs() < 1.0e-9);
    }

    #[test]
    fn a_fixed_view_keeps_its_basis_and_ignores_navigation() {
        let mut camera = Camera::default().looked_from(DVec3::new(1.0, 2.0, 3.0), DVec3::X, DVec3::Y, -DVec3::Z);
        assert_close(camera.eye(), DVec3::new(1.0, 2.0, 3.0));
        assert_close(camera.forward(), -DVec3::Z);
        assert_close(camera.right(), DVec3::X);
        assert_close(camera.up(), DVec3::Y);

        let before = camera;
        camera.orbit(1.0, 1.0);
        camera.zoom(0.5);
        camera.pan_pixels(10.0, 10.0, 800.0);
        camera.look_from(DVec3::X);
        assert_eq!(camera, before, "an authored view is not navigated by the viewport");

        let orbit = Camera::default();
        assert_eq!(orbit.looked_from(DVec3::NAN, DVec3::X, DVec3::Y, DVec3::Z), orbit);
        assert_eq!(orbit.looked_from(DVec3::ZERO, DVec3::X, DVec3::X, DVec3::Z), orbit);
    }

    #[test]
    fn scene_accepts_drawables() {
        let mut frame = SceneFrame::default();
        let mesh = Arc::new(MeshData::default());
        let id = Id::new(7).expect("id");
        frame.push(Drawable::new(id, mesh, DMat4::IDENTITY, [1.0; 4]));
        assert_eq!(frame.drawables().len(), 1);
        assert_eq!(frame.drawables()[0].id, id);
    }

    fn line_chain(id: Id) -> Polyline {
        Polyline::new(id, Arc::from(vec![DVec3::ZERO, DVec3::X]), DMat4::IDENTITY, [0.5, 0.5, 0.5, 1.0])
    }

    #[test]
    fn scene_accepts_polylines() {
        let mut frame = SceneFrame::default();
        let id = Id::new(7).expect("id");
        let points: Arc<[DVec3]> = Arc::from(vec![DVec3::ZERO, DVec3::X, DVec3::new(1.0, 1.0, 0.0)]);
        frame.push_polyline(Polyline::new(id, points.clone(), DMat4::IDENTITY, [1.0; 4]));

        assert_eq!(frame.polylines().len(), 1);
        assert_eq!(frame.polylines()[0].id, id);
        assert_eq!(frame.polylines()[0].points, points);
        assert!(frame.drawables().is_empty(), "a line chain is not a solid drawable");
    }

    #[test]
    fn a_world_translation_preview_moves_only_the_listed_polylines() {
        let moved = Id::new(2).expect("id");
        let other = Id::new(3).expect("id");
        let mut frame = SceneFrame::default();
        frame.push_polyline(line_chain(moved));
        frame.push_polyline(line_chain(other));

        let translated = frame.with_world_translation(&[moved], DVec3::new(1.0, 2.0, 3.0));
        let position = translated.polylines()[0].world_transform.transform_point3(DVec3::ZERO);
        assert!((position - DVec3::new(1.0, 2.0, 3.0)).length() < 1.0e-12);
        assert_eq!(translated.polylines()[1].world_transform, DMat4::IDENTITY);
        assert_eq!(frame.polylines()[0].world_transform, DMat4::IDENTITY, "the base frame keeps its transform");
    }

    #[test]
    fn a_world_rotation_preview_turns_only_the_listed_polylines() {
        let turned = Id::new(2).expect("id");
        let other = Id::new(3).expect("id");
        let mut frame = SceneFrame::default();
        let offset = DMat4::from_translation(DVec3::new(2.0, 0.0, 0.0));
        frame.push_polyline(Polyline::new(turned, Arc::from(vec![DVec3::ZERO, DVec3::X]), offset, [1.0; 4]));
        frame.push_polyline(line_chain(other));

        let quarter_turn = DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2);
        let rotated = frame.with_world_rotation(&[turned], DVec3::new(1.0, 0.0, 0.0), quarter_turn);
        let position = rotated.polylines()[0].world_transform.transform_point3(DVec3::ZERO);
        assert!((position - DVec3::new(1.0, 1.0, 0.0)).length() < 1.0e-12, "position was {position:?}");
        assert_eq!(rotated.polylines()[1].world_transform, DMat4::IDENTITY);
        assert_eq!(frame.polylines()[0].world_transform, offset);
    }

    #[test]
    fn selection_highlight_gives_every_selected_polyline_the_explicit_colour() {
        let selected = Id::new(2).expect("id");
        let other = Id::new(3).expect("id");
        let mut frame = SceneFrame::default();
        frame.push_polyline(line_chain(selected));
        frame.push_polyline(line_chain(other));

        let highlighted = frame.with_selection_highlight(&[selected]);
        assert_eq!(highlighted.polylines()[0].color, Some(SELECTION_LINE_COLOR));
        assert_ne!(Some(SELECTION_LINE_COLOR), frame.polylines()[0].color, "picking the colour must be visible");
        assert_eq!(highlighted.polylines()[1].color, frame.polylines()[1].color);
        assert_eq!(frame.polylines()[0].color, Some([0.5, 0.5, 0.5, 1.0]), "the base frame keeps its colour");
    }

    #[test]
    fn an_armed_booleans_base_ring_takes_the_base_ink_and_its_operands_the_selection_ink() {
        // X99: the base is the frame the result is born in, and a plain click on the second ring
        // drops it from the selection, so it needs an ink of its own to stay readable as an operand.
        let base = Id::new(2).expect("id");
        let operand = Id::new(3).expect("id");
        let untouched = Id::new(4).expect("id");
        let mut frame = SceneFrame::default();
        frame.push_polyline(line_chain(base));
        frame.push_polyline(line_chain(operand));
        frame.push_polyline(line_chain(untouched));
        let ink = SceneColors::DARK.boolean_base;
        assert_ne!(ink, SELECTION_LINE_COLOR, "the two roles are two inks");

        // The tool is the selection, so the base is not in it: the pair draws apart anyway.
        let armed = frame.with_picks(&[operand], Some(base));
        assert_eq!(armed.polylines()[0].color, Some(ink), "the base ring takes the base ink");
        assert_eq!(armed.polylines()[1].color, Some(SELECTION_LINE_COLOR), "the operand keeps the selection ink");
        assert_eq!(armed.polylines()[2].color, frame.polylines()[2].color, "a ring beside them is untouched");

        // A base that is also picked keeps the base ink: one ring, one role.
        let also_picked = frame.with_picks(&[base, operand], Some(base));
        assert_eq!(also_picked.polylines()[0].color, Some(ink));
    }

    #[test]
    fn dropping_the_base_returns_every_ring_to_the_selection_ink() {
        let base = Id::new(2).expect("id");
        let operand = Id::new(3).expect("id");
        let mut frame = SceneFrame::default();
        frame.push_polyline(line_chain(base));
        frame.push_polyline(line_chain(operand));
        assert_ne!(frame.with_picks(&[operand], Some(base)).polylines()[0].color, Some(SELECTION_LINE_COLOR));

        let dropped = frame.with_picks(&[base, operand], None);
        for chain in dropped.polylines() {
            assert_eq!(chain.color, Some(SELECTION_LINE_COLOR), "no ring keeps the base ink when no boolean is armed");
        }
    }

    #[test]
    fn a_base_body_takes_the_base_ink_and_not_the_selection_tint() {
        let base = Id::new(2).expect("id");
        let tool = Id::new(3).expect("id");
        let mut frame = SceneFrame::default();
        frame.push(Drawable::new(base, quad_mesh(), DMat4::IDENTITY, [0.5, 0.5, 0.5, 1.0]));
        frame.push(Drawable::new(tool, quad_mesh(), DMat4::IDENTITY, [0.5, 0.5, 0.5, 1.0]));

        let armed = frame.with_picks(&[tool], Some(base));
        let base_color = armed.drawables()[0].color;
        assert_ne!(base_color, frame.drawables()[0].color, "the base is marked while it is the frame");
        assert_ne!(base_color, armed.drawables()[1].color, "and the tool is a different mark");
        assert_ne!(Some(base_color), frame.with_picks(&[base], None).drawables().first().map(|d| d.color));
        assert_eq!(base_color[3], 1.0, "a tint never moves the alpha");
    }

    #[test]
    fn a_light_frame_carries_the_light_background_and_grid() {
        let id = Id::new(7).expect("id");
        let mut frame = SceneFrame::default();
        frame.push(Drawable::new(id, quad_mesh(), DMat4::IDENTITY, [0.62, 0.42, 0.22, 1.0]));
        assert_eq!(frame.background, SceneColors::DARK.background, "the default frame is the dark variant");

        frame.apply_colors(SceneColors::LIGHT);
        assert_eq!(frame.background, SceneColors::LIGHT.background);
        assert_eq!(frame.grid.minor_color, Grid::LIGHT.minor_color);
        assert_eq!(frame.grid.major_color, Grid::LIGHT.major_color);
        assert_eq!(frame.grid.axis_x_color, Grid::LIGHT.axis_x_color);
        assert_eq!(frame.grid.axis_y_color, Grid::LIGHT.axis_y_color);
        assert_eq!(frame.line_ink, SceneColors::LIGHT.line_ink);
        assert!(luminance(frame.background) > 0.5, "the light workspace clears to a light colour");
        assert!(luminance(frame.grid.major_color) < luminance(frame.background), "the grid reads on paper");
        assert_eq!(frame.drawables()[0].color, [0.62, 0.42, 0.22, 1.0], "a material colour never follows a theme");

        frame.apply_colors(SceneColors::DARK);
        assert_eq!(frame.background, SceneColors::DARK.background, "switching back restores the dark values");
        assert_eq!(frame.grid, Grid::DARK);
        assert_eq!(frame.line_ink, SceneColors::DARK.line_ink);
    }

    #[test]
    fn a_theme_switch_keeps_the_viewports_grid_switches() {
        let mut frame = SceneFrame::default();
        frame.grid.visible = false;
        frame.grid.spacing_mm = 25.0;
        frame.grid.major_every = 2;

        frame.apply_colors(SceneColors::LIGHT);
        assert!(!frame.grid.visible, "a variant never turns a hidden grid on");
        assert_eq!(frame.grid.spacing_mm, 25.0);
        assert_eq!(frame.grid.major_every, 2);
    }

    #[test]
    fn a_theme_switch_keeps_the_grid_strength() {
        // The strength is the workspace's and the hue is the theme's, so a variant repaint keeps the
        // opacity the user set (U64).
        let mut frame = SceneFrame::default();
        frame.grid.ink_strength = 0.25;
        frame.apply_colors(SceneColors::LIGHT);
        assert_eq!(frame.grid.ink_strength, 0.25, "a variant never resets a display strength");
        assert_eq!(frame.grid.minor_color[..3], SceneColors::LIGHT.grid.minor_color[..3], "the hue is the theme's");
    }

    #[test]
    fn a_line_without_a_colour_draws_in_the_frame_ink() {
        let id = Id::new(7).expect("id");
        let points: Arc<[DVec3]> = Arc::from(vec![DVec3::ZERO, DVec3::X]);
        let inked = Polyline::in_ink(id, Arc::clone(&points), DMat4::IDENTITY);
        let painted = Polyline::new(id, points, DMat4::IDENTITY, [1.0, 0.0, 0.0, 1.0]);

        let mut frame = SceneFrame::default();
        frame.push_polyline(inked);
        frame.push_polyline(painted);
        assert_eq!(frame.polylines()[0].drawn_color(frame.line_ink), SceneColors::DARK.line_ink);
        assert_eq!(frame.polylines()[1].drawn_color(frame.line_ink), [1.0, 0.0, 0.0, 1.0]);

        frame.apply_colors(SceneColors::LIGHT);
        assert_eq!(frame.polylines()[0].drawn_color(frame.line_ink), SceneColors::LIGHT.line_ink);
        assert_eq!(frame.polylines()[1].drawn_color(frame.line_ink), [1.0, 0.0, 0.0, 1.0], "an own colour stays put");
    }

    fn quad_mesh() -> Arc<MeshData> {
        Arc::new(MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            normals: Vec::new(),
            indices: vec![0, 1, 2, 0, 2, 3],
        })
    }

    /// A mesh of `count` triangles fanned around the unit square, so a test can tell one triangle
    /// apart from a thousand in what a cast read.
    fn fan_mesh(count: usize) -> Arc<MeshData> {
        let mut positions = Vec::with_capacity(count + 1);
        positions.push([0.5, 0.5, 0.0]);
        let mut indices = Vec::with_capacity(count * 3);
        for step in 0..count {
            let angle = std::f64::consts::TAU * (step as f64 / count as f64);
            positions.push([0.5 + 0.4 * angle.cos() as f32, 0.5 + 0.4 * angle.sin() as f32, 0.0]);
            let first = u32::try_from(step + 1).expect("a small index");
            let second = u32::try_from((step + 1) % count + 1).expect("a small index");
            indices.extend_from_slice(&[0, first, second]);
        }
        Arc::new(MeshData { positions, normals: Vec::new(), indices })
    }

    /// A field of `count` drawables sharing one mesh, `step` millimetres apart along `-Z` below the
    /// origin: the stack a ray from above crosses one after another.
    fn stacked_field(count: usize, mesh: Arc<MeshData>, step: f64) -> SceneFrame {
        let mut frame = SceneFrame::default();
        for index in 0..count {
            let id = Id::new(u64::try_from(index + 2).expect("a small id")).expect("id");
            let transform = DMat4::from_translation(DVec3::new(0.0, 0.0, -step * index as f64));
            frame.push(Drawable::with_box(id, mesh.clone(), transform, [1.0; 4], mesh.local_bounds()));
        }
        frame
    }

    #[test]
    fn a_pick_over_empty_space_reads_no_triangle_of_a_dense_frame() {
        let frame = stacked_field(2_000, fan_mesh(64), 10.0);
        let mesh_triangles = frame.drawables()[0].mesh.triangle_count();
        assert_eq!(mesh_triangles, 64, "the fixture builds the triangles the counter is compared with");

        // A vertical ray a hundred millimetres beside the stack: every drawable's box lies to one
        // side of it, so the frame answers without reading a vertex.
        let beside = Ray::new(DVec3::new(100.0, 0.5, 10.0), -DVec3::Z).expect("ray");
        crate::ray::reset_triangles_tested();
        assert_eq!(frame.pick(beside, 600.0), None);
        assert_eq!(crate::ray::triangles_tested(), 0, "2000 objects of {mesh_triangles} triangles each: none is cast");

        // And a ray that misses from the front, so the rejection is not only the empty-space case.
        let ahead = Ray::new(DVec3::new(0.5, 0.5, 10.0), DVec3::Z).expect("ray");
        crate::ray::reset_triangles_tested();
        assert_eq!(frame.pick(ahead, 600.0), None, "the stack is behind the ray");
        assert_eq!(crate::ray::triangles_tested(), 0);
    }

    #[test]
    fn a_pick_over_the_model_returns_the_same_id_and_skips_what_is_behind_it() {
        let frame = stacked_field(2_000, fan_mesh(64), 10.0);
        let nearest = frame.drawables()[0].id;

        let from_above = Ray::new(DVec3::new(0.5, 0.5, 10.0), -DVec3::Z).expect("ray");
        crate::ray::reset_triangles_tested();
        assert_eq!(
            frame.pick(from_above, 600.0),
            Some(nearest),
            "the top of the stack is the object the click points at"
        );
        assert_eq!(crate::ray::triangles_tested(), 64, "the objects behind the hit are rejected by their boxes");

        // From below the same stack answers with the lowest object, so the narrowing never reorders.
        let last = frame.drawables()[frame.drawables().len() - 1].id;
        let from_below = Ray::new(DVec3::new(0.5, 0.5, -20_000.0), DVec3::Z).expect("ray");
        assert_eq!(frame.pick(from_below, 600.0), Some(last));
    }

    #[test]
    fn a_drawable_of_unknown_extent_is_still_cast() {
        let id = Id::new(9).expect("id");
        let mut frame = SceneFrame::default();
        frame.push(Drawable::with_box(id, quad_mesh(), DMat4::IDENTITY, [1.0; 4], None));

        let through = Ray::new(DVec3::new(0.5, 0.5, 10.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(through, 600.0), Some(id), "no measured box means no rejection, not no drawable");
        let beside = Ray::new(DVec3::new(2.0, 2.0, 10.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(beside, 600.0), None);
    }

    #[test]
    fn the_centre_ray_looks_where_the_camera_does() {
        let camera = Camera::default();
        let ray = camera.ray_ndc(0.0, 0.0, 16.0 / 9.0);
        assert_close(ray.origin(), camera.eye());
        assert_close(ray.direction(), camera.forward());

        let invalid = camera.ray_ndc(f64::NAN, f64::INFINITY, -1.0);
        assert_close(invalid.direction(), camera.forward());
        assert!(invalid.origin().is_finite());
    }

    #[test]
    fn projection_and_ray_agree_inside_and_outside_the_viewport() {
        let camera = Camera::default();
        let aspect = 16.0 / 9.0;
        let point = camera.target() + camera.right() * 100.0 + camera.up() * 50.0;
        let ndc = camera.project_ndc(point, aspect).expect("point in front of the camera");
        assert!(ndc.x > 0.0 && ndc.y > 0.0, "ndc was {ndc:?}");
        let ray = camera.ray_ndc(ndc.x, ndc.y, aspect);
        let to_point = (point - ray.origin()).try_normalize().expect("direction");
        assert!(ray.direction().dot(to_point) > 1.0 - 1.0e-9);

        let centre = camera.project_ndc(camera.target(), aspect).expect("target");
        assert!(centre.length() < 1.0e-9, "centre was {centre:?}");

        let behind = camera.eye() + (camera.eye() - camera.target());
        assert!(camera.project_ndc(behind, aspect).is_none());
        assert!(camera.project_ndc(DVec3::new(f64::NAN, 0.0, 0.0), aspect).is_none());

        let inside = camera.ray_ndc(1.0, 0.0, aspect);
        let outside = camera.ray_ndc(2.0, 0.0, aspect);
        assert!(outside.direction().dot(camera.right()) > inside.direction().dot(camera.right()));
        assert!(camera.ray_ndc(1.0e300, 0.0, aspect).direction().is_finite());
    }

    #[test]
    fn a_camera_holds_a_box_while_every_corner_is_inside_its_frame() {
        // X119 asks this before it moves a view nobody framed: the corners of the model either fit in
        // the picture or they do not, and the answer has to be the one the projection gives.
        let corners = |centre: DVec3, half: DVec3| -> Vec<DVec3> {
            (0..8u8)
                .map(|bits| {
                    let axis = |bit: u8, value: f64| if bits & bit == 0 { -value } else { value };
                    centre + DVec3::new(axis(1, half.x), axis(2, half.y), axis(4, half.z))
                })
                .collect()
        };
        let camera = Camera::new(DVec3::ZERO, 1000.0).expect("a camera");
        let aspect = 16.0 / 9.0;
        assert!(camera.holds(&corners(DVec3::ZERO, DVec3::splat(100.0)), aspect), "a small box is inside the frame");
        assert!(camera.holds(&[], aspect), "and there is nothing to hold when there is no box");

        assert!(
            !camera.holds(&corners(DVec3::ZERO, DVec3::new(5000.0, 100.0, 100.0)), aspect),
            "off the side is not held"
        );
        let behind = camera.eye() + (camera.eye() - camera.target());
        assert!(!camera.holds(&corners(behind, DVec3::splat(10.0)), aspect), "nor is a box behind the eye");

        // The orthographic view has its own height, so a camera that stands back does not widen it.
        let tall = corners(DVec3::ZERO, DVec3::new(10.0, 1500.0, 10.0));
        let mut flat = camera;
        flat.set_orthographic();
        let far = Camera::new(DVec3::ZERO, 10_000.0).expect("a camera that stands back");
        assert!(!flat.holds(&tall, aspect), "an orthographic frame holds only its own height");
        assert!(far.holds(&tall, aspect), "a perspective frame widens with the distance it stands at");
    }

    #[test]
    fn ray_closest_point_on_an_axis_line_is_a_drag_parameter() {
        let ray = Ray::new(DVec3::new(5.0, 10.0, 0.0), -DVec3::Y).expect("ray");
        let parameter = ray.closest_parameter_on_line(DVec3::ZERO, DVec3::X).expect("line");
        assert!((parameter - 5.0).abs() < 1.0e-12, "parameter was {parameter}");

        let through_origin = Ray::new(DVec3::new(0.0, 1.0, 0.0), -DVec3::Y).expect("ray");
        assert!(through_origin.closest_parameter_on_line(DVec3::ZERO, DVec3::X).expect("line").abs() < 1.0e-12);

        let parallel = Ray::new(DVec3::ZERO, DVec3::X).expect("ray");
        assert!(parallel.closest_parameter_on_line(DVec3::ZERO, DVec3::X).is_none());
        assert!(ray.closest_parameter_on_line(DVec3::ZERO, DVec3::ZERO).is_none());
        assert!(ray.closest_parameter_on_line(DVec3::new(f64::NAN, 0.0, 0.0), DVec3::X).is_none());
    }

    #[test]
    fn a_world_translation_preview_moves_only_the_listed_drawables() {
        let moved = Id::new(2).expect("id");
        let other = Id::new(3).expect("id");
        let mut frame = SceneFrame::default();
        frame.push(Drawable::new(moved, quad_mesh(), DMat4::IDENTITY, [1.0; 4]));
        frame.push(Drawable::new(other, quad_mesh(), DMat4::IDENTITY, [1.0; 4]));

        let translated = frame.with_world_translation(&[moved], DVec3::new(1.0, 2.0, 3.0));
        let position = translated.drawables()[0].world_transform.transform_point3(DVec3::ZERO);
        assert!((position - DVec3::new(1.0, 2.0, 3.0)).length() < 1.0e-12);
        assert_eq!(translated.drawables()[1].world_transform, DMat4::IDENTITY);
        assert_eq!(frame.drawables()[0].world_transform, DMat4::IDENTITY);

        assert_eq!(frame.with_world_translation(&[], DVec3::X), frame);
        assert_eq!(frame.with_world_translation(&[moved], DVec3::new(f64::NAN, 0.0, 0.0)), frame);
    }

    #[test]
    fn a_world_rotation_preview_turns_only_the_listed_drawables_about_the_pivot() {
        let turned = Id::new(2).expect("id");
        let other = Id::new(3).expect("id");
        let mut frame = SceneFrame::default();
        let offset = DMat4::from_translation(DVec3::new(2.0, 0.0, 0.0));
        frame.push(Drawable::new(turned, quad_mesh(), offset, [1.0; 4]));
        frame.push(Drawable::new(other, quad_mesh(), DMat4::IDENTITY, [1.0; 4]));

        let quarter_turn = DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2);
        let rotated = frame.with_world_rotation(&[turned], DVec3::new(1.0, 0.0, 0.0), quarter_turn);
        let position = rotated.drawables()[0].world_transform.transform_point3(DVec3::ZERO);
        assert!((position - DVec3::new(1.0, 1.0, 0.0)).length() < 1.0e-12, "position was {position:?}");
        assert_eq!(rotated.drawables()[1].world_transform, DMat4::IDENTITY);
        assert_eq!(frame.drawables()[0].world_transform, offset);
    }

    #[test]
    fn a_world_rotation_preview_rejects_empty_ids_and_bad_input() {
        let turned = Id::new(2).expect("id");
        let mut frame = SceneFrame::default();
        frame.push(Drawable::new(turned, quad_mesh(), DMat4::IDENTITY, [1.0; 4]));

        let quarter_turn = DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2);
        assert_eq!(frame.with_world_rotation(&[], DVec3::ZERO, quarter_turn), frame);
        assert_eq!(frame.with_world_rotation(&[turned], DVec3::new(f64::NAN, 0.0, 0.0), quarter_turn), frame);
        assert_eq!(frame.with_world_rotation(&[turned], DVec3::ZERO, DQuat::from_xyzw(2.0, 0.0, 0.0, 0.0)), frame);
    }

    #[test]
    fn a_mesh_preview_replaces_only_the_named_drawable() {
        let edited = Id::new(2).expect("id");
        let other = Id::new(3).expect("id");
        let mut frame = SceneFrame::default();
        frame.push(Drawable::new(edited, quad_mesh(), DMat4::IDENTITY, [1.0; 4]));
        frame.push(Drawable::new(other, quad_mesh(), DMat4::IDENTITY, [1.0; 4]));

        let mut replacement = (*quad_mesh()).clone();
        replacement.positions[0] = [9.0, 9.0, 9.0];
        let replacement = Arc::new(replacement);
        let preview = frame.with_drawable_mesh(edited, replacement.clone());

        assert_eq!(preview.drawables()[0].mesh, replacement);
        assert_eq!(preview.drawables()[1].mesh, quad_mesh());
        assert_eq!(frame.drawables()[0].mesh, quad_mesh(), "the base frame keeps its mesh");
        assert_eq!(frame.with_drawable_mesh(Id::new(99).expect("id"), replacement), frame);
    }

    #[test]
    fn picking_returns_the_nearest_document_id() {
        let near = Id::new(2).expect("id");
        let far = Id::new(3).expect("id");
        let mut frame = SceneFrame::default();
        frame.push(Drawable::new(near, quad_mesh(), DMat4::IDENTITY, [1.0; 4]));
        frame.push(Drawable::new(far, quad_mesh(), DMat4::from_translation(DVec3::new(0.0, 0.0, 1.0)), [1.0; 4]));

        let from_above = Ray::new(DVec3::new(0.5, 0.5, 10.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(from_above, 600.0), Some(far));
        let from_below = Ray::new(DVec3::new(0.5, 0.5, -10.0), DVec3::Z).expect("ray");
        assert_eq!(frame.pick(from_below, 600.0), Some(near));

        let miss = Ray::new(DVec3::new(2.0, 2.0, 10.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(miss, 600.0), None);
    }

    #[test]
    fn picking_follows_the_world_transform() {
        let id = Id::new(4).expect("id");
        let mut frame = SceneFrame::default();
        let transform = DMat4::from_translation(DVec3::new(10.0, 0.0, 0.0));
        frame.push(Drawable::new(id, quad_mesh(), transform, [1.0; 4]));

        let hit = Ray::new(DVec3::new(10.5, 0.5, 10.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(hit, 600.0), Some(id));
        let original_position = Ray::new(DVec3::new(0.5, 0.5, 10.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(original_position, 600.0), None);
    }

    #[test]
    fn malformed_meshes_are_skipped_instead_of_panicking() {
        let id = Id::new(5).expect("id");
        let mesh =
            Arc::new(MeshData { positions: vec![[0.0; 3]], normals: Vec::new(), indices: vec![0, 1, 2, 0, 0, 0] });
        let mut frame = SceneFrame::default();
        frame.push(Drawable::new(id, mesh, DMat4::IDENTITY, [1.0; 4]));
        let ray = Ray::new(DVec3::new(0.0, 0.0, 1.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(ray, 600.0), None);
    }

    fn envelope(id: u64, size: DVec3, translation: DVec3, state: ProductEnvelopeState) -> ProductEnvelope {
        ProductEnvelope::rectangular(Id::new(id).expect("id"), size, DMat4::from_translation(translation), state)
    }

    #[test]
    fn a_product_envelope_is_drawn_in_the_three_states_of_the_decision() {
        assert!(ProductEnvelopeState::Open.draws(), "an open product always shows its box");
        assert!(ProductEnvelopeState::ClosedEmpty.draws(), "a closed empty piece is small and unmistakable");
        assert!(!ProductEnvelopeState::ClosedWithParts.draws(), "a piece with parts speaks for itself");

        let mut frame = SceneFrame::default();
        frame.push_product_envelope(envelope(1, DVec3::splat(100.0), DVec3::ZERO, ProductEnvelopeState::Open));
        assert_eq!(frame.product_envelopes().len(), 1);
        assert!(frame.drawables().is_empty() && frame.polylines().is_empty(), "a box is not a body");
        assert_eq!(frame.product_envelope_active, SceneColors::DARK.product_envelope_active);
        frame.apply_colors(SceneColors::LIGHT);
        assert_eq!(frame.product_envelope_active, SceneColors::LIGHT.product_envelope_active);
        assert_eq!(frame.product_envelope_inactive, SceneColors::LIGHT.product_envelope_inactive);
    }

    #[test]
    fn the_two_envelope_inks_stay_visible_on_their_background() {
        let dark = SceneColors::DARK;
        let light = SceneColors::LIGHT;
        assert_ne!(dark.product_envelope_active, dark.product_envelope_inactive, "open and closed must differ");
        assert!(
            (luminance(dark.product_envelope_active) - luminance(dark.product_envelope_inactive)).abs() > 0.1,
            "the active box is visibly not the inactive one"
        );
        assert!(luminance(dark.product_envelope_inactive) > luminance(dark.background));
        assert!(luminance(light.product_envelope_inactive) < luminance(light.background));
        assert!((luminance(light.product_envelope_active) - luminance(light.product_envelope_inactive)).abs() > 0.1);
    }

    #[test]
    fn a_product_envelope_is_picked_by_its_whole_box() {
        let id = Id::new(11).expect("id");
        let mut frame = SceneFrame::default();
        frame.push_product_envelope(envelope(
            11,
            DVec3::new(100.0, 100.0, 100.0),
            DVec3::ZERO,
            ProductEnvelopeState::Open,
        ));

        for point in [DVec3::new(50.0, 50.0, 50.0), DVec3::new(1.0, 1.0, 99.0), DVec3::new(50.0, 50.0, 200.0)] {
            let ray = Ray::new(point + DVec3::Z * 1000.0, -DVec3::Z).expect("ray");
            assert_eq!(frame.pick(ray, 600.0), Some(id), "a click on empty space inside the piece");
        }

        let outside = Ray::new(DVec3::new(150.0, 50.0, 1000.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(outside, 600.0), None);
        let beside = Ray::new(DVec3::new(50.0, 50.0, 1000.0), -DVec3::X).expect("ray");
        assert_eq!(frame.pick(beside, 600.0), None, "a ray may miss the box entirely");
    }

    /// A footprint whose outline leaves one corner of its bounding box empty: the shape of a corner
    /// cabinet, and of any drawn ring that is not a rectangle.
    fn cut_corner_envelope(id: u64) -> ProductEnvelope {
        let outline = [
            DVec2::new(0.0, 0.0),
            DVec2::new(100.0, 0.0),
            DVec2::new(100.0, 40.0),
            DVec2::new(40.0, 40.0),
            DVec2::new(40.0, 100.0),
            DVec2::new(0.0, 100.0),
        ];
        ProductEnvelope {
            id: Id::new(id).expect("id"),
            outline: Arc::from(outline),
            corners: Arc::from(outline),
            height: 100.0,
            world_transform: DMat4::IDENTITY,
            state: ProductEnvelopeState::Open,
        }
    }

    #[test]
    fn a_cut_footprint_is_picked_inside_its_outline_and_never_in_the_cut() {
        let id = Id::new(61).expect("id");
        let mut frame = SceneFrame::default();
        frame.push_product_envelope(cut_corner_envelope(61));

        for point in [DVec3::new(20.0, 20.0, 50.0), DVec3::new(20.0, 80.0, 50.0), DVec3::new(80.0, 20.0, 50.0)] {
            let ray = Ray::new(point + DVec3::Z * 1000.0, -DVec3::Z).expect("ray");
            assert_eq!(frame.pick(ray, 600.0), Some(id), "a click inside the reserved shape selects the piece");
        }
        for point in [DVec3::new(80.0, 80.0, 50.0), DVec3::new(60.0, 60.0, 50.0)] {
            let ray = Ray::new(point + DVec3::Z * 1000.0, -DVec3::Z).expect("ray");
            assert_eq!(frame.pick(ray, 600.0), None, "the cut corner reserves nothing, so it picks nothing");
        }
        let beside = Ray::new(DVec3::new(200.0, 20.0, 2000.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(beside, 600.0), None, "outside the outline nothing is reserved, however tall it is");
    }

    #[test]
    fn a_cut_footprint_is_smaller_than_its_box_when_the_innermost_piece_wins() {
        // The cut shape sits inside a large plain box: the click inside the shape reaches the cut
        // piece because its reserved volume, not its bounding box, is the tighter one.
        let cut = Id::new(71).expect("id");
        let outer = Id::new(72).expect("id");
        let mut frame = SceneFrame::default();
        frame.push_product_envelope(cut_corner_envelope(71));
        frame.push_product_envelope(envelope(72, DVec3::splat(1000.0), DVec3::ZERO, ProductEnvelopeState::Open));
        let ray = Ray::new(DVec3::new(20.0, 20.0, 2000.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(ray, 600.0), Some(cut));
        assert_ne!(outer, cut);
    }

    #[test]
    fn a_solid_wins_over_the_envelope_around_it() {
        let product = Id::new(12).expect("id");
        let panel = Id::new(13).expect("id");
        let mut frame = SceneFrame::default();
        frame.push_product_envelope(envelope(
            12,
            DVec3::new(100.0, 100.0, 100.0),
            DVec3::ZERO,
            ProductEnvelopeState::Open,
        ));
        frame.push(Drawable::new(panel, quad_mesh(), DMat4::IDENTITY, [1.0; 4]));

        let on_panel = Ray::new(DVec3::new(0.5, 0.5, 1000.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(on_panel, 600.0), Some(panel), "a panel under the cursor is the panel");
        let empty = Ray::new(DVec3::new(50.0, 50.0, 1000.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(empty, 600.0), Some(product), "the frame is picked only where no solid is");
    }

    #[test]
    fn the_innermost_product_wins_when_boxes_nest() {
        let outer = Id::new(21).expect("id");
        let inner = Id::new(22).expect("id");
        let mut frame = SceneFrame::default();
        frame.push_product_envelope(envelope(21, DVec3::splat(1000.0), DVec3::ZERO, ProductEnvelopeState::ClosedEmpty));
        frame.push_product_envelope(envelope(
            22,
            DVec3::splat(200.0),
            DVec3::new(300.0, 300.0, 0.0),
            ProductEnvelopeState::ClosedWithParts,
        ));

        let inside_both = Ray::new(DVec3::new(350.0, 350.0, 2000.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(inside_both, 600.0), Some(inner), "the tightest box is the piece meant");
        let outer_only = Ray::new(DVec3::new(900.0, 900.0, 2000.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(outer_only, 600.0), Some(outer));
        assert_ne!(inner, outer);
    }

    #[test]
    fn two_equally_tight_envelopes_tie_to_the_nearer_one() {
        let near = Id::new(51).expect("id");
        let far = Id::new(52).expect("id");
        let mut frame = SceneFrame::default();
        frame.push_product_envelope(envelope(51, DVec3::splat(100.0), DVec3::ZERO, ProductEnvelopeState::Open));
        frame.push_product_envelope(envelope(
            52,
            DVec3::splat(100.0),
            DVec3::new(0.0, 0.0, -500.0),
            ProductEnvelopeState::Open,
        ));
        let ray = Ray::new(DVec3::new(50.0, 50.0, 1000.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(ray, 600.0), Some(near), "the nearer box is the one the click points at");
        let behind = Ray::new(DVec3::new(50.0, 50.0, -1000.0), DVec3::Z).expect("ray");
        assert_eq!(frame.pick(behind, 600.0), Some(far), "from below, the same box is the nearer one");
    }

    #[test]
    fn a_hidden_envelope_stays_pickable_and_a_rotated_one_is_tested_in_its_own_frame() {
        let id = Id::new(31).expect("id");
        let mut frame = SceneFrame::default();
        frame.push_product_envelope(envelope(
            31,
            DVec3::splat(100.0),
            DVec3::ZERO,
            ProductEnvelopeState::ClosedWithParts,
        ));
        let hidden = Ray::new(DVec3::new(50.0, 50.0, 1000.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(hidden, 600.0), Some(id), "a closed piece with parts is still selected");

        let mut frame = SceneFrame::default();
        let mut turned = envelope(31, DVec3::splat(100.0), DVec3::ZERO, ProductEnvelopeState::Open);
        turned.world_transform = DMat4::from_quat(DQuat::from_rotation_z(FRAC_PI_2 / 2.0));
        frame.push_product_envelope(turned);
        let inside = Ray::new(DVec3::new(50.0, 50.0, 1000.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(inside, 600.0), Some(id), "the point is inside the turned box");
        let outside = Ray::new(DVec3::new(70.0, 0.0, 1000.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(outside, 600.0), None, "inside the world bounds but not inside the box");
    }

    #[test]
    fn a_malformed_envelope_is_skipped_instead_of_panicking() {
        let mut frame = SceneFrame::default();
        frame.push_product_envelope(envelope(41, DVec3::ZERO, DVec3::ZERO, ProductEnvelopeState::Open));
        frame.push_product_envelope(envelope(
            42,
            DVec3::new(f64::NAN, 100.0, 100.0),
            DVec3::ZERO,
            ProductEnvelopeState::Open,
        ));
        let mut no_inverse = envelope(43, DVec3::splat(100.0), DVec3::ZERO, ProductEnvelopeState::Open);
        no_inverse.world_transform = DMat4::from_scale(DVec3::ZERO);
        frame.push_product_envelope(no_inverse);
        let ray = Ray::new(DVec3::new(50.0, 50.0, 1000.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(ray, 600.0), None);
    }

    #[test]
    fn world_per_point_follows_the_projection_and_the_depth() {
        let camera = Camera::new(DVec3::ZERO, 1000.0).expect("camera");
        let expected = 2.0 * 1000.0 * (DEFAULT_FOV_Y * 0.5).tan() / 1000.0;
        assert!((camera.world_per_point(1000.0, 1000.0) - expected).abs() < 1.0e-12);
        assert!(camera.world_per_point(2000.0, 1000.0) > camera.world_per_point(1000.0, 1000.0));

        let orthographic = camera.with_projection(Projection::Orthographic { height: 500.0 });
        assert_eq!(orthographic.world_per_point(10.0, 1000.0), 0.5);
        assert_eq!(orthographic.world_per_point(900.0, 1000.0), 0.5);

        assert_eq!(camera.world_per_point(f64::NAN, 1000.0), 0.0);
        assert_eq!(camera.world_per_point(10.0, 0.0), 0.0);
    }

    #[test]
    fn ray_segment_measures_the_closest_approach() {
        let ray = Ray::new(DVec3::new(0.5, 0.0, 10.0), -DVec3::Z).expect("ray");
        let (along, separation) = ray_segment(ray, DVec3::ZERO, DVec3::X).expect("crossing");
        assert!((along - 10.0).abs() < 1.0e-9);
        assert!(separation < 1.0e-9);

        let offset = DVec3::new(0.0, 0.25, 0.0);
        let (_, separation) = ray_segment(ray, offset, offset + DVec3::X).expect("near miss");
        assert!((separation - 0.25).abs() < 1.0e-9);

        let past_the_end = Ray::new(DVec3::new(2.0, 0.0, 10.0), -DVec3::Z).expect("ray");
        let (_, separation) = ray_segment(past_the_end, DVec3::ZERO, DVec3::X).expect("clamped to the end");
        assert!((separation - 1.0).abs() < 1.0e-9);

        let (along, separation) = ray_segment(ray, DVec3::ZERO, DVec3::ZERO).expect("a point");
        assert!((along - 10.0).abs() < 1.0e-9);
        assert!((separation - 0.5).abs() < 1.0e-9, "the zero-length segment is its own start point");

        let behind = Ray::new(DVec3::new(0.5, 0.0, -10.0), -DVec3::Z).expect("ray");
        assert_eq!(ray_segment(behind, DVec3::ZERO, DVec3::X), None);
        assert_eq!(ray_segment(ray, DVec3::new(f64::NAN, 0.0, 0.0), DVec3::X), None);
    }

    #[test]
    fn picking_a_path_returns_its_document_id_near_the_line() {
        let id = Id::new(6).expect("id");
        let mut frame = SceneFrame::default();
        let points: Arc<[DVec3]> = Arc::from(vec![DVec3::ZERO, DVec3::X]);
        frame.push_polyline(Polyline::new(id, points, DMat4::IDENTITY, [1.0; 4]));

        // At ten millimetres deep and a 600-point viewport, six points are about 0.083 mm.
        let near = Ray::new(DVec3::new(0.5, 0.04, 10.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(near, 600.0), Some(id));
        let far = Ray::new(DVec3::new(0.5, 0.4, 10.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(far, 600.0), None);

        let exact = Ray::new(DVec3::new(0.5, 0.0, 10.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(exact, 0.0), Some(id), "an exact crossing needs no tolerance");
        assert_eq!(frame.pick(near, 0.0), None, "a broken viewport height means no radius");

        let miss = Ray::new(DVec3::new(5.0, 0.0, 10.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(miss, 600.0), None, "past the end of the chain");
    }

    #[test]
    fn picking_follows_the_polyline_world_transform() {
        let id = Id::new(7).expect("id");
        let mut frame = SceneFrame::default();
        let points: Arc<[DVec3]> = Arc::from(vec![DVec3::ZERO, DVec3::X]);
        let transform = DMat4::from_translation(DVec3::new(10.0, 0.0, 0.0));
        frame.push_polyline(Polyline::new(id, points, transform, [1.0; 4]));

        let hit = Ray::new(DVec3::new(10.5, 0.01, 10.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(hit, 600.0), Some(id));
        let original_position = Ray::new(DVec3::new(0.5, 0.0, 10.0), -DVec3::Z).expect("ray");
        assert_eq!(frame.pick(original_position, 600.0), None);
    }

    #[test]
    fn the_nearest_of_a_solid_and_a_path_wins_and_a_path_wins_a_tie() {
        let solid = Id::new(8).expect("id");
        let path = Id::new(9).expect("id");
        let points: Arc<[DVec3]> = Arc::from(vec![DVec3::new(0.0, 0.5, 0.0), DVec3::new(1.0, 0.5, 0.0)]);
        let above = Ray::new(DVec3::new(0.5, 0.5, 10.0), -DVec3::Z).expect("ray");

        let mut frame = SceneFrame::default();
        frame.push(Drawable::new(solid, quad_mesh(), DMat4::IDENTITY, [1.0; 4]));
        frame.push_polyline(Polyline::new(path, points.clone(), DMat4::IDENTITY, [1.0; 4]));
        assert_eq!(frame.pick(above, 600.0), Some(path), "a path on a face stays selectable");

        frame = SceneFrame::default();
        frame.push(Drawable::new(solid, quad_mesh(), DMat4::IDENTITY, [1.0; 4]));
        let below = DMat4::from_translation(DVec3::new(0.0, 0.0, -1.0));
        frame.push_polyline(Polyline::new(path, points, below, [1.0; 4]));
        assert_eq!(frame.pick(above, 600.0), Some(solid), "an occluded path loses to the solid");
    }

    #[test]
    fn malformed_polylines_are_skipped_instead_of_panicking() {
        let id = Id::new(10).expect("id");
        let ray = Ray::new(DVec3::new(0.5, 0.0, 10.0), -DVec3::Z).expect("ray");
        let mut frame = SceneFrame::default();
        let broken: Arc<[DVec3]> = Arc::from(vec![DVec3::new(f64::NAN, 0.0, 0.0), DVec3::X]);
        frame.push_polyline(Polyline::new(id, broken, DMat4::IDENTITY, [1.0; 4]));
        assert_eq!(frame.pick(ray, 600.0), None);

        let mut frame = SceneFrame::default();
        let points: Arc<[DVec3]> = Arc::from(vec![DVec3::ZERO, DVec3::X]);
        let broken = DMat4::from_translation(DVec3::new(f64::INFINITY, 0.0, 0.0));
        frame.push_polyline(Polyline::new(id, points, broken, [1.0; 4]));
        assert_eq!(frame.pick(ray, 600.0), None);

        let mut frame = SceneFrame::default();
        let single: Arc<[DVec3]> = Arc::from(vec![DVec3::ZERO]);
        frame.push_polyline(Polyline::new(id, single, DMat4::IDENTITY, [1.0; 4]));
        assert_eq!(frame.pick(ray, 600.0), None, "one point has no segment");
    }

    #[test]
    fn selection_highlight_tints_every_selected_drawable() {
        let selected = Id::new(2).expect("id");
        let also_selected = Id::new(4).expect("id");
        let other = Id::new(3).expect("id");
        let mut frame = SceneFrame::default();
        frame.push(Drawable::new(selected, quad_mesh(), DMat4::IDENTITY, [0.5, 0.5, 0.5, 1.0]));
        frame.push(Drawable::new(other, quad_mesh(), DMat4::IDENTITY, [0.5, 0.5, 0.5, 1.0]));
        frame.push(Drawable::new(also_selected, quad_mesh(), DMat4::IDENTITY, [0.5, 0.5, 0.5, 1.0]));

        let highlighted = frame.with_selection_highlight(&[selected, also_selected]);
        assert_ne!(highlighted.drawables()[0].color, frame.drawables()[0].color);
        assert_ne!(highlighted.drawables()[2].color, frame.drawables()[2].color);
        assert_eq!(highlighted.drawables()[1].color, frame.drawables()[1].color);
        assert_eq!(frame.drawables()[0].color, [0.5, 0.5, 0.5, 1.0]);

        assert_eq!(frame.with_selection_highlight(&[]), frame);
        let missing = Id::new(99).expect("id");
        assert_eq!(frame.with_selection_highlight(&[missing]), frame);
    }

    /// One drawable whose extent is exactly the box the test names.
    ///
    /// The box arrives the way the scene walk hands it over (X56), so the culling reads one value
    /// and the mesh is never measured.
    fn boxed(value: u64, min: DVec3, max: DVec3) -> Drawable {
        let bounds = Aabb::of_points([min, max]).expect("a finite box");
        let edges: Arc<[u32]> = Arc::from(vec![0, 1, 1, 2]);
        Drawable::with_box(Id::new(value).expect("id"), quad_mesh(), DMat4::IDENTITY, [1.0; 4], Some(bounds))
            .with_edges(edges)
    }

    /// A cube of `side` millimetres with its minimum corner at the origin, in the world at `at`.
    fn cube(value: u64, at: DVec3, side: f64) -> Drawable {
        let mut drawable = boxed(value, DVec3::ZERO, DVec3::splat(side));
        drawable.world_transform = DMat4::from_translation(at);
        drawable
    }

    /// A field of cubes a thousand millimetres apart, the camera inside it at the origin of the
    /// field: the wall X58 is measured on, cut down to a test.
    fn cube_field(side: u64, spacing: f64) -> SceneFrame {
        let mut frame = SceneFrame::default();
        let half = side as i64 / 2;
        for row in -half..=half {
            for column in -half..=half {
                let value = (row + half) as u64 * side + (column + half) as u64 + 1;
                let at = DVec3::new(column as f64 * spacing, row as f64 * spacing, 0.0);
                frame.push(cube(value, at, 100.0));
            }
        }
        frame
    }

    #[test]
    fn only_the_drawables_the_view_volume_meets_are_drawn() {
        // X58: the renderer drew every drawable of the frame. A camera inside a dense scene now
        // keeps the drawables its volume reaches and drops the rest, so an orbit over a wall of
        // cabinets pays for what is on screen.
        let frame = cube_field(41, 1_000.0);
        assert_eq!(frame.drawables().len(), 1_681, "a 41 x 41 field is the dense scene");
        let camera = Camera::new(DVec3::ZERO, 4_000.0).expect("camera");
        let frustum = Frustum::of_camera(camera, 1.0);
        let mut drawn = Vec::new();
        frustum.visible(frame.drawables().iter().map(Drawable::world_bounds), &mut drawn);

        let expected: Vec<usize> = (0..frame.drawables().len())
            .filter(|index| {
                let bounds = frame.drawables()[*index].world_bounds().expect("a measured box");
                frustum.meets(bounds)
            })
            .collect();
        assert_eq!(drawn, expected, "the list is exactly the drawables the volume meets");
        assert!(drawn.len() < frame.drawables().len() / 4, "the camera inside the field drops most of it");
        assert!(drawn.len() > 1, "and still draws the field around it, {} drawables", drawn.len());
        for index in &drawn {
            let drawable = &frame.drawables()[*index];
            assert!(drawable.edges.is_some(), "a drawn drawable keeps the edge list it was built with");
        }

        // Framing the whole field draws every drawable again: culling never hides what is in view.
        let whole = Camera::default().looked_from(DVec3::new(0.0, 0.0, 80_000.0), DVec3::X, DVec3::Y, -DVec3::Z);
        Frustum::of_camera(whole, 1.0).visible(frame.drawables().iter().map(Drawable::world_bounds), &mut drawn);
        assert_eq!(drawn.len(), frame.drawables().len(), "a camera far enough draws the whole field");
    }

    #[test]
    fn a_box_that_straddles_the_eye_is_kept() {
        // The awkward case: the near plane cuts the box, and the eye is inside it. A box is dropped
        // only when it lies wholly outside one plane, so the part of it in front of the eye is drawn.
        let camera = Camera::default().looked_from(DVec3::new(0.0, 0.0, 1_000.0), DVec3::X, DVec3::Y, -DVec3::Z);
        let frustum = Frustum::of_camera(camera, 1.0);
        let mut frame = SceneFrame::default();
        frame.push(boxed(1, DVec3::new(-50.0, -50.0, 995.0), DVec3::new(50.0, 50.0, 1_105.0)));
        frame.push(boxed(2, DVec3::new(-50.0, -50.0, 1_020.0), DVec3::new(50.0, 50.0, 1_120.0)));

        let mut drawn = Vec::new();
        frustum.visible(frame.drawables().iter().map(Drawable::world_bounds), &mut drawn);
        assert_eq!(drawn, vec![0], "the straddling box stays, the one wholly behind the eye goes");
    }

    #[test]
    fn a_box_whose_extent_was_never_measured_is_never_culled() {
        // An unknown extent is not an empty one: a drawable the frame did not measure is drawn,
        // because hiding a body is worse than drawing an invisible one.
        let camera = Camera::new(DVec3::new(1.0e6, 0.0, 0.0), 500.0).expect("camera");
        let frustum = Frustum::of_camera(camera, 1.0);
        let measured = Aabb::of_points([DVec3::ZERO, DVec3::X]).expect("a box");
        assert!(!frustum.meets(measured), "the measured box is far outside the volume");

        let mut drawn = Vec::new();
        frustum.visible([None, Some(measured)], &mut drawn);
        assert_eq!(drawn, vec![0], "the unmeasured box is kept, the measured one is dropped");
    }

    #[test]
    fn a_camera_is_a_pinhole_until_a_render_gives_it_a_lens() {
        let camera = Camera::default();
        assert!(camera.is_pinhole(), "the default camera is a pinhole");
        assert_eq!(camera.aperture(), 0.0);
        assert_eq!(camera.focus(), camera::DEFAULT_FOCUS_MM, "a lens that is switched on focuses where the view is");

        // A lens changes neither the eye nor the basis: only the ray of one sample does.
        let lens = camera.with_lens(50.0, 2_000.0).expect("a lens");
        assert!(!lens.is_pinhole());
        assert_eq!((lens.aperture(), lens.focus()), (50.0, 2_000.0));
        assert_eq!(lens.eye(), camera.eye());
        assert_eq!(lens.forward(), camera.forward());
        assert_eq!(lens.projection(), camera.projection());
        // A zero radius keeps the pinhole and is a lens that costs nothing.
        assert!(lens.with_lens(0.0, 2_000.0).expect("a pinhole").is_pinhole());

        // Both ends of the range are accepted and just outside them is a sentence.
        assert!(camera.with_lens(camera::MAX_APERTURE_MM, 1.0).is_ok());
        for (aperture, focus) in [
            (-1.0, 1_000.0),
            (camera::MAX_APERTURE_MM + 1.0, 1_000.0),
            (f64::NAN, 1_000.0),
            (10.0, 0.0),
            (10.0, f64::INFINITY),
        ] {
            assert!(camera.with_lens(aperture, focus).is_err(), "{aperture} mm at {focus} mm");
        }
    }

    #[test]
    fn a_window_of_a_known_area_carries_the_radiance_its_watts_are() {
        // A one-metre square radiating 100 W: the radiance is `P / (PI * A)` and the frame keeps the
        // watts, so the conversion is one function with one test (ADR 0247).
        let square = AreaLight {
            origin: DVec3::ZERO,
            half_width: DVec3::new(500.0, 0.0, 0.0),
            half_height: DVec3::new(0.0, 500.0, 0.0),
            color: [1.0, 1.0, 1.0],
            power: 100.0,
        };
        assert!((square.area_mm2() - 1_000_000.0).abs() < 1.0, "a square metre is a million square millimetres");
        let radiance = square.radiance();
        let expected = 100.0 / std::f64::consts::PI;
        for channel in radiance {
            assert!((f64::from(channel) - expected).abs() < 1.0e-4, "{radiance:?} is not {expected}");
        }
        // Radiance grows with the watts and falls with the area, because the power is spread over it.
        let brighter = AreaLight { power: 200.0, ..square };
        assert!(brighter.radiance()[0] > radiance[0]);
        let wider = AreaLight { half_width: DVec3::new(1_000.0, 0.0, 0.0), ..square };
        assert!(wider.radiance()[0] < radiance[0]);
        // A rectangle of a colour lights in that colour, and one of no power or no extent is dark.
        let tinted = AreaLight { color: [1.0, 0.5, 0.25], ..square };
        assert!((tinted.radiance()[1] / tinted.radiance()[0] - 0.5).abs() < 1.0e-6);
        assert_eq!(AreaLight { power: 0.0, ..square }.radiance(), [0.0; 3]);
        assert_eq!(AreaLight { half_width: DVec3::ZERO, ..square }.radiance(), [0.0; 3]);
        assert_eq!(AreaLight { power: f32::NEG_INFINITY, ..square }.radiance(), [0.0; 3]);
    }

    #[test]
    fn a_window_draws_the_four_corners_of_its_face() {
        let window = AreaLight {
            origin: DVec3::new(0.0, 0.0, 1_000.0),
            half_width: DVec3::new(600.0, 0.0, 0.0),
            half_height: DVec3::new(0.0, 700.0, 0.0),
            color: [1.0; 3],
            power: 40.0,
        };
        let corners = window.corners();
        assert_eq!(corners[0], DVec3::new(-600.0, -700.0, 1_000.0));
        assert_eq!(corners[2], DVec3::new(600.0, 700.0, 1_000.0));
        // The ring is counter-clockwise seen from the emitting side: the corners in order cross to a
        // normal that points the way the light emits.
        let across = (corners[1] - corners[0]).cross(corners[3] - corners[0]);
        assert!(across.normalize().dot(window.normal()) > 0.999, "{across:?}");
        assert!(window.normal().dot(-DVec3::Z) > 1.0 - 1.0e-9, "a light with its height along +Y emits along -Z");
        let turned = AreaLight { half_height: DVec3::new(0.0, -700.0, 0.0), ..window };
        assert!(turned.normal().dot(DVec3::Z) > 1.0 - 1.0e-9, "the same face the other way round emits the other way");
        assert!((window.area_mm2() - 1_200.0 * 1_400.0).abs() < 1.0e-6);
    }
}
