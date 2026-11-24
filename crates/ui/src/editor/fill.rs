//! The empty-space tool: the class that fills a niche, and the arm that owns the next click.
//!
//! A post and a shelf are the same gesture with a different class: the class's `fill` trait says
//! which pair of the niche's sides the part takes (ADR 0099), the class names the part in the tree,
//! and the shell measures the void with [`model::Project::niche`] (ADR 0103). The editor
//! only holds *which* class is armed, so the tool survives a document edit and never stores a
//! measurement.
//!
//! Arming refuses twice: no open product means no niche is offered (ADR 0098), and a class without
//! the `fill` trait has no axis pair to take, so the button and the arm cannot disagree about
//! whether the tool can run.
//!
//! Changes when: the fill gesture gains a class, a refusal or a second mode.

use engine::Session;
use model::{ClassCode, Fill, FillMode};
use text::{Lang, Message};

use super::Editor;

/// The classes a void can be filled with, in the order the menu offers them.
///
/// A post stands, a shelf lies and a wall closes the piece: the three are the whole vocabulary of a
/// niche, and the order is the one a maker reads them in (U123, U128).
pub const FILL_CLASSES: [ClassCode; 3] =
    [ClassCode::VERTICAL_PANEL, ClassCode::HORIZONTAL_PANEL, ClassCode::BACK_PANEL];

/// What the next click of the Panel cell makes, when the empty-space tool is not armed yet (U123).
///
/// One cell serves every part a void can be filled with, so the choice lives beside the arm rather
/// than in the map: the tool card offers the class and the build while the fill is armed, the node's
/// context menu offers the same two rows on a part that already exists, and neither is a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelBuild {
    /// The class a created part carries and takes its label from.
    pub class: ClassCode,
    /// How the part reads the section the walls leave.
    pub mode: FillMode,
    /// Whether the first click lays the part over the frame's face instead of inside the opening.
    ///
    /// The mounting is a creation key and not a stored form (ADR 0240): the click that follows is a
    /// board one thickness in front of the void, its ends running past the opening by the lap the
    /// frame allows. Switching a finished front the other way is an operation, not this bit.
    pub overlay: bool,
    /// Whether the gesture lays one board or a row of them spread evenly (U125).
    ///
    /// The row is chosen on the tool card, lives in the gesture and is written nowhere: it is the
    /// operation, not a parameter of the part (the owner, 2026-11-25).
    pub spread: FillSpread,
}

impl PanelBuild {
    /// Whether a class of this kind can be laid over the frame instead of inside the opening.
    ///
    /// Only the wall closes the piece from the front: a post stands and a shelf lies across the void,
    /// so neither has a face the frame's opening belongs to (ADR 0240). The key is asked — and kept —
    /// only where it means something, so choosing a shelf after a wall can never carry it along.
    pub fn can_overlay(class: ClassCode) -> bool {
        class.traits().and_then(|traits| traits.fill()) == Some(Fill::WidthAndHeight)
    }
}

impl Default for PanelBuild {
    /// The build a fresh workspace starts on: a post that takes the largest rectangle, which is the
    /// first row of the card and what the retired Post cell ran.
    fn default() -> Self {
        Self { class: ClassCode::VERTICAL_PANEL, mode: FillMode::Fit, overlay: false, spread: FillSpread::One }
    }
}

/// Whether a fill lays one board or a row of them spread evenly (U125).
///
/// The choice sits beside the build rather than inside it: [`FillMode`] says how *one* part reads
/// the section the walls leave, and a row is the same board repeated along its own thickness axis.
/// A row is a property of the gesture and never of the document, so nothing stores it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillSpread {
    /// One board, which is what the empty-space tool has always made.
    #[default]
    One,
    /// A row of equal boards along the part's free axis, the count from the hand and the plate.
    Even,
}

impl FillSpread {
    /// Whether the gesture lays a row.
    pub const fn is_even(self) -> bool {
        matches!(self, Self::Even)
    }

    /// The other choice, for a row of the tool card to switch to.
    pub const fn opposite(self) -> Self {
        match self {
            Self::One => Self::Even,
            Self::Even => Self::One,
        }
    }
}

/// The class the next click fills a niche with, the axis pair its trait names and its mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FillArm {
    /// The class a created part carries and takes its label from.
    pub class: ClassCode,
    /// The pair of the niche's sides the part takes; the third is the material's thickness.
    pub fill: Fill,
    /// How the part reads the section the walls leave.
    ///
    /// The mode is the method the cell ran — the one the workspace pinned — so a maker switches it
    /// from the ribbon's list and nothing else stores it (N10).
    pub mode: FillMode,
}

impl Editor {
    /// The armed empty-space tool, or `None` when the next click is not a fill.
    pub fn fill_arm(&self) -> Option<FillArm> {
        self.fill
    }

    /// What the next click of the Panel cell makes: the class and the build the card holds.
    pub fn panel_build(&self) -> PanelBuild {
        self.panel_build
    }

    /// Sets what the next click of the Panel cell makes, from the card or from a menu.
    ///
    /// The build is view state: it survives a document edit, it is not written to the scene file and
    /// it never reaches the undo stack. Walking away from it is what the arm's own cancel is.
    pub fn set_panel_build(&mut self, build: PanelBuild) {
        // The mounting is the wall's key and no other class has one: a build that walks from a wall to
        // a shelf or a post drops it instead of carrying a stance the class cannot take (X135).
        self.panel_build = PanelBuild { overlay: build.overlay && PanelBuild::can_overlay(build.class), ..build };
    }

    /// Arms the empty-space tool with `class` and `mode`, or refuses with the sentence that says why.
    ///
    /// A niche belongs to the open product, and a class that does not carry the `fill` trait has no
    /// size pair to take. Arming leaves every other pointer mode: the next click creates a part
    /// rather than starting a chain or placing a primitive. The mode travels with the arm, so
    /// switching method re-arms the tool where picking the same method again drops it.
    pub fn arm_fill(&mut self, session: &Session, class: ClassCode, mode: FillMode, lang: Lang) -> Result<(), String> {
        if session.active_product().is_none() {
            return Err(Message::FillNeedsProduct.text(lang).to_owned());
        }
        let fill = class
            .traits()
            .and_then(|traits| traits.fill())
            .ok_or_else(|| Message::FillNeedsFillTrait.text(lang).to_owned())?;
        self.leave_pointer_modes();
        self.fill = Some(FillArm { class, fill, mode });
        // The build the tool card, the operation's menu and the click all read follows the arm
        // wherever it was made: a cell, a menu row or a test that arms the tool directly (U128).
        self.set_panel_build(PanelBuild { class, mode, ..self.panel_build() });
        Ok(())
    }

    /// Drops the empty-space tool; the next click selects again.
    ///
    /// The niche the pointer measured belonged to this arm, so the value goes with it: a fill and the
    /// report row read one atom, and neither keeps a space the tool left (ADR 0118).
    pub fn cancel_fill(&mut self) {
        self.fill = None;
        self.void.clear();
    }
}
