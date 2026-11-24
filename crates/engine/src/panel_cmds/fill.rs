//! The part a point inside a void names: one command over the niche measurement.
//!
//! A post, a shelf and a back wall are one question (`Project::niche`, `Project::fill_plan`): which
//! board fills the space the walls leave, where does it stand and which outline does it store. The
//! window answers it twice — as the ghost it paints and as the part the click writes — and a caller
//! that cannot point had to redo the arithmetic itself, in millimetres and quaternions, and could
//! disagree with the ribbon about the class's axis pair. This command is that answer as one write:
//! the point, the class and the mode in, the measured part out.
//!
//! The measurement is the model's, not a second copy of it: the niche is measured at the point with
//! no sight — a program has no hidden parts to look through (ADR 0175) — and the plan it becomes is
//! the same [`FillPlan`](model::FillPlan) the ghost draws. The board and its thickness are the
//! catalogue's first panel material, which is what every created panel takes.
//!
//! The part is written through [`create_panel`], the one creation path `panel.create` uses, so the
//! two commands store a board the same way: one revision, one undo entry, the class in the same
//! command. The product is named by the caller and not by the session: a caller works in the piece
//! it is sure about, and whether a person has that piece open is the interface's question
//! (ADR 0098, ADR 0226).
//!
//! Changes when: the fill gains a class trait, a refusal or a third reading of the section.

use catalog::Directory;
use geom::{Contact, DVec3};
use model::{ClassCode, Fill, FillMode, FillMount, FillRequest, Id, Level, Project, Sight};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use text::{Lang, Message};

use super::PanelCreate;
#[cfg(test)]
use super::panel::PanelBuildSet;
use super::panel::create_panel;
use crate::class_rules::allow_creation;
use crate::commands::{CommandError, CommandOutcome, CommandSpec, Creation};
use crate::session::Session;

/// Stable id of the niche-filling command.
pub const FILL_ID: &str = "panel.fill";

/// User-facing label of the niche-filling command.
pub const FILL_LABEL: Message = Message::CmdFillNiche;

/// Parameters for `panel.fill`.
///
/// The three that name the part are the point inside the void, the class whose `fill` trait says
/// which pair of the niche's sides the board takes, and the mode that says whether the board is the
/// largest rectangle or the shape the walls leave.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelFill {
    /// Id of the container the part lands in: the product itself or a group inside it.
    pub parent: Id,
    /// The point inside the void, in world millimetres, exactly as `query niche` takes it.
    pub at: [f64; 3],
    /// The class the part carries; its `fill` trait chooses the axis pair.
    pub class: ClassCode,
    /// How the board reads the section the walls leave; the largest rectangle when absent.
    #[serde(default)]
    pub mode: FillMode,
    /// The place along the part's free axis, in the parent frame and millimetres.
    ///
    /// `None` lets the point's own coordinate answer, which is what a caller wants when it measured
    /// the void itself. A tool that carries the part across a board hands in the place it drew.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free: Option<f64>,
    /// The part's name; absent, it is the class's own word, made unique among its siblings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// How the board sits against the frame: inside the opening, or laid over its face.
    ///
    /// A mounting is a creation key, not a stored form (ADR 0240): an overlay board is a board one
    /// thickness in front of the void whose ends run past the opening by the lap the frame allows.
    #[serde(default, skip_serializing_if = "FillMount::is_inset")]
    pub mount: FillMount,
}

impl PanelFill {
    fn parse(params: &Value) -> Result<Self, CommandError> {
        serde_json::from_value(params.clone())
            .map_err(|error| CommandError::InvalidParams { command: FILL_ID, message: error.to_string() })
    }

    /// The axis pair the class's `fill` trait names, or the reason the class fills nothing.
    fn fill(&self) -> Result<Fill, CommandError> {
        self.class.traits().and_then(|traits| traits.fill()).ok_or_else(|| CommandError::InvalidParams {
            command: FILL_ID,
            message: format!("class {} does not fill a void", self.class),
        })
    }
}

/// The `panel.fill` command.
pub(crate) const FILL: CommandSpec =
    CommandSpec::new::<PanelFill>(FILL_ID, FILL_LABEL, fill_enabled, fill_run).creating(Creation::new(Level::Part));

fn fill_enabled(session: &Session, params: &Value) -> bool {
    let Ok(params) = PanelFill::parse(params) else {
        return false;
    };
    let project = session.project();
    // The interface's reading, exactly as `panel.create` asks it: the button is offered only inside
    // the open piece. A dispatch writes where the parameters name (ADR 0226).
    params.class.is_assignable()
        && project.is_container(params.parent)
        && params.fill().is_ok()
        && allow_creation(project, params.parent, session.active_product()).is_ok()
        && session.directory().boards().next().is_some()
}

fn fill_run(
    project: &mut Project,
    catalog: &Directory,
    params: &Value,
    contact: &dyn Contact,
) -> Result<CommandOutcome, CommandError> {
    let params = PanelFill::parse(params)?;
    let fill = params.fill()?;
    let board = catalog.boards().next().ok_or(CommandError::NoMaterial { command: FILL_ID })?;
    let point = DVec3::from_array(params.at);
    // One measurement, the query's own: a program has nothing switched off, so it reads every face
    // in the way (ADR 0175, ADR 0226).
    let niche = project
        .niche(point, Sight::none(), Some(contact))
        .map_err(|refusal| CommandError::NoNiche { message: crate::read::no_niche_sentence(params.at, refusal) })?;
    let request = FillRequest::new(point, fill, params.mode, board.thickness, params.free).with_mount(params.mount);
    let plan = project
        .fill_plan_in(&niche, params.parent, request, Sight::none())
        .map_err(|refusal| CommandError::FillRefused { command: FILL_ID, refusal })?;
    let base = params.name.clone().unwrap_or_else(|| class_word(params.class));
    let name = project.unique_child_name(params.parent, &base);
    let create = PanelCreate {
        parent: params.parent,
        name,
        nominal_size: plan.nominal_size,
        material: board.id,
        placement: plan.placement,
        class: Some(params.class),
        outline: Some(plan.outline),
        bevels: plan.bevels,
        trims: plan.trims,
    };
    let id = create_panel(project, catalog, create)?;
    Ok(CommandOutcome::created(id))
}

/// The word a part is named by when the caller names none: its class's own label.
fn class_word(class: ClassCode) -> String {
    class.label().map_or_else(|| Message::ToolPanel.text(Lang::En).to_owned(), |label| label.text(Lang::En).to_owned())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use geom::DVec3;
    use model::{Container, Envelope, PanelNominalSize, Placement, Project};
    use testkit::{millimetres, sample_catalog};

    use crate::batch::{BatchEffect, BatchOptions, BatchStep};
    use crate::guide::Stance;
    use crate::read::Query;

    use super::*;

    /// A session whose open product is an empty 600 x 560 x 720 cabinet.
    fn session() -> (Session, Id) {
        let root = Container::new(Id::new(1).expect("an id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("an empty project");
        let root = project.root().id();
        let envelope = Envelope::new(millimetres(600), millimetres(560), millimetres(720)).expect("a box");
        let product = project
            .add_product(root, "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a product");
        let mut session = Session::with_directory(project, sample_catalog());
        assert!(session.open_product(product), "the fixture opens its piece");
        (session, product)
    }

    /// The request the tests ask with: the middle of the empty cabinet.
    fn request(product: Id, class: ClassCode, mode: FillMode) -> PanelFill {
        PanelFill {
            parent: product,
            at: [300.0, 280.0, 360.0],
            class,
            mode,
            free: None,
            name: None,
            mount: FillMount::Inset,
        }
    }

    fn json_params<T: serde::Serialize>(value: &T) -> Value {
        serde_json::to_value(value).expect("the parameters serialize")
    }

    /// A case whose four members leave a 600 x 720 opening, and the point on it.
    ///
    /// Each member stands flush with the front plane and runs back through the case, so every edge of
    /// the opening has a part behind it for a front to lap over (U124, ADR 0240).
    fn framed() -> (Session, Id) {
        use model::{ClassCode, Container, Envelope, PanelNominalSize, Placement};
        let root = Container::new(Id::new(1).expect("an id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("an empty project");
        let root = project.root().id();
        let envelope = Envelope::new(millimetres(636), millimetres(560), millimetres(756)).expect("a box");
        let product = project
            .add_product(root, "Case", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a case");
        let boxed = geom::DQuat::from_rotation_axes(geom::DVec3::Y, geom::DVec3::Z, geom::DVec3::X);
        for (at, length, width) in [
            (geom::DVec3::ZERO, 560.0, 756.0),
            (geom::DVec3::new(618.0, 0.0, 0.0), 560.0, 756.0),
            (geom::DVec3::new(18.0, 0.0, 0.0), 600.0, 18.0),
            (geom::DVec3::new(18.0, 0.0, 738.0), 600.0, 18.0),
        ] {
            let placement = Placement::new(at, boxed).expect("a member placement");
            let size = PanelNominalSize::new(
                units::Length::from_millimetres(length as i64).expect("a length"),
                units::Length::from_millimetres(width as i64).expect("a width"),
            )
            .expect("a member size");
            let _ = project.add_panel(product, "Member", size, None, millimetres(18), placement);
        }
        let mut session = Session::with_directory(project, sample_catalog());
        assert!(session.open_product(product), "the fixture opens its case");
        (session, product)
    }

    #[test]
    fn an_overlay_click_writes_one_front_with_the_laps_the_frame_allowed() {
        // The mounting is a creation key (ADR 0240): one `panel.fill` with `overlay` makes the front
        // the ghost drew — 628 x 748 over a 600 x 720 opening — and the four trims ride in the same
        // undo entry.
        let (mut session, product) = framed();
        let mut params = request(product, ClassCode::BACK_PANEL, FillMode::Fit);
        params.at = [318.0, 280.0, 378.0];
        params.mount = FillMount::Overlay { laps: [millimetres(14); 4] };
        let id =
            session.dispatch(FILL_ID, &json_params(&params)).expect("the front lands").created_id().expect("an id");
        let panel = session.project().panel(id).expect("a panel");
        assert_eq!(
            panel.nominal_size(),
            PanelNominalSize::new(millimetres(748), millimetres(628)).expect("a size"),
            "the opening plus a 14 mm lap at each end"
        );
        assert_eq!(panel.trims().as_array(), [millimetres(-14); 4], "each end carries its lap");
        // Its inner face is the plane the void opens onto, so the whole body stands in front of it.
        let placement = session.project().world_placement(id).expect("a placement");
        let inner = placement.transform_point(geom::DVec3::new(374.0, 314.0, 18.0));
        assert!(inner.y.abs() < 1.0e-9, "the inner face lies on the opening's plane: {inner:?}");
        // One part, one undo entry.
        session.undo().expect("one undo step");
        assert_eq!(session.project().panel(id), None, "the whole part comes back");
        assert!(!session.can_undo(), "the fill was one entry");
    }

    #[test]
    fn a_part_rewrites_its_build_in_the_void_it_fills() {
        // The one row a board's own menu carries (U123): a shelf fitted as the largest rectangle is
        // re-cut as the niche's shape in the same void, one undo entry, and its class never moves.
        let (mut session, product) = session();
        let id = session
            .dispatch(FILL_ID, &json_params(&request(product, ClassCode::HORIZONTAL_PANEL, FillMode::Fit)))
            .expect("the shelf fills the void")
            .created_id()
            .expect("a panel id");
        let fitted = session.project().panel(id).expect("a panel").nominal_size();
        let revision = session.project().revision();

        session
            .dispatch(
                crate::panel_cmds::BUILD_SET_ID,
                &json_params(&PanelBuildSet { panel: id, mode: FillMode::Shape }),
            )
            .expect("the build changes");
        let shaped = session.project().panel(id).expect("the panel is still there");
        assert_eq!(session.project().class(id), Some(ClassCode::HORIZONTAL_PANEL), "the role stays");
        assert_eq!(session.project().revision(), revision.checked_next().expect("revision"), "one revision");
        assert_eq!(shaped.nominal_size(), fitted, "a straight void answers the same rectangle either way");

        session.undo().expect("one undo step");
        assert_eq!(
            session.project().panel(id).expect("the panel came back").nominal_size(),
            fitted,
            "one entry takes the whole change back"
        );
        // The fill itself is the entry behind it: two undos take the whole story back, so the build
        // change was exactly one entry.
        session.undo().expect("the fill");
        assert!(session.project().panel(id).is_none(), "one more undo takes the part");

        // A node that is not a panel is refused, and nothing moves.
        let refusal = session.dispatch(
            crate::panel_cmds::BUILD_SET_ID,
            &json_params(&PanelBuildSet { panel: product, mode: FillMode::Shape }),
        );
        assert!(refusal.is_err(), "a product has no build to rewrite");
    }

    #[test]
    fn a_post_a_shelf_and_a_wall_take_their_class_their_stance_and_their_size() {
        // The three ribbon cells are one command: the class names the part, its `fill` trait picks
        // the axis pair and the stance follows, and the niche's own size is the board's (N37).
        let cases: [(ClassCode, usize, i64, i64); 3] = [
            (ClassCode::HORIZONTAL_PANEL, 0, 600, 560),
            (ClassCode::VERTICAL_PANEL, 1, 720, 560),
            (ClassCode::BACK_PANEL, 2, 720, 600),
        ];
        for (class, stance, length, width) in cases {
            let (mut session, product) = session();
            let id = session
                .dispatch(FILL_ID, &json_params(&request(product, class, FillMode::Fit)))
                .unwrap_or_else(|error| panic!("{class} fills the void: {error}"))
                .created_id()
                .expect("a panel id");
            let panel = session.project().panel(id).expect("a panel");
            assert_eq!(session.project().class(id), Some(class), "{class}: the class travels with the part");
            assert_eq!(
                panel.nominal_size(),
                PanelNominalSize::new(millimetres(length), millimetres(width)).expect("a size"),
                "{class}: the board is the void's own pair"
            );
            // The stance is the documented one, not an equivalent quarter-turn: the panel's own axes
            // land where ADR 0105 says they do.
            let placed = session.project().world_placement(id).expect("a placement");
            let expected = Stance::ALL[stance].placement().expect("a unit rotation");
            for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
                assert!(
                    (placed.transform_vector(axis) - expected.transform_vector(axis)).length() < 1e-9,
                    "{class}: the {} stance's axes are {axis}",
                    Stance::ALL[stance].name()
                );
            }
            // The point's own coordinate answers on the free axis, and the board grows away from the
            // face it named: the hand's place is one of the part's two faces (ADR 0103, ADR 0105).
            let free = model::free_axis(class.traits().and_then(|traits| traits.fill()).expect("a fill pair"));
            let hand = request(product, class, FillMode::Fit).at[free];
            let base = placed.translation()[free];
            let far =
                base + placed.transform_vector(DVec3::Z)[free] * testkit::sample_panel_thickness().millimetres_f64();
            assert!(
                (base - hand).abs() < 1e-9 || (far - hand).abs() < 1e-9,
                "{class}: the hand named {hand}, and the board spans {base}..{far}"
            );
            // One part, one undo entry.
            session.undo().expect("one undo step");
            assert_eq!(session.project().panel(id), None, "{class}: the whole part comes back");
            assert!(!session.can_undo(), "{class}: the fill was one entry");
        }
    }

    #[test]
    fn the_free_place_pins_the_part_and_the_name_is_its_class_made_unique() {
        let (mut session, product) = session();
        let mut first = request(product, ClassCode::HORIZONTAL_PANEL, FillMode::Fit);
        first.free = Some(100.0);
        let id = session.dispatch(FILL_ID, &json_params(&first)).expect("a pinned shelf").created_id().expect("an id");
        let free = model::free_axis(model::Fill::WidthAndDepth);
        let placed = session.project().world_placement(id).expect("a placement");
        assert!((placed.translation()[free] - 100.0).abs() < 1e-9, "the number the caller pinned is where it sits");
        let second =
            session.dispatch(FILL_ID, &json_params(&first)).expect("a second shelf").created_id().expect("an id");
        let one = session.project().node(id).expect("a node").name().to_owned();
        let two = session.project().node(second).expect("a node").name().to_owned();
        assert_ne!(one, two, "two shelves of one piece are told apart: {one} / {two}");
    }

    #[test]
    fn an_overlay_front_keeps_its_laps_when_the_build_changes() {
        // The build row must not turn a front into a board inside the opening: the part is planned
        // again as the overlay it is, measured from its own inner face, so its size and its trims
        // survive the other build (ADR 0240).
        let (mut session, product) = framed();
        let mut params = request(product, ClassCode::BACK_PANEL, FillMode::Fit);
        params.at = [318.0, 280.0, 378.0];
        params.mount = FillMount::Overlay { laps: [millimetres(14); 4] };
        let id =
            session.dispatch(FILL_ID, &json_params(&params)).expect("the front lands").created_id().expect("an id");
        let before = session.project().panel(id).expect("a panel").nominal_size();
        let revision = session.project().revision();
        session
            .dispatch(
                crate::panel_cmds::BUILD_SET_ID,
                &json_params(&PanelBuildSet { panel: id, mode: FillMode::Shape }),
            )
            .expect("the build changes");
        let panel = session.project().panel(id).expect("the panel is still there");
        assert_eq!(panel.nominal_size(), before, "a front is the opening either way");
        assert_eq!(panel.trims().as_array(), [millimetres(-14); 4], "and it keeps the laps it took");
        assert_eq!(session.project().revision(), revision.checked_next().expect("revision"), "one revision");
        // The front still stands one thickness in front of the opening's plane.
        let placement = session.project().world_placement(id).expect("a placement");
        let inner = placement.transform_point(geom::DVec3::new(374.0, 314.0, 18.0));
        assert!(inner.y.abs() < 1.0e-6, "the inner face stays on the opening: {inner:?}");
    }

    #[test]
    fn a_dry_run_answers_the_part_and_writes_nothing() {
        let (mut session, product) = session();
        let before = session.project().revision();
        let steps =
            [BatchStep::new(FILL_ID, json_params(&request(product, ClassCode::HORIZONTAL_PANEL, FillMode::Fit)))];
        let planned = session.batch(&steps, BatchOptions::ALL_OR_NOTHING.dry_run(true));
        assert_eq!(planned.effect(), BatchEffect::Planned, "the dry run claims no write");
        let made = *planned.created().first().expect("the answer names the part it would make");
        assert_eq!(session.project().panel(made), None, "nothing was written");
        assert_eq!(session.project().revision(), before, "the document is untouched");
        let landed = session.batch(&steps, BatchOptions::ALL_OR_NOTHING);
        assert_eq!(landed.effect(), BatchEffect::Landed);
        assert_eq!(landed.created(), planned.created(), "the same step makes the same part");
        assert!(session.project().panel(made).is_some(), "the second run writes it");
    }

    #[test]
    fn a_point_outside_every_product_is_the_query_s_own_refusal() {
        let (mut session, product) = session();
        let mut params = request(product, ClassCode::HORIZONTAL_PANEL, FillMode::Fit);
        params.at = [5_000.0; 3];
        let refused = session.dispatch(FILL_ID, &json_params(&params)).expect_err("there is no void there");
        let asked = session.read(Query::Niche { at: params.at }).expect_err("the query refuses the same point");
        assert_eq!(refused.to_string(), asked.to_string(), "one sentence for one point");
        assert!(!session.can_undo(), "a refusal records no history");
    }

    #[test]
    fn a_void_in_another_product_and_a_class_without_a_fill_pair_are_both_refused() {
        // Two products side by side: a point in the second names a void that is not the first's part.
        let root = Container::new(Id::new(1).expect("an id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("an empty project");
        let root = project.root().id();
        let one = Envelope::new(millimetres(600), millimetres(560), millimetres(720)).expect("a box");
        let two = Envelope::new(millimetres(600), millimetres(560), millimetres(720)).expect("a box");
        let first =
            project.add_product(root, "One", ClassCode::GENERAL_PRODUCT, one, Placement::IDENTITY).expect("a product");
        let beside = Placement::IDENTITY
            .with_translation(millimetres(1_000), units::Length::ZERO, units::Length::ZERO)
            .expect("a translation");
        project.add_product(root, "Two", ClassCode::GENERAL_PRODUCT, two, beside).expect("a product beside it");
        let mut session = Session::with_directory(project, sample_catalog());
        assert!(session.open_product(first), "the first piece is open");

        let mut elsewhere = request(first, ClassCode::HORIZONTAL_PANEL, FillMode::Fit);
        elsewhere.at = [1_300.0, 280.0, 360.0];
        let refused = session.dispatch(FILL_ID, &json_params(&elsewhere)).expect_err("the void is the other piece");
        assert!(refused.to_string().contains("another product"), "{refused}");
        assert!(!session.can_undo(), "a refusal records no history");

        // A class with no `fill` trait has no pair to take, so the void is not its part.
        let class = ClassCode::GENERAL_PRODUCT;
        assert!(class.traits().and_then(|traits| traits.fill()).is_none(), "the fixture class fills nothing");
        let refused =
            session.dispatch(FILL_ID, &json_params(&request(first, class, FillMode::Fit))).expect_err("no pair");
        assert!(refused.to_string().contains("does not fill a void"), "{refused}");
        assert!(!session.can_undo(), "a refusal records no history");
    }

    #[test]
    fn a_workshop_with_no_board_has_nothing_to_cut_the_part_from() {
        let (session, product) = session();
        let mut empty = Session::new(session.project().clone());
        assert!(empty.open_product(product), "the piece is open, so the refusal is the catalogue's");
        let refused = empty
            .dispatch(FILL_ID, &json_params(&request(product, ClassCode::HORIZONTAL_PANEL, FillMode::Fit)))
            .expect_err("the catalogue holds no board");
        assert!(matches!(refused, CommandError::NoMaterial { .. }), "{refused}");
        assert!(!empty.can_undo(), "a refusal records no history");
    }
}
