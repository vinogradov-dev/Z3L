//! The shell's one frame, asserted without a window.
//!
//! `App::frame` is the whole interface: the header, the ribbon, the floating panels, the
//! scene, the cluster and the modals. These tests drive it with an [`egui::Context`] and
//! [`egui::RawInput`] — no window, no device, no adapter — and assert what a live session used to
//! check by eye: the scene rectangle and its camera aspect in every panel and ribbon state, the fact
//! that the chrome keeps the presses it covers, that the split views tile the scene and their
//! borders stay grabbable, that a modal keeps the keys, and that no chrome gesture writes a revision
//! (U78).
//!
//! Changes when: the frame's order, a chrome gesture, a split layout or the shell's key handling
//! changes.

#![allow(clippy::expect_used)]

use crate::file_picker::{Order, PickMode};
use eframe::egui;
use geom::DVec2;
use model::SnapKind;
use model::{ClassCode, ContourSegment, Envelope, Id};
use scene::Camera;
use text::{Lang, Message};
use ui::settings::{MAX_HANDLE_POINTS, MAX_RING_PERCENT, MIN_HANDLE_POINTS, MIN_RING_PERCENT};
use ui::{DrawIntent, DrawTool, FieldParam, MeasureStage, TransformMode};
use units::Length;

use super::*;

/// The window a headless frame is drawn into: the default launch size (M2).
fn screen() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 840.0))
}

/// Ctrl+Shift, the modifier pair the panel fold takes.
fn ctrl_shift() -> egui::Modifiers {
    egui::Modifiers { shift: true, ..egui::Modifiers::CTRL }
}

/// A headless application, the context that stands in for its window, and the workspace file it
/// would write to.
///
/// Everything below it is the real interface: [`App::headless`] composes the same shell the
/// window composes, and [`Harness::run`] draws the same frame `eframe::App::ui` draws (U77).
struct Harness {
    ctx: egui::Context,
    app: App,
    /// Where the workspace would be written; the tests never write it, but a headless run must not
    /// be able to reach the file in the OS config directory.
    path: std::path::PathBuf,
    /// The text the last frame drew, so a test reads what the user would see.
    drawn: Vec<String>,
    /// Where the last frame drew each text, so a test can press a row by the name on it.
    drawn_rects: Vec<(egui::Rect, String)>,
    /// The cursor the last frame asked the window for: the one place a cursor is decided (X37).
    cursor: egui::CursorIcon,
    /// The named controls of the last frame with the rectangle each one took.
    ///
    /// The ribbon's faces are not on a pitch a test can count — a split cell adds its caret column
    /// and a group its divider — so a test is told where the frame put a control instead of
    /// recomputing the strip's metrics.
    faces: Vec<(String, egui::Rect)>,
    /// The scene rectangle the frame before the last one gave the split views, so [`Harness::settle`]
    /// can tell a settled frame from one whose layout is still moving.
    last_scene: Option<egui::Rect>,
}

impl Harness {
    fn new(name: &str) -> Self {
        Self::with_session(name, crate::scene_build::demo_session().expect("the sample session builds"))
    }

    /// A headless application around one session, for a test that wants a document of its own.
    fn with_session(name: &str, session: engine::Session) -> Self {
        let path = testkit::scratch_dir(name).join("settings.json");
        let ctx = egui::Context::default();
        // The accessible tree names every ribbon face with the rectangle it took, which is how a test
        // aims at a caret without repeating the strip's layout.
        ctx.enable_accesskit();
        let app = App::headless(&ctx, path.clone(), Settings::default(), session)
            .expect("the application builds with no device, no adapter and no window");
        Self {
            ctx,
            app,
            path,
            drawn: Vec::new(),
            drawn_rects: Vec::new(),
            cursor: egui::CursorIcon::Default,
            faces: Vec::new(),
            last_scene: None,
        }
    }

    /// Runs one real frame with `events` and returns the chrome it gave the window.
    ///
    /// A window hands the frame's texture deltas to the renderer; a test has none, and epaint asserts
    /// on drop that a delta was applied, so the frame is closed here.
    fn run(&mut self, events: Vec<egui::Event>) -> Chrome {
        self.run_with(egui::Modifiers::NONE, events)
    }

    /// Runs one real frame with `modifiers` held, the way the window reports them.
    ///
    /// `egui` learns the held modifiers from a modifier event, not from the keys or the pointer
    /// press that carries them, so a test that shifts a click reports the pair it holds.
    fn run_with(&mut self, modifiers: egui::Modifiers, mut events: Vec<egui::Event>) -> Chrome {
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        let mut chrome = None;
        let mut output =
            self.ctx.run_ui(egui::RawInput { screen_rect: Some(screen()), events, ..Default::default() }, |ui| {
                chrome = Some(self.app.frame(ui));
            });
        self.drawn = crate::test_support::drawn_text(&output.shapes);
        self.drawn_rects = crate::test_support::drawn_text_rects(&output.shapes);
        self.cursor = output.platform_output.cursor_icon;
        self.faces = accessible_faces(&output);
        self.last_scene = self.app.tabs.active().scene_rect();
        // A window applies the viewport commands it is given; a headless run has to play that part,
        // or a carry that asks the pointer back onto its piece never gets an answer (ADR 0156).
        output.textures_delta.clear();
        chrome.expect("the frame drew the interface")
    }

    /// Runs one idle frame, with no input at all.
    fn frame(&mut self) -> Chrome {
        self.run(Vec::new())
    }

    /// How many repaint requests the shell's own code made in the frame before the last one.
    ///
    /// `egui` names a frame's causes one pass later, and its own widgets ask for frames of their own
    /// (a scroll area that follows its content, a fading hover), so a delay is no evidence: the
    /// file that asked is. The shell's own file is where a frame asks for the frame after it, and a
    /// call that landed is the reason it does (X111).
    fn shell_requests(&self) -> usize {
        self.ctx.repaint_causes().iter().filter(|cause| cause.file.ends_with("app/src/main.rs")).count()
    }

    /// Draws idle frames until the scene rectangle stops moving.
    ///
    /// A gesture leaves work for the frame after it: the side panels narrow over
    /// [`ANIMATION_SECONDS`], and the status bar grows for the one frame that announces a rebuild.
    /// The measurement is read after both have settled, so a test compares the layout and not the
    /// middle of an animation.
    fn settle(&mut self) -> Chrome {
        let mut chrome = self.frame();
        let mut stable = 0;
        for _ in 0..12 {
            let previous = self.last_scene;
            chrome = self.frame();
            stable = if self.last_scene == previous { stable + 1 } else { 0 };
            if stable >= 2 {
                return chrome;
            }
        }
        chrome
    }

    /// Moves the pointer to `pos` and presses `button` there, without releasing it.
    fn press_at(&mut self, pos: egui::Pos2, button: egui::PointerButton) {
        self.run(vec![egui::Event::PointerMoved(pos)]);
        self.run(vec![Self::button(pos, button, true)]);
    }

    /// Releases `button` at `pos`.
    fn release_at(&mut self, pos: egui::Pos2, button: egui::PointerButton) {
        self.run(vec![Self::button(pos, button, false)]);
    }

    fn button(pos: egui::Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
        egui::Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE }
    }

    /// Clicks `pos` with `button`: the pointer arrives, the button goes down and comes back up.
    ///
    /// The first frame lets egui lay the area or the widget out, so the press lands on the rectangle
    /// the user is looking at instead of on last frame's.
    fn click(&mut self, pos: egui::Pos2, button: egui::PointerButton) -> Chrome {
        self.run(vec![egui::Event::PointerMoved(pos)]);
        self.press_at(pos, button);
        self.release_at(pos, button);
        self.settle()
    }

    /// Clicks `pos` with the primary button while `modifiers` are held.
    ///
    /// The scene tree builds a multi-selection from a shifted click, so a test that needs two
    /// selected nodes presses the way a person does.
    fn click_with(&mut self, pos: egui::Pos2, modifiers: egui::Modifiers) -> Chrome {
        self.run_with(modifiers, vec![egui::Event::PointerMoved(pos)]);
        self.run_with(modifiers, vec![Self::button(pos, egui::PointerButton::Primary, true)]);
        self.run_with(modifiers, vec![Self::button(pos, egui::PointerButton::Primary, false)]);
        let chrome = self.settle();
        // The window reports the release of the modifier too, so the next press is a plain one.
        self.run_with(egui::Modifiers::NONE, Vec::new());
        chrome
    }

    /// Moves the pointer to `pos` and lets the frame settle, so the ghost is measured before a click.
    fn hover(&mut self, pos: egui::Pos2) {
        self.run(vec![egui::Event::PointerMoved(pos)]);
        self.settle();
    }

    /// Drags `button` from `from` by `delta`, one frame per step.
    fn drag(&mut self, from: egui::Pos2, delta: egui::Vec2, button: egui::PointerButton) -> Chrome {
        self.press_at(from, button);
        self.run(vec![egui::Event::PointerMoved(from + delta)]);
        self.release_at(from + delta, button);
        self.settle()
    }

    /// Takes the piece under `at` with a click, carries it by `travel` and puts it down.
    ///
    /// The object move holds nothing (ADR 0151), so its gesture is two clicks with the pointer
    /// travelling in between: the first takes the piece by its body, the second drops it.
    fn carry(&mut self, at: egui::Pos2, travel: egui::Vec2) -> Chrome {
        self.click(at, egui::PointerButton::Primary);
        self.hover(at + travel);
        self.click(at + travel, egui::PointerButton::Primary)
    }

    /// Presses and releases `key`, the way a window reports one shortcut.
    fn press(&mut self, key: egui::Key, modifiers: egui::Modifiers) -> Chrome {
        let event = |pressed: bool| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
        self.run(vec![event(true)]);
        self.run(vec![event(false)]);
        self.settle()
    }

    /// The scene rectangle the last frame gave the split views.
    fn scene(&self) -> egui::Rect {
        self.app.tabs.active().scene_rect().expect("a drawn frame gives the scene a rectangle")
    }

    /// The camera the last frame looked through.
    fn camera(&self) -> Camera {
        self.app.tabs.active().camera()
    }

    /// The document revision the active scene is at.
    fn revision(&self) -> model::Revision {
        self.app.tabs.active().session.project().revision()
    }

    /// The nodes the viewport selection holds.
    fn selected(&self) -> Vec<Id> {
        self.app.tabs.active().editor.selected_nodes().to_vec()
    }

    /// The nodes the frame last uploaded draws as selected.
    fn highlighted(&self) -> Vec<Id> {
        self.app.tabs.active().highlighted_nodes().to_vec()
    }

    /// Whether the last frame drew `text`.
    fn drew(&self, text: &str) -> bool {
        self.drawn.iter().any(|line| line.contains(text))
    }

    /// The screen point at the middle of `node`'s own frame, so a test right-clicks that node.
    ///
    /// A panel is measured from its base corner — its nominal size and thickness — and any other
    /// node from its origin, which is what the pick resolves on the model the test built.
    fn node_point(&self, node: Id) -> egui::Pos2 {
        let document = self.app.tabs.active();
        let project = document.session.project();
        let world = project.world_placement(node).expect("the node has a world frame");
        let local = project.panel(node).map_or(geom::DVec3::ZERO, |panel| {
            let size = panel.nominal_size();
            geom::DVec3::new(
                size.length().millimetres_f64() / 2.0,
                size.width().millimetres_f64() / 2.0,
                panel.last_known_thickness().millimetres_f64() / 2.0,
            )
        });
        let rect = document.scene_rect().expect("a settled frame gives the scene a rectangle");
        gizmo_math::screen_point(document.camera(), rect, world.transform_point(local)).expect("the node is on screen")
    }

    /// The first text the last frame drew inside `rect`, read top to bottom.
    ///
    /// A menu is a list, so the first entry is the topmost label inside the surface it drew: the
    /// test reads the order the person sees instead of the order the list was built in.
    fn first_text_in(&self, rect: egui::Rect) -> Option<(egui::Pos2, String)> {
        self.drawn_rects
            .iter()
            .filter(|(at, _)| rect.contains_rect(*at))
            .min_by(|left, right| left.0.top().total_cmp(&right.0.top()))
            .map(|(at, text)| (at.center(), text.clone()))
    }

    /// The middle of the scene tree's row whose name the last frame drew.
    ///
    /// A tree row is not a button: it is an interact region with a painted name, so the press lands
    /// on the name the test read, inside the left panel and nowhere else.
    fn tree_row(&self, chrome: &Chrome, name: &str) -> egui::Pos2 {
        let tree = chrome.panels.left.expect("the scene tree is up");
        self.drawn_rects
            .iter()
            .find(|(rect, text)| tree.contains_rect(*rect) && text.contains(name))
            .map(|(rect, _)| rect.center())
            .unwrap_or_else(|| panic!("the tree drew no row named {name}"))
    }

    /// The middle of the status bar's cell whose caption the last frame drew.
    ///
    /// The status bar is the one strip under the scene rectangle, so the caption a test reads there
    /// is the control it presses instead of a coordinate the bar's layout would have to repeat.
    /// The middle of the header's mode cell that prints `caption`.
    ///
    /// The five gestures are one band in the header's right cluster (U115), so a cell is found above
    /// the scene: below it is the status bar, which now reports and draws no control.
    fn mode_cell(&self, caption: &str) -> egui::Pos2 {
        let above_scene = self.scene().top();
        self.drawn_rects
            .iter()
            .find(|(rect, text)| text == caption && rect.center().y < above_scene)
            .map(|(rect, _)| rect.center())
            .unwrap_or_else(|| panic!("the header drew no mode cell named {caption}"))
    }

    /// Runs frames until `done` holds, the way a window left open does while something outside it
    /// finishes.
    ///
    /// A hot reload is a worker thread's answer, so a test waits for the frame it lands in instead
    /// of assuming one. The wait is bounded and returns whether the condition holds, so a condition
    /// that never holds fails the assertion the test already wrote instead of hanging the suite.
    fn until(&mut self, mut done: impl FnMut(&App) -> bool) -> bool {
        for _ in 0..400 {
            if done(&self.app) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            self.frame();
        }
        done(&self.app)
    }

    /// The thickness the session resolves for `panel`: what the last rebuild cut the panel from.
    fn thickness(&self, panel: Id) -> Length {
        self.app.tabs.active().session.panel_geometry(panel).expect("a panel resolves").thickness()
    }

    /// The middle of the control the last frame published as `label`.
    ///
    /// A test presses the middle of a face, so the point is what it asks for and the rectangle stays
    /// the frame's own business.
    fn face(&self, label: &str) -> Option<egui::Pos2> {
        self.faces.iter().find_map(|(name, rect)| (name == label).then_some(rect.center()))
    }
}

/// The named controls of one frame with the rectangle each one took.
///
/// A control's name is its accessible label, which is the hint a ribbon face is drawn with, so the
/// frame itself says where it put the control a test wants to press.
fn accessible_faces(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else { return Vec::new() };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let label = node.label()?;
            let bounds = node.bounds()?;
            let rect = egui::Rect::from_min_max(
                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
            );
            Some((label.to_owned(), rect))
        })
        .collect()
}

/// The aspect ratio the renderer derives from a rectangle: each view's own, not the target's.
fn aspect(rect: egui::Rect) -> f64 {
    f64::from(rect.width()) / f64::from(rect.height())
}

#[test]
fn the_whole_interface_frames_without_a_gpu_and_without_a_window() {
    let mut harness = Harness::new("frame-headless");
    let revision = harness.revision();
    let chrome = harness.frame();
    assert!(chrome.ribbon.height() > 0.0, "the frame gives the ribbon a band");
    assert!(chrome.panels.left.is_some() && chrome.panels.right.is_some(), "and both panels a rectangle");
    assert!(!harness.drawn.is_empty(), "the frame draws the header, the ribbon and the scene");
    harness.frame();
    assert!(!harness.drawn.is_empty(), "a second frame draws the same interface again");
    assert_eq!(harness.revision(), revision, "drawing a frame writes no document");
    assert_eq!(harness.app.tabs.len(), 1, "and opens no tab");
    assert!(harness.app.tabs.active().last_error.is_none(), "nothing failed");
}

#[test]
fn a_headless_run_leaves_the_workspace_file_alone() {
    // The store the headless application is built with points at a scratch file, never at the
    // workspace in the OS config directory, and a frame that changes no setting writes nothing.
    let mut harness = Harness::new("frame-settings");
    let before = b"{\"version\": 1}\n";
    std::fs::write(&harness.path, before).expect("the test writes the workspace file");
    harness.frame();
    harness.frame();
    let after = std::fs::read(&harness.path).expect("the file is still there");
    assert_eq!(after, before, "a headless run leaves the workspace file byte-identical");
    assert!(!harness.path.with_extension("json.tmp").exists(), "and leaves no temporary sibling behind");
}

#[test]
fn the_floor_switch_is_the_workspace_and_costs_no_undo_entry() {
    // The floor sits with the snapping boxes because it answers the same question — where a dragged
    // body may come to rest — but it is a workspace value and not document data: turning it off
    // applies to the next drag, writes no placement and leaves no undo entry (ADR 0146, ADR 0177).
    let mut harness = Harness::new("frame-floor");
    harness.frame();
    let revision = harness.revision();
    let can_undo = harness.app.tabs.active().session.can_undo();
    harness.app.settings.snaps.floor = false;
    harness.frame();
    assert_eq!(harness.revision(), revision, "a workspace switch writes no document");
    assert_eq!(harness.app.tabs.active().session.can_undo(), can_undo, "and pushes no undo entry");
}

#[test]
fn the_scene_rectangle_and_its_camera_hold_in_every_panel_and_ribbon_state() {
    // The regression U72 fixed: a fold, a hide or a restore moved the scene rectangle and therefore
    // the camera aspect, so the model panned when a panel was folded. Only the frame can prove it:
    // the rectangle the viewport really used, and the camera it really drew with.
    let mut harness = Harness::new("frame-scene-rect");
    harness.frame();
    let mut scenes = Vec::new();
    for ribbon_open in [true, false] {
        if !ribbon_open {
            harness.app.handle_header_action(HeaderAction::ToggleRibbon, &harness.ctx.clone());
            harness.settle();
        }
        let chrome = harness.settle();
        let reference = harness.scene();
        let camera = harness.camera();
        assert!((reference.top() - chrome.ribbon.bottom()).abs() < f32::EPSILON, "the band sets the scene's top");
        assert!(chrome.panels.left.is_some() && chrome.panels.right.is_some(), "both panels are up");
        // The panels float over the scene, so it runs under them: their rectangles sit inside its
        // horizontal span. That is what makes a fold unable to widen or narrow it.
        let left = chrome.panels.left.expect("the scene tree is up");
        let right = chrome.panels.right.expect("the inspector is up");
        assert!(
            reference.left() <= left.left() && reference.right() >= right.right(),
            "the scene runs under both panels"
        );
        scenes.push(reference);

        // Both panels folded, then both hidden, then back: the cycle the header buttons and the
        // shortcuts run. Every state keeps the rectangle and the camera the ribbon fold fixes.
        for (state, keys) in [
            ("folded", (egui::Key::Num0, ctrl_shift())),
            ("hidden", (egui::Key::Num0, egui::Modifiers::CTRL)),
            ("restored", (egui::Key::Num0, egui::Modifiers::CTRL)),
            ("unfolded", (egui::Key::Num0, ctrl_shift())),
        ] {
            let geometry = harness.press(keys.0, keys.1);
            let scene = harness.scene();
            let context = format!("{state}, ribbon open = {ribbon_open}");
            assert_eq!(geometry.ribbon, chrome.ribbon, "{context}: the band does not move");
            assert_eq!(scene, reference, "{context}: no edge of the scene moves");
            assert!((aspect(scene) - aspect(reference)).abs() < f64::EPSILON, "{context}: the aspect is the same");
            assert_eq!(harness.camera(), camera, "{context}: and the camera is the same");
            let panels = harness.app.shell.panels;
            let (hidden, collapsed) = (!panels.any_visible(), panels.all_collapsed());
            match state {
                "folded" => assert!(collapsed && !hidden, "{context}: the fold shortcut folds both panels"),
                "hidden" => assert!(hidden, "{context}: the hide shortcut hides both panels"),
                "restored" => assert!(!hidden && collapsed, "{context}: a second press brings the folded pair back"),
                _ => assert!(!hidden && !collapsed, "{context}: the fold shortcut unfolds them again"),
            }
        }
    }
    // The ribbon's own fold is the one thing that may move the scene: it gives the band back.
    let (open, folded) = (scenes[0], scenes[1]);
    assert!(folded.height() > open.height(), "a folded ribbon gives the scene its band");
    assert!((folded.width() - open.width()).abs() < f32::EPSILON, "and never a column of it");
}

#[test]
fn a_press_on_a_panel_is_chrome_and_a_press_beside_it_reaches_the_scene() {
    // U66 and U72 were accepted with "a click on a panel selects nothing"; this is that sentence as
    // an assertion on the real frame, with a navigation mode armed so a camera gesture would show.
    let mut harness = Harness::new("frame-chrome-press");
    let chrome = harness.settle();
    let tree = chrome.panels.left.expect("the scene tree is up");
    // A point in the empty body below the tree's rows: the panel's chrome, not a row and not the
    // header's fold button.
    let on_panel = egui::pos2(tree.center().x, tree.bottom() - 12.0);
    assert!(chrome.covers(on_panel), "the whole panel is chrome");

    let selected = harness.selected();
    let camera = harness.camera();
    harness.click(on_panel, egui::PointerButton::Primary);
    assert_eq!(harness.selected(), selected, "a click on a panel is chrome and starts no pick");
    assert!(!harness.app.shell.divider_dragging, "and grabs no border");
    assert_eq!(harness.camera(), camera, "and starts no camera gesture");

    // A drag across the same panel is chrome too, and being chrome is what stops the orbit.
    harness.app.shell.cluster.navigation = Some(crate::view_cluster::NavigationMode::Orbit);
    harness.drag(on_panel, egui::vec2(60.0, 40.0), egui::PointerButton::Primary);
    assert_eq!(harness.camera(), camera, "a drag on a panel orbits nothing");

    // The scene keeps the presses the panels do not cover: one point past the panel's inner edge.
    let beside = egui::pos2(tree.right() + 2.0, tree.center().y);
    assert!(!chrome.covers(beside), "one point past the panel is the scene");
    assert!(harness.scene().contains(beside), "and inside the scene rectangle");
    harness.drag(beside, egui::vec2(60.0, 40.0), egui::PointerButton::Primary);
    assert_ne!(harness.camera(), camera, "the same drag there orbits the view");

    // And a press there selects whatever is under it, so the guard above cannot pass by refusing
    // every pick in the workspace.
    harness.app.shell.cluster.navigation = None;
    let selected = harness.selected();
    harness.click(harness.scene().center(), egui::PointerButton::Primary);
    let after = harness.selected();
    assert!(!after.is_empty() && after != selected, "a click in the middle of the scene selects the model there");
}

#[test]
fn an_open_method_list_owns_the_cursor_across_its_whole_rectangle() {
    // The list hangs under the ribbon band and over the scene, so an armed tool used to take the
    // pointer back across the key label and the reserved pin column — the parts of a row no control
    // of its own was drawn over — while the name kept the list's cursor (X37).
    let mut harness = Harness::new("frame-list-cursor");
    harness.settle();
    // The circle is armed by its key, so the viewport has a pointer to lose to the list.
    harness.press(egui::Key::C, egui::Modifiers::NONE);
    harness.hover(harness.scene().center());
    assert_eq!(harness.cursor, egui::CursorIcon::None, "the armed tool keeps the shell's square");

    // The caret face publishes its name, so the frame says where it put the control to press.
    let caret = harness.face(Message::ToolMethods.text(Lang::En)).expect("the split cell publishes its caret face");
    harness.click(caret, egui::PointerButton::Primary);
    let chrome = harness.settle();
    let list = chrome.list.expect("an open list is chrome over the scene");
    assert!(list.width() >= ui::toolbars::list::WIDTH, "the popup is drawn at the approved width");

    // Four points across the open list: the header, a row's name, its key and its pin. Every one of
    // them is the list, so every one of them shows the list's cursor.
    let row = list.top() + ui::toolbars::list::HEADER_HEIGHT + ui::toolbars::list::ROW_HEIGHT / 2.0;
    let probes = [
        ("the header", egui::pos2(list.center().x, list.top() + 8.0)),
        ("the name", egui::pos2(list.left() + list.width() * 0.35, row)),
        ("the key", egui::pos2(list.left() + list.width() * 0.80, row)),
        ("the pin", egui::pos2(list.left() + list.width() * 0.93, row)),
    ];
    for (column, point) in probes {
        harness.hover(point);
        assert_eq!(harness.cursor, egui::CursorIcon::Default, "the list owns the pointer over {column}");
        assert!(harness.app.shell.work_cursor.is_none(), "the list is a menu: no square covers {column}");
        assert!(chrome.covers(point), "{column} is inside the list's rectangle");
    }

    // The caret closes the list again, and the frame after it gives the pointer back to the tool.
    harness.click(caret, egui::PointerButton::Primary);
    let chrome = harness.settle();
    assert!(chrome.list.is_none(), "the question the caret asked is answered and the list is gone");
    assert!(harness.face(Message::ToolMethods.text(Lang::En)).is_some(), "the caret is still there");
    harness.hover(harness.scene().center());
    assert_eq!(harness.cursor, egui::CursorIcon::None, "the armed tool gets the shell's square back");
}

#[test]
fn two_columns_and_quad_tile_the_scene_rect_and_their_borders_stay_grabbable() {
    use crate::view_layout::LayoutKind;

    let mut harness = Harness::new("frame-layout");
    harness.settle();
    for kind in [LayoutKind::TwoColumns, LayoutKind::TwoRows, LayoutKind::Quad] {
        harness.app.tabs.active_mut().set_layout(kind);
        let chrome = harness.settle();
        let scene = harness.scene();
        let layout = harness.app.tabs.active().layout();
        let regions = layout.regions(scene);
        assert_eq!(regions.len(), kind.leaves(), "{kind:?} draws one view per leaf");
        let mut tiled = 0.0;
        for (_, view) in &regions {
            assert!(scene.contains_rect(*view), "{kind:?}: every view stays inside the scene: {view:?}");
            assert!(view.width() > 0.0 && view.height() > 0.0, "{kind:?}: a view has an area");
            tiled += view.width() * view.height();
        }
        assert!(
            (tiled - scene.width() * scene.height()).abs() < 1.0,
            "{kind:?} tiles the scene rect exactly: {tiled} against {}",
            scene.width() * scene.height()
        );

        // A border is inside the scene, outside the chrome, and a press there grabs it.
        let divider = harness.app.tabs.active().layout().dividers(scene).into_iter().next().expect("a border");
        let handle = divider.hit_rect().center();
        assert!(scene.contains(handle), "{kind:?}: the border band is inside the scene");
        assert!(!chrome.covers(handle), "{kind:?}: and not under a panel");
        // A border is not the work area: it keeps the system resize arrow and the square is not
        // painted over it (X41).
        harness.hover(handle);
        assert!(
            matches!(harness.cursor, egui::CursorIcon::ResizeHorizontal | egui::CursorIcon::ResizeVertical),
            "{kind:?}: the border keeps the system resize cursor, not {:?}",
            harness.cursor
        );
        assert!(harness.app.shell.work_cursor.is_none(), "{kind:?}: and draws no square over the border");
        let selected = harness.selected();
        harness.press_at(handle, egui::PointerButton::Primary);
        assert!(harness.app.shell.divider_dragging, "{kind:?}: a press there grabs the border");
        assert_eq!(harness.selected(), selected, "{kind:?}: and selects nothing");
        harness.release_at(handle, egui::PointerButton::Primary);
        harness.settle();
        assert!(!harness.app.shell.divider_dragging, "{kind:?}: and the release ends the drag");
    }
}

#[test]
fn a_modal_owns_the_keys() {
    // The sentence ADR 0076 makes: a modal keeps the keys for the frame it is up. Before U78 this was
    // re-verified by hand, and the drawing-tool table was not gated at all.
    let mut harness = Harness::new("frame-modal");
    harness.settle();
    let revision = harness.revision();

    // One real edit, through the keyboard and the pointer, so undo and redo have something to do:
    // the rectangle tool and two corners of the work plane.
    harness.press(egui::Key::R, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.drawing().is_some(), "R starts the rectangle tool");
    let scene = harness.scene();
    harness.click(scene.center() - egui::vec2(120.0, 90.0), egui::PointerButton::Primary);
    harness.click(scene.center() + egui::vec2(120.0, 90.0), egui::PointerButton::Primary);
    let edited = harness.revision();
    assert_ne!(edited, revision, "the two clicks commit one path");
    assert!(harness.app.tabs.active().editor.drawing().is_none(), "and the tool ends with the figure");
    assert!(harness.app.tabs.active().can_undo(), "the edit is undoable");
    // A committed drawing hands over to the corner grips; leave them, so a tool key has room to run.
    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.contour_edit().is_none(), "Escape leaves the contour edit");

    // A camera the fit would change, so a reframe under the modal would show.
    let resting = harness.camera();
    harness.drag(harness.scene().center(), egui::vec2(50.0, 0.0), egui::PointerButton::Middle);
    let orbited = harness.camera();
    assert_ne!(orbited, resting, "a middle drag orbits the view");

    harness.app.handle_header_action(HeaderAction::Settings, &harness.ctx.clone());
    harness.settle();
    assert!(harness.drew(Message::SettingsTitle.text(Lang::En)), "the Settings window is up: {:?}", harness.drawn);

    harness.press(egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(harness.revision(), edited, "Ctrl+Z under a modal undoes nothing");
    assert!(harness.app.tabs.active().can_undo(), "the edit is still there to undo");
    harness.press(egui::Key::F, egui::Modifiers::NONE);
    assert_eq!(harness.camera(), orbited, "F under a modal does not reframe");
    harness.press(egui::Key::R, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.drawing().is_none(), "and no tool starts under it");
    // The save is a pick now, so the assertion is the pick and not a drawn title: no dialog is
    // asked for while a modal owns the keys (X91).
    let picks = harness.app.picker.produced();
    harness.press(egui::Key::S, egui::Modifiers::CTRL);
    assert_eq!(harness.app.picker.produced(), picks, "Ctrl+S under a modal starts no pick");

    // The gate is the modal and not a dead key: with it closed, the same press undoes the edit.
    harness.app.settings_open = false;
    harness.settle();
    harness.press(egui::Key::Z, egui::Modifiers::CTRL);
    assert!(!harness.app.tabs.active().can_undo(), "the undo ran once the modal was gone");
    assert!(harness.app.tabs.active().can_redo(), "and one edit was one undo entry");
}

#[test]
fn a_cancelled_pick_opens_and_writes_nothing() {
    // X91: the file manager is another process, so its answer lands at the frame boundary. A cancel
    // is an answer — the person closed the dialog — and it must leave every tab exactly as it was.
    let mut harness = Harness::new("frame-pick-cancel");
    harness.settle();
    let tabs = harness.app.tabs.iter().count();
    let revision = harness.revision();
    let path = harness.app.tabs.active().path().map(|path| path.to_owned());

    let ctx = harness.ctx.clone();
    let order = Order { mode: PickMode::Open, folder: None, name: None, lang: Lang::En };
    harness.app.handle_file_action(FileAction::Pick(order), &ctx);
    assert_eq!(harness.app.picker.produced(), 1, "the frame asked the file manager");
    assert!(harness.app.picker.answer(None), "and the person closed the dialog");
    harness.settle();

    assert_eq!(harness.app.tabs.iter().count(), tabs, "a cancelled pick opens no tab");
    assert_eq!(harness.revision(), revision, "and writes no document");
    assert_eq!(harness.app.tabs.active().path().map(|path| path.to_owned()), path, "the open file is unchanged");
    assert!(harness.app.tabs.active().last_error.is_none(), "a cancel is not a failure to report");
}

#[test]
fn a_picked_name_saves_a_scene_the_next_open_can_see() {
    // The dialog filters by the scene extension but does not append it, so `cabinet` typed into the
    // field is written as `cabinet.z3l`: a file the next Open lists (X91).
    let mut harness = Harness::new("frame-pick-save");
    harness.settle();
    let directory = testkit::scratch_dir("frame-pick-save-scene");
    let typed = directory.join("cabinet");
    let ctx = harness.ctx.clone();
    let order = Order {
        mode: PickMode::Save,
        folder: Some(directory.clone()),
        name: Some("cabinet".to_owned()),
        lang: Lang::En,
    };
    harness.app.handle_file_action(FileAction::Pick(order), &ctx);
    assert!(harness.app.picker.answer(Some(typed.clone())), "the person confirmed the name");
    harness.settle();

    let written = directory.join("cabinet.z3l");
    assert_eq!(harness.app.tabs.active().path().map(|path| path.to_owned()), Some(written.clone()));
    assert!(written.is_file(), "the scene is on disk under the name the next Open filters for");
    assert!(!harness.app.tabs.active().is_dirty(), "a save leaves the scene saved");
}

#[test]
fn a_pick_belongs_to_the_tab_that_asked_for_it() {
    // ADR 0081: the pick is the tab's, not the window's. The file manager draws its own window, so a
    // person can activate another tab behind it; the picked path must still land in the tab whose
    // gesture asked for it.
    let mut harness = Harness::new("frame-pick-tab");
    harness.settle();
    let asking = harness.app.tabs.active_id();
    let directory = testkit::scratch_dir("frame-pick-tab-scene");
    let ctx = harness.ctx.clone();
    let order = Order { mode: PickMode::Save, folder: Some(directory.clone()), name: None, lang: Lang::En };
    harness.app.handle_file_action(FileAction::Pick(order), &ctx);
    harness.app.handle_file_action(FileAction::New, &ctx);
    let elsewhere = harness.app.tabs.active_id();
    assert_ne!(elsewhere, asking, "the new scene took the front while the pick was up");

    let path = directory.join("cabinet.z3l");
    assert!(harness.app.picker.answer(Some(path.clone())));
    harness.settle();

    assert_eq!(harness.app.tabs.active_id(), asking, "the tab that asked is in front again");
    let saved = harness.app.tabs.active();
    assert_eq!(saved.path(), Some(path.as_path()), "and it is the tab that took the picked path");
    assert!(path.is_file(), "the scene was written");
    let other = harness.app.tabs.document(elsewhere).expect("the new tab is still open");
    assert_eq!(other.path(), None, "the scene that was in front took nothing");
}

#[test]
fn the_render_tab_turns_the_workspace_into_the_render_place_and_back() {
    // The render place is a view of the scene in front, not a replacement for it (ADR 0211): the
    // header's tab opens it, the page draws instead of the viewport and the ribbon, nothing floats
    // over it, Escape and a document tab both go back, and no document revision moves either way.
    let mut harness = Harness::new("frame-render-place");
    let chrome = harness.settle();
    let revision = harness.revision();
    assert!(chrome.ribbon.is_positive(), "the document draws its ribbon band");
    assert!(chrome.panels.left.is_some(), "and its panels");
    assert!(!chrome.covers(harness.last_scene.expect("the viewport").center()), "the scene is drawn");

    harness.app.handle_header_action(crate::header::HeaderAction::Render, &harness.ctx);
    assert!(harness.app.render_open, "the tab opens the place");
    let chrome = harness.frame();
    let words = harness.drawn.clone();
    let title = Message::RenderTitle.text(text::Lang::En);
    assert!(words.iter().any(|word| word == title), "the page names itself: {words:?}");
    assert!(words.iter().any(|word| word == Message::RenderDestination.text(text::Lang::En)), "{words:?}");
    assert_eq!(chrome.ribbon, egui::Rect::NOTHING, "no ribbon band is drawn behind the place");
    assert!(chrome.panels.left.is_none() && chrome.panels.right.is_none(), "and no panel floats over it");
    assert_eq!(harness.revision(), revision, "opening the place writes no document");

    // The same tab again goes back to the document, which is what a pressed tab does.
    harness.app.handle_header_action(crate::header::HeaderAction::Render, &harness.ctx);
    assert!(!harness.app.render_open);
    let chrome = harness.settle();
    assert!(chrome.ribbon.is_positive() && chrome.panels.left.is_some(), "the viewport and its chrome are back");
    assert_eq!(harness.revision(), revision, "and still no document was written");
}

#[test]
fn a_modifier_click_in_the_viewport_builds_the_selection_order() {
    // U102: a viewport click reads the same two modifiers a tree row does — Shift appends the node
    // the click resolved to and takes it back out on a second click, Ctrl removes one member, and a
    // plain click replaces the order. The uploaded frame highlights every member, so an added node
    // is drawn selected and not only remembered (ADR 0029: the selection is view state).
    let mut harness = Harness::new("frame-selection-order");
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    let chrome = harness.settle();
    let revision = harness.revision();

    let (group, drawer, shelf, front) = {
        let project = harness.app.tabs.active().session.project();
        let mut panels = Vec::new();
        collect_panels(project.root().children(), &mut panels);
        let id_named = |name: &str| {
            panels
                .iter()
                .find(|panel| panel.name() == name)
                .map(|panel| panel.id())
                .expect("the sample draws that panel")
        };
        let shelf = id_named("Loose shelf");
        let front = id_named("Drawer front");
        (
            project.parent_of(shelf).expect("the shelf sits in the group"),
            project.parent_of(front).expect("the front sits in the component"),
            shelf,
            front,
        )
    };
    let shelf_point = harness.node_point(shelf);
    let drawer_point = harness.node_point(front);
    for at in [shelf_point, drawer_point] {
        assert!(!chrome.covers(at), "the panel the test presses is in the scene, not under chrome: {at:?}");
    }

    harness.click(shelf_point, egui::PointerButton::Primary);
    assert_eq!(harness.selected(), vec![group], "a plain click picks the closed group above the shelf");
    harness.click_with(drawer_point, egui::Modifiers::SHIFT);
    assert_eq!(harness.selected(), vec![group, drawer], "Shift appends the picked node");
    assert_eq!(harness.highlighted(), vec![group, drawer], "and the frame draws both as selected");

    harness.click_with(shelf_point, egui::Modifiers::SHIFT);
    assert_eq!(harness.selected(), vec![drawer], "a second Shift-click on the same node toggles it out");

    harness.click_with(drawer_point, egui::Modifiers::CTRL);
    assert!(harness.selected().is_empty(), "Ctrl removes the one member and nothing else");
    assert!(harness.highlighted().is_empty(), "the scene drops the highlight with it");

    harness.click_with(shelf_point, egui::Modifiers::SHIFT);
    harness.click_with(drawer_point, egui::Modifiers::SHIFT);
    assert_eq!(harness.selected(), vec![group, drawer], "Shift builds the order again");
    harness.click_with(drawer_point, egui::Modifiers::CTRL);
    assert_eq!(harness.selected(), vec![group], "Ctrl takes one member and keeps the rest in order");

    harness.click(drawer_point, egui::PointerButton::Primary);
    assert_eq!(harness.selected(), vec![drawer], "a plain click replaces the order");
    assert_eq!(harness.revision(), revision, "a selection writes no document");
    assert!(!harness.app.tabs.active().can_undo(), "and no undo entry");
}

#[test]
fn the_delete_key_asks_before_it_removes_a_multi_selection() {
    // U85: Shift-click builds a multi-selection but nothing removed it, and looping `node.delete`
    // left one undo entry per node. The key takes the whole set in one command and asks first,
    // because a batch of rows goes in one step and a row the person did not mean to take is easy to
    // miss (ADR 0112).
    let mut harness = Harness::new("frame-delete-many");
    let chrome = harness.settle();
    let before = harness.revision();

    // Two nodes, picked the way a person picks them: a plain click, then a shifted one.
    harness.click(harness.tree_row(&chrome, "Back panel"), egui::PointerButton::Primary);
    harness.click_with(harness.tree_row(&chrome, "Loose shelf"), egui::Modifiers::SHIFT);
    let selected = harness.selected();
    assert_eq!(selected.len(), 2, "the shifted click adds a row to the selection: {selected:?}");

    harness.press(egui::Key::Delete, egui::Modifiers::NONE);
    assert_eq!(harness.revision(), before, "the key asks before the document changes");
    assert_eq!(harness.selected(), selected, "and the selection is still there");
    let question =
        Message::counted(2, Message::DeleteNodesTitleOne, Message::DeleteNodesTitleFew, Message::DeleteNodesTitleMany)
            .text(Lang::En);
    assert!(harness.drew(&question), "one question names the whole selection: {:?}", harness.drawn);

    // Escape is the way out: the question owns the keys, so it has to answer the key that cancels.
    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!harness.drew(&question), "Escape cancels the question: {:?}", harness.drawn);
    assert_eq!(harness.revision(), before, "and nothing was removed");
    assert_eq!(harness.selected(), selected, "the selection is still there to act on");

    harness.press(egui::Key::Delete, egui::Modifiers::NONE);
    assert!(harness.drew(&question), "the key asks again: {:?}", harness.drawn);
    let proceed = harness.face(Message::CmdDelete.text(Lang::En)).expect("the question publishes its answer");
    harness.click(proceed, egui::PointerButton::Primary);
    let project = harness.app.tabs.active().session.project();
    assert!(project.panel(selected[0]).is_none() && project.panel(selected[1]).is_none(), "both rows are gone");
    assert!(harness.app.tabs.active().can_undo(), "the batch is one undo entry");

    harness.press(egui::Key::Z, egui::Modifiers::CTRL);
    let project = harness.app.tabs.active().session.project();
    assert!(
        project.panel(selected[0]).is_some() && project.panel(selected[1]).is_some(),
        "one undo brings the whole selection back"
    );
}

#[test]
fn the_delete_key_removes_one_node_without_a_question() {
    // Undo covers a single row, so the key takes it at once: only a batch is worth a question.
    let mut harness = Harness::new("frame-delete-one");
    let chrome = harness.settle();
    let before = harness.revision();
    harness.click(harness.tree_row(&chrome, "Loose shelf"), egui::PointerButton::Primary);
    let [selected] = harness.selected()[..] else {
        panic!("the click selects exactly one row: {:?}", harness.selected());
    };

    harness.press(egui::Key::Delete, egui::Modifiers::NONE);
    let project = harness.app.tabs.active().session.project();
    assert!(project.panel(selected).is_none(), "one row goes at once");
    assert!(!harness.drew(Message::DeleteNodesBody.text(Lang::En)), "and it asks nothing: {:?}", harness.drawn);
    assert_ne!(harness.revision(), before, "the removal is a document change");
    assert!(harness.app.tabs.active().can_undo(), "that undo brings it back");
}

#[test]
fn a_locked_node_refuses_the_delete_key_and_names_itself() {
    // The lock is a guard on a gesture, not a document rule (ADR 0065): the key refuses the whole
    // selection before anything is dispatched, and the status bar says which node is locked.
    let mut harness = Harness::new("frame-delete-locked");
    let chrome = harness.settle();
    let before = harness.revision();
    harness.click(harness.tree_row(&chrome, "Loose shelf"), egui::PointerButton::Primary);
    let selected = harness.selected();
    harness.app.tabs.active_mut().editor.set_locked(selected[0], true);

    harness.press(egui::Key::Delete, egui::Modifiers::NONE);
    let error = harness.app.tabs.active().last_error.clone().unwrap_or_default();
    assert!(error.contains("Loose shelf") && error.contains("locked"), "the refusal names the row: {error}");
    assert_eq!(harness.revision(), before, "and the document is untouched");
    assert!(harness.app.tabs.active().session.project().panel(selected[0]).is_some());
}

#[test]
fn the_grid_key_turns_the_workspaces_snap_and_nothing_else() {
    // The switch is the workspace's set of kinds and not a tool's, so the key works with no draft in
    // front and the value survives a restart; it writes no revision and leaves no undo entry (U96,
    // U116).
    let mut harness = Harness::new("frame-grid-key");
    harness.settle();
    let idle = harness.app.tabs.active().snaps().grid;
    harness.press(egui::Key::G, egui::Modifiers::NONE);
    assert_ne!(harness.app.tabs.active().snaps().grid, idle, "the key turns the grid with no draft");
    harness.press(egui::Key::G, egui::Modifiers::NONE);
    assert_eq!(harness.app.tabs.active().snaps().grid, idle, "and turns it back");

    harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::Line);
    harness.settle();
    let revision = harness.revision();
    harness.press(egui::Key::G, egui::Modifiers::NONE);
    assert_ne!(harness.app.tabs.active().snaps().grid, idle, "the key runs with a draft too");
    assert_eq!(harness.revision(), revision, "the grid is view state");
}

#[test]
fn the_master_snap_switch_turns_the_whole_set_and_writes_no_revision() {
    // U116: one cell of the View tab's Snaps group turns every kind off, and a press from a mixed set
    // turns them all on again; the value is the workspace's, so no revision and no undo entry move.
    let mut harness = Harness::new("frame-snap-master");
    harness.settle();
    let document = harness.app.tabs.active();
    assert!(document.snaps().all_on(), "a fresh workspace snaps everything");
    let revision = harness.revision();
    let document = harness.app.tabs.active_mut();
    document.set_all_snaps(false);
    assert!(!document.snaps().any(), "the master turns every kind off");
    assert_eq!(
        document.take_snaps_change(),
        Some(crate::document::SnapChoice { kinds: model::SnapKinds::none(), mode: model::SnapMode::Foot }),
        "and the shell is told once"
    );
    assert_eq!(document.take_snaps_change(), None, "once, not every frame");

    // One kind back on is a mixed set, and the master turns the rest on.
    document.set_snap_kind(model::SnapKind::Vertex, true);
    document.set_all_snaps(true);
    assert!(document.snaps().all_on(), "a mixed set comes back whole");
    assert_eq!(harness.revision(), revision, "the snap set is view state");
}

#[test]
fn the_snap_mode_cell_switches_the_reading_and_the_workspace_keeps_it() {
    // U119: one press switches the reading every snap answers with. It is view state, so no revision
    // moves, and the workspace file carries it into the next run exactly as it carries the kinds.
    let mut harness = Harness::new("frame-snap-mode");
    harness.settle();
    let revision = harness.revision();
    let opening = harness.app.tabs.active().snap_mode();
    harness.app.tabs.active_mut().toggle_snap_mode();
    let switched = harness.app.tabs.active().snap_mode();
    assert_ne!(switched, opening, "the press switches the reading");

    harness.run(Vec::new());
    assert_eq!(harness.app.settings.snaps.mode, switched, "and the shell carries it into the workspace");
    let written = std::fs::read_to_string(&harness.path).expect("the workspace file is written");
    let restored = ui::settings::Settings::from_json(&written).expect("the file parses");
    assert_eq!(restored.snaps.mode, switched, "the next run opens in the same reading");
    assert_eq!(harness.revision(), revision, "the mode is view state");
}

#[test]
fn no_chrome_gesture_writes_a_revision() {
    // A fold, a hide, a ribbon fold and a camera move are view state: the document must not gain an
    // undo entry, a dirty marker or a rebuild from any of them.
    let mut harness = Harness::new("frame-view-state");
    harness.settle();
    let revision = harness.revision();
    let camera = harness.camera();

    harness.press(egui::Key::Num0, ctrl_shift());
    assert!(harness.app.shell.panels.all_collapsed(), "the fold shortcut folded both panels");
    harness.press(egui::Key::Num0, egui::Modifiers::CTRL);
    assert!(!harness.app.shell.panels.any_visible(), "the hide shortcut hid both panels");
    harness.app.handle_header_action(HeaderAction::ToggleRibbon, &harness.ctx.clone());
    harness.settle();
    assert!(!harness.app.tabs.active().ribbon_open(), "the ribbon folded");
    harness.drag(harness.scene().center(), egui::vec2(40.0, 0.0), egui::PointerButton::Middle);
    assert_ne!(harness.camera(), camera, "the drag orbited the view");

    assert_eq!(harness.revision(), revision, "no chrome gesture writes the document");
    assert!(!harness.app.tabs.active().is_dirty(), "and none of them marks the scene unsaved");
    assert!(!harness.app.tabs.active().can_undo(), "and none of them adds an undo entry");
}

#[test]
fn the_gizmo_size_is_view_state_and_writes_no_revision() {
    // A handle length and a ring radius are workspace settings: moving them between the two ends
    // of the range redraws the gizmos but adds no undo entry and marks no scene unsaved (U86).
    let mut harness = Harness::new("frame-gizmo-size");
    harness.settle();
    let revision = harness.revision();

    harness.app.settings.gizmo.handle_points = MIN_HANDLE_POINTS;
    harness.app.settings.gizmo.ring_percent = MIN_RING_PERCENT;
    harness.settle();
    harness.app.settings.gizmo.handle_points = MAX_HANDLE_POINTS;
    harness.app.settings.gizmo.ring_percent = MAX_RING_PERCENT;
    harness.settle();

    assert_eq!(harness.revision(), revision, "a gizmo size is view state, never an edit");
    assert!(!harness.app.tabs.active().is_dirty(), "and it does not mark the scene unsaved");
    assert!(!harness.app.tabs.active().can_undo(), "and it adds no undo entry");
}

/// An empty scene holding one wide panel at the origin, and the panel's id.
///
/// The panel is wide enough that a point on a transform handle stays over its face, so one point can
/// be a handle in `Move` and a pick of the same part in `Select` (U87).
fn one_wide_panel() -> (engine::Session, Id) {
    let mm = testkit::millimetres;
    let root = model::Container::new(Id::new(1).expect("an id"), "Root", model::Placement::IDENTITY, Vec::new());
    let mut project = model::Project::new(root).expect("an empty project");
    let size = model::PanelNominalSize::new(mm(2000), mm(1500)).expect("a panel size");
    let panel = project
        .add_panel(project.root().id(), "Board", size, None, mm(18), model::Placement::IDENTITY)
        .expect("a panel at the origin");
    (engine::Session::with_directory(project, testkit::sample_catalog()), panel)
}

/// A point on a translate handle of `node`, with the screen direction the handle points in.
///
/// The point sits past the dead zone around the origin and well inside the scene rectangle, so the
/// same point can be a handle in `Move` and a press in `Select`; picking the axis by where it lands
/// keeps the fixture's camera out of the test.
fn handle_point(harness: &Harness, node: Id) -> (egui::Pos2, egui::Vec2) {
    handle_point_on(harness, node, gizmo_math::ALL_AXES, None).expect("a handle runs inside the scene rectangle")
}

/// A point on the handle of `node` along `axis`, for a tool that offers `axes`.
///
/// The `axes` are the ones the mode draws, so a test can ask where a handle *would* be in a mode that
/// does not draw it: that is how the object move's refused directions are pressed (N22).
fn handle_point_on(
    harness: &Harness,
    node: Id,
    axes: [bool; 3],
    only: Option<gizmo_math::Axis>,
) -> Option<(egui::Pos2, egui::Vec2)> {
    let pivot = harness.app.tabs.active().session.project().pivot_world(node)?;
    let rect = harness.scene();
    let camera = harness.camera();
    let length = harness.app.settings.gizmo.handle_length();
    let origin = gizmo_math::screen_point(camera, rect, pivot)?;
    let spec = gizmo_math::HandleSpec { axes, length };
    gizmo_math::axis_handles(camera, rect, pivot, origin, spec)
        .into_iter()
        .filter(|handle| only.is_none_or(|axis| handle.axis == axis))
        .map(|handle| (origin + handle.direction * (length * 0.6), handle.direction))
        .find(|(at, _)| rect.contains(*at))
}

#[test]
fn a_window_opens_in_select_and_the_same_press_moves_only_where_a_handle_is_drawn() {
    // U87: `Select` is the third state and the one a window opens in, so a person who only wants to
    // look, pick and read is never handed a handle. The press is the same point throughout, which is
    // what makes the assertion about the mode and not about the pointer: it picks in `Select` and it
    // starts a drag in `Move`.
    let (session, panel) = one_wide_panel();
    let mut harness = Harness::with_session("frame-select-mode", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    assert_eq!(harness.app.shell.mode, TransformMode::Select, "a fresh window opens in Select");
    assert!(
        harness.drew(Message::Select.text(Lang::En))
            && harness.drew(Message::Move.text(Lang::En))
            && harness.drew(Message::Object.text(Lang::En))
            && harness.drew(Message::Rotate.text(Lang::En)),
        "the header's band offers Select beside the four gestures"
    );

    harness.app.tabs.active_mut().editor.select_node(Some(panel));
    harness.settle();
    let (at, direction) = handle_point(&harness, panel);

    // Select: nothing is drawn, so nothing consumes the press and the click picks the part.
    harness.app.tabs.active_mut().editor.select_node(None);
    harness.settle();
    harness.click(at, egui::PointerButton::Primary);
    assert_eq!(harness.selected(), vec![panel], "in Select a press where a handle would be picks the part");

    let revision = harness.revision();
    harness.drag(at, direction * 40.0, egui::PointerButton::Primary);
    assert_eq!(harness.revision(), revision, "and a drag there moves nothing: no gizmo is armed");

    // The header's own cell changes the mode, and the same drag now moves the part.
    let cell = harness.mode_cell(&TransformMode::Move.caption(Lang::En));
    harness.click(cell, egui::PointerButton::Primary);
    assert_eq!(harness.app.shell.mode, TransformMode::Move, "the header's cell changes the mode");
    let revision = harness.revision();
    harness.drag(at, direction * 40.0, egui::PointerButton::Primary);
    assert_eq!(harness.revision().get(), revision.get() + 1, "Move draws the handle, so the press starts one drag");
}

/// A board and a wall across its sweep, both boxes at the root: no furniture, so the turn is
/// measured by the contact port on two bodies (X88).
fn board_beside_a_wall() -> (model::Project, Id) {
    let mm = testkit::millimetres;
    let root = model::Container::new(Id::new(1).expect("an id"), "Root", model::Placement::IDENTITY, Vec::new());
    let mut project = model::Project::new(root).expect("an empty project");
    let root_id = project.root().id();
    let place =
        |x: i64, y: i64, z: i64| model::Placement::IDENTITY.with_translation(mm(x), mm(y), mm(z)).expect("a placement");
    let cuboid = |size: (i64, i64, i64)| {
        let half = [size.0 / 2, size.1 / 2, size.2 / 2];
        model::Mesh::cuboid(half.map(|value| mm(-value)), half.map(mm)).expect("a box")
    };
    let board = project
        .add_object(root_id, "Board", place(250, 10, 350), model::Geometry::Mesh(cuboid((500, 20, 700))))
        .expect("a board");
    project
        .add_object(root_id, "Wall", place(0, 210, 350), model::Geometry::Mesh(cuboid((3000, 20, 700))))
        .expect("a wall");
    (project, board)
}

/// The screen point on the node's Z ring at 45 degrees, and the travel to the turn `turn` radians on.
///
/// The ring is the gizmo's own: the pivot of the anchor, `ring_factor` of the camera distance, in the
/// plane the world Z axis turns in. The press sits at 45 degrees because the ring crosses its
/// neighbours at the axes: there one point is two rings, and the grab would take whichever came first
/// in the axis order.
fn ring_drag(harness: &Harness, node: Id, turn: f64) -> (egui::Pos2, egui::Vec2) {
    let document = harness.app.tabs.active();
    let project = document.session.project();
    let pivot = project.pivot_world(node).expect("a pivot");
    let radius = document.camera().distance() * harness.app.settings.gizmo.ring_factor();
    let rect = document.scene_rect().expect("a settled frame gives the scene a rectangle");
    let at = |angle: f64| {
        let offset = geom::DVec3::new(radius * angle.cos(), radius * angle.sin(), 0.0);
        gizmo_math::screen_point(document.camera(), rect, pivot + offset).expect("the ring is on screen")
    };
    let start = std::f64::consts::FRAC_PI_4;
    let (from, to) = (at(start), at(start + turn));
    (from, to - from)
}

/// How far the node's own frame has turned about the world Z axis, in radians.
fn turned_by(harness: &Harness, node: Id) -> f64 {
    let rotation = harness.app.tabs.active().session.project().world_placement(node).expect("a frame").rotation();
    2.0 * rotation.z.atan2(rotation.w)
}

#[test]
fn the_object_turn_stops_on_the_face_where_the_free_turn_passes_through() {
    // X88 at the frame: one ring drag, two rules. The free turn takes the hand's angle and lets the
    // board through the wall; the object turn is cut where the board's own face meets the wall's, and
    // the release writes that smaller turn as one measured placement (ADR 0180, ADR 0179).
    let (project, board) = board_beside_a_wall();
    let session = engine::Session::with_directory(project, testkit::sample_catalog());
    let mut harness = Harness::with_session("frame-object-turn", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.tabs.active_mut().editor.select_node(Some(board));
    harness.settle();

    let turn = std::f64::consts::FRAC_PI_3;
    let (from, delta) = ring_drag(&harness, board, turn);

    harness.press(egui::Key::E, egui::Modifiers::NONE);
    assert_eq!(harness.app.shell.mode, TransformMode::Rotate, "`E` arms the free turn");
    let revision = harness.revision();
    harness.drag(from, delta, egui::PointerButton::Primary);
    let free = turned_by(&harness, board);
    assert!((free - turn).abs() < 0.05, "the free turn keeps the hand's angle: {free}");
    assert_eq!(harness.revision().get(), revision.get() + 1, "and the release is one placement");
    harness.app.tabs.active_mut().session.undo().expect("one undo step");

    harness.press(egui::Key::E, egui::Modifiers::SHIFT);
    assert_eq!(harness.app.shell.mode, TransformMode::ObjectRotate, "`Shift+E` arms the object turn");
    harness.drag(from, delta, egui::PointerButton::Primary);
    let measured = turned_by(&harness, board);
    assert!(measured > 0.1, "the turn still makes headway: {measured}");
    assert!(measured < free - 0.1, "and it stops short of the wall: {measured} against the hand's {turn}");
    // The stopped turn is a placement the document accepts: the wall is a face, not an overlap.
    let landed = harness.app.tabs.active().session.project().object(board).expect("the board").placement();
    assert_eq!(
        harness.app.tabs.active().session.project().resting_conflict(&[(board, landed)], Some(&contact_parry::Parry)),
        None,
        "the turn the ring showed is the turn the write takes"
    );
}

#[test]
fn each_of_the_five_gestures_answers_its_key_and_the_bar_prints_the_pair() {
    // U113: the mode is the window's view state, so a key arms it for the whole window and writes no
    // document; the band then prints the key beside the gesture it arms, which is what makes the
    // five discoverable without the manual (X88 adds the shifted pair of the turn).
    let mut harness = Harness::new("frame-mode-keys");
    harness.settle();
    assert_eq!(harness.app.shell.mode, TransformMode::Select, "a window opens in Select");
    let revision = harness.revision();
    for (key, modifiers, mode) in [
        (egui::Key::W, egui::Modifiers::NONE, TransformMode::Move),
        (egui::Key::W, egui::Modifiers::SHIFT, TransformMode::Object),
        (egui::Key::E, egui::Modifiers::NONE, TransformMode::Rotate),
        (egui::Key::E, egui::Modifiers::SHIFT, TransformMode::ObjectRotate),
        (egui::Key::Q, egui::Modifiers::NONE, TransformMode::Select),
    ] {
        harness.press(key, modifiers);
        assert_eq!(harness.app.shell.mode, mode, "`{key:?}` with {modifiers:?} arms {mode:?}");
        assert_eq!(harness.revision(), revision, "a mode key writes no document");
    }
    for mode in TransformMode::ALL {
        let caption = mode.caption(Lang::En);
        assert!(harness.drew(&caption), "the header's band prints `{caption}`: {:?}", harness.drawn);
    }
}

#[test]
fn the_tool_card_stays_where_it_opened_so_the_hand_reaches_it() {
    // U115: the card is the only surface that finishes a draft and applies a contour edit, so it has
    // to be reachable. A card anchored to the pointer every frame could never be reached, because the
    // hand that chases it is what moves it: the owner saw exactly that. It opens where the hand was
    // and then stays, and the hand walks onto it.
    let (session, _object) = one_sketch_path();
    let mut harness = Harness::with_session("frame-card-reach", session);
    harness.settle();
    harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::Polyline);
    harness.settle();
    let scene = harness.scene();
    harness.click(scene.center() - egui::vec2(120.0, 0.0), egui::PointerButton::Primary);
    harness.click(scene.center() + egui::vec2(120.0, 0.0), egui::PointerButton::Primary);
    let opened = harness.app.tabs.active().tool_card_rect().expect("the draft draws its card");
    let mut at = scene.center() + egui::vec2(120.0, 0.0);
    for _ in 0..16 {
        at += egui::vec2(6.0, 6.0);
        harness.run(vec![egui::Event::PointerMoved(at)]);
        if opened.contains(at) {
            break;
        }
    }
    let now = harness.app.tabs.active().tool_card_rect().expect("the card is still up");
    assert_eq!(now, opened, "the card stays where it opened instead of following the hand");
    let pointer = harness.ctx.input(|input| input.pointer.hover_pos()).expect("the frame has a pointer");
    assert!(now.contains(pointer), "the hand reaches the card it was chasing: {pointer:?} is not in {now:?}");
}

#[test]
fn a_draft_and_a_contour_edit_commit_from_their_tool_card() {
    // U115: the status bar's *Finish*, *Apply*, *Cancel* and the along/free switch are the armed
    // tool's card — the one row that names the tool and holds its two buttons. The card writes
    // nothing itself: it reports the press and the shell runs the path that already owns it.
    let (session, object) = one_sketch_path();
    let mut harness = Harness::with_session("frame-card-commit", session);
    harness.settle();
    let ctx = harness.ctx.clone();

    // A running draft draws one row: its tool's name, no number, the finish pair.
    harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::Polyline);
    harness.settle();
    let scene = harness.scene();
    // Two points, so the draft can be finished: the card's check appears only for a draft the commit
    // would accept, which is the draft's own word (ADR 0077).
    harness.click(scene.center() - egui::vec2(120.0, 0.0), egui::PointerButton::Primary);
    harness.click(scene.center() + egui::vec2(120.0, 0.0), egui::PointerButton::Primary);
    let view = harness.app.tabs.active().card_view(ui::language(&harness.ctx)).expect("a running draft draws its card");
    assert!(view.rows.is_empty(), "a draft asks for no number");
    assert_eq!(view.title, Some(Message::ToolPolyline), "the row names the tool");
    assert!(matches!(view.commit, ui::ToolCommit::Pair { .. }), "and holds its two buttons");
    // The cross is the bar's old *Cancel*: the draft is dropped and the document never changed.
    let revision = harness.revision();
    harness.app.tabs.active_mut().apply_tool_outcome(
        &ctx,
        ui::ToolOutcome { action: Some(ui::ToolAction::Cancel), ..ui::ToolOutcome::default() },
    );
    assert!(harness.app.tabs.active().editor.drawing().is_none(), "the card's cross drops the draft");
    assert_eq!(harness.revision(), revision, "and writes nothing");

    // A live contour edit draws its own row: the ring's name, its one switch and the pair.
    {
        let document = harness.app.tabs.active_mut();
        document.editor.select_node(Some(object));
        assert!(
            document.editor.start_contour_edit(document.session.project(), ui::ContourTarget::Path(object)),
            "an open chain is editable"
        );
    }
    let view = harness.app.tabs.active().card_view(ui::language(&harness.ctx)).expect("a contour edit draws its card");
    assert_eq!(view.title, Some(Message::ContourEdit), "the row names the edit");
    let toggle = view.toggle.expect("the corner motion is one switch on the card");
    assert_eq!(toggle.words, Message::FreeMove, "free movement is the default");
    harness
        .app
        .tabs
        .active_mut()
        .apply_tool_outcome(&ctx, ui::ToolOutcome { toggle: true, ..ui::ToolOutcome::default() });
    let view = harness.app.tabs.active().card_view(ui::language(&harness.ctx)).expect("the card stays up");
    assert_eq!(view.toggle.expect("the switch is still there").words, Message::AlongContour, "the press flips it");
    // The check is the edit's one command: it ends the mode and leaves one undo entry.
    harness.app.tabs.active_mut().apply_tool_outcome(
        &ctx,
        ui::ToolOutcome { action: Some(ui::ToolAction::Apply), ..ui::ToolOutcome::default() },
    );
    assert!(harness.app.tabs.active().editor.contour_edit().is_none(), "the check applies and leaves the edit");
    assert!(harness.app.tabs.active().last_error.is_none(), "and the applied working copy is accepted");
}

#[test]
fn the_viewport_menu_arms_a_gesture_and_writes_nothing() {
    // The same four are in the node menu, beside the eye and the padlock: they are view state, so
    // the tab records the wish, the shell takes it after the menus have drawn, and no revision moves.
    let (project, product, panel) = product_with_a_side_panel();
    let mut session = engine::Session::with_directory(project, testkit::sample_catalog());
    assert!(session.open_product(product), "the fixture product opens");
    let mut harness = Harness::with_session("frame-mode-menu", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();

    let at = harness.node_point(panel);
    let revision = harness.revision();
    harness.click(at, egui::PointerButton::Secondary);
    harness.settle();
    let menu = harness.app.tabs.active().menu_rect().expect("a right click opens the node's actions");
    let rotate = TransformMode::Rotate.label(Lang::En);
    let entry = harness
        .drawn_rects
        .iter()
        .find(|(rect, text)| menu.contains_rect(*rect) && text == rotate)
        .map(|(rect, _)| rect.center())
        .unwrap_or_else(|| panic!("the menu offers `{rotate}`: {:?}", harness.drawn));
    harness.click(entry, egui::PointerButton::Primary);
    assert_eq!(harness.app.shell.mode, TransformMode::Rotate, "the menu entry arms the gesture it names");
    assert_eq!(harness.revision(), revision, "and a gesture is view state: no undo entry, no revision");
    assert!(harness.app.tabs.active().menu_rect().is_none(), "the menu closes when an entry runs");
}

#[test]
fn the_furniture_move_takes_a_panel_by_its_body_and_slides_it_its_own_way() {
    // N24: the object move draws no handle. A click anywhere on the board takes it, the pointer
    // carries it with no button held, and the next click puts it down as one undo entry; what it may
    // travel is still the document's answer, so the board never moves across its own faces (ADR 0147).
    let (session, panel) = one_wide_panel();
    let mut harness = Harness::with_session("frame-furniture-panel", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.tabs.active_mut().editor.select_node(None);
    harness.settle();

    let cell = harness.mode_cell(&TransformMode::Object.caption(Lang::En));
    harness.click(cell, egui::PointerButton::Primary);
    assert_eq!(harness.app.shell.mode, TransformMode::Object, "the header's cell arms the object move");
    harness.settle();

    let axes = harness.app.tabs.active().session.project().move_axes(&[panel]);
    assert_eq!(axes, [false, false, true], "a board offers its own thickness and nothing else");
    // The middle of the board's face is no handle: the body is what a click takes.
    let body = harness.node_point(panel);
    let revision = harness.revision();
    let before = harness.app.tabs.active().session.project().world_box(panel).expect("the board has a box").0;
    harness.click(body, egui::PointerButton::Primary);
    assert!(harness.app.tabs.active().holding_piece(), "a click on the board takes it");
    assert_eq!(harness.selected(), vec![panel], "and the selected piece is what the pointer carries");
    assert_eq!(harness.revision(), revision, "taking a piece writes nothing");

    // The pointer carries it: up and to the side, which the board may only partly obey.
    harness.hover(body + egui::vec2(60.0, -80.0));
    assert_eq!(harness.revision(), revision, "carrying it is a preview, not a revision");
    let carried = harness.app.tabs.active().carried_delta();
    assert!(carried.z > 0.0, "the board rises in its thickness direction: {carried:?}");

    harness.click(body + egui::vec2(60.0, -80.0), egui::PointerButton::Primary);
    assert!(!harness.app.tabs.active().holding_piece(), "the second click puts it down");
    assert_eq!(harness.revision().get(), revision.get() + 1, "one carried piece is one undo entry");
    let after = harness.app.tabs.active().session.project().world_box(panel).expect("a box").0;
    assert!(after.z > before.z, "the board moved along its thickness: {after:?} from {before:?}");
    assert!(
        (after.x - before.x).abs() < 1.0e-6 && (after.y - before.y).abs() < 1.0e-6,
        "and never across its own faces: {after:?} from {before:?}"
    );
}

#[test]
fn a_floor_frame_is_carried_across_the_floor_in_one_gesture() {
    // X74: a frame is not a board. A floor-bound product may travel on both horizontal axes at
    // once, so one diagonal hand move carries it across the floor, and the vertical stays the
    // class's to refuse. The single-axis latch belongs to a panel (N22).
    let (session, frame) = one_floor_frame("10.01.01");
    let mut harness = Harness::with_session("frame-carry-floor", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    harness.settle();

    let project = harness.app.tabs.active().session.project();
    let before = project.world_box(frame).expect("the frame has a box");
    let pivot = project.pivot_world(frame).expect("a pivot");
    let target = pivot + geom::DVec3::new(200.0, 150.0, 0.0);
    let body = piece_middle(&harness, frame);
    let travel =
        gizmo_math::screen_point(harness.camera(), harness.scene(), target).expect("the target is on screen") - body;
    let revision = harness.revision();
    harness.carry(body, travel);

    let after = harness.app.tabs.active().session.project().world_box(frame).expect("a box");
    assert!(
        (after.0.x - before.0.x).abs() > 1.0,
        "the frame crossed the floor in X: {:?} from {:?}",
        after.0,
        before.0
    );
    assert!((after.0.y - before.0.y).abs() > 1.0, "and in Y in the same gesture: {:?} from {:?}", after.0, before.0);
    assert!(
        (after.0.z - before.0.z).abs() < 1.0e-6,
        "while its class kept its height: {:?} from {:?}",
        after.0,
        before.0
    );
    assert_eq!(harness.revision().get(), revision.get() + 1, "one carry is one undo entry");
}

#[test]
fn a_frame_stops_on_the_axis_it_meets_and_slides_on_the_other() {
    // X74: contact is resolved per axis. Two floor frames 100 mm apart: a diagonal carry stops the
    // first frame's face on the second's and keeps carrying it along that face instead of freezing
    // the whole gesture the way a one-axis drag did.
    let (session, mover, neighbour) = two_floor_frames();
    let mut harness = Harness::with_session("frame-carry-slide", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    harness.settle();

    let project = harness.app.tabs.active().session.project();
    let before = project.world_box(mover).expect("the frame has a box");
    let contact = project.world_box(neighbour).expect("the neighbour has a box").0.x;
    let middle = (before.0 + before.1) * 0.5;
    let target = middle + geom::DVec3::new(400.0, 400.0, 0.0);
    let body = piece_middle(&harness, mover);
    let travel =
        gizmo_math::screen_point(harness.camera(), harness.scene(), target).expect("the target is on screen") - body;
    harness.carry(body, travel);

    let after = harness.app.tabs.active().session.project().world_box(mover).expect("a box");
    assert!(
        (after.1.x - contact).abs() < 1.0e-6,
        "the frame's face landed on the neighbour at {contact}: {} ",
        after.1.x
    );
    assert!(
        after.0.y > before.0.y + 1.0,
        "and the same gesture slid it along the face: {:?} from {:?}",
        after.0,
        before.0
    );
    assert!((after.0.z - before.0.z).abs() < 1.0e-6, "while the floor kept its height");
}

#[test]
fn a_frames_class_decides_whether_it_may_be_hung() {
    // The owner's report: changing the class in the inspector seemed to change nothing. The rule
    // is read at the take, so the same node follows its new class on the next take (X74): a floor
    // cabinet may not rise and a wall cabinet may.
    let (session, frame) = one_floor_frame("10.01.01");
    let mut harness = Harness::with_session("frame-carry-class", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    harness.settle();

    let body = piece_middle(&harness, frame);
    let pivot = harness.app.tabs.active().session.project().pivot_world(frame).expect("a pivot");
    let upward = gizmo_math::screen_point(harness.camera(), harness.scene(), pivot + geom::DVec3::new(0.0, 0.0, 300.0))
        .expect("the target is on screen")
        - body;

    harness.click(body, egui::PointerButton::Primary);
    harness.hover(body + upward);
    assert!(harness.app.tabs.active().holding_piece(), "the cabinet is in hand");
    let carried = harness.app.tabs.active().carried_delta();
    assert!(carried.z.abs() < 1.0e-6, "a base cabinet keeps its height: {carried:?}");
    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!harness.app.tabs.active().holding_piece(), "Escape puts the cabinet back");

    // The inspector's own command assigns the class: one `node.property.set` on the same node.
    let value = serde_json::to_value(model::PropertyValue::class("10.01.02".parse().expect("a wall cabinet")))
        .expect("the class value");
    harness
        .app
        .tabs
        .active_mut()
        .session
        .dispatch(
            engine::node_cmds::NODE_PROPERTY_SET_ID,
            &serde_json::json!({ "node": frame, "key": model::SYSTEM_CLASS, "value": value }),
        )
        .expect("the class changes");
    harness.settle();

    let body = piece_middle(&harness, frame);
    harness.click(body, egui::PointerButton::Primary);
    harness.hover(body + upward);
    assert!(harness.app.tabs.active().holding_piece(), "the hung cabinet is in hand");
    let carried = harness.app.tabs.active().carried_delta();
    assert!(carried.z > 1.0, "a wall cabinet hangs where the hand puts it: {carried:?}");
    // X75: and it keeps the hand while it rises, because a piece no axis holds back is met on the
    // camera's own plane and that plane is the one it travels in.
    let shown = carried_screen(&harness, pivot);
    assert!((shown - upward).length() < 1.5, "the hung cabinet keeps the hand: {shown:?} vs {upward:?}");
    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
}

/// Two floor-bound product frames with no parts, 100 mm apart on X, so a diagonal carry can meet
/// the neighbour on one axis while the other keeps travelling.
fn two_floor_frames() -> (engine::Session, Id, Id) {
    let mm = testkit::millimetres;
    let root = model::Container::new(Id::new(1).expect("an id"), "Root", model::Placement::IDENTITY, Vec::new());
    let mut project = model::Project::new(root).expect("an empty project");
    let envelope = Envelope::new(mm(600), mm(560), mm(720)).expect("a frame");
    let class: ClassCode = "10.01.01".parse().expect("a base cabinet");
    let mover = project
        .add_product(project.root().id(), "Case", class, envelope.clone(), model::Placement::IDENTITY)
        .expect("the moving frame");
    let beside = model::Placement::IDENTITY.with_translation(mm(700), Length::ZERO, Length::ZERO).expect("a placement");
    let neighbour = project.add_product(project.root().id(), "Next", class, envelope, beside).expect("the neighbour");
    (engine::Session::with_directory(project, testkit::sample_catalog()), mover, neighbour)
}

/// A product frame with no parts, so a press lands on its own envelope.
fn one_floor_frame(code: &str) -> (engine::Session, Id) {
    let mm = testkit::millimetres;
    let root = model::Container::new(Id::new(1).expect("an id"), "Root", model::Placement::IDENTITY, Vec::new());
    let mut project = model::Project::new(root).expect("an empty project");
    let envelope = Envelope::new(mm(600), mm(560), mm(720)).expect("a frame");
    let class: ClassCode = code.parse().expect("a product class");
    let frame =
        project.add_product(project.root().id(), "Case", class, envelope, model::Placement::IDENTITY).expect("a frame");
    (engine::Session::with_directory(project, testkit::sample_catalog()), frame)
}

#[test]
fn a_piece_in_hand_goes_back_on_escape_and_a_click_that_never_moved_writes_nothing() {
    // The two ways a hold ends without a placement: Escape puts the piece back, and a click that
    // never travelled commits a zero delta, which is no command at all (ADR 0151).
    let (session, panel) = one_wide_panel();
    let mut harness = Harness::with_session("frame-furniture-cancel", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    // The carry is the hand's: the workspace's snaps would catch the piece at round millimetres and
    // the test is about the hand, not about them (U116).
    harness.app.tabs.active_mut().set_snap_kind(SnapKind::Centre, false);
    harness.settle();

    let body = harness.node_point(panel);
    let revision = harness.revision();
    let start = harness.app.tabs.active().session.project().world_box(panel).expect("a box").0;
    harness.click(body, egui::PointerButton::Primary);
    harness.hover(body + egui::vec2(40.0, -90.0));
    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!harness.app.tabs.active().holding_piece(), "Escape puts the piece back");
    assert_eq!(harness.revision(), revision, "and writes nothing");
    let restored = harness.app.tabs.active().session.project().world_box(panel).expect("a box").0;
    assert!((restored - start).length() < 1.0e-6, "the board is where it was: {restored:?} from {start:?}");

    // A click that takes the piece and a click that puts it back without travelling: no command.
    harness.click(body, egui::PointerButton::Primary);
    assert!(harness.app.tabs.active().holding_piece(), "the board is taken again");
    harness.click(body, egui::PointerButton::Primary);
    assert!(!harness.app.tabs.active().holding_piece(), "and put down without moving");
    assert_eq!(harness.revision(), revision, "a zero move is no undo entry");
}

#[test]
fn the_object_move_answers_the_place_the_board_lands_and_never_inside_its_neighbour() {
    // X72 gave the object move its contact, and X77 makes it a rule about the place the board lands:
    // a drag whose destination is inside the neighbour stops flush on its face, and a drag whose
    // destination is beyond the neighbour carries the board past it and lands in the free place
    // there. What it may not do is come to rest inside. The free move is not measured at all
    // (ADR 0180), so it is not asked here.
    let mode = TransformMode::Object;
    let (ended, low, _high, revisions) = moved_divider(mode, 60.0, "frame-object-flush");
    assert!(
        (ended - low).abs() < 1.0e-6,
        "{mode:?} stops the board flush against the neighbour at {low}: it ends at {ended}"
    );
    assert_eq!(revisions, 1, "{mode:?} costs one undo entry");
    let (ended, low, high, revisions) = moved_divider(mode, 400.0, "frame-carry-beyond");
    assert!(
        ended >= high - 1.0e-6,
        "{mode:?} lands the board in the free place beyond the neighbour at {high}: it ends at {ended} (low {low})"
    );
    assert_eq!(revisions, 1, "{mode:?} costs one undo entry");
}

#[test]
fn the_free_move_carries_the_board_through_its_neighbour() {
    // The free move is the maker's own hand: no snap, no stop and no refusal, so the board goes
    // where the pointer puts it and its write is not turned back (ADR 0180). The drag that would
    // stop the object move flush on the neighbour's face passes it here, and one undo entry is what
    // it costs.
    let (ended, low, _high, revisions) = moved_divider(TransformMode::Move, 60.0, "frame-free-through");
    assert!(ended > low + 1.0, "the free move passes the face the object move stops on ({low}): it ends at {ended}");
    assert_eq!(revisions, 1, "and costs one undo entry");
}

/// Moves the divider of [`divider_beside_a_panel`] `points` along world X and answers the face it
/// ends on, the neighbour's two faces and how many revisions the gesture cost.
///
/// The drag is the same question the two tools are asked with their own gesture: the free move takes
/// the handle and drags it, the object move takes the board by its body and carries it (N24).
fn moved_divider(mode: TransformMode, points: f32, name: &str) -> (f64, f64, f64, u64) {
    let (session, divider, beside) = divider_beside_a_panel();
    let mut harness = Harness::with_session(name, session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.tabs.active_mut().editor.select_node(Some(divider));
    harness.app.shell.mode = mode;
    harness.settle();
    let project = harness.app.tabs.active().session.project();
    let axes = project.move_axes(&[divider]);
    assert_eq!(axes, [true, false, false], "a divider crosses the case, so X is the only way it travels");
    let (handle, direction) = handle_point_on(&harness, divider, axes, Some(gizmo_math::Axis::X))
        .expect("the direction X runs in is on screen");
    let revision = harness.revision();
    match mode {
        TransformMode::Object => {
            // Any part of the body takes it; the travel names the axis it may move along.
            let body = harness.node_point(divider);
            harness.carry(body, direction * points);
        }
        _ => {
            harness.drag(handle, direction * points, egui::PointerButton::Primary);
        }
    }
    let project = harness.app.tabs.active().session.project();
    let ended = project.world_box(divider).expect("the divider has a box").1.x;
    let (low, high) = project.world_box(beside).expect("the neighbour has a box");
    (ended, low.x, high.x, harness.revision().get() - revision.get())
}

/// A `3000 x 600 x 720` frame holding a divider and the panel beside it, 50 mm apart.
///
/// The divider is turned a quarter about Y, so its thickness — and with it the only axis the
/// object move offers it — runs along world X and the gap is measured across it (N22).
fn divider_beside_a_panel() -> (engine::Session, Id, Id) {
    let mm = testkit::millimetres;
    let root = model::Container::new(Id::new(1).expect("an id"), "Root", model::Placement::IDENTITY, Vec::new());
    let mut project = model::Project::new(root).expect("an empty project");
    let class: ClassCode = "10.01.01".parse().expect("a base cabinet");
    let envelope = Envelope::new(mm(3000), mm(600), mm(720)).expect("a frame");
    let frame = project
        .add_product(project.root().id(), "Case", class, envelope, model::Placement::IDENTITY)
        .expect("a product");
    let size = model::PanelNominalSize::new(mm(600), mm(560)).expect("a panel size");
    let quarter = model::EulerAngles { y: units::Degrees::from_millidegrees(90_000), ..model::EulerAngles::default() };
    let turned = model::Placement::IDENTITY
        .with_rotation_euler(quarter)
        .and_then(|placement| placement.with_translation(mm(1000), Length::ZERO, Length::ZERO))
        .expect("a quarter-turned placement");
    let divider = project.add_panel(frame, "Divider", size, None, mm(18), turned).expect("a divider");
    let beside_at = model::Placement::IDENTITY
        .with_translation(mm(1068), Length::ZERO, mm(-600))
        .expect("a placement beside the divider");
    let beside = project.add_panel(frame, "Beside", size, None, mm(18), beside_at).expect("a panel beside the divider");
    let mut session = engine::Session::with_directory(project, testkit::sample_catalog());
    // The product is open, so a click lands on the part and not on the frame around it: the
    // object move takes the piece the click named (N24).
    assert!(session.open_product(frame), "the fixture product opens");
    (session, divider, beside)
}

#[test]
fn a_carried_piece_measures_its_gaps_in_the_scene_and_a_press_over_one_puts_it_down() {
    // N26: a carried piece's two gaps are dimensioned where they are measured, so the object move
    // draws no card at all — the numbers are one field in the middle of each arrow and the words are
    // the inspector's. X66 still holds: a field that sits in the scene covers the very press that
    // puts the piece down, so it takes the keyboard and never the pointer.
    let (session, shelf) = a_raised_shelf();
    let mut harness = Harness::with_session("frame-carry-gaps", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    harness.settle();
    let axes = harness.app.tabs.active().session.project().move_axes(&[shelf]);
    let (_, up) = handle_point_on(&harness, shelf, axes, Some(gizmo_math::Axis::Z))
        .expect("the direction Z runs in is on screen");
    let body = harness.node_point(shelf);

    // The hand travels down, so the gap to the floor is the field beside it.
    harness.click(body, egui::PointerButton::Primary);
    harness.hover(body - up * 10.0);
    assert!(harness.app.tabs.active().holding_piece(), "the shelf is in hand");
    assert!(!harness.app.tabs.active().tool_card_is_up(), "a carried piece draws no card");
    let chrome = harness.settle();
    let scene = harness.scene();
    let fields: Vec<egui::Rect> = harness
        .drawn_rects
        .iter()
        .filter(|(rect, text)| scene.contains_rect(*rect) && text == "mm")
        .map(|(rect, _)| *rect)
        .filter(|rect| !chrome.panels.panels().any(|panel| panel.contains_rect(*rect)))
        .collect();
    assert_eq!(fields.len(), 2, "one field per gap, drawn in the scene: {fields:?}");
    assert!(harness.drew(Message::ToolGapLow.text(Lang::En)), "the inspector names the low gap");
    assert!(harness.drew(Message::ToolGapHigh.text(Lang::En)), "and the high one");

    // The press over a field is the scene's, not the plate's: the piece is put down where the hand
    // holds it, as one undo entry, rather than the number under it being scrubbed.
    let revision = harness.revision();
    harness.click(fields[0].center(), egui::PointerButton::Primary);
    assert!(!harness.app.tabs.active().holding_piece(), "the press over a field puts the piece down");
    assert_eq!(harness.revision().get(), revision.get() + 1, "and commits it as one undo entry");
}

#[test]
fn a_gap_typed_into_a_scene_field_lands_where_it_was_typed_as_one_undo_entry() {
    // The Done of N26: the arrow's own field is the typed path to the gap. A number pins the face
    // against the pointer, the opposite face follows, and the click that puts the piece down is the
    // one command — one undo entry for the whole gesture (ADR 0103).
    let (session, shelf) = a_raised_shelf();
    let mut harness = Harness::with_session("frame-carry-typed-gap", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    harness.settle();
    let start = harness.app.tabs.active().session.project().world_placement(shelf).expect("a world frame");
    let axes = harness.app.tabs.active().session.project().move_axes(&[shelf]);
    let (_, up) = handle_point_on(&harness, shelf, axes, Some(gizmo_math::Axis::Z))
        .expect("the direction Z runs in is on screen");
    let body = harness.node_point(shelf);
    // The document still holds the shelf where it was taken, so a reading of the gap adds the
    // carry's own preview to the stored face (a drag previews and the click commits it).
    let gap = |harness: &Harness| {
        let document = harness.app.tabs.active();
        let stored = document.session.project().world_box(shelf).expect("a box").0.z;
        stored + document.carried_delta().z
    };

    // The hand travels down, so the field that opens is the gap to the floor.
    harness.click(body, egui::PointerButton::Primary);
    harness.hover(body - up * 10.0);
    assert!(harness.app.tabs.active().holding_piece(), "the shelf is in hand");
    let revision = harness.revision();
    type_text(&mut harness, "5");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert!(
        (gap(&harness) - 5.0).abs() < 1.0,
        "the typed number pinned the face 5 mm off the floor: {}",
        gap(&harness)
    );
    assert_eq!(harness.revision(), revision, "typing writes nothing on its own");

    harness.click(body - up * 10.0, egui::PointerButton::Primary);
    assert!(!harness.app.tabs.active().holding_piece(), "the click puts the piece down");
    assert_eq!(harness.revision().get(), revision.get() + 1, "one carried piece is one undo entry");
    let landed = harness.app.tabs.active().session.project().world_box(shelf).expect("a box").0.z;
    assert!((landed - 5.0).abs() < 1.0, "and the document holds the gap that was typed: {landed}");
    harness.press(egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        harness.app.tabs.active().session.project().world_placement(shelf),
        Some(start),
        "one undo takes the whole gesture back"
    );
}

/// A cabinet holding one shelf raised 100 mm off its floor: a piece with a gap on both sides.
///
/// The shelf's thickness runs along world Z, so the object move offers it that one axis and both
/// of its gaps have a surface to measure against (N26).
fn a_raised_shelf() -> (engine::Session, Id) {
    let mm = testkit::millimetres;
    let root = model::Container::new(Id::new(1).expect("an id"), "Root", model::Placement::IDENTITY, Vec::new());
    let mut project = model::Project::new(root).expect("an empty project");
    let envelope = Envelope::new(mm(600), mm(560), mm(720)).expect("a frame");
    let frame = project
        .add_product(project.root().id(), "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, model::Placement::IDENTITY)
        .expect("a product");
    let size = model::PanelNominalSize::new(mm(500), mm(400)).expect("a panel size");
    let raised = model::Placement::IDENTITY.with_translation(mm(50), mm(80), mm(100)).expect("a raised placement");
    let shelf = project.add_panel(frame, "Shelf", size, None, mm(18), raised).expect("a shelf");
    let mut session = engine::Session::with_directory(project, testkit::sample_catalog());
    assert!(session.open_product(frame), "the fixture product opens");
    (session, shelf)
}

#[test]
fn a_sideways_hand_never_carries_a_piece_away() {
    // The defect a maker reported: the panel ran away from the cursor. The distance was the ray's
    // projection onto the axis line, and a pointer that strayed sideways from that line met it at an
    // arbitrary place. A carry is measured by the hand's own travel along the axis, so a move across
    // it is no move at all (N25).
    let (session, divider, _beside) = divider_beside_a_panel();
    let mut harness = Harness::with_session("frame-carry-sideways", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    harness.settle();
    let axes = harness.app.tabs.active().session.project().move_axes(&[divider]);
    assert_eq!(axes, [true, false, false], "the divider crosses the case, so X is the only way it travels");
    let (_, along) = handle_point_on(&harness, divider, axes, Some(gizmo_math::Axis::X))
        .expect("the direction X runs in is on screen");
    let across = egui::vec2(-along.y, along.x);
    let body = harness.node_point(divider);

    harness.click(body, egui::PointerButton::Primary);
    assert!(harness.app.tabs.active().holding_piece(), "the divider is in hand");
    // Away from the neighbour, so the rule does not clamp either measurement.
    harness.hover(body - along * 40.0);
    let carried = harness.app.tabs.active().carried_delta();
    assert!(carried.length() > 0.0, "the hand along the axis carried it: {carried:?}");

    for step in [40.0_f32, 120.0, 240.0] {
        harness.hover(body - along * 40.0 + across * step);
        let sideways = harness.app.tabs.active().carried_delta();
        assert!(
            (sideways - carried).length() < 1.0,
            "a hand {step} points across the axis moves nothing: {sideways:?} from {carried:?}"
        );
    }
}

#[test]
fn a_carried_piece_keeps_the_hand_through_a_long_move_and_a_zoom() {
    // X65, in perspective: the scale of the axis changes as the piece travels (a perspective camera)
    // and again when the view is zoomed, so the hand's travel is integrated frame by frame. A piece
    // measured against one anchor held for the whole carry lags behind the hand and looks like it
    // ran away as soon as the camera moves.
    let (project, product, panel) = product_with_a_side_panel();
    let mut session = engine::Session::with_directory(project, testkit::sample_catalog());
    assert!(session.open_product(product), "the fixture product opens");
    let mut harness = Harness::with_session("frame-carry-perspective", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    // The carry is the hand's: the workspace's snaps would catch the piece at round millimetres and
    // the test is about the hand, not about them (U116).
    harness.app.tabs.active_mut().set_snap_kind(SnapKind::Centre, false);
    harness.settle();

    let body = harness.node_point(panel);
    let piece = |harness: &Harness| {
        let project = harness.app.tabs.active().session.project();
        let moved = project.pivot_world(panel).expect("a pivot") + harness.app.tabs.active().carried_delta();
        gizmo_math::screen_point(harness.camera(), harness.scene(), moved).expect("the piece is on screen")
    };
    harness.click(body, egui::PointerButton::Primary);
    let start = piece(&harness);
    let mut fingers = Vec::new();
    for step in 1..=3 {
        let at = body - egui::vec2(0.0, 40.0 * step as f32);
        harness.hover(at);
        fingers.push((at, piece(&harness)));
    }
    for (index, (at, moved)) in fingers.iter().enumerate() {
        let hand = (at.y - body.y).abs();
        let travelled = (moved.y - start.y).abs();
        assert!(
            (travelled - hand).abs() < hand * 0.2,
            "step {}: the hand moved {hand} pt and the piece {travelled} pt",
            index + 1
        );
    }

    // A zoom under the carry is the eye's and not the hand's: the wheel zooms towards the pointer, the
    // piece keeps its millimetres, and the pointer is what comes back to it (X67, ADR 0156).
    let at = fingers.last().expect("three steps").0;
    let carried = harness.app.tabs.active().carried_delta();
    let wheel = |points: f32| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, points),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
    };
    for _ in 0..2 {
        harness.run(vec![egui::Event::PointerMoved(at), wheel(-80.0)]);
        harness.settle();
    }
    assert_eq!(harness.app.tabs.active().carried_delta(), carried, "a zoom carries nothing");
    // The wheel smooths itself over a few frames; then the carry puts its hand back on the piece and
    // the mouse carries it again from there.
    for _ in 0..16 {
        harness.run(Vec::new());
    }
    assert_eq!(harness.app.tabs.active().carried_delta(), carried, "and the piece still stands where it was");
    let hand = harness.app.tabs.active().carry_hand().expect("the carry has its own hand after a zoom");
    let back = hand - piece_middle(&harness, panel);
    assert!(back.length() < 3.0, "the hand is back in the middle of the board: {back:?}");

    let before = harness.app.tabs.active().carried_delta();
    harness.hover(at - egui::vec2(0.0, 40.0));
    assert!(
        (harness.app.tabs.active().carried_delta() - before).length() > 1.0,
        "after the zoom the mouse carries the piece again from the hand"
    );
}

#[test]
fn a_piece_in_hand_blocks_no_view_and_the_view_moves_no_piece() {
    // N25, X67: the hold owns no button, so nothing about it may own the viewport. The camera keeps
    // orbiting, panning and zooming under a piece in hand — and none of that moves the piece: a view
    // gesture writes nothing, and the pointer is what comes back to the piece afterwards (ADR 0156).
    let (session, panel) = one_wide_panel();
    let mut harness = Harness::with_session("frame-furniture-view", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    // The carry is the hand's: the workspace's snaps would catch the piece at round millimetres and
    // the test is about the hand, not about them (U116).
    harness.app.tabs.active_mut().set_snap_kind(SnapKind::Centre, false);
    harness.settle();

    let body = harness.node_point(panel);
    harness.click(body, egui::PointerButton::Primary);
    // The pointer carries the piece, so every move of the test's pointer is a move of the piece: the
    // camera gestures below start from where the hand already is.
    let at = body + egui::vec2(50.0, -70.0);
    harness.hover(at);
    assert!(harness.app.tabs.active().holding_piece(), "the board is in hand");
    let carried = harness.app.tabs.active().carried_delta();
    assert!(carried.z > 0.0, "the pointer carried it along its thickness: {carried:?}");
    let placed = harness.app.tabs.active().session.project().world_placement(panel).expect("a world frame");

    // Orbit: the camera moves, the piece does not.
    let camera = harness.camera();
    let orbit = egui::vec2(60.0, 25.0);
    harness.drag(at, orbit, egui::PointerButton::Middle);
    assert_ne!(harness.camera(), camera, "the camera orbits under a piece in hand");
    let kept = harness.app.tabs.active().carried_delta();
    assert!((kept - carried).length() < 1.0e-3, "and the piece is not dragged along: {kept:?} from {carried:?}");
    assert!(harness.app.tabs.active().holding_piece(), "the piece is still in hand after the orbit");

    // Zoom: the same answer for the wheel.
    let camera = harness.camera();
    let wheel = |points: f32| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, points),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
    };
    // The wheel goes through the scene, so the pointer stays clear of the card the carry draws.
    let wheel_at = at + orbit + egui::vec2(60.0, 40.0);
    harness.run(vec![egui::Event::PointerMoved(wheel_at), wheel(-40.0)]);
    harness.settle();
    assert_ne!(harness.camera(), camera, "the wheel zooms under a piece in hand");
    let kept = harness.app.tabs.active().carried_delta();
    assert!((kept - carried).length() < 1.0e-3, "and the piece keeps its distance: {kept:?}");
    assert_eq!(
        harness.app.tabs.active().session.project().world_placement(panel),
        Some(placed),
        "no view gesture moved the board in the room"
    );

    // The gesture is over — the wheel smooths itself out over a few frames — so the carry's hand
    // comes back to the piece instead of the piece to it.
    for _ in 0..12 {
        harness.run(Vec::new());
    }
    let hand = harness.app.tabs.active().carry_hand().expect("the carry has its own hand after a gesture");
    let back = hand - piece_middle(&harness, panel);
    assert!(back.length() < 3.0, "the hand is back on the piece: {back:?}");
    assert!((harness.app.tabs.active().carried_delta() - carried).length() < 1.0e-3, "and the piece has not moved");

    // The click that puts it down commits the distance the hand added, and only that.
    let revision = harness.revision();
    harness.click(hand, egui::PointerButton::Primary);
    assert!(!harness.app.tabs.active().holding_piece(), "the click puts it down");
    assert_eq!(harness.revision().get(), revision.get() + 1, "one carried piece is one undo entry");
}

#[test]
fn a_tool_or_a_command_under_the_hand_puts_the_piece_back() {
    // N25: a hold is not a mode. A tool that takes the pointer, or a command that lands under the
    // hand, ends it and writes nothing — the same answer Escape gives.
    let (session, panel) = one_wide_panel();
    let mut harness = Harness::with_session("frame-furniture-not-a-mode", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    harness.settle();
    let body = harness.node_point(panel);
    let revision = harness.revision();

    // The ruler takes the pointer, so the piece goes back.
    harness.click(body, egui::PointerButton::Primary);
    harness.hover(body + egui::vec2(40.0, -60.0));
    assert!(harness.app.tabs.active().holding_piece());
    harness.press(egui::Key::M, egui::Modifiers::NONE);
    assert!(!harness.app.tabs.active().holding_piece(), "the ruler puts the piece back");
    assert_eq!(harness.revision(), revision, "and writes nothing");
    harness.press(egui::Key::M, egui::Modifiers::NONE);

    // A command under the hand does the same: the document the rule was read from is gone. One piece
    // is put down first, so the undo has a placement to take back.
    harness.carry(body, egui::vec2(40.0, -60.0));
    let placed = harness.revision();
    assert_eq!(placed.get(), revision.get() + 1, "the first piece landed");
    let at = harness.node_point(panel);
    harness.click(at, egui::PointerButton::Primary);
    harness.hover(at + egui::vec2(40.0, -60.0));
    assert!(harness.app.tabs.active().holding_piece(), "the board is in hand again");
    harness.press(egui::Key::Z, egui::Modifiers::CTRL);
    assert!(!harness.app.tabs.active().holding_piece(), "an undo puts the piece back");
    assert!(harness.revision() != placed, "and the undo is the one the key asked for");
}

#[test]
fn each_tab_keeps_its_own_open_product() {
    // Openness is session view state (ADR 0098), and a tab owns its session: a new tab starts with
    // every product closed, and the first tab still works in the frame it opened.
    // Openness is session view state (ADR 0098), and a tab owns its session: a new tab starts with
    // every product closed, and the first tab still works in the frame it opened.
    let mut harness = Harness::new("frame-product-tabs");
    harness.settle();
    let first_tab = harness.app.tabs.active_id();
    let draft = ui::product_card::ProductDraft {
        envelope: Envelope::new(testkit::millimetres(600), testkit::millimetres(560), testkit::millimetres(720))
            .expect("a valid box"),
        class: ClassCode::GENERAL_PRODUCT,
    };
    harness.app.tabs.active_mut().create_product(draft, Lang::En).expect("create the frame");
    let first = harness.app.tabs.active().session.active_product().expect("the first tab works in it");

    harness.app.new_tab().expect("a new tab");
    let second_tab = harness.app.tabs.active_id();
    assert_ne!(first_tab, second_tab);
    assert_eq!(harness.app.tabs.active().session.active_product(), None, "a new tab opens no product");

    assert!(harness.app.tabs.activate(first_tab));
    assert_eq!(harness.app.tabs.active().session.active_product(), Some(first), "the first tab kept it");
    assert!(harness.app.tabs.activate(second_tab));
    assert_eq!(harness.app.tabs.active().session.active_product(), None);
}

#[test]
fn the_product_card_creates_a_frame_that_is_open_and_receives_the_next_part() {
    let mut harness = Harness::new("frame-product-card");
    harness.settle();
    let before = harness.revision();
    harness.app.product_card.start();
    harness.settle();
    assert!(harness.app.product_card.is_open(), "the card is up");

    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    let document = harness.app.tabs.active();
    let product = document.session.active_product().expect("the card created a frame and opened it");
    assert!(document.session.project().container(product).is_some_and(|product| product.is_product()));
    assert_eq!(harness.revision().get(), before.get() + 1, "a frame is one command");
    assert!(!harness.app.product_card.is_open(), "the confirmed card closed");
    assert_eq!(
        document.editor.creation_target(document.session.project()),
        product,
        "the next part lands inside the new frame"
    );
}

#[test]
fn the_contour_cell_cuts_a_panel_to_the_ring_the_pointer_drew() {
    // One cell, one gesture, one part: *Panel from contour* arms the free-outline tool and the close
    // is the one command that cuts the board, so no path object is left beside it (ADR 0114).
    let mut harness = Harness::new("frame-contour-panel");
    harness.settle();
    let draft = ui::product_card::ProductDraft {
        envelope: Envelope::new(testkit::millimetres(600), testkit::millimetres(560), testkit::millimetres(720))
            .expect("a valid box"),
        class: ClassCode::GENERAL_PRODUCT,
    };
    harness.app.tabs.active_mut().create_product(draft, Lang::En).expect("create the frame");
    let product = harness.app.tabs.active().session.active_product().expect("the frame is open");
    let before = harness.revision();

    // The cell lives on the Furniture tab, and a fresh workspace opens another one.
    if let Some(tab) = ui::toolbars::spec::tab("furniture") {
        harness.app.tabs.active_mut().editor.set_ribbon_tab(tab);
    }
    harness.settle();
    let hint = Message::PanelContourDraw.with(Lang::En, [Message::ToolContourPanel.text(Lang::En)]);
    let cell = harness.face(&hint).expect("the contour cell is on the strip");
    harness.click(cell, egui::PointerButton::Primary);
    assert_eq!(
        harness.app.tabs.active().editor.drawing_intent(),
        ui::DrawIntent::Panel,
        "the cell arms a draft that becomes a board, not a path"
    );

    // Three clicks on the work plane, then the one *Close* every surface shares.
    let centre = harness.scene().center();
    for corner in [egui::vec2(-160.0, -90.0), egui::vec2(160.0, -90.0), egui::vec2(0.0, 140.0)] {
        harness.click(centre + corner, egui::PointerButton::Primary);
    }
    harness.app.tabs.active_mut().close_draft(Lang::En).expect("the ring closes into a panel");
    assert!(harness.app.tabs.active().editor.drawing().is_none(), "the tool ends with the panel");

    let document = harness.app.tabs.active();
    let project = document.session.project();
    let children = project.container(product).expect("the product is a container").children();
    assert_eq!(children.len(), 1, "the gesture wrote one node and left no path beside it: {children:?}");
    let panel = children.first().and_then(|node| node.as_panel()).expect("the node the ring cut is a panel");
    assert!(panel.is_contour(), "the panel's outline is the drawn ring, not the rectangle around it");
    let ring = panel.outline_contour().expect("a contour panel stores its ring");
    assert_eq!(ring.segments().count(), 3, "three clicks enclose a triangle");
    assert_eq!(harness.revision().get(), before.get() + 1, "the board is one command after the frame");
    assert_eq!(document.editor.selected_node(), Some(panel.id()), "the new board is selected");
}

/// Every panel in `children`, depth first, for a test that names one instead of an id.
fn collect_panels<'a>(children: &'a [model::Node], panels: &mut Vec<&'a model::Panel>) {
    for child in children {
        match child {
            model::Node::Panel(panel) => panels.push(panel),
            model::Node::Container(container) => collect_panels(container.children(), panels),
            model::Node::Object(_) | model::Node::Camera(_) | model::Node::Light(_) => {}
        }
    }
}

/// A product of 600 x 560 x 720 mm holding one 500 x 400 mm side panel, and both ids.
/// The middle of a piece's world box, in the points the viewport draws it at.
fn piece_middle(harness: &Harness, node: Id) -> egui::Pos2 {
    let project = harness.app.tabs.active().session.project();
    let (low, high) = project.world_box(node).expect("the piece has a box");
    let moved = (low + high) * 0.5 + harness.app.tabs.active().carried_delta();
    gizmo_math::screen_point(harness.camera(), harness.scene(), moved).expect("the piece is on screen")
}

fn product_with_a_side_panel() -> (model::Project, Id, Id) {
    let mm = testkit::millimetres;
    let root = model::Container::new(Id::new(1).expect("an id"), "Root", model::Placement::IDENTITY, Vec::new());
    let mut project = model::Project::new(root).expect("an empty project");
    let envelope = Envelope::new(mm(600), mm(560), mm(720)).expect("a box");
    let product = project
        .add_product(project.root().id(), "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, model::Placement::IDENTITY)
        .expect("a product");
    let size = model::PanelNominalSize::new(mm(500), mm(400)).expect("a panel size");
    let panel = project
        .add_panel(product, "Side", size, None, mm(18), model::Placement::IDENTITY)
        .expect("a part inside the product");
    (project, product, panel)
}

#[test]
fn a_click_inside_an_open_product_selects_the_part_under_the_pointer() {
    // X38: the tree selected the part and the viewport selected the whole product, because the
    // product's openness is the session's view state and the pick rule read only the editor's open
    // set. Once a level is open, a click has to reach the part the pointer is on.
    let (project, product, panel) = product_with_a_side_panel();
    let mut session = engine::Session::with_directory(project, testkit::sample_catalog());
    assert!(session.open_product(product), "the fixture product opens");
    let mut harness = Harness::with_session("frame-open-product-pick", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();

    let at = {
        let document = harness.app.tabs.active_mut();
        let world = document.session.project().world_placement(panel).expect("a part has a placement");
        let thickness = document
            .session
            .project()
            .panel(panel)
            .map(|panel| panel.last_known_thickness().millimetres_f64())
            .expect("the part is a panel");
        let centre = world.transform_point(geom::DVec3::new(250.0, 200.0, thickness * 0.5));
        let rect = document.scene_rect().expect("a settled frame gives the scene a rectangle");
        let at = gizmo_math::screen_point(document.camera(), rect, centre).expect("the part is on screen");
        assert!(rect.contains(at), "the press lands on the scene: {at:?} in {rect:?}");
        at
    };
    harness.click(at, egui::PointerButton::Primary);

    assert_eq!(harness.selected(), vec![panel], "a click lands on the part, not on the product around it");
    assert!(
        harness.drawn.iter().any(|line| line.contains("Side") && line.contains("Panel")),
        "the status bar names the part: {:?}",
        harness.drawn
    );
}

#[test]
fn a_right_click_opens_the_actions_of_the_panel_under_the_pointer_and_writes_nothing() {
    // U92: the tree row menu was the only context menu in the program, so the actions of the piece
    // under the hand were three clicks away. A right click over the scene now opens the same list,
    // and the right *drag* stays the camera pan it already was.
    let (project, product, panel) = product_with_a_side_panel();
    let mut session = engine::Session::with_directory(project, testkit::sample_catalog());
    assert!(session.open_product(product), "the fixture product opens");
    let mut harness = Harness::with_session("frame-context-menu", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();

    let at = harness.node_point(panel);
    let revision = harness.revision();
    let selected = harness.selected();
    harness.click(at, egui::PointerButton::Secondary);

    let scene = harness.scene();
    let menu = harness.app.tabs.active().menu_rect().expect("a right click opens the actions under it");
    assert!(menu.width() > 0.0 && menu.height() > 0.0, "the menu took a rectangle: {menu:?}");
    assert!(scene.contains_rect(menu), "and it hangs over the scene it was opened on");
    let (first, label) = harness.first_text_in(menu).expect("the menu drew its entries");
    assert_eq!(label, Message::TreeHide.text(Lang::En), "the first entry of a panel is its eye");
    assert_eq!(harness.revision(), revision, "opening a menu writes no document");
    assert_eq!(harness.selected(), selected, "and the selection is untouched: the menu acts on what it names");

    // The entry under the pointer runs: hiding is view state, so it closes the menu and moves no
    // revision either. The press is chrome, so nothing behind the menu is picked.
    harness.click(first, egui::PointerButton::Primary);
    assert!(harness.app.tabs.active().menu_rect().is_none(), "choosing an entry closes the menu");
    assert!(harness.app.tabs.active().editor.flags().is_hidden(panel), "the eye hid the panel");
    assert_eq!(harness.revision(), revision, "a view entry writes no document");

    // A right drag is the pan the viewport already owned, so it opens nothing.
    let camera = harness.camera();
    let centre = harness.scene().center();
    harness.drag(centre, egui::vec2(60.0, 30.0), egui::PointerButton::Secondary);
    assert_ne!(harness.camera(), camera, "a right drag still pans the view");
    assert!(harness.app.tabs.active().menu_rect().is_none(), "and opens no menu on the way");
}

#[test]
fn a_group_and_empty_space_each_open_the_list_they_own() {
    // The same right click answers with what the pointer names: a closed group offers the view
    // entries and its structural edits, and a corner of the scene offers the one document action.
    let mut harness = Harness::new("frame-context-menu-groups");
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();

    let (group, part) = {
        let project = harness.app.tabs.active().session.project();
        let mut panels = Vec::new();
        collect_panels(project.root().children(), &mut panels);
        let part = panels
            .iter()
            .find(|panel| panel.name() == "Loose shelf")
            .map(|panel| panel.id())
            .expect("the sample shelf");
        let group = project.parent_of(part).expect("the shelf sits in the group");
        (group, part)
    };
    let at = harness.node_point(part);
    harness.click(at, egui::PointerButton::Secondary);
    let menu = harness.app.tabs.active().menu_rect().expect("a closed group opens its own actions");
    assert_eq!(
        harness.first_text_in(menu).map(|(_, text)| text),
        Some(Message::TreeHide.text(Lang::En).to_owned()),
        "the group's menu is a node's menu too"
    );
    for label in [Message::TreeAddChild, Message::CmdUngroup, Message::CmdDelete] {
        assert!(
            harness.drawn_rects.iter().any(|(rect, text)| menu.contains_rect(*rect) && *text == label.text(Lang::En)),
            "the group owns `{}`: {:?}",
            label.text(Lang::En),
            harness.drawn
        );
    }
    assert!(harness.app.tabs.active().session.project().contains_node(group), "the menu only read the tree");
    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().menu_rect().is_none(), "Escape closes it");

    // Nothing is drawn at the middle of the scene once the group is hidden, so the menu there is
    // the document's: it offers *Show all* and nothing that would delete a node the pointer does
    // not name. The entry then brings the model back, which is the reason the entry exists.
    harness.app.tabs.active_mut().editor.set_hidden(group, true);
    harness.settle();
    let at = harness.scene().center();
    let revision = harness.revision();
    harness.click(at, egui::PointerButton::Secondary);
    let menu = harness.app.tabs.active().menu_rect().expect("empty space opens the document action");
    assert_eq!(
        harness.first_text_in(menu).map(|(_, text)| text),
        Some(Message::TreeShowAll.text(Lang::En).to_owned()),
        "empty space owns one entry: {menu:?}"
    );
    assert!(
        !harness
            .drawn_rects
            .iter()
            .any(|(rect, text)| menu.contains_rect(*rect) && *text == Message::CmdDelete.text(Lang::En)),
        "and nothing destructive"
    );
    assert_eq!(harness.revision(), revision, "no menu writes a document");
}

#[test]
fn an_open_level_brackets_its_eight_corners_and_a_closed_one_draws_none() {
    // U89: over the scene an open level and a closed one look identical, so the level the next
    // click works in draws a bracket on each corner of its box, and only while it is open.
    use model::{NodeQuery, StructuralKind};

    let mut harness = Harness::new("frame-open-brackets");
    harness.settle();
    assert!(harness.app.tabs.active().brackets.is_empty(), "nothing is open, so nothing is bracketed");

    let group = {
        let project = harness.app.tabs.active().session.project();
        project
            .query(&NodeQuery::any().with_kind(StructuralKind::Container))
            .into_iter()
            .find(|id| project.container(*id).is_some_and(|container| container.name() == "Loose parts"))
            .expect("the sample has a group")
    };
    let revision = harness.revision();
    let document = harness.app.tabs.active_mut();
    document.editor.enter_viewport_level(&mut document.session, Some(group));
    harness.settle();

    let document = harness.app.tabs.active();
    assert_eq!(document.session.project().revision(), revision, "opening a level writes no document");
    assert_eq!(document.brackets.len(), 8, "one bracket per corner of the open level");
    let scene = harness.scene();
    for bracket in &document.brackets {
        assert!(scene.contains(bracket.corner), "the bracket of an open level is in the picture: {:?}", bracket.corner);
        assert!((6.0..=18.0).contains(&bracket.length), "an arm stays between 6 and 18 points");
    }

    // Closing the same level takes the brackets with it, on the next frame, still without a write.
    let document = harness.app.tabs.active_mut();
    document.editor.exit_level(&mut document.session);
    harness.settle();
    let document = harness.app.tabs.active();
    assert!(document.brackets.is_empty(), "a closed level draws no bracket");
    assert_eq!(document.session.project().revision(), revision, "and closing writes no document either");

    // A product is opened by the session rather than by the open set, and brackets its envelope.
    let (project, product, _panel) = product_with_a_side_panel();
    let mut session = engine::Session::with_directory(project, testkit::sample_catalog());
    assert!(session.open_product(product), "the fixture product opens");
    let mut harness = Harness::with_session("frame-open-product-brackets", session);
    harness.settle();
    assert_eq!(harness.app.tabs.active().brackets.len(), 8, "the open product brackets its authored box");

    let document = harness.app.tabs.active_mut();
    document.editor.close_product(&mut document.session);
    harness.settle();
    assert!(harness.app.tabs.active().brackets.is_empty(), "a closed product draws no bracket");
}

#[test]
fn the_drawn_frame_lands_as_one_product_of_exactly_the_ghost_size() {
    // The frame is created and edited like a box (ADR 0101, N19): the frame cell arms the sketch's
    // rectangle, the figure is the reserved space's base, and the height pull dispatches
    // `product.create` instead of a body's extrude.
    let mut harness = Harness::new("frame-drawn-product");
    harness.settle();
    let before = harness.revision();
    let mm = testkit::millimetres;
    {
        let document = harness.app.tabs.active_mut();
        document.editor.start_product_box_drawing(DrawTool::Rectangle);
        document.editor.drawing_point([mm(0), mm(0)]).expect("the anchor");
        document.editor.drawing_point([mm(600), mm(560)]).expect("the opposite corner");
        assert_eq!(document.session.project().revision(), before, "the base is a draft, not a node");
        document.editor.start_product_pull(Lang::En).expect("the finished rectangle is the base");
        document.editor.pull_box_height(mm(720)).expect("the pull");
        let ghost = document.editor.box_draft().and_then(ui::BoxDraft::sizes).expect("the ghost sizes");
        assert_eq!(ghost, [mm(600), mm(560), mm(720)]);
        document.finish_box(Lang::En).expect("the gesture commits the drawn frame");
    }
    let document = harness.app.tabs.active();
    let frame = document.session.active_product().expect("the drawn frame is opened for its parts");
    let container = document.session.project().container(frame).expect("a container");
    assert!(container.is_product(), "a drawn frame is a product");
    let envelope = container.envelope().expect("an envelope");
    assert_eq!(envelope.components(), [mm(600), mm(560), mm(720)], "the frame keeps exactly the ghost's sides");
    assert!(envelope.footprint().is_rectangle(), "a drawn rectangle reserves the plain box the card writes");
    assert_eq!(container.class(), ClassCode::GENERAL_PRODUCT, "the drawn gesture takes the placeholder class");
    assert_eq!(harness.revision().get(), before.get() + 1, "the whole gesture is one command");
    assert!(document.session.can_undo(), "and one undo entry");
    assert_eq!(document.editor.selected_nodes(), [frame], "the new frame is selected, so its grips show");
    assert_eq!(document.editor.creation_target(document.session.project()), frame, "the next part lands inside");
}

#[test]
fn a_contour_frame_stores_its_ring_and_pulls_its_height() {
    // A frame drawn from a closed outline (ADR 0159): the sketch ring is the reserved space's base,
    // and the height is pulled from it exactly as the drawn box's is. Nothing is written until the
    // pull is accepted, so the ring and the height are one command and one undo entry.
    let mut harness = Harness::new("frame-contour-product");
    harness.settle();
    let before = harness.revision();
    let mm = testkit::millimetres;
    let ring = [[0, 0], [600, 0], [600, 400], [0, 400]];
    {
        let document = harness.app.tabs.active_mut();
        document.editor.start_product_drawing();
        for [x, y] in ring {
            document.editor.drawing_point([mm(x), mm(y)]).expect("a point of the outline");
        }
        assert_eq!(document.session.project().revision(), before, "the ring is a draft, not a node");
        document.commit_drawing(Lang::En).expect("the ring becomes the base and the pull starts");
        assert_eq!(
            document.session.project().revision(),
            before,
            "the height is still being pulled, so nothing is written"
        );
        assert!(document.editor.box_draft().is_some(), "the pull owns the pointer once the outline is finished");
        document.editor.pull_box_height(mm(500)).expect("the pull");
        document.finish_box(Lang::En).expect("the gesture commits the drawn frame");
    }
    let document = harness.app.tabs.active();
    let frame = document.session.active_product().expect("the drawn frame is opened for its parts");
    let envelope = document.session.project().container(frame).expect("a container").envelope().expect("an envelope");
    assert!(envelope.footprint().is_ring(), "the reserved space keeps the ring it was drawn from");
    assert_eq!(
        envelope.components(),
        [mm(600), mm(400), mm(500)],
        "the height is the pull's, the sides the ring's bounds"
    );
    assert_eq!(harness.revision().get(), before.get() + 1, "the whole gesture is one command");
    assert_eq!(document.editor.selected_nodes(), [frame], "the new frame is selected");
}

#[test]
fn the_frame_cell_and_the_box_cell_draw_one_base_with_one_set_of_plates() {
    // N19: the frame's reserved box is drawn by the tool a solid's profile is drawn with, so the two
    // cells arm the same sketch method and the plates that name the base's two sides are one set of
    // rows read from one draft (ADR 0101).
    use ui::toolbars::spec::{GroupSpec, ToolKind};

    /// The cell one id names, in the map's own order.
    fn cell(tab: &'static str, tool: &str) -> &'static ui::toolbars::spec::ToolSpec {
        ui::toolbars::spec::tab(tab)
            .expect("the tab is on the map")
            .live_groups()
            .flat_map(GroupSpec::live_tools)
            .find(|candidate| candidate.id == tool)
            .expect("the cell is on the map")
    }

    /// The sketch method the cell's `method` row draws its base with.
    fn drawn(cell: &'static ui::toolbars::spec::ToolSpec, method: &str) -> DrawTool {
        match cell.methods().find(|spec| spec.id == method).and_then(|spec| spec.run) {
            Some(ToolKind::ProductBox(tool) | ToolKind::Body(tool)) => tool,
            other => panic!("`{method}` does not draw a base: {other:?}"),
        }
    }

    let frame_tool = drawn(cell("furniture", "furniture.product.frame"), "furniture.product.frame.draw");
    let box_tool = drawn(cell("model", "model.primitives.cuboid"), "model.primitives.cuboid");
    assert_eq!(frame_tool, box_tool, "the frame and the box are drawn by one method");

    // The plates of the base: the same rows, in the same order, with the same letters.
    let mm = testkit::millimetres;
    let mut editor = ui::Editor::new();
    let plates = |editor: &ui::Editor| {
        ui::PointState::live(editor.drawing().expect("the base is being drawn"), DVec2::new(600.0, 400.0))
            .expect("the second corner takes its numbers")
            .rows
    };
    editor.start_product_box_drawing(frame_tool);
    editor.drawing_point([mm(0), mm(0)]).expect("the anchor");
    let frame_plates = plates(&editor);
    editor.start_body_drawing(box_tool);
    editor.drawing_point([mm(0), mm(0)]).expect("the anchor");
    let box_plates = plates(&editor);
    assert_eq!(frame_plates, box_plates, "the frame cell and the box cell draw the same base plates");
    assert_eq!(
        frame_plates.iter().map(|row| row.param).collect::<Vec<_>>(),
        [FieldParam::Width, FieldParam::Height],
        "and the two sides are the width and the depth the frame reserves"
    );
}

#[test]
fn the_frame_pull_draws_the_plate_the_solid_pull_draws() {
    // N19, U104: the frame's height and a solid's height are one number on one plate — `H`, no unit,
    // no word above it, no button pair, and the gesture's own Enter and Escape as its commit and its
    // cancel — so the two gestures that build one document read as one tool.
    let mm = testkit::millimetres;
    let without_the_number = |view: &ui::ToolView| {
        let mut view = view.clone();
        for row in &mut view.rows {
            row.value = 0.0;
        }
        view
    };

    let mut frame = Harness::with_session("frame-plate-frame", crate::scene_build::empty_session().expect("empty"));
    frame.settle();
    {
        let document = frame.app.tabs.active_mut();
        document.editor.start_product_box_drawing(DrawTool::Rectangle);
        document.editor.drawing_point([mm(0), mm(0)]).expect("the anchor");
        document.editor.drawing_point([mm(600), mm(400)]).expect("the base");
        document.editor.start_product_pull(Lang::En).expect("the base is fixed");
        document.editor.pull_box_height(mm(720)).expect("the pull");
    }
    let frame_view = frame.app.tabs.active().tool_view().expect("the frame's pull draws a plate");
    assert_eq!(frame_view.rows.first().map(|row| row.value), Some(720.0), "the plate shows the pull");

    let (session, body) = one_body();
    let mut solid = Harness::with_session("frame-plate-solid", session);
    solid.settle();
    {
        let document = solid.app.tabs.active_mut();
        document.editor.select_node(Some(body));
        document.extrude_gizmo.start(body, mm(40));
    }
    let solid_view = solid.app.tabs.active().tool_view().expect("the solid's pull draws a plate");

    assert_eq!(without_the_number(&frame_view), without_the_number(&solid_view), "one plate, one tool");
    assert_eq!(frame_view.title, None, "no word stands above the number");
    assert_eq!(frame_view.commit, ui::ToolCommit::Keys, "the gesture's own keys are the commit");
    let [row] = frame_view.rows.as_slice() else { panic!("a pull's plate holds one field") };
    assert_eq!(row.label, Some(ui::FieldLabel::Symbol("H")), "the letter names the height");
    assert_eq!(row.unit, "", "and no unit repeats what the letter says");
}

#[test]
fn the_frames_drawn_box_is_drawn_by_the_sketch_and_committed_by_one_click() {
    // The whole gesture through the real shell, as the hand makes it (N19): two clicks of the
    // sketch's rectangle fix the base, the pointer pulls the height, and the click that accepts it is
    // one `product.create` and one undo entry.
    let mut harness = Harness::with_session("frame-drawn-gesture", crate::scene_build::empty_session().expect("empty"));
    harness.settle();
    let before = harness.revision();
    let (anchor, corner) = {
        let document = harness.app.tabs.active();
        let rect = document.scene_rect().expect("the settled frame gives the scene a rectangle");
        let camera = document.camera();
        let project = |x: f64, y: f64, z: f64| {
            crate::gizmo_math::screen_point(camera, rect, geom::DVec3::new(x, y, z)).expect("the plane is on screen")
        };
        (project(-300.0, -200.0, 0.0), project(300.0, 200.0, 0.0))
    };
    harness.app.tabs.active_mut().editor.start_product_box_drawing(DrawTool::Rectangle);
    harness.hover(anchor);
    harness.click(anchor, egui::PointerButton::Primary);
    assert!(harness.app.tabs.active().editor.drawing().is_some(), "the first click anchors the base");
    harness.hover(corner);
    harness.click(corner, egui::PointerButton::Primary);
    {
        let document = harness.app.tabs.active();
        assert!(document.editor.drawing().is_none(), "the second click finishes the figure");
        let draft = document.editor.box_draft().expect("the finished figure is the frame's base");
        let sizes = draft.sizes().expect("a rectangle base has three sides");
        assert_eq!((sizes[0].millimetres_f64(), sizes[1].millimetres_f64()), (600.0, 400.0), "the size the hand drew");
        assert_eq!(document.session.project().revision(), before, "the base is a draft and writes nothing");
    }

    // The pointer rises 200 mm along the plane's normal, which is where the ghost's vertical runs.
    let raised = {
        let document = harness.app.tabs.active();
        let rect = document.scene_rect().expect("the scene rectangle");
        crate::gizmo_math::screen_point(document.camera(), rect, geom::DVec3::new(-300.0, -200.0, 200.0))
            .expect("the pull is on screen")
    };
    harness.hover(raised);
    let pulled = {
        let draft = harness.app.tabs.active().editor.box_draft().cloned().expect("the pull is running");
        let height = draft.height().millimetres_f64();
        assert!((height - 200.0).abs() <= 2.0, "the pull reads the axis under the pointer, was {height}");
        draft.height()
    };
    harness.click(raised, egui::PointerButton::Primary);

    let document = harness.app.tabs.active();
    assert!(document.editor.box_draft().is_none(), "the click accepts the height");
    let frame = document.session.active_product().expect("the drawn frame is opened for its parts");
    let envelope = document.session.project().container(frame).and_then(|node| node.envelope()).expect("an envelope");
    assert_eq!(envelope.z(), pulled, "the frame keeps the height the pull showed");
    assert!(envelope.footprint().is_rectangle(), "and the plain base a rectangle reserves");
    assert_eq!(harness.revision().get(), before.get() + 1, "one click, one command");
    assert_eq!(document.editor.selected_nodes(), [frame], "the new frame is selected");

    // One undo takes the whole frame back: the base and the height were never two entries.
    let document = harness.app.tabs.active_mut();
    document.session.undo().expect("one undo");
    assert!(document.session.project().container(frame).is_none(), "the frame is gone as one entry");
    assert!(!document.session.can_undo(), "the gesture wrote nothing else to undo");
}

/// The signed volume of a closed polygon mesh, in cubic millimetres.
fn solid_volume(mesh: &model::Mesh) -> f64 {
    let corner = |index: u32| {
        let position = mesh.positions()[index as usize];
        (position[0].millimetres_f64(), position[1].millimetres_f64(), position[2].millimetres_f64())
    };
    let mut total = 0.0;
    for face in mesh.faces() {
        for index in 1..face.len().saturating_sub(1) {
            let a = corner(face[0]);
            let b = corner(face[index]);
            let c = corner(face[index + 1]);
            let cross = (b.1 * c.2 - b.2 * c.1, b.2 * c.0 - b.0 * c.2, b.0 * c.1 - b.1 * c.0);
            total += (a.0 * cross.0 + a.1 * cross.1 + a.2 * cross.2) / 6.0;
        }
    }
    total
}

/// Arms a body cell and draws its profile with `tool`, then returns the profile's node.
///
/// The cell and the sketch share one draft: `start_body_drawing` is what the box and cylinder cells
/// run, and the points are the ones a sketch stage places (ADR 0138).
fn draw_body_profile(harness: &mut Harness, tool: DrawTool) -> Id {
    let points = stage_samples(tool);
    let document = harness.app.tabs.active_mut();
    document.editor.start_body_drawing(tool);
    for point in points {
        document.editor.drawing_point([mesh(point[0]), mesh(point[1])]).expect("a valid point of the body's profile");
    }
    document.commit_drawing(Lang::En).expect("the profile commits");
    let object = document.extrude_gizmo.object().expect("the pull holds the profile");
    assert!(document.extrude_gizmo.is_growing(), "a drawn profile awaits its height");
    assert_eq!(
        document.session.project().object(object).map(|node| node.geometry().as_path().is_some()),
        Some(true),
        "the profile is a path for the length of the pull"
    );
    object
}

/// A scene with one swept body and one ball, placed apart: the shapes a snap must read.
fn body_and_ball_session() -> engine::Session {
    let root = model::Container::new(Id::new(1).expect("a root id"), "Root", model::Placement::IDENTITY, Vec::new());
    let mut project = model::Project::new(root).expect("an empty project");
    let root = project.root().id();
    let square = model::Path::rectangle([mesh(-100), mesh(-75)], [mesh(100), mesh(75)]).expect("a rectangle");
    let body = model::Constructive::extrude(square, mesh(300)).expect("a body");
    project.add_object(root, "Box", model::Placement::IDENTITY, model::Geometry::Constructive(body)).expect("a box");
    let ball = model::Primitive::sphere(mesh(100), 16, 8).expect("a ball");
    let placed = model::Placement::new(geom::DVec3::new(600.0, 0.0, 150.0), model::Placement::IDENTITY.rotation())
        .expect("a place");
    project.add_object(root, "Ball", placed, model::Geometry::Primitive(ball)).expect("a ball");
    engine::Session::with_directory(project, testkit::sample_catalog())
}

/// Arms the sketch and installs the workspace's snap mode, then lets the viewport settle.
fn sketch_in_mode(harness: &mut Harness, mode: model::SnapMode) {
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    for _ in 0..24 {
        harness.run(Vec::new());
    }
    {
        let document = harness.app.tabs.active_mut();
        let kinds = document.snaps();
        document.set_snaps(crate::document::SnapChoice { kinds, mode });
        document.editor.start_drawing(ui::DrawTool::Line);
    }
    for _ in 0..24 {
        harness.run(Vec::new());
    }
}

#[test]
fn a_body_and_a_ball_answer_a_snap_on_every_corner_edge_face_and_centre() {
    // The owner's report: a drawn cube snapped only to the profile it was drawn from, the marker
    // stood away from the place the click would take, and a ball snapped nowhere. Every corner of
    // the body, the middle of every edge and the middle of every face is a candidate; a ball answers
    // at its own centre and at its quadrants.
    //
    // The table is walked in both modes, because the mode is what decides what the answer *is*
    // (U119, U121). In 2.5D the sketch draws where the view says, so the click stores the candidate's
    // foot on that plane. In 3D the sketch takes the plane of the face under the pointer: a candidate
    // lying on that face — the middle of the face, the middle of one of its edges — is in the sketch
    // plane and captures where it is, while a place off the face the hand is on, the far corner
    // hidden behind the body or anything of a ball, is not a place the sketch can store and the grid
    // answers instead. In both modes the marker stands on the point the click stores.
    for mode in [model::SnapMode::Foot, model::SnapMode::Space] {
        let mut harness = Harness::with_session("snap-body", body_and_ball_session());
        sketch_in_mode(&mut harness, mode);

        let project = harness.app.tabs.active().session.project();
        let boxed = project
            .object(Id::new(2).expect("the box id"))
            .expect("the box")
            .geometry()
            .as_constructive()
            .expect("a body");
        let [x, y] = boxed.contour().expect("a ring").bounds_micrometres().0.map(|value| value as f64 / 1000.0);
        // Each place, and whether the sketch's plane must be the face the hand points at: the middle
        // of a visible face is where the plane is answered; the middle of an edge draws no plane of
        // its own — a ray exactly along it stops at nothing — the far vertical edge is behind the
        // body, and a ball's surface names the plane that touches it, which holds no place of the
        // ball.
        let spots = [
            ("top face", geom::DVec3::new(x + 100.0, y + 75.0, 300.0), true),
            ("side face", geom::DVec3::new(x + 200.0, y + 75.0, 150.0), true),
            ("top edge", geom::DVec3::new(x + 100.0, y, 300.0), false),
            ("vertical edge", geom::DVec3::new(x, y, 150.0), false),
            ("ball centre", geom::DVec3::new(600.0, 0.0, 150.0), false),
            ("ball pole", geom::DVec3::new(600.0, 0.0, 250.0), false),
        ];
        for (name, world, on_the_face) in spots {
            let at = gizmo_math::screen_point(harness.camera(), harness.scene(), world).expect("the spot is on screen");
            harness.run(vec![egui::Event::PointerMoved(at)]);
            let document = harness.app.tabs.active();
            let kind = document.draw_gizmo.snap_kind();
            let point = document.draw_gizmo.hover_point().expect("the pointer places a point");
            let marker = document.draw_gizmo.hover_screen().expect("a marker at the point it stores");
            let plane = document.draw_gizmo.plane(harness.camera());
            let stored = plane.world(geom::DVec2::new(point[0].millimetres_f64(), point[1].millimetres_f64()));
            match mode {
                model::SnapMode::Foot => {
                    assert_eq!(
                        plane,
                        gizmo_math::WorkPlane::from_camera(harness.camera()),
                        "2.5D draws where the view says"
                    );
                    assert!(kind.is_some(), "the {name} captures the pointer: {world:?} at {at:?}");
                    assert_ne!(kind, Some(SnapKind::Grid), "the {name} answers from the scene, not the plane's grid");
                    assert!(
                        (stored.x - world.x).abs() < 0.01 && (stored.y - world.y).abs() < 0.01,
                        "the {name} reads a candidate above the plane by its perpendicular foot: {stored:?} \
                         against {world:?}"
                    );
                }
                model::SnapMode::Space => {
                    if on_the_face {
                        assert_ne!(
                            plane,
                            gizmo_math::WorkPlane::from_camera(harness.camera()),
                            "the {name} gives the sketch the plane of the face under the pointer"
                        );
                    }
                    // A place of the plane the pointer named captures it and is stored where it is,
                    // because in 3D the sketch takes no candidate that stands off its plane.
                    let foot = plane.world(plane.local(world));
                    if (world - foot).length() <= 1.0e-3 {
                        assert!(
                            kind.is_some_and(|kind| kind != SnapKind::Grid),
                            "the {name} is a place of the sketch plane: {kind:?}"
                        );
                        assert!(
                            (stored - world).length() < 0.01,
                            "the {name} is stored where it is: {stored:?} against {world:?}"
                        );
                    }
                }
            }
            let drawn =
                gizmo_math::screen_point(harness.camera(), harness.scene(), stored).expect("the point is on screen");
            assert!(
                (marker - drawn).length() < 0.5,
                "the {name} draws its marker at the point the click takes, in {mode:?}: {marker:?} against {drawn:?}"
            );
        }
    }
}

#[test]
fn a_round_body_answers_its_rim_and_its_axis_and_the_air_beside_it_captures_nothing() {
    // U120: a swept circle used to answer the eight corners of the square around its base, which are
    // points in the air, while its upper rim and its axis were nowhere. The body now answers its own
    // rims and axis ends, and the pointer over the corner of that square takes nothing of it.
    let mut harness = Harness::with_session("snap-round", round_bodies_session());
    sketch_in_mode(&mut harness, model::SnapMode::Foot);
    let spots = [
        ("base rim", geom::DVec3::new(60.0, 0.0, 0.0), Some(SnapKind::Vertex)),
        ("upper rim", geom::DVec3::new(0.0, 60.0, 200.0), Some(SnapKind::Vertex)),
        ("axis end", geom::DVec3::new(0.0, 0.0, 200.0), Some(SnapKind::Centre)),
        ("cone apex", geom::DVec3::new(240.0, 0.0, 160.0), Some(SnapKind::Vertex)),
        ("box corner", geom::DVec3::new(60.0, 60.0, 200.0), None),
    ];
    for (name, world, expected) in spots {
        let at = gizmo_math::screen_point(harness.camera(), harness.scene(), world).expect("the spot is on screen");
        harness.run(vec![egui::Event::PointerMoved(at)]);
        let document = harness.app.tabs.active();
        let kind = document.draw_gizmo.snap_kind();
        let point = document.draw_gizmo.hover_point().expect("the pointer places a point");
        match expected {
            Some(expected) => {
                assert_eq!(kind, Some(expected), "the {name} captures the pointer: {world:?} at {at:?}");
                let stored = geom::DVec3::new(point[0].millimetres_f64(), point[1].millimetres_f64(), 0.0);
                assert!(
                    (stored.x - world.x).abs() < 0.01 && (stored.y - world.y).abs() < 0.01,
                    "the {name} reads its own place by its perpendicular foot: {stored:?} against {world:?}"
                );
            }
            None => assert!(
                kind.is_none_or(|kind| kind == SnapKind::Grid),
                "the {name} names no place of the body, so nothing of it may capture: {kind:?}"
            ),
        }
    }
}

/// A scene with one swept circle and one cone, placed apart: the round bodies a snap must read.
fn round_bodies_session() -> engine::Session {
    let root = model::Container::new(Id::new(1).expect("a root id"), "Root", model::Placement::IDENTITY, Vec::new());
    let mut project = model::Project::new(root).expect("an empty project");
    let root = project.root().id();
    let ring = model::Path::circle([mesh(0), mesh(0)], mesh(60)).expect("a circle");
    let cylinder = model::Constructive::extrude(ring, mesh(200)).expect("a cylinder");
    project
        .add_object(root, "Cylinder", model::Placement::IDENTITY, model::Geometry::Constructive(cylinder))
        .expect("a cylinder");
    let cone = model::Primitive::cone(mesh(60), mesh(0), mesh(160), 24).expect("a cone");
    let placed = model::Placement::new(geom::DVec3::new(240.0, 0.0, 0.0), model::Placement::IDENTITY.rotation())
        .expect("a place");
    project.add_object(root, "Cone", placed, model::Geometry::Primitive(cone)).expect("a cone");
    engine::Session::with_directory(project, testkit::sample_catalog())
}

#[test]
fn a_box_is_drawn_by_any_rectangle_method_and_swept_on_z() {
    // V4 (ADR 0138): the box cell carries the sketch's four rectangle methods, and the body it
    // builds is the solid the retired cuboid primitive drew — eight corners, six faces, and the
    // profile's own area swept along Z.
    for tool in [
        DrawTool::Rectangle,
        DrawTool::RectangleFromCenter,
        DrawTool::RectangleFromCenterDirected,
        DrawTool::RectangleTilted,
    ] {
        let mut harness =
            Harness::with_session("body-box-profile", crate::scene_build::empty_session().expect("an empty session"));
        harness.settle();
        let object = draw_body_profile(&mut harness, tool);
        let before = harness.revision();
        {
            let document = harness.app.tabs.active_mut();
            document.finish_extrude(object, mesh(300), None, Lang::En).expect("the check sweeps the body");
        }
        let document = harness.app.tabs.active();
        let project = document.session.project();
        let node = project.object(object).expect("the drawn box is in the document");
        let body = node.geometry().as_constructive().expect("the profile became a constructive body");
        assert_eq!(body.height(), Some(mesh(300)), "{tool:?}: the height is the pulled one");
        assert_eq!(body.path().shape(), Some(model::PathShape::Rectangle), "{tool:?}: a box keeps its rectangle");
        let ([min_x, min_y], [max_x, max_y]) = body.contour().expect("a ring").bounds_micrometres();
        let area = (max_x - min_x) as f64 * (max_y - min_y) as f64 / 1.0e6;
        assert!(area > 0.0, "{tool:?}: the profile encloses space");
        let solid = param::build_constructive_mesh(body).expect("the body builds");
        assert_eq!(
            (solid.positions().len(), solid.faces().len()),
            (8, 6),
            "{tool:?}: a box keeps eight corners and six faces, as the retired primitive did"
        );
        let volume = solid_volume(&solid);
        assert!(
            (volume - area * 300.0).abs() < area * 300.0 * 1.0e-6,
            "{tool:?}: the swept volume is the profile's area times the height, got {volume}"
        );
        assert_eq!(project.revision().get(), before.get() + 1, "{tool:?}: the check is one command");
        // The whole gesture — the profile and the body — is one undo entry.
        let document = harness.app.tabs.active_mut();
        document.session.undo().expect("undo the drawn body");
        assert!(
            document.session.project().object(object).is_none(),
            "{tool:?}: one undo takes the profile and the body it became"
        );
    }
}

#[test]
fn a_cylinder_is_drawn_by_any_circle_method_and_swept_on_z() {
    // The cylinder cell carries the sketch's circle methods, so the rim can be drawn by its centre,
    // by a diameter, through three points or tangent to two lines; the body is the one the retired
    // cylinder primitive drew, swept to the pulled height.
    for tool in [DrawTool::Circle, DrawTool::CircleDiameter, DrawTool::CircleThreePoints, DrawTool::CircleTangent] {
        let mut harness = Harness::with_session(
            "body-cylinder-profile",
            crate::scene_build::empty_session().expect("an empty session"),
        );
        harness.settle();
        let object = draw_body_profile(&mut harness, tool);
        {
            let document = harness.app.tabs.active_mut();
            document.finish_extrude(object, mesh(300), None, Lang::En).expect("the check sweeps the body");
        }
        let document = harness.app.tabs.active();
        let project = document.session.project();
        let node = project.object(object).expect("the drawn cylinder is in the document");
        let body = node.geometry().as_constructive().expect("the profile became a constructive body");
        assert_eq!(body.height(), Some(mesh(300)), "{tool:?}: the height is the pulled one");
        assert_eq!(body.path().shape(), Some(model::PathShape::Circle), "{tool:?}: a cylinder keeps its circle");
        let ([min_x, min_y], [max_x, max_y]) = body.contour().expect("a ring").bounds_micrometres();
        let radius = (max_x - min_x) as f64 / 2.0 / 1_000.0;
        assert!((radius - (max_y - min_y) as f64 / 2.0 / 1_000.0).abs() < 0.001, "{tool:?}: the ring is round");
        let solid = param::build_constructive_mesh(body).expect("the body builds");
        let expected = std::f64::consts::PI * radius * radius * 300.0;
        let volume = solid_volume(&solid);
        assert!(
            (volume - expected).abs() < expected * 5.0e-3,
            "{tool:?}: the swept volume is the circle's area times the height, got {volume} of {expected}"
        );
        // One undo takes the rim and the body it became.
        let document = harness.app.tabs.active_mut();
        document.session.undo().expect("undo the drawn cylinder");
        assert!(document.session.project().object(object).is_none(), "{tool:?}: one undo takes the gesture");
    }
}

/// Starts a sphere cell's round and places the points of its stage, then commits it.
///
/// The cell and the sketch share one draft: `start_primitive_drawing` is what the sphere cell runs,
/// and the points are the ones the stage declares (ADR 0139).
fn draw_sphere(harness: &mut Harness, tool: DrawTool) -> Id {
    let points = stage_samples(tool);
    let document = harness.app.tabs.active_mut();
    document.editor.start_primitive_drawing(tool).expect("the sphere cell draws a round");
    assert_eq!(document.editor.drawing_intent(), DrawIntent::Primitive(model::PrimitiveKind::Sphere));
    for point in points {
        document.editor.drawing_point([mesh(point[0]), mesh(point[1])]).expect("a valid point of the round");
    }
    assert!(document.editor.drawing().is_some_and(ui::DrawDraft::is_finished), "{tool:?}: the round is complete");
    document.commit_drawing(Lang::En).expect("the sphere commits");
    document.editor.selected_node().expect("the new sphere is selected")
}

/// Starts a cone cell's base round, places the points of its stage, and commits it.
///
/// The commit writes the cone sharp at the ribbon's height and hands the pointer to the pull, which
/// is where its height and its top circle are chosen (ADR 0141).
fn draw_cone(harness: &mut Harness, tool: DrawTool) -> Id {
    let points = stage_samples(tool);
    let document = harness.app.tabs.active_mut();
    document.editor.start_primitive_drawing(tool).expect("the cone cell draws a base round");
    assert_eq!(document.editor.drawing_intent(), DrawIntent::Primitive(model::PrimitiveKind::Cone));
    for point in points {
        document.editor.drawing_point([mesh(point[0]), mesh(point[1])]).expect("a valid point of the round");
    }
    assert!(document.editor.drawing().is_some_and(ui::DrawDraft::is_finished), "{tool:?}: the round is complete");
    document.commit_drawing(Lang::En).expect("the cone's base commits");
    document.editor.selected_node().expect("the new cone is selected")
}

#[test]
fn a_cone_is_drawn_by_its_base_circle_and_finished_by_its_pull() {
    // The cone cell carries the sketch's circle methods for the round it stands on; the commit stores
    // the shape sharp at the ribbon's height inside the pull's edit scope, and the ✓ writes the height
    // and the taper as one entry (ADR 0141).
    for (tool, base, centre) in [
        (DrawTool::Cone, 250.0, [0.0, 0.0]),
        (DrawTool::ConeDiameter, 250.0, [0.0, 0.0]),
        (DrawTool::ConeThreePoints, 125.0, [100.0, 75.0]),
    ] {
        let mut harness =
            Harness::with_session("cone-drawn", crate::scene_build::empty_session().expect("an empty session"));
        harness.settle();
        let before = harness.revision();
        let object = draw_cone(&mut harness, tool);
        {
            let document = harness.app.tabs.active();
            let project = document.session.project();
            assert_eq!(project.revision().get(), before.get() + 1, "{tool:?}: the base is one command");
            let node = project.object(object).expect("{tool:?}: the cone is in the document");
            assert_eq!(node.name(), "Cone", "{tool:?}: the node is what the cell places");
            let stored = node.geometry().as_primitive().expect("{tool:?}: the parameters are stored");
            assert_eq!(stored.kind(), model::PrimitiveKind::Cone, "{tool:?}: a cone");
            let model::PrimitiveShape::Cone(cone) = stored.base() else {
                panic!("{tool:?}: the shape is a cone");
            };
            let measured = cone.base_radius().millimetres_f64();
            assert!((measured - base).abs() < 0.001, "{tool:?}: the base radius is {base}, got {measured}");
            assert!(cone.is_sharp(), "{tool:?}: a fresh cone ends in the apex the card may taper");
            assert_eq!(cone.height(), document.editor.extrude_height(), "{tool:?}: the ribbon's height");
            let world = project.world_placement(object).expect("{tool:?}: a world placement");
            assert!(
                (world.translation() - geom::DVec3::new(centre[0], centre[1], 0.0)).length() < 1e-9,
                "{tool:?}: the base circle's centre is the origin, was {:?}",
                world.translation()
            );
        }
        // The pull is already open on the node it created, and its two numbers are the card's.
        let document = harness.app.tabs.active_mut();
        assert!(document.extrude_gizmo.is_active(), "{tool:?}: the pull is open");
        assert_eq!(document.extrude_gizmo.object(), Some(object));
        assert_eq!(document.extrude_gizmo.top(), Some(Length::ZERO), "{tool:?}: the taper starts at the apex");
        let view = document.tool_view().expect("{tool:?}: the pull draws a card");
        assert_eq!(view.rows.len(), 2, "{tool:?}: the height and the top radius");
        // Dragging previews the solid without writing it.
        let revision = document.session.project().revision();
        document.preview_extrude(object, mesh(300), Some(mesh(40))).expect("{tool:?}: the preview builds");
        assert_eq!(document.session.project().revision(), revision, "{tool:?}: a preview writes nothing");
        let preview = document.session.preview().geometry(object).expect("{tool:?}: a previewed solid");
        let model::Geometry::Primitive(body) = preview else {
            panic!("a cone body");
        };
        let model::PrimitiveShape::Cone(previewed) = body.base() else {
            panic!("{tool:?}: the preview is a cone");
        };
        assert_eq!(previewed.height(), mesh(300));
        assert_eq!(previewed.top_radius(), mesh(40));
        // The ✓ writes both numbers into the scope the base opened: one undo takes the whole shape.
        document.finish_extrude(object, mesh(300), Some(mesh(40)), Lang::En).expect("{tool:?}: the check stores");
        let project = document.session.project();
        let stored = project.object_primitive(object).expect("{tool:?}: still a cone");
        let model::PrimitiveShape::Cone(cone) = stored.base() else {
            panic!("{tool:?}: the shape is a cone");
        };
        assert_eq!(cone.height(), mesh(300), "{tool:?}: the pulled height");
        assert_eq!(cone.top_radius(), mesh(40), "{tool:?}: the typed top radius");
        assert!(!document.extrude_gizmo.is_active(), "{tool:?}: the pull closes on the check");
        document.session.undo().expect("undo the drawn cone");
        assert!(
            document.session.project().object(object).is_none(),
            "{tool:?}: one undo takes the base circle and the taper"
        );
    }
}

#[test]
fn a_cancelled_cone_pull_leaves_no_cone() {
    // The base circle wrote the node inside the pull's scope, so Escape rolls that scope back: no
    // node, no undo entry (ADR 0141).
    let mut harness =
        Harness::with_session("cone-cancelled", crate::scene_build::empty_session().expect("an empty session"));
    harness.settle();
    let object = draw_cone(&mut harness, DrawTool::Cone);
    let document = harness.app.tabs.active_mut();
    document.preview_extrude(object, mesh(500), Some(mesh(20))).expect("a preview");
    document.cancel_extrude();
    assert!(document.session.project().object(object).is_none(), "the cancelled cone is gone");
    assert!(!document.extrude_gizmo.is_active(), "and the pull is closed");
    assert_eq!(document.editor.selected_node(), None, "nothing is left to name");
    assert!(!document.session.can_undo(), "the base circle it wrote is not an entry to undo");
}

#[test]
fn a_sphere_is_drawn_by_any_of_its_three_rounds() {
    // The sphere cell carries three methods of one circle: the centre and a rim point, the same pair
    // typed as a diameter, and three points whose circumcircle it wraps. Each one stores a sphere —
    // the centre is its placement and the round its radius — in one command (ADR 0139).
    for (tool, radius, centre) in [
        (DrawTool::Sphere, 250.0, [0.0, 0.0]),
        (DrawTool::SphereDiameter, 250.0, [0.0, 0.0]),
        (DrawTool::SphereThreePoints, 125.0, [100.0, 75.0]),
    ] {
        let mut harness =
            Harness::with_session("sphere-drawn", crate::scene_build::empty_session().expect("an empty session"));
        harness.settle();
        let before = harness.revision();
        let object = draw_sphere(&mut harness, tool);
        let document = harness.app.tabs.active();
        let project = document.session.project();
        assert_eq!(project.revision().get(), before.get() + 1, "{tool:?}: one gesture is one command");
        let node = project.object(object).expect("{tool:?}: the sphere is in the document");
        assert_eq!(node.name(), "Sphere", "{tool:?}: the node is what the cell places");
        let stored = node.geometry().as_primitive().expect("{tool:?}: the parameters are stored");
        assert_eq!(stored.kind(), model::PrimitiveKind::Sphere, "{tool:?}: a sphere");
        let ([min_x, min_y, min_z], [max_x, max_y, max_z]) = stored.bounds_micrometres();
        let measured = (max_x - min_x) as f64 / 2_000.0;
        assert!((measured - radius).abs() < 0.001, "{tool:?}: the radius is {radius}, got {measured}");
        assert_eq!((min_y + max_y) as f64 / 2_000.0, 0.0, "{tool:?}: the ball is centred in its local frame");
        assert_eq!((min_z + max_z) as f64 / 2_000.0, 0.0, "{tool:?}: the ball is centred in its local frame");
        let world = project.world_placement(object).expect("{tool:?}: a world placement");
        assert!(
            (world.translation() - geom::DVec3::new(centre[0], centre[1], 0.0)).length() < 1e-9,
            "{tool:?}: the round's centre is the origin, was {:?}",
            world.translation()
        );
        // One undo takes the whole drawn sphere, because the round wrote nothing of its own.
        let document = harness.app.tabs.active_mut();
        document.session.undo().expect("undo the drawn sphere");
        assert!(document.session.project().object(object).is_none(), "{tool:?}: one undo takes the sphere");
    }
}

#[test]
fn escape_during_the_height_pull_leaves_the_drawn_profile_behind() {
    // The profile is in the document for the length of the pull so the shaded preview can show the
    // body; the pull's edit scope is what makes Escape leave nothing at all (ADR 0138).
    let mut harness =
        Harness::with_session("body-pull-cancel", crate::scene_build::empty_session().expect("an empty session"));
    harness.settle();
    let object = draw_body_profile(&mut harness, DrawTool::Rectangle);
    assert!(harness.app.tabs.active().session.project().object(object).is_some(), "the profile is drawn");
    harness.app.tabs.active_mut().cancel_extrude();
    let document = harness.app.tabs.active();
    assert!(document.session.project().object(object).is_none(), "Escape takes the profile back");
    assert!(!document.extrude_gizmo.is_active(), "and drops the pull");
    assert!(!document.session.can_undo(), "a cancelled create leaves no history entry");
    assert!(!document.session.can_redo(), "and no redo entry for a gesture nobody finished");
}

#[test]
fn a_drag_on_a_frame_corner_resizes_the_envelope_in_one_undo_step() {
    // The grips read a frame exactly as they read a solid (ADR 0101): a press mints the edit scope,
    // every move writes `product.envelope.set`, and the release leaves one undo entry.
    let mut harness = Harness::with_session("frame-grip-drag", crate::scene_build::empty_session().expect("empty"));
    harness.settle();
    harness.app.product_card.start();
    harness.settle();
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    // The new frame is the only object, so the fit frames its reserved box and every grip is on the
    // scene rectangle.
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    let (frame, grip, reached, before) = {
        let document = harness.app.tabs.active_mut();
        let frame = document.session.active_product().expect("the card opened a frame");
        let project = document.session.project();
        let envelope = project.container(frame).and_then(|frame| frame.envelope()).expect("an envelope");
        let [width, depth, _height] = envelope.components().map(|side| side.millimetres_f64());
        let world = project.world_placement(frame).expect("the frame's world placement");
        let rect = document.scene_rect().expect("a drawn frame gives the scene a rectangle");
        let camera = document.camera();
        let at = |x: f64, y: f64| {
            gizmo_math::screen_point(camera, rect, world.transform_point(geom::DVec3::new(x, y, 0.0)))
                .expect("the frame's base is on screen")
        };
        (frame, at(width, depth), at(width - 100.0, depth - 100.0), harness.revision())
    };
    assert!(harness.app.tabs.active().editor.selected_node() == Some(frame), "the new frame is selected");
    let rect = harness.app.tabs.active().scene_rect().expect("a scene rectangle");
    assert!(rect.contains(grip) && rect.contains(reached), "the drag stays on the scene: {grip:?} {reached:?}");

    harness.press_at(grip, egui::PointerButton::Primary);
    harness.run(vec![egui::Event::PointerMoved(reached)]);
    harness.release_at(reached, egui::PointerButton::Primary);
    harness.settle();

    let envelope_sides = |harness: &Harness| {
        harness
            .app
            .tabs
            .active()
            .session
            .project()
            .container(frame)
            .and_then(|frame| frame.envelope())
            .expect("the envelope is still there")
            .components()
            .map(|side| side.millimetres_f64())
    };
    let sides = envelope_sides(&harness);
    assert!((sides[0] - 500.0).abs() <= 5.0, "the width followed the corner: {sides:?}");
    assert!((sides[1] - 460.0).abs() <= 5.0, "the depth followed the corner: {sides:?}");
    assert_eq!(harness.revision().get(), before.get() + 1, "the whole drag is one command");
    harness.app.tabs.active_mut().session.undo().expect("one undo takes the drag back");
    let sides = envelope_sides(&harness);
    assert!((sides[0] - 600.0).abs() < 1.0e-9 && (sides[1] - 560.0).abs() < 1.0e-9, "the frame is back: {sides:?}");
}

#[test]
fn the_empty_space_tool_creates_the_ghost_as_one_classed_part_in_one_undo_step() {
    // An empty scene: the sample session's loose parts stand inside a frame created at the
    // origin, and a void holding a part is refused by the niche (ADR 0106).
    let mut harness = Harness::with_session("frame-fill", crate::scene_build::empty_session().expect("empty"));
    harness.settle();
    harness.app.product_card.start();
    harness.settle();
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);

    let (product, before, pointer) = {
        let document = harness.app.tabs.active_mut();
        let product = document.session.active_product().expect("the card opens the frame");
        let project = document.session.project();
        let envelope = project.container(product).and_then(|product| product.envelope()).expect("an envelope");
        let size = geom::DVec3::new(
            envelope.x().millimetres_f64(),
            envelope.y().millimetres_f64(),
            envelope.z().millimetres_f64(),
        );
        let placement = project.world_placement(product).expect("a frame");
        let centre = placement.transform_point(size * 0.5);
        let rect = document.scene_rect().expect("a drawn frame gives the scene a rectangle");
        let camera = document.camera();
        let pointer = gizmo_math::screen_point(camera, rect, centre).expect("the frame is on screen");
        assert!(rect.contains(pointer), "the press lands on the scene: {pointer:?} in {rect:?}");
        document
            .editor
            .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, model::FillMode::Fit, Lang::En)
            .expect("the shelf fill arms inside the open frame");
        (product, harness.revision(), pointer)
    };

    harness.hover(pointer);
    harness.press_at(pointer, egui::PointerButton::Primary);
    let planned = harness.app.tabs.active().fill_gizmo.ghost().cloned().expect("the pointer measures a ghost");
    harness.release_at(pointer, egui::PointerButton::Primary);
    harness.settle();

    let created = {
        let document = harness.app.tabs.active();
        // The click wrote the ghost that was on screen; the tool stays armed and measures the next
        // part from the face under the pointer, which moved onto the part just created.
        let created = document.editor.selected_node().expect("the new part is selected");
        let panel = document.session.project().panel(created).expect("the click created a panel");
        assert_eq!(panel.nominal_size(), planned.nominal_size, "the part is exactly the ghost's size");
        assert_eq!(panel.placement(), planned.placement, "and exactly the ghost's placement");
        assert_eq!(document.session.project().parent_of(created), Some(product), "it lands in the open frame");
        assert_eq!(
            document.session.project().class(created),
            Some(ClassCode::HORIZONTAL_PANEL),
            "the tool's class travels with the same command"
        );
        created
    };
    assert!(harness.revision() > before, "the click writes the document once");

    // One undo takes the whole fill back, class and all: the click was one entry, not two.
    let document = harness.app.tabs.active_mut();
    assert!(document.session.can_undo());
    document.session.undo().expect("one undo step");
    assert!(document.session.project().panel(created).is_none(), "one undo removes the part");
}

#[test]
fn the_wall_cell_creates_the_frontal_board_in_one_undo_step() {
    // N28: the wall is the first fill whose free axis is the depth, so it is the first board the
    // carriage carries towards and away from the maker. The click is still one `panel.create` and
    // one undo entry, and the part stands in the frontal plane (ADR 0105, ADR 0200).
    let mut harness = Harness::with_session("frame-fill-wall", crate::scene_build::empty_session().expect("empty"));
    harness.settle();
    harness.app.product_card.start();
    harness.settle();
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);

    let before = harness.revision();
    let (product, pointer, envelope_size) = {
        let document = harness.app.tabs.active_mut();
        let product = document.session.active_product().expect("the card opens the frame");
        let project = document.session.project();
        let envelope = project.container(product).and_then(|product| product.envelope()).expect("an envelope");
        let size = geom::DVec3::new(
            envelope.x().millimetres_f64(),
            envelope.y().millimetres_f64(),
            envelope.z().millimetres_f64(),
        );
        let placement = project.world_placement(product).expect("a frame");
        let centre = placement.transform_point(size * 0.5);
        let rect = document.scene_rect().expect("a drawn frame gives the scene a rectangle");
        let pointer = gizmo_math::screen_point(document.camera(), rect, centre).expect("the frame is on screen");
        assert!(rect.contains(pointer), "the press lands on the scene: {pointer:?} in {rect:?}");
        document
            .editor
            .arm_fill(&document.session, ClassCode::BACK_PANEL, model::FillMode::Fit, Lang::En)
            .expect("the wall arms inside the open frame");
        (product, pointer, (envelope.x(), envelope.y(), envelope.z()))
    };

    harness.hover(pointer);
    harness.press_at(pointer, egui::PointerButton::Primary);
    let planned = harness.app.tabs.active().fill_gizmo.ghost().cloned().expect("the pointer measures a ghost");
    harness.release_at(pointer, egui::PointerButton::Primary);
    harness.settle();

    let created = {
        let document = harness.app.tabs.active();
        let created = document.editor.selected_node().expect("the new part is selected");
        let panel = document.session.project().panel(created).expect("the click created a panel");
        assert_eq!(panel.nominal_size(), planned.nominal_size, "the part is exactly the ghost's size");
        assert_eq!(panel.placement(), planned.placement, "and exactly the ghost's placement");
        // The wall is the niche's height by its width, and its material runs along the depth.
        assert_eq!(panel.nominal_size().length(), envelope_size.2, "the length is the niche's height");
        assert_eq!(panel.nominal_size().width(), envelope_size.0, "the width is the niche's width");
        let rotation = panel.placement().rotation();
        let close = |first: geom::DVec3, second: geom::DVec3| (first - second).length() <= 1.0e-9;
        assert!(close(rotation.mul_vec3(geom::DVec3::X), geom::DVec3::Z), "the length runs up the case");
        assert!(close(rotation.mul_vec3(geom::DVec3::Y), geom::DVec3::X), "the width runs across the front");
        assert!(close(rotation.mul_vec3(geom::DVec3::Z), geom::DVec3::Y), "and the material faces the maker");
        assert_eq!(document.session.project().parent_of(created), Some(product), "it lands in the open frame");
        assert_eq!(
            document.session.project().class(created),
            Some(ClassCode::BACK_PANEL),
            "the wall cell's class travels with the same command"
        );
        created
    };
    assert!(harness.revision() > before, "the click writes the document once");

    let document = harness.app.tabs.active_mut();
    assert!(document.session.can_undo());
    document.session.undo().expect("one undo step");
    assert!(document.session.project().panel(created).is_none(), "one undo removes the board");
}

/// A harness whose open product holds two posts standing at an angle, plus the pointer at the
/// void's centre and the product's id.
///
/// The posts' inner faces run from `(100, 0)` to `(200, 560)` and from `(500, 0)` to `(400, 560)`,
/// so the free section is a trapezoid: 400 mm wide at the floor and 200 mm wide at the back.
fn splayed_shelf_harness(name: &str) -> (Harness, model::Id, egui::Pos2) {
    // An empty scene: the sample session's loose parts stand inside a frame created at the
    // origin, and a void holding a part is refused by the niche (ADR 0106).
    let mut harness = Harness::with_session(name, crate::scene_build::empty_session().expect("empty"));
    harness.settle();
    harness.app.product_card.start();
    harness.settle();
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    let document = harness.app.tabs.active_mut();
    let product = document.session.active_product().expect("the card opens the frame");
    let slant = 100.0 / 560.0_f64.hypot(100.0);
    let drop = 560.0 / 560.0_f64.hypot(100.0);
    for (through, normal) in [((100.0, 0.0), (drop, -slant)), ((500.0, 0.0), (-drop, -slant))] {
        let direction = (-normal.1, normal.0);
        let start_y = 280.0 - 600.0 * direction.1;
        let start_x = through.0 + (start_y - through.1) * (direction.0 / direction.1);
        let rotation = geom::DQuat::from_rotation_z(normal.1.atan2(normal.0))
            * geom::DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2);
        let translation = geom::DVec3::new(start_x, start_y, 720.0) - geom::DVec3::new(normal.0, normal.1, 0.0) * 18.0;
        let placement = model::Placement::new(translation, rotation).expect("a placement");
        let nominal =
            model::PanelNominalSize::new(testkit::millimetres(720), testkit::millimetres(1200)).expect("a post size");
        document
            .editor
            .create_panel_sized(
                &mut document.session,
                product,
                "Post".to_owned(),
                nominal,
                None,
                Vec::new(),
                placement,
                None,
            )
            .expect("add a post");
    }
    let project = document.session.project();
    let envelope = project.container(product).and_then(|product| product.envelope()).expect("an envelope");
    let size = geom::DVec3::new(
        envelope.x().millimetres_f64(),
        envelope.y().millimetres_f64(),
        envelope.z().millimetres_f64(),
    );
    let centre = project.world_placement(product).expect("a frame").transform_point(size * 0.5);
    let rect = document.scene_rect().expect("a drawn frame gives the scene a rectangle");
    let pointer = gizmo_math::screen_point(document.camera(), rect, centre).expect("the frame is on screen");
    assert!(rect.contains(pointer), "the press lands on the scene: {pointer:?} in {rect:?}");
    (harness, product, pointer)
}

/// A harness whose open product holds a back wall leaned 10 degrees, plus the pointer in the void
/// and the product's id.
///
/// The wall's inner face passes through `y = 560` at the floor and comes forward as it rises, so a
/// shelf cut to the wide face meets it with a bevel rather than a square end (N12).
fn leaned_shelf_harness(name: &str) -> (Harness, model::Id, egui::Pos2) {
    let mut harness = Harness::with_session(name, crate::scene_build::empty_session().expect("empty"));
    harness.settle();
    harness.app.product_card.start();
    harness.settle();
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    let document = harness.app.tabs.active_mut();
    let product = document.session.active_product().expect("the card opens the frame");
    let wall = geom::DQuat::from_rotation_z(-std::f64::consts::FRAC_PI_2)
        * geom::DQuat::from_rotation_y(-std::f64::consts::FRAC_PI_2);
    let rotation = geom::DQuat::from_rotation_x(10.0_f64.to_radians()) * wall;
    let placement = model::Placement::new(geom::DVec3::new(0.0, 560.0, 0.0), rotation).expect("a placement");
    let nominal =
        model::PanelNominalSize::new(testkit::millimetres(1600), testkit::millimetres(600)).expect("a wall size");
    document
        .editor
        .create_panel_sized(
            &mut document.session,
            product,
            "Wall".to_owned(),
            nominal,
            None,
            Vec::new(),
            placement,
            None,
        )
        .expect("add the leaned wall");
    let project = document.session.project();
    let envelope = project.container(product).and_then(|product| product.envelope()).expect("an envelope");
    let size = geom::DVec3::new(
        envelope.x().millimetres_f64(),
        envelope.y().millimetres_f64(),
        envelope.z().millimetres_f64(),
    );
    // A point inside the void: the wall's face is at 560 - z tan 10, so 300 mm back is free.
    let inside = geom::DVec3::new(size.x * 0.5, size.y * 0.5, size.z * 0.5);
    let rect = document.scene_rect().expect("a drawn frame gives the scene a rectangle");
    let pointer = gizmo_math::screen_point(document.camera(), rect, inside).expect("the frame is on screen");
    assert!(rect.contains(pointer), "the press lands on the scene: {pointer:?} in {rect:?}");
    (harness, product, pointer)
}

#[test]
fn a_beveled_fill_is_one_click_and_one_undo() {
    // The wall leans, so the shaped shelf meets it with an end bevel. The bevels travel in the same
    // `panel.create` as the outline, so the click is one part and one undo entry, not a create
    // followed by one edit per edge (ADR 0107, ADR 0108).
    let (mut harness, product, pointer) = leaned_shelf_harness("frame-fill-beveled");
    let document = harness.app.tabs.active_mut();
    document
        .editor
        .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, model::FillMode::Shape, Lang::En)
        .expect("the shelf fill arms inside the open frame");
    let before = harness.revision();
    harness.hover(pointer);
    harness.press_at(pointer, egui::PointerButton::Primary);
    let planned = harness.app.tabs.active().fill_gizmo.ghost().cloned().expect("the leaned void took a ghost");
    assert_eq!(planned.bevels.len(), 1, "the wall asks for one bevel: {:?}", planned.bevels);
    harness.release_at(pointer, egui::PointerButton::Primary);
    harness.settle();

    let document = harness.app.tabs.active();
    let created = document.editor.selected_node().expect("the new part is selected");
    let panel = document.session.project().panel(created).expect("the click created a panel");
    assert!(panel.is_contour(), "a beveled part is a contour panel");
    assert_eq!(panel.bevels(), planned.bevels.as_slice(), "the stored bevels are the ghost's");
    assert_eq!(panel.placement().rotation(), planned.placement.rotation(), "and its orientation");
    let (stored, planned_at) = (panel.placement().translation(), planned.placement.translation());
    assert!((stored - planned_at).length() < 1.0, "the free axis follows the pointer: {stored:?} vs {planned_at:?}");
    assert_eq!(document.session.project().parent_of(created), Some(product), "it lands in the open frame");
    assert!(harness.revision() > before, "the click writes the document once");

    let document = harness.app.tabs.active_mut();
    document.session.undo().expect("one undo step");
    assert!(document.session.project().panel(created).is_none(), "one undo removes the beveled part");
}

#[test]
fn the_shape_mode_creates_the_whole_section_as_the_ghost_draws_it() {
    // Two posts stand at an angle, so the free section is a trapezoid: in the shape mode the ghost
    // draws the polygon and the click stores exactly that ring, not the rectangle around it
    // (ADR 0099).
    let (mut harness, product, pointer) = splayed_shelf_harness("frame-fill-shaped");
    let document = harness.app.tabs.active_mut();
    document
        .editor
        .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, model::FillMode::Shape, Lang::En)
        .expect("the shelf fill arms inside the open frame");

    harness.hover(pointer);
    harness.press_at(pointer, egui::PointerButton::Primary);
    let planned = harness.app.tabs.active().fill_gizmo.ghost().cloned().expect("the splayed void took a ghost");
    harness.release_at(pointer, egui::PointerButton::Primary);
    harness.settle();

    let document = harness.app.tabs.active();
    let outline = planned.outline.contour().cloned().expect("the ghost carries the measured polygon");
    let created = document.editor.selected_node().expect("the new part is selected");
    let panel = document.session.project().panel(created).expect("the click created a panel");
    assert!(panel.is_contour(), "the shelf takes the polygon, not the rectangle around it");
    assert_eq!(Some(&outline), panel.outline_contour(), "the stored ring is the ghost's ring");
    assert_eq!(panel.placement(), planned.placement, "and the ghost's placement");
    assert_eq!(panel.nominal_size(), planned.nominal_size, "and the ghost's size");
    assert_eq!(document.session.project().parent_of(created), Some(product), "it lands in the open frame");

    let document = harness.app.tabs.active_mut();
    document.session.undo().expect("one undo step");
    assert!(document.session.project().panel(created).is_none(), "one undo removes the shaped part");
}

#[test]
fn the_fit_mode_keeps_a_standard_panel_in_a_splayed_niche() {
    // The same trapezoid, read the other way: the part is the largest rectangle inside it, so it
    // keeps the rectangle panel, its card and its resize (ADR 0042, N10).
    let (mut harness, product, pointer) = splayed_shelf_harness("frame-fill-fitted");
    let document = harness.app.tabs.active_mut();
    document
        .editor
        .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, model::FillMode::Fit, Lang::En)
        .expect("the shelf fill arms inside the open frame");
    let before = harness.revision();
    harness.hover(pointer);
    harness.press_at(pointer, egui::PointerButton::Primary);
    let planned = harness.app.tabs.active().fill_gizmo.ghost().cloned().expect("the splayed void took a ghost");
    assert!(planned.outline.contour().is_none(), "a fit is a standard rectangle");
    assert_eq!(planned.nominal_size.length(), testkit::millimetres(200), "the narrowest width fits");
    assert_eq!(planned.nominal_size.width(), testkit::millimetres(560), "and the whole depth");
    harness.release_at(pointer, egui::PointerButton::Primary);
    harness.settle();

    let document = harness.app.tabs.active();
    let created = document.editor.selected_node().expect("the new part is selected");
    let panel = document.session.project().panel(created).expect("the click created a panel");
    assert!(!panel.is_contour(), "the part is the standard panel the fit chose");
    assert_eq!(panel.nominal_size(), planned.nominal_size, "and the ghost's size");
    assert_eq!(panel.placement(), planned.placement, "and the ghost's placement");
    assert_eq!(document.session.project().parent_of(created), Some(product), "it lands in the open frame");
    assert!(harness.revision() > before, "the click writes the document once");

    let document = harness.app.tabs.active_mut();
    document.session.undo().expect("one undo step");
    assert!(document.session.project().panel(created).is_none(), "one undo removes the fitted part");
}

#[test]
fn a_typed_distance_places_the_fill_exactly_and_the_click_creates_it() {
    // An empty scene: the sample session's loose parts stand inside a frame created at the
    // origin, and a void holding a part is refused by the niche (ADR 0106).
    let mut harness = Harness::with_session("frame-fill-typed", crate::scene_build::empty_session().expect("empty"));
    harness.settle();
    harness.app.product_card.start();
    harness.settle();
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);

    let (product, pointer, away) = {
        let document = harness.app.tabs.active_mut();
        let product = document.session.active_product().expect("the card opens the frame");
        let project = document.session.project();
        let envelope = project.container(product).and_then(|product| product.envelope()).expect("an envelope");
        let size = geom::DVec3::new(
            envelope.x().millimetres_f64(),
            envelope.y().millimetres_f64(),
            envelope.z().millimetres_f64(),
        );
        let placement = project.world_placement(product).expect("a frame");
        let centre = placement.transform_point(size * 0.5);
        // A point inside the void, off the centre line the two distance fields sit on, so the
        // creating click never lands on a plate.
        let away = placement.transform_point(geom::DVec3::new(size.x * 0.2, size.y * 0.2, size.z * 0.5));
        let rect = document.scene_rect().expect("a drawn frame gives the scene a rectangle");
        let camera = document.camera();
        let pointer = gizmo_math::screen_point(camera, rect, centre).expect("the frame is on screen");
        let away = gizmo_math::screen_point(camera, rect, away).expect("the left face is on screen");
        assert!(rect.contains(pointer) && rect.contains(away), "{away:?} in {rect:?}");
        document
            .editor
            .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, model::FillMode::Fit, Lang::En)
            .expect("the shelf fill arms inside the open frame");
        (product, pointer, away)
    };
    assert_eq!(product, harness.app.tabs.active().session.active_product().expect("the open frame"));

    // The pointer measures the void and the low row takes the keyboard without a click, so the
    // maker types into the number the gesture is driving.
    harness.run(vec![egui::Event::PointerMoved(pointer)]);
    harness.settle();
    let card = harness.app.tabs.active().tool_card_rect().expect("the fill's card drew beside the pointer");
    assert!(screen().contains_rect(card), "the card is on screen: {card:?}");
    harness.run(vec![egui::Event::Text("120".to_owned())]);
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);

    // Typing and Enter pinned the low face at 120 mm: no document write has happened yet.
    let (before, ghost) = {
        let document = harness.app.tabs.active();
        let ghost = document.fill_gizmo.ghost().cloned().expect("the ghost is still measured");
        assert!((ghost.placement.translation().z - 120.0).abs() < 1.0e-9, "{:?}", ghost.placement);
        (harness.revision(), ghost)
    };
    let document = harness.app.tabs.active();
    assert_eq!(document.session.project().revision(), before, "the overlay writes nothing");

    // Moving the pointer away does not follow it: the pinned face stays where the number put it.
    harness.run(vec![egui::Event::PointerMoved(away)]);
    let still = harness.app.tabs.active().fill_gizmo.ghost().cloned().expect("the ghost stays");
    assert!((still.placement.translation().z - 120.0).abs() < 1.0e-9, "{:?}", still.placement);
    assert_eq!(still.placement, ghost.placement, "a pinned distance holds the pointer out");

    harness.click(away, egui::PointerButton::Primary);
    let document = harness.app.tabs.active();
    let created = document.editor.selected_node().expect("the click created a part");
    let panel = document.session.project().panel(created).expect("it is a panel");
    assert!((panel.placement().translation().z - 120.0).abs() < 1.0e-9, "{:?}", panel.placement());
    assert_eq!(panel.placement(), ghost.placement, "the click writes exactly the pinned ghost");
    assert_eq!(document.session.project().parent_of(created), Some(product));
    assert!(harness.revision() > before, "the click is the one write of the gesture");

    let document = harness.app.tabs.active_mut();
    document.session.undo().expect("one undo step");
    assert!(document.session.project().panel(created).is_none(), "one undo removes the pinned part");
}

/// The on-screen display is one switch and one block: the corner names the camera and the scene's
/// cost only while the switch is on, and the block outlives the cube above it (U106).
#[test]
fn the_display_block_hangs_in_the_corner_while_its_switch_is_on() {
    let mut harness = Harness::new("osd_block");
    let chrome = harness.settle();
    let scene = harness.app.tabs.active().scene_rect().expect("the frame gave the views a rectangle");
    assert!(crate::gizmo::widget_rect(scene, &chrome).is_some(), "the corner has room for the block");
    assert!(
        !harness.drawn.iter().any(|text| text.contains(" objects · ")),
        "the block is off until the switch is turned on: {:?}",
        harness.drawn
    );

    harness.app.shell.cluster.show_osd = true;
    harness.settle();
    let text = harness.drawn.join("\n");
    assert!(text.contains("Pitch "), "the block names the camera the view draws with: {text}");
    assert!(text.contains(" objects · "), "and what the frame costs to draw: {text}");
    assert!(text.contains(" triangles"), "including the triangle count: {text}");

    // The cube and the block are two switches: a maker who wants the corner free for the model can
    // keep the numbers and drop the widget, which is the whole point of the second switch.
    harness.app.shell.cluster.show_widget = false;
    harness.settle();
    let text = harness.drawn.join("\n");
    assert!(text.contains("Pitch "), "the block stays when the cube goes: {text}");
}

#[test]
fn the_square_marks_the_work_area_only() {
    // The owner's rule: the custom square is the pointer of the scene rectangle, and the system
    // cursor is the affordance of every panel, widget, field and resizing border. A menu keeps the
    // arrow too, because the square would cover the row it is meant to pick (X41).
    let mut harness = Harness::new("frame-work-cursor");
    let chrome = harness.settle();
    let scene = harness.scene();
    let centre = scene.center();
    assert_eq!(
        harness.ctx.layer_id_at(centre).map(|layer| layer.order),
        Some(egui::Order::Background),
        "the scene is the background layer the square belongs to"
    );
    harness.hover(centre);
    assert_eq!(harness.cursor, egui::CursorIcon::None, "the scene hides the system arrow");
    let cursor = harness.app.shell.work_cursor.expect("the shell drew its own cursor");
    assert_eq!(cursor.square.center(), centre, "the square sits on the point the click lands on");
    assert_eq!(cursor.dot.center(), centre, "and so does its centre dot");
    assert!(cursor.crosshair.is_none(), "a fresh workspace draws the square alone");

    // A side panel is a system surface: the arrow comes back and no square is painted on it.
    let tree = chrome.panels.left.expect("the scene tree is up");
    let on_panel = egui::pos2(tree.center().x, tree.bottom() - 12.0);
    harness.hover(on_panel);
    assert_eq!(harness.cursor, egui::CursorIcon::Default, "a panel keeps the system arrow");
    assert!(harness.app.shell.work_cursor.is_none(), "and draws no square over its widgets");

    // The floating cluster is a toolbar, not the work area, even though it hangs over the scene.
    let on_cluster = egui::pos2(scene.center().x, scene.bottom() - ui::theme::FLOATING_OFFSET - 8.0);
    assert_eq!(
        harness.ctx.layer_id_at(on_cluster).map(|layer| layer.order),
        Some(egui::Order::Middle),
        "the point is on the floating cluster, not on the scene behind it"
    );
    harness.hover(on_cluster);
    assert_eq!(harness.cursor, egui::CursorIcon::Default, "the cluster keeps the system arrow");
    assert!(harness.app.shell.work_cursor.is_none(), "and draws no square over the toolbar");

    // The corner orientation widget is a widget drawn inside the work area: it keeps the arrow too.
    let widget = crate::gizmo::widget_rect(scene, &chrome).expect("the corner widget has a free corner");
    let on_widget = widget.center();
    harness.hover(on_widget);
    assert_eq!(harness.cursor, egui::CursorIcon::Default, "the corner widget keeps the system arrow");
    assert!(harness.app.shell.work_cursor.is_none(), "and draws no square over the widget");

    // An armed drawing tool keeps the square inside the work area; a contour is drawn at the point
    // the square names.
    harness.press(egui::Key::C, egui::Modifiers::NONE);
    harness.hover(centre);
    assert_eq!(harness.cursor, egui::CursorIcon::None, "a contour tool keeps the shell's square");
    assert!(harness.app.shell.work_cursor.is_some(), "and the square is painted");

    // A frame's own gesture does the same: its base is the sketch's figure, so the square marks the
    // work area for it and for the pull that follows.
    {
        let document = harness.app.tabs.active_mut();
        document.editor.start_product_box_drawing(DrawTool::Rectangle);
        document.editor.drawing_point([testkit::millimetres(0), testkit::millimetres(0)]).expect("the anchor");
        document.editor.drawing_point([testkit::millimetres(600), testkit::millimetres(400)]).expect("the base");
        document.editor.start_product_pull(Lang::En).expect("the pull starts");
    }
    harness.hover(centre);
    assert_eq!(harness.cursor, egui::CursorIcon::None, "a frame tool keeps the shell's square");
    assert!(harness.app.shell.work_cursor.is_some(), "and the square is painted");

    // A modal is a system surface: the arrow comes back and no square covers its fields.
    harness.app.product_card.start();
    harness.frame();
    harness.hover(screen().center());
    assert_eq!(harness.cursor, egui::CursorIcon::Default, "a modal keeps the system arrow");
    assert!(harness.app.shell.work_cursor.is_none(), "and draws no square over its fields");

    // Closing the card gives the square back to the scene.
    harness.app.product_card.close();
    harness.hover(centre);
    assert_eq!(harness.cursor, egui::CursorIcon::None, "the scene gets the square back");
    assert!(harness.app.shell.work_cursor.is_some(), "and the square is painted again");
}

#[test]
fn the_work_cursor_is_sized_by_the_settings_and_its_crosshair_spans_the_view() {
    // The size is a hand's preference with two ends that must both work (U88): the smallest square
    // a hand can still grab, and the largest that does not hide the part under the pointer.
    let mut harness = Harness::new("frame-work-cursor-size");
    harness.settle();
    let centre = harness.scene().center();
    harness.app.settings.pointer.cursor_size = ui::settings::MIN_CURSOR_POINTS;
    harness.hover(centre);
    let small = harness.app.shell.work_cursor.expect("the scene draws its cursor");
    let smallest = f32::from(u16::try_from(ui::settings::MIN_CURSOR_POINTS).expect("a small constant"));
    assert_eq!(small.square.width(), smallest, "the setting is the square's side, in points");
    assert_eq!(small.square.center(), centre, "the smallest square is still centred");

    harness.app.settings.pointer.cursor_size = ui::settings::MAX_CURSOR_POINTS;
    harness.app.settings.pointer.crosshair = true;
    let view = harness.scene();
    harness.hover(view.center());
    let large = harness.app.shell.work_cursor.expect("the scene draws its cursor");
    let largest = f32::from(u16::try_from(ui::settings::MAX_CURSOR_POINTS).expect("a small constant"));
    assert_eq!(large.square.width(), largest, "the largest square is the setting, not a multiple of it");
    assert_eq!(large.square.center(), view.center(), "and it is centred on the pointer");
    let [horizontal, vertical] = large.crosshair.expect("the switch draws both arms");
    assert_eq!((horizontal.left(), horizontal.right()), (view.left(), view.right()), "one hairline spans the width");
    assert_eq!(horizontal.center().y, view.center().y, "and it passes through the pointer");
    assert!(horizontal.height() > 0.0, "an arm is a stroke wide, or the view would carry no line at all");
    assert_eq!((vertical.top(), vertical.bottom()), (view.top(), view.bottom()), "the other spans the height");
    assert_eq!(vertical.center().x, view.center().x, "and passes through the pointer too");
    assert!(vertical.width() > 0.0, "and both arms are painted, not only the horizontal one");
}

/// Every text the last frame drew inside `rect`.
fn texts_inside(harness: &Harness, rect: egui::Rect) -> Vec<String> {
    harness.drawn_rects.iter().filter(|(at, _)| rect.contains_rect(*at)).map(|(_, text)| text.clone()).collect()
}

/// The armed tool's card and the inspector show one draft (U97, ADR 0119).
///
/// Every word the card drew must appear in the inspector: both read the same values, so a number
/// that disagreed would be a second source of truth rather than a copy.
fn assert_the_card_and_the_inspector_agree(harness: &Harness, chrome: &Chrome) {
    let document = harness.app.tabs.active();
    let card = document.tool_card_rect().expect("the armed tool drew its card");
    let inspector = chrome.panels.right.expect("the inspector is on");
    assert!(screen().contains_rect(card), "the card is on screen: {card:?}");
    let in_card = texts_inside(harness, card);
    let in_inspector = texts_inside(harness, inspector);
    assert!(!in_card.is_empty(), "the card drew its rows: {card:?}");
    for text in &in_card {
        assert!(
            in_inspector.contains(text),
            "the inspector mirrors {text:?}: card {in_card:?}, inspector {in_inspector:?}"
        );
    }
}

#[test]
fn a_frame_shows_its_height_in_the_card_and_the_inspector_and_they_never_disagree() {
    let mut harness = Harness::with_session("frame-tool-card-box", crate::scene_build::empty_session().expect("empty"));
    harness.settle();
    let mm = testkit::millimetres;
    {
        let document = harness.app.tabs.active_mut();
        document.editor.start_product_box_drawing(DrawTool::Rectangle);
        document.editor.drawing_point([mm(0), mm(0)]).expect("the anchor");
        document.editor.drawing_point([mm(300), mm(200)]).expect("the base");
        document.editor.start_product_pull(Lang::En).expect("the pull starts");
        document.editor.pull_box_height(mm(120)).expect("the pull");
    }
    let chrome = harness.settle();
    let document = harness.app.tabs.active();
    let card = document.tool_card_rect().expect("the height pull draws its card");
    let in_card = texts_inside(&harness, card);
    assert!(in_card.iter().any(|text| text == "120"), "the card shows the pulled height: {in_card:?}");
    assert!(
        in_card.iter().any(|text| text == "H"),
        "and the one letter names it, exactly as the solid's pull does: {in_card:?}"
    );
    assert_the_card_and_the_inspector_agree(&harness, &chrome);
}

#[test]
fn a_fill_shows_its_two_gaps_in_the_card_and_the_inspector_and_they_never_disagree() {
    let mut harness =
        Harness::with_session("frame-tool-card-fill", crate::scene_build::empty_session().expect("empty"));
    harness.settle();
    harness.app.product_card.start();
    harness.settle();
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);

    let pointer = {
        let document = harness.app.tabs.active_mut();
        let product = document.session.active_product().expect("the card opens the frame");
        let project = document.session.project();
        let envelope = project.container(product).and_then(|product| product.envelope()).expect("an envelope");
        let size = geom::DVec3::new(
            envelope.x().millimetres_f64(),
            envelope.y().millimetres_f64(),
            envelope.z().millimetres_f64(),
        );
        let centre = project.world_placement(product).expect("a frame").transform_point(size * 0.5);
        let rect = document.scene_rect().expect("a drawn frame gives the scene a rectangle");
        let pointer = gizmo_math::screen_point(document.camera(), rect, centre).expect("the frame is on screen");
        document
            .editor
            .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, model::FillMode::Fit, Lang::En)
            .expect("the shelf fill arms inside the open frame");
        pointer
    };
    harness.hover(pointer);
    let chrome = harness.settle();
    let document = harness.app.tabs.active();
    let card = document.tool_card_rect().expect("the fill's card drew beside the pointer");
    let in_card = texts_inside(&harness, card);
    assert!(in_card.iter().any(|text| text == "Low gap"), "the card names both gaps: {in_card:?}");
    assert!(in_card.iter().any(|text| text == "High gap"), "the card names both gaps: {in_card:?}");
    assert_the_card_and_the_inspector_agree(&harness, &chrome);
}

/// A scene holding one extruded body: a 600 x 400 mm rectangle pulled 18 mm, and its id.
fn one_body() -> (engine::Session, Id) {
    use model::{Constructive, Container, Geometry, Path, Placement, Project};
    let mm = testkit::millimetres;
    let root = Container::new(Id::new(1).expect("an id"), "Scene", Placement::IDENTITY, Vec::new());
    let mut project = Project::new(root).expect("an empty project");
    let ring = Path::rectangle([mm(0), mm(0)], [mm(600), mm(400)]).expect("a rectangle");
    let body = Constructive::extrude(ring, mm(18)).expect("a body");
    let id = project
        .add_object(project.root().id(), "Body", Placement::IDENTITY, Geometry::Constructive(body))
        .expect("the body is added");
    (engine::Session::with_directory(project, testkit::sample_catalog()), id)
}

#[test]
fn a_pull_shows_its_height_in_the_card_and_the_inspector_and_they_never_disagree() {
    let (session, body) = one_body();
    let mut harness = Harness::with_session("frame-tool-card-pull", session);
    harness.settle();
    {
        let document = harness.app.tabs.active_mut();
        document.editor.select_node(Some(body));
        document.extrude_gizmo.start(body, testkit::millimetres(40));
    }
    harness.run(vec![egui::Event::PointerMoved(harness.scene().center())]);
    let chrome = harness.settle();
    let document = harness.app.tabs.active();
    let card = document.tool_card_rect().expect("the pull draws its card");
    let in_card = texts_over(&harness, card);
    assert!(in_card.iter().any(|text| text == "40"), "the card shows the pulled height: {in_card:?}");
    assert_the_card_and_the_inspector_agree(&harness, &chrome);
}

/// A harness holding one extruded body with its pull open at `height`, and the body's id.
fn pulling(name: &str, height: units::Length) -> (Harness, Id) {
    let (session, body) = one_body();
    let mut harness = Harness::with_session(name, session);
    harness.settle();
    {
        let document = harness.app.tabs.active_mut();
        document.editor.select_node(Some(body));
        document.extrude_gizmo.start(body, height);
    }
    harness.run(vec![egui::Event::PointerMoved(harness.scene().center())]);
    harness.settle();
    (harness, body)
}

/// Every text the last frame drew over `rect`, by where its middle landed.
///
/// A focused field paints its number through a text edit, whose glyph rectangle can overhang the
/// card by a point or two, so a test that asks what a card *shows* reads the words over it rather
/// than the shapes wholly inside it.
fn texts_over(harness: &Harness, rect: egui::Rect) -> Vec<String> {
    harness.drawn_rects.iter().filter(|(at, _)| rect.contains(at.center())).map(|(_, text)| text.clone()).collect()
}

/// The height the document holds for an extruded body.
fn body_height(harness: &Harness, body: Id) -> Option<units::Length> {
    harness
        .app
        .tabs
        .active()
        .session
        .project()
        .object(body)
        .and_then(|node| node.geometry().as_constructive())
        .and_then(model::Constructive::height)
}

#[test]
fn the_extrusion_card_is_one_lettered_field_with_no_tick_or_cross() {
    // U104: the owner wants the pull's plate to be a number, not a dialog. `H` names it, no unit
    // repeats it, no word sits above it, and the gesture's own keys are its commit and its cancel —
    // so the card draws no ✓ and no ✕ and the field is the only control it holds.
    let (harness, _) = pulling("frame-tool-card-pull-lettered", testkit::millimetres(40));
    let card = harness.app.tabs.active().tool_card_rect().expect("the pull draws its card");
    let mut texts = texts_over(&harness, card);
    texts.sort();
    assert_eq!(texts, ["40", "H"], "one field, lettered and without a unit or a title");
    let buttons: Vec<&str> =
        harness.faces.iter().filter(|(_, rect)| card.contains_rect(*rect)).map(|(name, _)| name.as_str()).collect();
    assert!(buttons.is_empty(), "the card offers no tick and no cross: {buttons:?}");
}

#[test]
fn the_extrusion_field_commits_with_enter_and_cancels_with_escape() {
    // The field is the only control, so the keys are the whole commit path: Enter writes the height
    // once and Escape drops the pull without touching the document.
    let (mut harness, body) = pulling("frame-tool-card-pull-enter", testkit::millimetres(40));
    let revision = harness.revision();
    harness.run(vec![egui::Event::Text("55".to_owned())]);
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    let document = harness.app.tabs.active();
    assert!(!document.extrude_gizmo.is_active(), "Enter commits the pull");
    assert_eq!(body_height(&harness, body), Some(testkit::millimetres(55)), "at the typed height");
    assert_eq!(
        document.session.project().revision(),
        revision.checked_next().expect("a revision"),
        "one command, not one per frame"
    );
    assert!(document.session.can_undo(), "and one undo entry");

    let (mut harness, body) = pulling("frame-tool-card-pull-escape", testkit::millimetres(40));
    let document = harness.app.tabs.active();
    let committed = body_height(&harness, body);
    let revision = document.session.project().revision();
    harness.run(vec![egui::Event::Text("55".to_owned())]);
    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
    let document = harness.app.tabs.active();
    assert!(!document.extrude_gizmo.is_active(), "Escape cancels the pull");
    assert_eq!(harness.revision(), revision, "and writes nothing");
    assert_eq!(body_height(&harness, body), committed, "the body keeps the height the document holds");
}

#[test]
fn a_press_inside_the_card_is_chrome_and_never_a_pick_or_a_finish() {
    let (session, body) = one_body();
    let mut harness = Harness::with_session("frame-tool-card-chrome", session);
    harness.settle();
    {
        let document = harness.app.tabs.active_mut();
        document.editor.select_node(Some(body));
        document.extrude_gizmo.start(body, testkit::millimetres(40));
    }
    harness.run(vec![egui::Event::PointerMoved(harness.scene().center())]);
    harness.settle();
    let (card, revision) = {
        let document = harness.app.tabs.active();
        (document.tool_card_rect().expect("the pull draws its card"), harness.revision())
    };
    // The card floats over the scene, so the press belongs to its field: the pull stays open and
    // the document keeps its revision (ADR 0080, ADR 0119).
    harness.click(card.center(), egui::PointerButton::Primary);
    let document = harness.app.tabs.active();
    assert!(document.extrude_gizmo.is_active(), "a press on the card never finishes the pull");
    assert_eq!(harness.revision(), revision, "and never writes a document");
    assert!(document.tool_card_rect().is_some(), "the card is still there, holding the keyboard");
}

#[test]
fn escape_closes_the_card_and_writes_nothing() {
    let mut harness =
        Harness::with_session("frame-tool-card-escape", crate::scene_build::empty_session().expect("empty"));
    harness.settle();
    let mm = testkit::millimetres;
    {
        let document = harness.app.tabs.active_mut();
        document.editor.start_product_box_drawing(DrawTool::Rectangle);
        document.editor.drawing_point([mm(0), mm(0)]).expect("the anchor");
        document.editor.drawing_point([mm(300), mm(200)]).expect("the base");
        document.editor.start_product_pull(Lang::En).expect("the pull starts");
        document.editor.pull_box_height(mm(120)).expect("the pull");
    }
    harness.run(vec![egui::Event::PointerMoved(harness.scene().center())]);
    harness.settle();
    let before = harness.revision();
    assert!(harness.app.tabs.active().tool_card_rect().is_some(), "the card is up before the key");
    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
    let document = harness.app.tabs.active();
    assert!(document.editor.box_draft().is_none(), "Escape drops the draft");
    assert!(document.tool_card_rect().is_none(), "and its card goes with it");
    assert_eq!(harness.revision(), before, "a cancelled draft writes nothing");
}

#[test]
fn the_ruler_measures_two_points_and_writes_nothing() {
    // U98, ADR 0118: the ruler is an atom. Two clicks fix a length, a third starts a new
    // measurement, and Escape leaves it — with no revision, no undo entry and no document field.
    let session = crate::scene_build::empty_session().expect("an empty scene builds");
    let mut harness = Harness::with_session("frame-measure", session);
    harness.settle();
    let revision = harness.revision();
    assert!(!harness.app.tabs.active().can_undo(), "an empty scene has no history");

    harness.press(egui::Key::M, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.measure().is_some(), "M arms the ruler");
    assert!(
        harness.drew(Message::MeasureFirstPoint.text(Lang::En)),
        "the bar says what to click first: {:?}",
        harness.drawn
    );

    let scene = harness.scene();
    harness.click(scene.center() - egui::vec2(120.0, 40.0), egui::PointerButton::Primary);
    let first = {
        let measure = harness.app.tabs.active().editor.measure().expect("the ruler is armed");
        assert_eq!(measure.stage(), MeasureStage::Second, "the first click places the first point");
        measure.first().expect("the first click is stored")
    };
    assert_eq!(first[2], Length::ZERO, "an empty scene measures on the work plane");
    for axis in first {
        assert_eq!(axis.micrometres() % 10_000, 0, "the grid snapped the point: {axis}");
    }
    assert!(
        matches!(harness.app.tabs.active().measure_gizmo.hover().and_then(|hover| hover.kind), Some(SnapKind::Grid)),
        "the marker says the grid captured the pointer"
    );

    harness.click(scene.center() + egui::vec2(140.0, 40.0), egui::PointerButton::Primary);
    let value = {
        let measure = harness.app.tabs.active().editor.measure().expect("the ruler is armed");
        assert_eq!(measure.stage(), MeasureStage::Measured, "the second click fixes the value");
        measure.value().expect("two points make a length")
    };
    assert!(value.is_positive(), "the two clicks are a real distance: {value}");
    assert!(harness.drew(&value.to_string()), "the status bar shows the length: {:?}", harness.drawn);
    assert!(
        harness.drew(Message::MeasureAgain.text(Lang::En)),
        "and says the next click starts over: {:?}",
        harness.drawn
    );

    // The third click starts a new measurement instead of extending the last one.
    harness.click(scene.center() + egui::vec2(30.0, 100.0), egui::PointerButton::Primary);
    let measure = harness.app.tabs.active().editor.measure().expect("the ruler is still armed");
    assert_eq!(measure.stage(), MeasureStage::Second, "the third click starts a new measurement");
    assert_eq!(measure.value(), Some(Length::ZERO), "and it begins at its own point");
    assert_eq!(harness.revision(), revision, "the ruler writes no document");
    assert!(!harness.app.tabs.active().can_undo(), "and no undo entry");

    // Escape leaves the tool; nothing was written, so there is nothing to undo.
    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.measure().is_none(), "Escape leaves the ruler");
    assert!(harness.app.tabs.active().measure_gizmo.hover().is_none(), "and drops its point");
    assert_eq!(harness.revision(), revision, "Escape writes nothing");
    assert!(!harness.app.tabs.active().can_undo(), "and the history stays empty");
}

#[test]
fn the_niche_is_a_tool_the_ribbon_arms_and_the_fill_reads() {
    // U99, ADR 0118: the niche is a tool of its own. The Furniture cell arms it, the pointer names the
    // cell the status bar prints, Escape leaves it without a document change — and the fill, armed at
    // the same pointer, plans its part from the very cell the niche tool named.
    let mut harness = Harness::with_session("frame-niche", crate::scene_build::empty_session().expect("empty"));
    harness.settle();
    let draft = ui::product_card::ProductDraft {
        envelope: Envelope::new(testkit::millimetres(600), testkit::millimetres(560), testkit::millimetres(720))
            .expect("a valid box"),
        class: ClassCode::GENERAL_PRODUCT,
    };
    harness.app.tabs.active_mut().create_product(draft, Lang::En).expect("create the frame");
    let revision = harness.revision();

    // The pointer sits on the middle of the reserved box, which is the whole void of an empty piece.
    let product = harness.app.tabs.active().session.active_product().expect("the frame is open");
    let pointer = {
        let document = harness.app.tabs.active();
        let project = document.session.project();
        let envelope = project.container(product).and_then(|product| product.envelope()).expect("an envelope");
        let size = geom::DVec3::new(
            envelope.x().millimetres_f64(),
            envelope.y().millimetres_f64(),
            envelope.z().millimetres_f64(),
        );
        let centre = project.world_placement(product).expect("a frame").transform_point(size * 0.5);
        let rect = document.scene_rect().expect("a drawn frame gives the scene a rectangle");
        gizmo_math::screen_point(document.camera(), rect, centre).expect("the frame is on screen")
    };

    // The cell lives on the Furniture tab, and a fresh workspace opens the Sketch tab.
    if let Some(tab) = ui::toolbars::spec::tab("furniture") {
        harness.app.tabs.active_mut().editor.set_ribbon_tab(tab);
    }
    harness.settle();
    let cell = harness
        .faces
        .iter()
        .find(|(name, _)| name.starts_with(Message::ToolVoid.text(Lang::En)))
        .map(|(_, rect)| rect.center())
        .expect("the niche cell is on the strip");
    harness.click(cell, egui::PointerButton::Primary);
    assert!(harness.app.tabs.active().editor.void_armed(), "the cell arms the niche tool");
    assert_eq!(harness.revision(), revision, "arming it writes nothing");

    harness.hover(pointer);
    let measured = {
        let document = harness.app.tabs.active();
        let void = document.editor.void();
        assert_eq!(void.product(), Some(product), "the pointer names the open product's void");
        let size = ["600", "560", "720"].map(|side| format!("{side} mm"));
        let text = Message::VoidSize.with(Lang::En, [Message::StatusSize.with(Lang::En, size)]);
        assert!(harness.drew(&text), "the status bar names the cell the pointer holds: {:?}", harness.drawn);
        assert!(harness.drew(Message::VoidHint.text(Lang::En)), "and says what to do next: {:?}", harness.drawn);
        void.niche().cloned().expect("the pointer names a cell")
    };
    assert_eq!(harness.revision(), revision, "measuring the void writes nothing");

    // Escape leaves the tool and drops the cell it named; the document never changed.
    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!harness.app.tabs.active().editor.void_armed(), "Escape leaves the niche tool");
    assert!(harness.app.tabs.active().editor.void().niche().is_none(), "and drops its cell");
    assert_eq!(harness.revision(), revision, "Escape writes nothing");

    // The fill tool arms the same atom, so its ghost is built from the cell the niche tool named.
    {
        let document = harness.app.tabs.active_mut();
        document
            .editor
            .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, model::FillMode::Fit, Lang::En)
            .expect("the shelf fill arms inside the open frame");
    }
    harness.hover(pointer);
    let ghost = harness.app.tabs.active().fill_gizmo.ghost().cloned().expect("the fill plans a part");
    assert_eq!(ghost.niche, measured, "one pointer, one cell: the fill reads the niche tool's value");
    assert_eq!(harness.revision(), revision, "and the fill wrote nothing before its click");
}

#[test]
fn a_hidden_facade_lets_the_shelf_fill_measure_the_case_behind_it() {
    // X73: the owner switched a frame's facades off, armed the shelf fill, and the ghost stuck in
    // the phantom planes. A switched-off part is invisible to the eye and to the pointer, so the
    // cell is the case's own void, and the part still stops at the facade's inner face.
    let mm = testkit::millimetres;
    let root = model::Container::new(Id::new(1).expect("an id"), "Root", model::Placement::IDENTITY, Vec::new());
    let mut project = model::Project::new(root).expect("an empty project");
    let envelope = Envelope::new(mm(600), mm(560), mm(720)).expect("a box");
    let product = project
        .add_product(project.root().id(), "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, model::Placement::IDENTITY)
        .expect("a product");
    // A wall stance: the part's length runs up the case, its width across it, its thickness along +Y.
    let wall = geom::DQuat::from_rotation_z(-std::f64::consts::FRAC_PI_2)
        * geom::DQuat::from_rotation_y(-std::f64::consts::FRAC_PI_2);
    let size = model::PanelNominalSize::new(mm(720), mm(600)).expect("a wall size");
    let mut shell = |at: f64, name: &str| {
        let placement = model::Placement::new(geom::DVec3::new(0.0, at, 0.0), wall).expect("a panel placement");
        project.add_panel(product, name, size, None, mm(18), placement).expect("a panel")
    };
    let far_side = shell(0.0, "Far");
    let near_side = shell(542.0, "Near");
    let mut session = engine::Session::with_directory(project, testkit::sample_catalog());
    assert!(session.open_product(product), "the fixture product opens");
    let mut harness = Harness::with_session("frame-hidden-facade-fill", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();

    // The panel the camera looks at is the one a maker would switch off; the case's other side
    // stays drawn, so the ray has a surface to rest on behind it.
    let facade = {
        let document = harness.app.tabs.active();
        if document.camera().eye().y > 280.0 { near_side } else { far_side }
    };
    let pointer = {
        let document = harness.app.tabs.active();
        let rect = document.scene_rect().expect("a drawn frame gives the scene a rectangle");
        gizmo_math::screen_point(document.camera(), rect, geom::DVec3::new(300.0, 280.0, 360.0))
            .expect("the case is on screen")
    };

    {
        let document = harness.app.tabs.active_mut();
        document
            .editor
            .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, model::FillMode::Fit, Lang::En)
            .expect("the shelf fill arms inside the open frame");
    }
    harness.hover(pointer);
    let revision = harness.revision();
    let shown = harness.app.tabs.active().fill_gizmo.ghost().cloned().expect("the fill plans a shelf");

    harness.app.tabs.active_mut().editor.set_hidden(facade, true);
    harness.hover(pointer);
    let hidden = harness.app.tabs.active().fill_gizmo.ghost().cloned().expect("the hidden facade plans a shelf");

    assert!(
        hidden.niche.size.y > shown.niche.size.y,
        "the hidden facade is no wall of the void: {:?} against {:?}",
        hidden.niche.size,
        shown.niche.size
    );
    assert_eq!(
        hidden.niche.size,
        geom::DVec3::new(600.0, 542.0, 720.0),
        "the cell is the case's own space between its two sides"
    );
    assert_eq!(hidden.nominal_size.length(), mm(600), "the shelf spans the case");
    assert_eq!(
        hidden.nominal_size.width(),
        mm(524),
        "and its depth stops at the facade's inner face: the facade is 18 thick and the case 542 deep"
    );
    assert!(
        (hidden.placement.translation().y - 18.0).abs() < 1.0e-3,
        "its near face rests on the far side's own face, never inside the facade: {:?}",
        hidden.placement
    );
    assert_eq!(harness.revision(), revision, "the flag never reaches the document");
}

#[test]
fn the_ruler_snaps_to_the_board_under_the_pointer() {
    // The probe is the ruler's point resolver (ADR 0103, 0118): a click lands on the face the
    // person points at, not on the work plane behind it and not on the reserved box around a part.
    let mut harness = Harness::new("frame-measure-surface");
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    let node = *harness.selected().first().expect("the demo selects its first panel");
    let at = harness.node_point(node);
    assert!(harness.scene().contains(at), "the pointer lands on the scene: {at:?}");

    harness.press(egui::Key::M, egui::Modifiers::NONE);
    harness.hover(at);
    let hover = harness.app.tabs.active().measure_gizmo.hover().expect("the pointer resolves a point");
    assert_ne!(
        hover.kind,
        Some(SnapKind::Grid),
        "a scene point captures the pointer and not the work plane's grid: {:?}",
        hover.kind
    );
    let revision = harness.revision();
    harness.click(at, egui::PointerButton::Primary);

    let document = harness.app.tabs.active();
    let measure = document.editor.measure().expect("the ruler is armed");
    assert_eq!(measure.stage(), MeasureStage::Second, "the click places the first point");
    let first = measure.first().expect("the first click is stored");
    let project = document.session.project();
    let panel = project.panel(node).expect("the selected node is a panel");
    let world = project.world_placement(node).expect("the panel has a world frame");
    let inverse = world.inverse().expect("a rigid placement inverts");
    let local = inverse.transform_point(geom::DVec3::new(
        first[0].millimetres_f64(),
        first[1].millimetres_f64(),
        first[2].millimetres_f64(),
    ));
    let thickness = panel.last_known_thickness().millimetres_f64();
    assert!(
        local.z.abs() < 0.01 || (local.z - thickness).abs() < 0.01,
        "the point is on the panel's A or B face: {local:?} of thickness {thickness}"
    );
    assert_eq!(harness.revision(), revision, "measuring on a surface writes nothing");
}

#[test]
fn the_ruler_answers_the_candidate_the_eye_sees_in_either_snap_mode() {
    // U119: the ruler stores a point in space, so it declares `Free` and the workspace's mode makes
    // no difference to it. A surface is not above or below a plane the tool draws on — it is the
    // place the click takes — so the same click answers the same point in 2.5D and in 3D.
    let mut answers = Vec::new();
    for mode in [model::SnapMode::Foot, model::SnapMode::Space] {
        let mut harness = Harness::new("frame-measure-mode");
        harness.settle();
        harness.app.tabs.active_mut().frame_all();
        harness.settle();
        {
            let document = harness.app.tabs.active_mut();
            let kinds = document.snaps();
            document.set_snaps(crate::document::SnapChoice { kinds, mode });
        }
        let node = *harness.selected().first().expect("the demo selects its first panel");
        let at = harness.node_point(node);
        harness.press(egui::Key::M, egui::Modifiers::NONE);
        harness.hover(at);
        let hover = harness.app.tabs.active().measure_gizmo.hover().expect("the pointer resolves a point");
        answers.push((hover.point, hover.kind));
    }
    assert_eq!(answers[0], answers[1], "one point in both modes: the ruler has no plane to drop a foot onto");
}

#[test]
fn the_ruler_measures_the_board_inside_a_product_not_its_reserved_box() {
    // The reserved box is a probe surface (ADR 0103) and the first thing the ray meets from
    // outside, so the ruler steps past its near face: the person points at the board inside.
    let (project, product, panel) = product_with_a_side_panel();
    let mut session = engine::Session::with_directory(project, testkit::sample_catalog());
    assert!(session.open_product(product), "the fixture product opens");
    let mut harness = Harness::with_session("frame-measure-product", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();

    let at = harness.node_point(panel);
    harness.press(egui::Key::M, egui::Modifiers::NONE);
    harness.hover(at);
    let hover = harness.app.tabs.active().measure_gizmo.hover().expect("the pointer resolves a point");
    assert_ne!(
        hover.kind,
        Some(SnapKind::Grid),
        "a face of the board captures the pointer and not the envelope or the plane: {:?}",
        hover.kind
    );
    harness.click(at, egui::PointerButton::Primary);

    let document = harness.app.tabs.active();
    let first = document.editor.measure().and_then(ui::Measure::first).expect("the point is stored");
    let project = document.session.project();
    let piece = project.panel(panel).expect("the node is a panel");
    let world = project.world_placement(panel).expect("the panel has a world frame");
    let local = world.inverse().expect("a rigid placement inverts").transform_point(geom::DVec3::new(
        first[0].millimetres_f64(),
        first[1].millimetres_f64(),
        first[2].millimetres_f64(),
    ));
    let nominal = piece.nominal_size();
    let faces = [
        (local.x, 0.0, nominal.length().millimetres_f64()),
        (local.y, 0.0, nominal.width().millimetres_f64()),
        (local.z, 0.0, piece.last_known_thickness().millimetres_f64()),
    ];
    assert!(
        faces.iter().any(|(value, low, high)| (*value - low).abs() < 0.01 || (*value - high).abs() < 0.01),
        "the point lies on a face of the board: {local:?}"
    );
}

/// Types `text` into the plate that holds the keyboard, one frame per character.
fn type_text(harness: &mut Harness, text: &str) {
    for character in text.chars() {
        harness.run(vec![egui::Event::Text(character.to_string())]);
    }
}

/// The points the running draft has placed so far.
fn draft_points(harness: &Harness) -> Vec<[Length; 2]> {
    harness.app.tabs.active().editor.drawing().map(|draft| draft.points().to_vec()).unwrap_or_default()
}

/// The point the running draft's plates would place under the pointer.
fn placed_point(harness: &Harness) -> [Length; 2] {
    harness.app.tabs.active().draw_gizmo.point_preview().expect("the plates name a point")
}

/// A work-plane point as the geometry boundary reads it.
fn offset(from: [Length; 2], to: [Length; 2]) -> DVec2 {
    DVec2::new(to[0].millimetres_f64() - from[0].millimetres_f64(), to[1].millimetres_f64() - from[1].millimetres_f64())
}

#[test]
fn a_typed_length_and_angle_place_a_sketch_point_exactly() {
    // The plates of the point being placed are the point's own numbers (ADR 0126): a line drawn to
    // 600 mm at 30 degrees is typed, not aimed, and the preview follows the numbers rather than the
    // pointer. A fixed tool commits on its last point, so the whole figure is one undo entry.
    let mut harness = Harness::new("frame-sketch-numbers");
    harness.settle();
    let before = harness.revision();
    harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::Line);
    harness.settle();
    let scene = harness.scene();
    harness.click(scene.center() - egui::vec2(120.0, 60.0), egui::PointerButton::Primary);
    let anchor = draft_points(&harness).first().copied().expect("the click places the first point");
    // The plates rebuild for the segment's numbers, and the pointer is somewhere else entirely.
    harness.hover(scene.center() + egui::vec2(40.0, 160.0));
    assert!(harness.app.tabs.active().draw_gizmo.field_rect().is_some(), "the point shows its plates");

    type_text(&mut harness, "600");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "30");
    // Tab handed the length over, so the plate locks it and the preview already follows it; the
    // angle that is still being typed is the promise Enter makes.
    let previewed = offset(anchor, placed_point(&harness));
    assert!((previewed.length() - 600.0).abs() < 0.002, "the preview follows the typed length: {previewed:?}");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);

    assert!(harness.app.tabs.active().editor.drawing().is_none(), "Enter places the last point and commits");
    let document = harness.app.tabs.active();
    let object = harness.selected().first().copied().expect("the line is selected");
    let project = document.session.project();
    let path = project.object_path(object).expect("the node is a path");
    let segment = path.segments().next().expect("a line is one segment");
    let (start, end) = (segment.start().expect("a start"), segment.end().expect("an end"));
    let drawn = offset(start, end);
    assert!((drawn.length() - 600.0).abs() < 0.002, "the typed length: {drawn:?}");
    assert!((drawn.y.atan2(drawn.x).to_degrees() - 30.0).abs() < 0.01, "the typed angle: {drawn:?}");
    // The command re-anchors the figure at its creation pivot, so the world start is the placement.
    let world = project.world_placement(object).expect("the line has a world frame");
    let at = world.transform_point(geom::DVec3::new(start[0].millimetres_f64(), start[1].millimetres_f64(), 0.0));
    let anchor_at = geom::DVec3::new(anchor[0].millimetres_f64(), anchor[1].millimetres_f64(), 0.0);
    assert!((at - anchor_at).length() < 0.01, "the line starts where the first click placed it: {at:?}");
    assert_eq!(harness.revision().get(), before.get() + 1, "one click, one point, one path, one undo entry");
    assert!(document.can_undo());
}

#[test]
fn the_by_angle_line_is_placed_entirely_from_its_plates() {
    // X53: the line list's second method is the typed pair. No click places anything: X and Y name
    // the start, the length and the angle name the end, and Enter commits the one open line.
    let mut harness = Harness::new("frame-sketch-line-angle");
    harness.settle();
    let before = harness.revision();
    harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::LineByAngle);
    harness.settle();
    let scene = harness.scene();
    harness.hover(scene.center());
    type_text(&mut harness, "100");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "100");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    let start = draft_points(&harness).first().copied().expect("the typed start point");
    assert_eq!(start, [mesh(100), mesh(100)], "X and Y placed the start the person typed");

    type_text(&mut harness, "600");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "30");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.drawing().is_none(), "the pair finishes the line and commits");

    let object = harness.selected().first().copied().expect("the line is selected");
    let project = harness.app.tabs.active().session.project();
    let path = project.object_path(object).expect("the node is a path");
    assert!(!path.is_closed(), "a line is open");
    let segment = path.segments().next().expect("a line is one segment");
    assert_eq!(path.entries().len(), 1, "one segment, not two");
    let drawn = offset(segment.start().expect("a start"), segment.end().expect("an end"));
    assert!((drawn.length() - 600.0).abs() < 0.002, "the typed length: {drawn:?}");
    assert!((drawn.y.atan2(drawn.x).to_degrees() - 30.0).abs() < 0.01, "the typed angle: {drawn:?}");
    // The command anchors an open path at its first point, so the placement is where it starts.
    let world = project.world_placement(object).expect("the line has a world frame");
    let at = world.transform_point(geom::DVec3::ZERO);
    assert!((at - geom::DVec3::new(100.0, 100.0, 0.0)).length() < 0.01, "it starts at the typed point: {at:?}");
    assert_eq!(harness.revision().get(), before.get() + 1, "one line, one undo entry");
}

#[test]
fn a_typed_number_outlives_the_pointer_and_escape_gives_the_keys_back() {
    // A typed number is a promise: the pointer and its snap cannot move it. Escape drops the promise
    // first and ends the tool second, so one key never means two things in one frame (ADR 0126).
    let mut harness = Harness::new("frame-sketch-lock");
    harness.settle();
    let before = harness.revision();
    harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::Line);
    harness.settle();
    let scene = harness.scene();
    harness.click(scene.center() - egui::vec2(100.0, 80.0), egui::PointerButton::Primary);
    let anchor = draft_points(&harness).first().copied().expect("the first point");
    harness.hover(scene.center() + egui::vec2(120.0, 90.0));
    type_text(&mut harness, "300");
    // Tab hands the number over, which is what locks it; a field still being typed is not yet a
    // promise and the pointer keeps driving it (ADR 0126).
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    assert!((offset(anchor, placed_point(&harness)).length() - 300.0).abs() < 0.002, "the typed length holds");

    // The pointer moves onto a different grid point; the promise still holds.
    harness.hover(scene.center() + egui::vec2(-40.0, 200.0));
    assert!((offset(anchor, placed_point(&harness)).length() - 300.0).abs() < 0.002, "a snap cannot move it");

    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.drawing().is_some(), "the first Escape keeps the tool");
    assert_eq!(draft_points(&harness).len(), 1, "and places no point");
    assert!(
        (offset(anchor, placed_point(&harness)).length() - 300.0).abs() > 1.0,
        "the released length follows the pointer again"
    );
    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.drawing().is_none(), "the second Escape ends the tool");
    assert_eq!(harness.revision().get(), before.get(), "neither Escape wrote anything");
}

#[test]
fn chrome_that_takes_the_keys_takes_nothing_from_the_live_point() {
    // X54: a plate shows the live number rounded to its own decimal, and losing the keyboard is not
    // a promise. A press on a panel or on the corner axes widget used to commit that rounding and
    // lock the parameter, so the next point froze where the pointer happened to be when the person
    // pressed the chrome. Both surfaces the owner named are pressed here, each on a fresh draft
    // because a new stage is what puts the keyboard back in a plate.
    let mut harness = Harness::new("frame-sketch-blur");
    harness.settle();
    let scene = harness.scene();
    let start = scene.center() - egui::vec2(120.0, 60.0);
    let near = scene.center() + egui::vec2(160.0, 96.0);
    let elsewhere = scene.center() - egui::vec2(240.0, 150.0);
    let arm = |harness: &mut Harness| {
        harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::Line);
        harness.settle();
        harness.click(start, egui::PointerButton::Primary);
    };

    // The corner axes widget: a press on the cube takes the keyboard and the view stays the one it
    // already showed, so the number the plate was showing is left to the pointer.
    arm(&mut harness);
    let camera = harness.camera();
    let widget = harness.app.shell.orientation_rect.expect("the axes widget draws in the corner");
    a_press_leaves_the_live_length(&mut harness, near, elsewhere, widget.min + egui::vec2(2.0, 2.0), "the axes widget");
    assert_eq!(harness.camera(), camera, "pressing the axes widget changed no view");

    // The left panel: the press lands on its empty body, which is chrome.
    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
    arm(&mut harness);
    let chrome = harness.settle();
    let tree = chrome.panels.left.expect("the scene tree is up");
    let on_panel = egui::pos2(tree.center().x, tree.bottom() - 12.0);
    a_press_leaves_the_live_length(&mut harness, near, elsewhere, on_panel, "the left panel");
    assert_eq!(draft_points(&harness).len(), 1, "and no press placed a second point");
}

/// Measures the live length at two pointer places, presses `at`, and measures again.
///
/// The plate shows its number rounded, so the second reading is what the pointer names only while a
/// press on chrome stays out of the numbers (X54).
fn a_press_leaves_the_live_length(
    harness: &mut Harness,
    near: egui::Pos2,
    elsewhere: egui::Pos2,
    at: egui::Pos2,
    surface: &str,
) {
    let length_at = |harness: &Harness| {
        let from = draft_points(harness).last().copied().expect("the draft has a point to measure from");
        offset(from, placed_point(harness)).length()
    };
    harness.hover(near);
    let live = length_at(harness);
    assert!((live * 10.0).round() / 10.0 != live, "{surface}: the live length must not round-trip: {live}");
    harness.hover(elsewhere);
    let promised = length_at(harness);
    assert!((promised - live).abs() > 1.0, "{surface}: the two pointer places name two lengths: {live} and {promised}");
    harness.hover(near);
    harness.click(at, egui::PointerButton::Primary);
    harness.hover(elsewhere);
    let after = length_at(harness);
    assert!(
        (after - promised).abs() < 0.01,
        "{surface}: the pointer still drives the point after the press: {promised} -> {after}"
    );
}

#[test]
fn the_polyline_segment_kind_comes_from_the_right_click_menu() {
    // Tab belongs to the plates now, so a polyline's line/arc switch moved to the right-click menu;
    // the menu is a chrome surface, so choosing a row never places a point behind it (ADR 0126,
    // 0128).
    let mut harness = Harness::new("frame-sketch-menu");
    harness.settle();
    harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::Polyline);
    harness.settle();
    let scene = harness.scene();
    harness.click(scene.center() - egui::vec2(120.0, 0.0), egui::PointerButton::Primary);
    harness.click(scene.center() + egui::vec2(120.0, 0.0), egui::PointerButton::Primary);
    let points = draft_points(&harness).len();
    assert_eq!(points, 2, "two clicks, two points");

    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    assert!(
        !harness.app.tabs.active().editor.drawing().is_some_and(|draft| draft.arc_mode()),
        "Tab moves the keyboard between the plates instead of switching the segment kind"
    );

    harness.click(scene.center() + egui::vec2(0.0, 120.0), egui::PointerButton::Secondary);
    assert!(harness.app.tabs.active().draft_menu_rect().is_some(), "the right button opens the draft menu");
    assert_eq!(draft_points(&harness).len(), points, "and places no point");
    let row = harness
        .drawn_rects
        .iter()
        .find(|(_, text)| text.contains(Message::DraftSegmentArc.text(Lang::En)))
        .map(|(rect, _)| rect.center())
        .expect("the menu drew its arc row");
    harness.click(row, egui::PointerButton::Primary);
    assert!(
        harness.app.tabs.active().editor.drawing().is_some_and(|draft| draft.arc_mode()),
        "the menu chose the arc segment"
    );
    assert_eq!(draft_points(&harness).len(), points, "and still wrote no point");
    assert!(harness.app.tabs.active().draft_menu_rect().is_none(), "the choice closes the menu");
}

#[test]
fn a_typed_diameter_builds_a_circle_of_half_its_number() {
    // C4: the circle's second method is typed, not aimed. Two plates, X and Y, place the centre
    // exactly; the third is the diameter, and the document stores half of it as the radius of the
    // four quarter arcs.
    let mut harness = Harness::new("frame-sketch-diameter");
    harness.settle();
    let before = harness.revision();
    harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::CircleDiameter);
    harness.settle();
    let scene = harness.scene();
    harness.hover(scene.center());
    type_text(&mut harness, "100");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "100");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(draft_points(&harness), vec![[mesh(100), mesh(100)]], "the centre the plates named");

    // One plate is no choice to move to: Enter commits the typed number and places the point, which
    // is the same key order the two-plate stage needed a Tab for.
    harness.hover(scene.center() + egui::vec2(80.0, 0.0));
    type_text(&mut harness, "200");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.drawing().is_none(), "Enter places the last point and commits");

    let object = harness.selected().first().copied().expect("the circle is selected");
    let project = harness.app.tabs.active().session.project();
    let ContourSegment::Arc { center, radius, .. } = project
        .object_path(object)
        .expect("the drawn figure is a path")
        .entries()
        .first()
        .expect("a circle is four arcs")
        .segment()
    else {
        panic!("a circle is four arcs");
    };
    assert_eq!(center, [Length::ZERO, Length::ZERO], "the command anchors a closed figure at its bounds centre");
    assert_eq!(radius, mesh(100), "a 200 mm diameter is a 100 mm radius");
    assert_eq!(
        project.object_path(object).expect("the path").bounds_micrometres(),
        ([-100_000, -100_000], [100_000, 100_000]),
        "the circle is the 200 mm one that was asked for"
    );
    let world = project.world_placement(object).expect("the circle has a world frame");
    let at = world.transform_point(geom::DVec3::ZERO);
    assert!((at - geom::DVec3::new(100.0, 100.0, 0.0)).length() < 0.01, "centred where the plates said: {at:?}");
    assert_eq!(harness.revision().get(), before.get() + 1, "one circle, one undo entry");
}

#[test]
fn a_tangent_circle_is_built_from_its_corner_two_lines_and_a_radius() {
    // X52: the circle cell's fourth method. The corner is typed as a point, each line as the
    // direction of a click on it, and the radius as the last number, so the committed ring touches
    // both lines whatever the pointer was over.
    let mut harness = Harness::new("frame-sketch-tangent");
    harness.settle();
    let before = harness.revision();
    harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::CircleTangent);
    harness.settle();
    let scene = harness.scene();
    harness.hover(scene.center());
    type_text(&mut harness, "100");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "100");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    // A line is named by its direction alone: the distance the plate does not read stays the
    // pointer's own and never reaches the figure.
    type_text(&mut harness, "90");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    type_text(&mut harness, "180");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(draft_points(&harness).len(), 3, "the corner and one point on each line");
    type_text(&mut harness, "50");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.drawing().is_none(), "Enter places the radius and commits");

    let object = harness.selected().first().copied().expect("the circle is selected");
    let project = harness.app.tabs.active().session.project();
    let path = project.object_path(object).expect("the drawn figure is a path");
    assert!(path.is_closed(), "a tangent circle is a ring");
    let ContourSegment::Arc { center, radius, .. } = path.entries().first().expect("a circle is four arcs").segment()
    else {
        panic!("a circle is four arcs");
    };
    assert_eq!(radius, mesh(50), "the typed radius is the stored one");
    assert_eq!(center, [Length::ZERO, Length::ZERO], "the command anchors a closed figure at its bounds centre");
    // The corner is at 100, 100 on the work plane and the two lines run through it, so the centre is
    // one radius to the side of each: the wedge it went into is the one the pointer was in.
    let world = project.world_placement(object).expect("the circle has a world frame");
    let at = world.transform_point(geom::DVec3::new(center[0].millimetres_f64(), center[1].millimetres_f64(), 0.0));
    assert!(((at.x - 100.0).abs() - 50.0).abs() < 0.01, "the centre stands one radius off the vertical line: {at:?}");
    assert!(((at.y - 100.0).abs() - 50.0).abs() < 0.01, "and one radius off the horizontal one: {at:?}");
    assert_eq!(harness.revision().get(), before.get() + 1, "one circle, one undo entry");
}

#[test]
fn an_arc_from_a_typed_radius_takes_the_side_the_pointer_chose() {
    // C4: an arc from its start, end and radius. The two ends are typed, so the chord is exact; the
    // radius is typed and the pointer only names the side the centre sits on.
    let mut harness = Harness::new("frame-sketch-arc-radius");
    harness.settle();
    harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::ArcByRadius);
    harness.settle();
    let scene = harness.scene();
    harness.hover(scene.center());
    type_text(&mut harness, "100");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "100");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    type_text(&mut harness, "200");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "90");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(draft_points(&harness), vec![[mesh(100), mesh(100)], [mesh(100), mesh(300)]], "a 200 mm chord");

    // The pointer names the side; the number it would place is the distance the typed radius
    // replaces, and the direction both share is the one the third point keeps.
    harness.hover(scene.center() + egui::vec2(0.0, -150.0));
    let probe = placed_point(&harness);
    type_text(&mut harness, "150");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.drawing().is_none(), "Enter places the radius and commits");

    let object = harness.selected().first().copied().expect("the arc is selected");
    let project = harness.app.tabs.active().session.project();
    let path = project.object_path(object).expect("the drawn figure is a path");
    assert!(!path.is_closed(), "an arc is open");
    let ContourSegment::Arc { center, radius, .. } = path.entries().first().expect("one arc").segment() else {
        panic!("the figure is one arc");
    };
    assert_eq!(radius, mesh(150), "the typed radius is the stored one");
    // The stored chain is local to the object, so the side is read in the world frame where the
    // pointer named it.
    let world = project.world_placement(object).expect("the arc has a world frame");
    let to_world = |point: DVec2| {
        let at = world.transform_point(geom::DVec3::new(point.x, point.y, 0.0));
        DVec2::new(at.x, at.y)
    };
    let settled = path.entries().first().expect("one arc").segment();
    let (start, end) = (settled.start().expect("a start"), settled.end().expect("an end"));
    let (start, end) = (
        to_world(DVec2::new(start[0].millimetres_f64(), start[1].millimetres_f64())),
        to_world(DVec2::new(end[0].millimetres_f64(), end[1].millimetres_f64())),
    );
    let centre_at = to_world(DVec2::new(center[0].millimetres_f64(), center[1].millimetres_f64()));
    let probe_at = DVec2::new(probe[0].millimetres_f64(), probe[1].millimetres_f64());
    let middle = (start + end) / 2.0;
    let chord = end - start;
    let side = |point: DVec2| chord.x * (point.y - middle.y) - chord.y * (point.x - middle.x);
    assert!(side(centre_at).abs() > 1.0, "the centre is off the chord");
    assert!(side(centre_at) * side(probe_at) > 0.0, "the centre is on the side the pointer chose");
}

#[test]
fn an_arc_radius_under_half_its_chord_is_refused_and_the_draft_keeps_its_points() {
    // C4: the refusal is the model's own sentence, it reaches the status line, and the draft keeps
    // the two clicks so the radius can be named again.
    let mut harness = Harness::new("frame-sketch-arc-short");
    harness.settle();
    harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::ArcByRadius);
    harness.settle();
    let scene = harness.scene();
    harness.hover(scene.center());
    type_text(&mut harness, "100");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "100");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    type_text(&mut harness, "200");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "90");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);

    // The radius is the distance from the arc's start, so 100 mm is the smallest a 200 mm chord takes.
    harness.hover(scene.center() + egui::vec2(0.0, -60.0));
    type_text(&mut harness, "50");
    let before = harness.revision();
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.drawing().is_some(), "the draft keeps its points");
    assert_eq!(draft_points(&harness).len(), 2, "the refused click wrote nothing");
    let error = harness.app.tabs.active().last_error.clone().unwrap_or_default();
    assert!(error.contains("half its chord"), "the status line names the refusal: {error}");
    assert_eq!(harness.revision(), before, "and nothing was written");
}

#[test]
fn a_typed_apothem_builds_the_polygon_round_its_circle() {
    // C5: the circumscribed method names the distance from the centre to a side. The two numbers are
    // typed, so the ring is exact: the apothem is the side's own distance and the rim is that over
    // cos(pi/6), which is what the stored bounds measure.
    let mut harness = Harness::new("frame-sketch-apothem");
    harness.settle();
    let before = harness.revision();
    harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::PolygonApothem);
    harness.settle();
    let scene = harness.scene();
    harness.hover(scene.center());
    type_text(&mut harness, "100");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "100");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(draft_points(&harness), vec![[mesh(100), mesh(100)]], "the centre the plates named");

    harness.hover(scene.center() + egui::vec2(0.0, 120.0));
    type_text(&mut harness, "80");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "90");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.drawing().is_none(), "Enter places the side and commits");

    let object = harness.selected().first().copied().expect("the polygon is selected");
    let project = harness.app.tabs.active().session.project();
    let path = project.object_path(object).expect("the drawn figure is a path");
    assert_eq!(path.entries().len(), 6, "the count the ribbon started with");
    let (min, max) = path.bounds_micrometres();
    assert_eq!((min[1], max[1]), (-80_000, 80_000), "the apothem typed is the distance to the two sides");
    let rim = 80.0 / (std::f64::consts::PI / 6.0).cos();
    let across = max[0] as f64 / 1_000.0;
    assert!((across - rim).abs() < 0.002, "the rim is the apothem over cos(pi/6): {across} against {rim}");
    assert_eq!(harness.revision().get(), before.get() + 1, "one polygon, one undo entry");
}

#[test]
fn a_typed_edge_builds_the_polygon_from_that_side() {
    // C5: the polygon by an edge starts at the corner the person clicked and turns
    // counter-clockwise, so the clicked corner and the clicked direction are the ring's own.
    let mut harness = Harness::new("frame-sketch-polygon-edge");
    harness.settle();
    harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::PolygonByEdge);
    harness.settle();
    let scene = harness.scene();
    harness.hover(scene.center());
    type_text(&mut harness, "100");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "100");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(draft_points(&harness), vec![[mesh(100), mesh(100)]], "the corner the plates named");

    type_text(&mut harness, "100");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "30");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.drawing().is_none(), "Enter places the edge and commits");

    let object = harness.selected().first().copied().expect("the polygon is selected");
    let project = harness.app.tabs.active().session.project();
    let path = project.object_path(object).expect("the drawn figure is a path");
    assert!(path.is_closed(), "a polygon is a ring");
    assert_eq!(path.entries().len(), 6);
    let world = project.world_placement(object).expect("the polygon has a world frame");
    let vertices = ring_vertices(path, world);
    let corner = DVec2::new(100.0, 100.0);
    let start = nearest(&vertices, corner);
    let direction = DVec2::new(30f64.to_radians().cos(), 30f64.to_radians().sin());
    assert!((vertices[start] - corner).length() < 0.002, "the clicked corner is a vertex: {:?}", vertices[start]);
    let next = vertices.get(start + 1).copied().unwrap_or(vertices[0]);
    let edge = next - vertices[start];
    assert!((edge.length() - 100.0).abs() < 0.002, "the typed edge is one side: {edge:?}");
    assert!((edge.normalize() - direction).length() < 1e-4, "it runs the way the angle said: {edge:?}");
    // The rest of the ring is on the left of that edge, which is the winding the tool promises.
    let middle = (vertices[start] + next) / 2.0;
    let center = vertices.iter().fold(DVec2::ZERO, |sum, vertex| sum + *vertex) / vertices.len() as f64;
    assert!((center - middle).dot(DVec2::new(-direction.y, direction.x)) > 0.0, "the ring grows on the left");
}

#[test]
fn a_typed_height_builds_the_rectangle_on_its_edge() {
    // C5: a rectangle on an angled base edge. The edge is the width, so the third plate is the height
    // and nothing else — and it stands square to an edge that is not on an axis.
    let mut harness = Harness::new("frame-sketch-rectangle-edge");
    harness.settle();
    let before = harness.revision();
    harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::RectangleTilted);
    harness.settle();
    let scene = harness.scene();
    harness.hover(scene.center());
    type_text(&mut harness, "100");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "100");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    type_text(&mut harness, "200");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "30");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(draft_points(&harness).len(), 2, "a 200 mm edge at 30 degrees");

    harness.hover(scene.center() + egui::vec2(0.0, -120.0));
    let probe = placed_point(&harness);
    type_text(&mut harness, "60");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.drawing().is_none(), "Enter places the height and commits");

    let object = harness.selected().first().copied().expect("the rectangle is selected");
    let project = harness.app.tabs.active().session.project();
    let path = project.object_path(object).expect("the drawn figure is a path");
    assert_eq!(path.entries().len(), 4, "four sides");
    assert!(path.is_closed());
    let world = project.world_placement(object).expect("the rectangle has a world frame");
    let vertices = ring_vertices(path, world);
    let corner = DVec2::new(100.0, 100.0);
    let start = nearest(&vertices, corner);
    let direction = DVec2::new(30f64.to_radians().cos(), 30f64.to_radians().sin());
    let next = vertices.get(start + 1).copied().unwrap_or(vertices[0]);
    let edge = next - vertices[start];
    assert!((vertices[start] - corner).length() < 0.002, "the first corner is the click: {:?}", vertices[start]);
    assert!((edge.length() - 200.0).abs() < 0.002, "the typed edge is one side: {edge:?}");
    assert!((edge.normalize() - direction).length() < 1e-4, "it turns by the typed 30 degrees: {edge:?}");
    // The height is the pointer's side and the typed number: the side after the edge is square to it.
    let third = vertices.get(start + 2).copied().unwrap_or(vertices[0]);
    let rise = third - next;
    assert!((rise.length() - 60.0).abs() < 0.002, "the typed height is the next side: {rise:?}");
    assert!(rise.normalize().dot(direction).abs() < 1e-4, "square to the edge: {rise:?}");
    // The far corner is the point the plates showed before the Enter, on the side the pointer was on.
    let probe = DVec2::new(probe[0].millimetres_f64(), probe[1].millimetres_f64());
    let left = DVec2::new(-direction.y, direction.x);
    let side = |point: DVec2| (point - vertices[start]).dot(left);
    assert!(
        side(third) * side(probe) > 0.0,
        "the rectangle grows on the side the pointer was on: {third:?} against {probe:?}"
    );
    assert_eq!(harness.revision().get(), before.get() + 1, "one rectangle, one undo entry");
}

#[test]
fn the_side_count_plate_rebuilds_the_polygon() {
    // C5: the count is live at the cursor. A typed 8 is the draft's own number, so the ring previews and
    // commits as an octagon without a point being placed for it.
    let mut harness = Harness::new("frame-sketch-polygon-sides");
    harness.settle();
    let before = harness.revision();
    harness.app.tabs.active_mut().editor.start_drawing(ui::DrawTool::RegularPolygon);
    harness.settle();
    let scene = harness.scene();
    harness.hover(scene.center());
    type_text(&mut harness, "100");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "100");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(draft_points(&harness), vec![[mesh(100), mesh(100)]], "the centre");
    assert_eq!(harness.app.tabs.active().editor.drawing().map(|draft| draft.sides()), Some(6));

    // The plates of the shaping stage are the radius, the direction and the count.
    harness.hover(scene.center() + egui::vec2(150.0, 0.0));
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "8");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    assert_eq!(
        harness.app.tabs.active().editor.drawing().map(|draft| draft.sides()),
        Some(8),
        "the typed count reached the draft"
    );
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.drawing().is_none(), "Enter commits the octagon");

    let object = harness.selected().first().copied().expect("the polygon is selected");
    let project = harness.app.tabs.active().session.project();
    assert_eq!(project.object_path(object).expect("a path").entries().len(), 8, "eight sides, as typed");
    assert_eq!(harness.revision().get(), before.get() + 1, "one polygon, one undo entry");
}

/// The corners of a closed ring in the world frame, in travel order. The stored chain is local to the
/// object, so a test that names where a person clicked reads them through the world placement.
fn ring_vertices(path: &model::Path, world: model::Placement) -> Vec<DVec2> {
    path.entries()
        .iter()
        .filter_map(|entry| entry.segment().start().ok())
        .map(|point| {
            let at =
                world.transform_point(geom::DVec3::new(point[0].millimetres_f64(), point[1].millimetres_f64(), 0.0));
            DVec2::new(at.x, at.y)
        })
        .collect()
}

/// The index of the ring corner nearest `point`.
fn nearest(vertices: &[DVec2], point: DVec2) -> usize {
    let mut best = (usize::MAX, f64::INFINITY);
    for (index, vertex) in vertices.iter().enumerate() {
        let distance = (*vertex - point).length();
        if distance < best.1 {
            best = (index, distance);
        }
    }
    best.0
}

/// `value` whole millimetres, as the drawing plates name a length.
fn mesh(value: i64) -> Length {
    Length::from_millimetres(value).expect("whole millimetres fit a length")
}

/// A session whose root holds one open chain: (0,0) → (120,0) → (120,80) mm.
fn one_sketch_path() -> (engine::Session, Id) {
    let mm = testkit::millimetres;
    let mut project = testkit::sample_project();
    let root = project.root().id();
    let path = model::Path::new(
        vec![
            model::ContourSegment::line([mm(0), mm(0)], [mm(120), mm(0)]).expect("the first side"),
            model::ContourSegment::line([mm(120), mm(0)], [mm(120), mm(80)]).expect("the second side"),
        ],
        false,
    )
    .expect("an open chain");
    let object = project
        .add_object(root, "Sketch", model::Placement::IDENTITY, model::Geometry::Path(path))
        .expect("a path object");
    (engine::Session::with_directory(project, testkit::sample_catalog()), object)
}

#[test]
fn an_along_contour_drag_grows_the_side_across_frames() {
    // X96: with the card's *Along contour* switch on, a corner travels both ways along its segment.
    // It used to stop one micrometre inside its own end, so a ring could be shortened and never
    // lengthened; the pointer now walks past the corner along the segment's own line and the side
    // grows frame by frame.
    let (mut harness, object, corner) = sketching("frame-contour-along-grow");
    let ctx = harness.ctx.clone();
    harness
        .app
        .tabs
        .active_mut()
        .apply_tool_outcome(&ctx, ui::ToolOutcome { toggle: true, ..ui::ToolOutcome::default() });
    let start = node_point_mm(&harness, object, [0.0, 0.0]);
    let far = node_point_mm(&harness, object, [120.0, 0.0]);
    // The screen direction that leaves the corner away from the far end, so the side can only grow.
    let outward = (start - far).normalized();
    let first_frame = drafted_corner(&harness, corner);

    harness.press_at(start, egui::PointerButton::Primary);
    let mut corners = Vec::new();
    for step in 1..=6 {
        // The hand walks away along the side's own line and a little off it, so a free move would
        // carry the corner off that line and an along move cannot.
        let at = start + outward * (24.0 * step as f32) + egui::vec2(0.0, 6.0 * step as f32);
        harness.run(vec![egui::Event::PointerMoved(at)]);
        corners.push(drafted_corner(&harness, corner));
    }
    harness.release_at(start + outward * 144.0 + egui::vec2(0.0, 36.0), egui::PointerButton::Primary);
    harness.settle();

    let grown = drafted_corner(&harness, corner);
    assert_eq!(grown[1], Length::ZERO, "the corner stays on its segment's line: {grown:?}");
    assert!(grown[0] < Length::ZERO, "the corner passed its own end: {grown:?}");
    assert!(
        corners.windows(2).all(|pair| pair[1][0] <= pair[0][0]),
        "the side grows frame after frame instead of stopping: {corners:?}"
    );
    assert!(first_frame[0] >= Length::ZERO, "the corner starts at its own end: {first_frame:?}");
    assert!(corners.last().is_some_and(|last| last[0] < Length::ZERO), "the last frame is past the start: {corners:?}");
}

/// A headless frame in contour edit mode on the open chain, framed so its corners are grabbable.
///
/// The mode is entered the way the inspector card enters it, and the chain's first corner is the one
/// every latch test moves.
fn sketching(name: &str) -> (Harness, Id, model::PathCorner) {
    let (session, object) = one_sketch_path();
    let mut harness = Harness::with_session(name, session);
    harness.settle();
    let corner = {
        let document = harness.app.tabs.active();
        let path = document.session.project().object_path(object).expect("the path");
        model::PathCorner::Start(path.id_at(0).expect("the first segment"))
    };
    {
        let document = harness.app.tabs.active_mut();
        // These tests are about the gesture, not about the pointer's captures: the sample scene puts
        // a candidate a few millimetres from the points they drag to, so a snap would answer instead
        // of the pointer and the assertions below would measure the resolver, not the gesture (U119).
        let mode = document.snap_mode();
        document.set_snaps(crate::document::SnapChoice { kinds: model::SnapKinds::none(), mode });
        document.editor.select_node(Some(object));
        assert!(
            document.editor.start_contour_edit(document.session.project(), ui::ContourTarget::Path(object)),
            "an open chain is editable"
        );
    }
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    (harness, object, corner)
}

#[test]
fn a_contour_corner_answers_the_workspace_mode_and_the_marker_stands_on_the_point_it_takes() {
    // U119: the contour editor asks the one resolver, on the ring's own plane. The sample scene
    // offers a face centre whose foot on this plane is (60, 40); in 2.5D it captures the dragged
    // corner and the corner lands on that foot, and in 3D it stands off the plane and captures
    // nothing, so the corner keeps the pointer's own point. In both modes the marker is painted at
    // the corner the working copy holds, which is exactly the point the drop writes.
    for mode in [model::SnapMode::Foot, model::SnapMode::Space] {
        let name = match mode {
            model::SnapMode::Foot => "frame-contour-snap-foot",
            model::SnapMode::Space => "frame-contour-snap-space",
        };
        let (mut harness, object, corner) = sketching(name);
        {
            let document = harness.app.tabs.active_mut();
            document.set_snaps(crate::document::SnapChoice { kinds: model::SnapKinds::ALL, mode });
        }
        // The candidate is a face centre of the sample scene that stands above the ring's plane, and
        // the pointer goes to where the eye sees it: the plane is the question, not the screen.
        let marks = {
            let document = harness.app.tabs.active();
            document.session.project().snap_marks(document.editor.flags().sight())
        };
        let mark = marks
            .iter()
            .find(|mark| mark.kind == model::SnapKind::Centre && mark.point.z > 1.0)
            .expect("the sample scene has a face centre above the plane")
            .point;
        let at = gizmo_math::screen_point(harness.camera(), harness.scene(), mark).expect("on screen");
        let from = node_point_mm(&harness, object, [0.0, 0.0]);
        harness.click(from, egui::PointerButton::Primary);
        harness.click(from, egui::PointerButton::Primary);
        harness.hover(at);

        let drafted = drafted_corner(&harness, corner);
        let capture = harness.app.tabs.active().contour_gizmo.snap().map(|(_, kind)| kind);
        match mode {
            model::SnapMode::Foot => {
                assert!(
                    same_point(
                        drafted,
                        [
                            Length::from_micrometres((mark.x * 1_000.0).round() as i64),
                            Length::from_micrometres((mark.y * 1_000.0).round() as i64),
                        ]
                    ),
                    "2.5D lands the corner on the candidate's foot on the ring's plane: {drafted:?} from {mark:?}"
                );
                assert!(
                    capture.is_some_and(|kind| kind != model::SnapKind::Grid),
                    "and names the scene place that took it: {capture:?}"
                );
            }
            model::SnapMode::Space => {
                // 3D takes only a candidate that really lies in the ring's plane; the face centre the
                // pointer points at stands above it, so it never captures, and whatever does capture
                // is a candidate of the plane itself.
                let pointer = node_point_under(&harness, object, at);
                match capture {
                    None => assert!(
                        same_point(drafted, pointer),
                        "nothing captured, so the corner keeps the pointer's point: {drafted:?} against {pointer:?}"
                    ),
                    Some(kind) => {
                        let coplanar = marks.iter().any(|mark| {
                            mark.point.z.abs() <= 1.0e-3
                                && same_point(
                                    drafted,
                                    [
                                        Length::from_micrometres((mark.point.x * 1_000.0).round() as i64),
                                        Length::from_micrometres((mark.point.y * 1_000.0).round() as i64),
                                    ],
                                )
                        });
                        assert!(coplanar, "3D captures only a candidate in the plane: {drafted:?} ({kind:?})");
                    }
                }
            }
        }

        // The marker reads the working copy's corner, so the point it draws is the point the drop
        // writes: one place, and no second decision about where the marker stands.
        harness.click(at, egui::PointerButton::Primary);
        assert!(same_point(stored_corner(&harness, object, corner), drafted), "the drop writes the marker's point");
    }
}

/// The screen point of one point of a node's own plane, in millimetres.
fn node_point_mm(harness: &Harness, node: Id, local: [f64; 2]) -> egui::Pos2 {
    let document = harness.app.tabs.active();
    let world = document.session.project().world_placement(node).expect("the node has a world frame");
    let rect = document.scene_rect().expect("a settled frame gives the scene a rectangle");
    let at = geom::DVec3::new(local[0], local[1], 0.0);
    gizmo_math::screen_point(document.camera(), rect, world.transform_point(at)).expect("the point is on screen")
}

/// The point of a node's own plane under a viewport position, in the units the gizmo resolves.
fn node_point_under(harness: &Harness, node: Id, pointer: egui::Pos2) -> [Length; 2] {
    let document = harness.app.tabs.active();
    let project = document.session.project();
    let placement = project.world_placement(node).expect("the node has a world frame");
    let rect = document.scene_rect().expect("a settled frame gives the scene a rectangle");
    let ray = gizmo_math::ray_to(pointer, rect, document.camera());
    let origin = placement.transform_point(geom::DVec3::ZERO);
    let normal = placement.transform_vector(geom::DVec3::Z);
    let hit = gizmo_math::ray_plane(ray, origin, normal).expect("the pointer meets the node's plane");
    let local = placement.rotation().inverse() * (hit - origin);
    [
        Length::from_micrometres((local.x * 1_000.0).round() as i64),
        Length::from_micrometres((local.y * 1_000.0).round() as i64),
    ]
}

/// Whether two document points are the same within the one-micrometre joint tolerance.
fn same_point(first: [Length; 2], second: [Length; 2]) -> bool {
    first.iter().zip(second.iter()).all(|(a, b)| (a.micrometres() - b.micrometres()).abs() <= 1)
}

/// The corner of the live working copy.
fn drafted_corner(harness: &Harness, corner: model::PathCorner) -> [Length; 2] {
    harness
        .app
        .tabs
        .active()
        .editor
        .contour_edit()
        .expect("a live contour edit")
        .draft()
        .grip_position(ui::ContourGrip::Corner(corner))
        .expect("the corner is on the draft")
}

/// The corner the document stores.
fn stored_corner(harness: &Harness, object: Id, corner: model::PathCorner) -> [Length; 2] {
    let path = harness.app.tabs.active().session.project().object_path(object).expect("the path");
    ui::ContourDraft::Chain(path.clone())
        .grip_position(ui::ContourGrip::Corner(corner))
        .expect("the corner is on the stored path")
}

#[test]
fn a_latched_corner_move_writes_one_command_and_one_revision() {
    // C3: a click selects, a click on the selected corner latches the move, a ghost and the two
    // guides follow the pointer, and the next click drops it as one command (ADR 0129).
    let (mut harness, object, corner) = sketching("frame-contour-latch");
    let before = harness.revision();
    let from = node_point_mm(&harness, object, [0.0, 0.0]);
    let to = node_point_mm(&harness, object, [60.0, 45.0]);
    let scene = harness.scene();
    assert!(scene.contains(from) && scene.contains(to), "the whole move is on screen: {from:?} {to:?}");

    // The first click only selects the corner: a selection is view state.
    harness.click(from, egui::PointerButton::Primary);
    assert_eq!(harness.revision(), before, "selecting a corner writes nothing");
    assert!(harness.app.tabs.active().editor.contour_edit().is_some(), "the mode stays live");

    // The second click latches the move, and the pointer then carries the corner.
    harness.click(from, egui::PointerButton::Primary);
    assert_eq!(harness.revision(), before, "a latch writes nothing");
    harness.hover(to);
    assert_eq!(harness.revision(), before, "following the pointer writes nothing");
    let expected = node_point_under(&harness, object, to);
    let drafted = drafted_corner(&harness, corner);
    assert!(same_point(drafted, expected), "the working copy follows the pointer: {drafted:?} vs {expected:?}");

    // The next click drops it: one command, one revision, one undo entry.
    harness.click(to, egui::PointerButton::Primary);
    assert_eq!(harness.revision().get(), before.get() + 1, "the drop is one command");
    let stored = stored_corner(&harness, object, corner);
    assert!(same_point(stored, expected), "the document holds the dropped corner: {stored:?} vs {expected:?}");
    assert!(harness.app.tabs.active().can_undo(), "the drop is one undo entry");
    assert!(
        harness.app.tabs.active().editor.contour_edit().is_some(),
        "the mode keeps reshaping the contour, so the next corner takes one click to latch"
    );

    harness.app.tabs.active_mut().session.undo().expect("the drop undoes in one entry");
    let back = stored_corner(&harness, object, corner);
    assert!(same_point(back, [Length::ZERO, Length::ZERO]), "one undo takes the move back: {back:?}");
}

#[test]
fn a_drop_without_motion_writes_nothing() {
    // The latch promises a move; a click that names the corner's own position has nothing to write,
    // so the document keeps its revision and the mode keeps its clean draft.
    let (mut harness, object, corner) = sketching("frame-contour-latch-still");
    let before = harness.revision();
    let corner_at = node_point_mm(&harness, object, [0.0, 0.0]);
    harness.click(corner_at, egui::PointerButton::Primary);
    harness.click(corner_at, egui::PointerButton::Primary);
    assert!(harness.app.tabs.active().contour_gizmo.is_latched(), "the second click latched the move");

    harness.click(corner_at, egui::PointerButton::Primary);
    assert!(!harness.app.tabs.active().contour_gizmo.is_latched(), "the third click dropped it");
    assert_eq!(harness.revision(), before, "a drop that moved nothing writes nothing");
    assert!(same_point(drafted_corner(&harness, corner), [Length::ZERO, Length::ZERO]), "the corner stayed put");
    assert!(!harness.app.tabs.active().can_undo(), "and no undo entry was minted");
}

#[test]
fn escape_after_a_latch_puts_the_corner_back_and_leaves_the_document_untouched() {
    // Escape gives up the move before it gives up the mode: the corner returns to where it was
    // latched, the working copy is clean, and nothing was written.
    let (mut harness, object, corner) = sketching("frame-contour-latch-escape");
    let before = harness.revision();
    let from = node_point_mm(&harness, object, [0.0, 0.0]);
    let to = node_point_mm(&harness, object, [70.0, 50.0]);
    harness.click(from, egui::PointerButton::Primary);
    harness.click(from, egui::PointerButton::Primary);
    assert!(harness.app.tabs.active().contour_gizmo.is_latched(), "latched");
    harness.hover(to);
    assert!(harness.app.tabs.active().contour_gizmo.is_latched(), "the hover did not drop it");
    assert!(
        !same_point(drafted_corner(&harness, corner), [Length::ZERO, Length::ZERO]),
        "the corner moved: {:?}",
        drafted_corner(&harness, corner)
    );

    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert_eq!(harness.revision(), before, "Escape writes nothing");
    assert!(same_point(drafted_corner(&harness, corner), [Length::ZERO, Length::ZERO]), "the corner is back");
    assert!(!harness.app.tabs.active().contour_gizmo.is_latched(), "the latch is given up");
    assert!(harness.app.tabs.active().editor.contour_edit().is_some(), "the mode is still reshaping");
    assert!(!harness.app.tabs.active().can_undo(), "nothing was written to undo");
}

#[test]
fn a_press_and_drag_still_moves_a_corner_in_one_gesture() {
    // The latch adds a path, it does not replace the one-gesture move: a press that becomes a drag
    // takes the corner with it and arms no latch.
    let (mut harness, object, corner) = sketching("frame-contour-drag");
    let before = harness.revision();
    let from = node_point_mm(&harness, object, [0.0, 0.0]);
    let delta = egui::vec2(70.0, -40.0);
    harness.drag(from, delta, egui::PointerButton::Primary);
    assert_eq!(harness.revision(), before, "a drag edits the working copy, not the document");
    assert!(!harness.app.tabs.active().contour_gizmo.is_latched(), "a drag is not a latch");
    let expected = node_point_under(&harness, object, from + delta);
    let drafted = drafted_corner(&harness, corner);
    assert!(same_point(drafted, expected), "one gesture moves the corner: {drafted:?} vs {expected:?}");
    assert!(harness.app.tabs.active().editor.contour_edit().is_some(), "the mode is still live");
}

#[test]
fn a_typed_distance_and_angle_move_the_latched_corner() {
    // The two plates of a latched move are the delta of the move, so a corner moved to 50 mm at
    // 120° is typed, not aimed: the same grammar the point being placed has (ADR 0126, 0129).
    let (mut harness, object, corner) = sketching("frame-contour-latch-typed");
    let before = harness.revision();
    let from = node_point_mm(&harness, object, [0.0, 0.0]);
    harness.click(from, egui::PointerButton::Primary);
    harness.click(from, egui::PointerButton::Primary);
    assert!(harness.app.tabs.active().contour_gizmo.is_latched(), "the second click latched the move");

    type_text(&mut harness, "50");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    type_text(&mut harness, "120");
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);

    assert_eq!(harness.revision().get(), before.get() + 1, "Enter drops the move as one command");
    let stored = stored_corner(&harness, object, corner);
    let radians = 120f64.to_radians();
    let expected = [
        Length::from_micrometres((50.0 * radians.cos() * 1_000.0).round() as i64),
        Length::from_micrometres((50.0 * radians.sin() * 1_000.0).round() as i64),
    ];
    assert!(same_point(stored, expected), "the typed distance and angle moved the corner: {stored:?}");
}

/// Every drawing tool the Sketch tab ships, in the order the ribbon declares them.
fn sketch_tools() -> [DrawTool; 17] {
    [
        DrawTool::Line,
        DrawTool::LineByAngle,
        DrawTool::Polyline,
        DrawTool::Arc,
        DrawTool::ArcFromCenter,
        DrawTool::ArcByRadius,
        DrawTool::Rectangle,
        DrawTool::RectangleFromCenter,
        DrawTool::RectangleFromCenterDirected,
        DrawTool::RectangleTilted,
        DrawTool::Circle,
        DrawTool::CircleDiameter,
        DrawTool::CircleThreePoints,
        DrawTool::CircleTangent,
        DrawTool::RegularPolygon,
        DrawTool::PolygonApothem,
        DrawTool::PolygonByEdge,
    ]
}

/// The three rounds the sphere cell offers, which share the sketch's stages but not its intent.
fn sphere_tools() -> [DrawTool; 3] {
    [DrawTool::Sphere, DrawTool::SphereDiameter, DrawTool::SphereThreePoints]
}

/// Starts `tool` the way a ribbon cell does: a fresh draft and no pointer state from the old one.
fn arm_sketch(harness: &mut Harness, tool: DrawTool) {
    harness.app.tabs.active_mut().editor.start_drawing_with(tool, 6);
    harness.app.tabs.active_mut().draw_gizmo.reset();
}

/// The points a tool takes, chosen so every stage but the last can still be continued.
///
/// A second point of a rectangle must leave a height, while a third point of an arc must leave the
/// two before it non-collinear, so the sequence follows the count the tool declares.
fn stage_samples(tool: DrawTool) -> &'static [[i64; 2]] {
    match tool.points() {
        Some(3) => &[[0, 0], [200, 0], [200, 150]],
        // The corner, a point on each of its two lines, then a centre inside the wedge: every stage
        // of a tangent circle leaves the next one something to be measured against.
        Some(4) => &[[0, 0], [200, 0], [0, 200], [100, 100]],
        Some(_) => &[[0, 0], [200, 150]],
        None => &[[0, 0], [200, 150], [0, 200]],
    }
}

/// Places a point of a stage directly, so a test does not depend on where a click lands.
fn push_stage_point(harness: &mut Harness, point: [i64; 2]) {
    harness
        .app
        .tabs
        .active_mut()
        .editor
        .drawing_point([mesh(point[0]), mesh(point[1])])
        .expect("a valid point of the stage");
}

/// The screen positions a stage is read at: the middle, both diagonals and the four corners.
fn sketch_hovers(scene: egui::Rect) -> [egui::Pos2; 7] {
    let inset = egui::vec2(40.0, 40.0);
    [
        scene.center(),
        scene.center() + egui::vec2(scene.width() * 0.3, 0.0),
        scene.center() + egui::vec2(0.0, scene.height() * 0.3),
        scene.left_top() + inset,
        scene.right_top() + egui::vec2(-inset.x, inset.y),
        scene.left_bottom() + egui::vec2(inset.x, -inset.y),
        scene.right_bottom() - inset,
    ]
}

/// Every plate of the stage is drawn, stays inside the view and covers no other plate.
fn assert_plates_apart(plates: &[(FieldParam, egui::Rect)], scene: egui::Rect, context: &str) {
    assert!(!plates.is_empty(), "{context}: the stage shows its numbers");
    for (param, rect) in plates {
        assert!(rect.is_positive(), "{context}: {param:?} drew a plate: {rect:?}");
        assert!(scene.contains_rect(*rect), "{context}: {param:?} stays in the view: {rect:?}");
    }
    for (index, (param, rect)) in plates.iter().enumerate() {
        for (other, other_rect) in &plates[index + 1..] {
            assert!(!rect.intersects(*other_rect), "{context}: {param:?} covers {other:?}: {rect:?} vs {other_rect:?}");
        }
    }
}

/// No plate reaches the pointer.
///
/// A field that settles under the cursor is a field the pointer has to fight: the sketch stops
/// following the hand until the pointer leaves the plate again. The clearance the layout keeps is
/// measured here, one point inside it, so a plate that merely touches the rim still passes.
fn assert_plates_clear_of(plates: &[(FieldParam, egui::Rect)], pointer: egui::Pos2, context: &str) {
    for (param, rect) in plates {
        assert!(
            !rect.expand(ui::CURSOR_CLEARANCE - 1.0).contains(pointer),
            "{context}: {param:?} sits under the pointer: {rect:?} holds {pointer:?}"
        );
    }
}

#[test]
fn every_sketch_stage_draws_every_plate_apart() {
    // The reclamation behind X43: the first point showed one plate because X and Y shared an anchor,
    // and a length and its angle covered each other when their anchors came close. Every tool and
    // every stage it can measure is read at seven positions of the view; at each one the whole set
    // is on screen, no plate covers another, and the first point always shows both coordinates.
    let mut harness = Harness::new("frame-sketch-plates");
    harness.settle();
    let scene = harness.scene();
    let hovers = sketch_hovers(scene);
    let samples = [[0_i64, 0_i64], [200, 0], [200, 150]];
    // The sphere's three rounds are stages of the same table, so their plates are read here too.
    for tool in sketch_tools().into_iter().chain(sphere_tools()) {
        let need = tool.points().unwrap_or(3);
        for placed in 0..need {
            if tool.primitive_kind().is_some() {
                harness.app.tabs.active_mut().editor.start_primitive_drawing(tool).expect("a sphere round");
                harness.app.tabs.active_mut().draw_gizmo.reset();
            } else {
                arm_sketch(&mut harness, tool);
            }
            for point in &samples[..placed] {
                push_stage_point(&mut harness, *point);
            }
            for (index, hover) in hovers.iter().enumerate() {
                harness.hover(*hover);
                let plates = harness.app.tabs.active().draw_gizmo.plate_rects();
                let context = format!("{tool:?} after {placed} points at hover {index}");
                assert_plates_apart(&plates, scene, &context);
                assert_plates_clear_of(&plates, *hover, &context);
                if placed == 0 {
                    assert!(plates.iter().any(|(param, _)| *param == FieldParam::X), "{context}: X is drawn");
                    assert!(plates.iter().any(|(param, _)| *param == FieldParam::Y), "{context}: Y is drawn");
                }
            }
        }
    }

    // The polyline has a stage no other tool has: an arc armed at the chain's end, which the next
    // click finishes. It measures the same length and angle from the stored end and must lay out.
    arm_sketch(&mut harness, DrawTool::Polyline);
    push_stage_point(&mut harness, [0, 0]);
    push_stage_point(&mut harness, [200, 0]);
    harness.app.tabs.active_mut().editor.set_drawing_arc_mode(true);
    harness.app.tabs.active_mut().editor.drawing_arc_through([mesh(200), mesh(150)]).expect("an arc is armed");
    for (index, hover) in hovers.iter().enumerate() {
        harness.hover(*hover);
        let plates = harness.app.tabs.active().draw_gizmo.plate_rects();
        let context = format!("a pending arc at hover {index}");
        assert_plates_apart(&plates, scene, &context);
        assert_plates_clear_of(&plates, *hover, &context);
        assert!(plates.iter().any(|(param, _)| *param == FieldParam::Length), "{context}: the chord is read");
        assert!(plates.iter().any(|(param, _)| *param == FieldParam::Angle), "{context}: the angle is read");
    }
}

#[test]
fn switching_a_sketch_tool_mid_draft_starts_a_clean_stage() {
    // A tool change is one gesture, not "cancel, then start" (ADR 0076, X12): the draft, the arc
    // mode, the typed locks and the plates of the old tool go with it, and the new tool's first
    // point is its own X and Y. Every pair of neighbouring tools is switched after zero, one and
    // two points, and the pending arc and the panel intent are checked on their own.
    let mut harness = Harness::new("frame-sketch-switch");
    harness.settle();
    let scene = harness.scene();
    let hover = scene.center() + egui::vec2(32.0, 24.0);
    let tools = sketch_tools();
    for (index, from) in tools.iter().enumerate() {
        let to = tools[(index + 1) % tools.len()];
        let need = from.points().unwrap_or(2);
        let samples = stage_samples(*from);
        for placed in 0..=need {
            arm_sketch(&mut harness, *from);
            for point in &samples[..placed] {
                push_stage_point(&mut harness, *point);
            }
            arm_sketch(&mut harness, to);
            harness.hover(hover);
            let context = format!("{from:?} with {placed} points switched to {to:?}");
            let draft = harness.app.tabs.active().editor.drawing().expect("the new tool runs");
            assert_eq!(draft.tool(), to, "{context}: the new tool owns the draft");
            assert!(draft.points().is_empty(), "{context}: no point of the old draft survives");
            assert!(!draft.arc_mode(), "{context}: the old arc mode is gone");
            assert!(draft.pending_arc().is_none(), "{context}: the old arc is gone");
            assert_eq!(harness.app.tabs.active().editor.drawing_intent(), DrawIntent::Path, "{context}: a sketch");
            let plates = harness.app.tabs.active().draw_gizmo.plate_rects();
            assert_plates_apart(&plates, scene, &context);
            assert!(plates.iter().any(|(param, _)| *param == FieldParam::X), "{context}: X is the new point");
            assert!(plates.iter().any(|(param, _)| *param == FieldParam::Y), "{context}: Y is the new point");
        }
    }

    // A tool change that bypasses the shell's pointer reset still rebuilds the plates: a typed lock
    // promises one number of one point of one tool, so the new tool's first point is its own X and Y.
    arm_sketch(&mut harness, DrawTool::Circle);
    harness.hover(hover);
    let expected = harness.app.tabs.active().draw_gizmo.point_preview().expect("the circle's first point");
    arm_sketch(&mut harness, DrawTool::Line);
    harness.hover(hover);
    type_text(&mut harness, "999");
    harness.press(egui::Key::Tab, egui::Modifiers::NONE);
    harness.app.tabs.active_mut().editor.start_drawing_with(DrawTool::Circle, 6);
    harness.hover(hover);
    assert!(
        harness.app.tabs.active().editor.drawing().is_some_and(|draft| draft.tool() == DrawTool::Circle),
        "the circle owns the draft"
    );
    let previewed = harness.app.tabs.active().draw_gizmo.point_preview().expect("the circle's first point");
    assert_eq!(previewed, expected, "the line's typed X does not reach the circle's first point");

    // Choosing the same cell again restarts it, so a double click does not keep a half figure.
    arm_sketch(&mut harness, DrawTool::Line);
    push_stage_point(&mut harness, [0, 0]);
    arm_sketch(&mut harness, DrawTool::Line);
    harness.hover(hover);
    assert!(
        harness.app.tabs.active().editor.drawing().is_some_and(|draft| draft.points().is_empty()),
        "the same tool starts from its first point"
    );

    // A pending arc is part of the old draft and cannot leak into the new tool.
    arm_sketch(&mut harness, DrawTool::Polyline);
    push_stage_point(&mut harness, [0, 0]);
    push_stage_point(&mut harness, [200, 0]);
    harness.app.tabs.active_mut().editor.set_drawing_arc_mode(true);
    harness.app.tabs.active_mut().editor.drawing_arc_through([mesh(200), mesh(150)]).expect("an arc is armed");
    assert!(harness.app.tabs.active().editor.drawing().is_some_and(|draft| draft.pending_arc().is_some()));
    arm_sketch(&mut harness, DrawTool::Circle);
    harness.hover(hover);
    let draft = harness.app.tabs.active().editor.drawing().expect("the circle runs");
    assert_eq!(draft.tool(), DrawTool::Circle, "the circle replaced the chain");
    assert!(draft.pending_arc().is_none(), "the armed arc went with the chain");
    assert!(!draft.arc_mode(), "a circle has no arc mode");

    // The panel-from-contour intent is the draft's, so a sketch cell after it is a path again.
    harness.app.tabs.active_mut().editor.start_panel_drawing();
    push_stage_point(&mut harness, [0, 0]);
    push_stage_point(&mut harness, [200, 0]);
    assert_eq!(harness.app.tabs.active().editor.drawing_intent(), DrawIntent::Panel);
    arm_sketch(&mut harness, DrawTool::Circle);
    assert_eq!(
        harness.app.tabs.active().editor.drawing_intent(),
        DrawIntent::Path,
        "a sketch cell never inherits the board intent"
    );

    // Another pointer mode leaves the drawing and its plates; a later sketch starts clean.
    arm_sketch(&mut harness, DrawTool::Rectangle);
    push_stage_point(&mut harness, [0, 0]);
    harness.hover(hover);
    assert!(!harness.app.tabs.active().draw_gizmo.plate_rects().is_empty(), "the rectangle shows its plates");
    harness.app.tabs.active_mut().editor.arm_measure();
    harness.hover(hover);
    assert!(harness.app.tabs.active().editor.drawing().is_none(), "the ruler left the drawing");
    assert!(harness.app.tabs.active().draw_gizmo.plate_rects().is_empty(), "and its plates went with it");
    arm_sketch(&mut harness, DrawTool::Arc);
    harness.hover(hover);
    let plates = harness.app.tabs.active().draw_gizmo.plate_rects();
    assert_plates_apart(&plates, scene, "an arc after the ruler");
    assert!(plates.iter().any(|(param, _)| *param == FieldParam::X), "the arc starts at a point");
    assert!(plates.iter().any(|(param, _)| *param == FieldParam::Y), "the arc starts at a point");
}

/// The screen rectangle the frame's whole model covers, projected from its own vertices.
///
/// A test aims a band at the model the way the person does — by looking at it — instead of repeating
/// where the camera put it.
fn model_screen_rect(harness: &Harness) -> egui::Rect {
    let document = harness.app.tabs.active();
    let frame = document.frame();
    let scene = harness.scene();
    let mut rect = egui::Rect::NOTHING;
    for drawable in frame.drawables() {
        for position in drawable.mesh.positions.iter() {
            let world = drawable.world_transform.transform_point3(geom::DVec3::new(
                f64::from(position[0]),
                f64::from(position[1]),
                f64::from(position[2]),
            ));
            if let Some(point) = gizmo_math::screen_point(frame.camera, scene, world) {
                rect = rect.union(egui::Rect::from_min_max(point, point));
            }
        }
    }
    rect
}

/// A point of the scene with nothing drawn under it and no chrome over it.
///
/// The sample scene is fitted into the view with a margin, so the points just outside the model's
/// own screen rectangle are empty space; which of the four the floating panels leave free is the
/// frame's business, so the search asks the chrome instead of repeating a layout.
fn free_scene_point(harness: &mut Harness, model: egui::Rect) -> egui::Pos2 {
    let chrome = harness.settle();
    let scene = harness.scene();
    let offset = 40.0;
    let candidates = [
        egui::pos2(model.left() - offset, model.bottom() + offset),
        egui::pos2(model.right() + offset, model.bottom() + offset),
        egui::pos2(model.left() - offset, model.top() - offset),
        egui::pos2(model.right() + offset, model.top() - offset),
    ];
    for candidate in candidates {
        if !scene.contains(candidate) || chrome.covers(candidate) {
            continue;
        }
        let document = harness.app.tabs.active();
        if crate::mesh_pick::pick_surface(scene, document.frame(), candidate).is_none() {
            return candidate;
        }
    }
    panic!("the sample scene leaves no empty corner")
}

#[test]
fn a_band_is_drawn_only_while_the_pointer_drags_on_empty_space() {
    // U103: a press on the scene with nothing under it can become a rubber band, and nothing else
    // can. The band is drawn between the press and the release and never after it, a press on the
    // model is still a pick, and the release is what selects.
    let mut harness = Harness::new("frame-rubber-band");
    harness.settle();
    let model = model_screen_rect(&harness);
    let empty = free_scene_point(&mut harness, model);
    // The corner holds no shape, so a click there is the plain click that takes nothing.
    harness.click(empty, egui::PointerButton::Primary);
    assert!(harness.selected().is_empty(), "the empty corner takes nothing");
    assert!(harness.app.tabs.active().rubber_band().is_none(), "a click leaves no band behind");

    // A press on the model picks it, and the drag that follows draws no band.
    harness.click(harness.scene().center(), egui::PointerButton::Primary);
    let picked = harness.selected();
    assert_eq!(picked.len(), 1, "the middle of the scene holds one object");
    let on_model = harness.node_point(picked[0]);
    harness.press_at(on_model, egui::PointerButton::Primary);
    harness.run(vec![egui::Event::PointerMoved(on_model + egui::vec2(40.0, 20.0))]);
    assert!(harness.app.tabs.active().rubber_band().is_none(), "a drag from the model is not a band");
    harness.release_at(on_model + egui::vec2(40.0, 20.0), egui::PointerButton::Primary);
    harness.settle();

    // A drag on empty space draws the band around the whole model, and the release selects with it.
    let model = model_screen_rect(&harness);
    let from = egui::pos2(model.left() - 60.0, model.bottom() + 60.0);
    let to = egui::pos2(model.right() + 60.0, model.top() - 60.0);
    let chrome = harness.settle();
    assert!(!chrome.covers(from) && !chrome.covers(to), "both corners of the band are the scene");
    let revision = harness.revision();
    harness.press_at(from, egui::PointerButton::Primary);
    harness.run(vec![egui::Event::PointerMoved(to)]);
    let band = harness.app.tabs.active().rubber_band().expect("the drag draws the band");
    assert_eq!(band.from, from, "the band starts where the press landed");
    assert_eq!(band.to, to, "and follows the pointer");
    assert!(harness.selected().is_empty() || harness.selected() == picked, "drawing a band selects nothing yet");
    harness.release_at(to, egui::PointerButton::Primary);
    harness.settle();
    assert!(harness.app.tabs.active().rubber_band().is_none(), "the released band is gone");
    let taken = harness.selected();
    assert!(taken.contains(&picked[0]), "a window around the model takes what a click on it takes");
    assert!(taken.len() > 1, "and the sample scene gives the band several objects");
    assert_eq!(harness.revision(), revision, "selecting is view state, never a document edit");

    // Escape drops a live band: the release that follows selects nothing and closes no level.
    let level = harness.selected();
    harness.press_at(from, egui::PointerButton::Primary);
    harness.run(vec![egui::Event::PointerMoved(to)]);
    assert!(harness.app.tabs.active().rubber_band().is_some(), "the band is live before the key");
    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().rubber_band().is_none(), "Escape drops the band");
    harness.release_at(to, egui::PointerButton::Primary);
    harness.settle();
    assert_eq!(harness.selected(), level, "the dropped band selects nothing and changes no level");
}

#[test]
fn a_board_changed_in_the_directory_reaches_the_open_scene() {
    // The directory is edited in its own application, so the window has to apply a new revision by
    // itself: the panel is cut from the new board, the card names it, and the history is the one the
    // person left, because a directory is not the document (ADR 0148, `nomenclature.md` §13, O5).
    let size = model::PanelNominalSize::new(testkit::millimetres(600), testkit::millimetres(400)).expect("a panel");
    let root = model::Container::new(Id::new(1).expect("id"), "Root", model::Placement::IDENTITY, Vec::new());
    let mut project = model::Project::new(root).expect("a project");
    let root = project.root().id();
    let panel = project
        .add_panel(
            root,
            "Side",
            size,
            Some(testkit::sample_material_id()),
            testkit::millimetres(18),
            model::Placement::IDENTITY,
        )
        .expect("a panel cut from the sample board");
    let mut session = engine::Session::with_directory(project, testkit::sample_catalog());
    session
        .dispatch("panel.rename", &serde_json::json!({ "panel": panel, "name": "Side 2" }))
        .expect("one edit, so the history has something to lose");
    let mut harness = Harness::with_session("frame-directory-row", session);
    harness.app.tabs.active_mut().editor.select_node(Some(panel));
    harness.settle();
    assert!(harness.drew("Chipboard"), "the card names the board the panel is cut from");
    assert_eq!(harness.thickness(panel), testkit::millimetres(18), "and the panel is 18 mm thick");

    // Another application writes the workshop's folder while the window is open.
    let folder =
        harness.path.parent().expect("the settings file has a folder").join(crate::directory_watch::DIRECTORY_DIR);
    crate::test_support::write_sample_folder(&folder, testkit::millimetres(25), "Graphite oak");
    let board = testkit::sample_material_id();
    let reloaded = harness
        .until(|app| app.tabs.active().session.directory().board(board).is_some_and(|row| row.name == "Graphite oak"));
    assert!(reloaded, "the window reads the new rows without a restart");

    harness.settle();
    assert!(harness.drew("Graphite oak"), "the card names the board the directory now holds");
    assert_eq!(harness.thickness(panel), testkit::millimetres(25), "and the panel is cut again, from it");
    assert!(harness.app.tabs.active().can_undo(), "the history is the one the person left");
    harness.app.tabs.active_mut().undo();
    assert_eq!(
        harness.app.tabs.active().session.project().panel(panel).expect("panel").name(),
        "Side",
        "and it takes back the person's edit, not the directory's"
    );
}

#[test]
fn a_carried_piece_gives_its_hand_back_when_the_mouse_leaves_the_work_area() {
    // X70's follow-up: while the carry's hand is displaced the square is not where the system pointer
    // is, and everything hit-tested at the system pointer would disagree with what the maker sees.
    // The wheel works under the hand all the same, and leaving the work area hands the pointer back.
    let (session, panel) = one_wide_panel();
    let mut harness = Harness::with_session("frame-hand-leaves", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    // The carry is the hand's: the workspace's snaps would catch the piece at round millimetres and
    // the test is about the hand, not about them (U116).
    harness.app.tabs.active_mut().set_snap_kind(SnapKind::Centre, false);
    harness.settle();

    let body = harness.node_point(panel);
    harness.click(body, egui::PointerButton::Primary);
    let at = body + egui::vec2(40.0, -60.0);
    harness.hover(at);
    assert!(harness.app.tabs.active().holding_piece(), "the panel is in hand");
    let carried = harness.app.tabs.active().carried_delta();

    // A pan puts the hand on the piece; it is not where the system pointer is any more.
    let pan = egui::vec2(300.0, 20.0);
    harness.drag(at, pan, egui::PointerButton::Secondary);
    harness.run(Vec::new());
    assert!(harness.app.tabs.active().carry_hand().is_some(), "the hand came back to the piece");

    // The wheel still zooms under that hand, and it zooms about it.
    let camera = harness.camera();
    let wheel = |points: f32| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, points),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
    };
    harness.run(vec![wheel(-60.0)]);
    harness.settle();
    assert_ne!(harness.camera(), camera, "the wheel zooms with a carried piece in hand");
    let kept = harness.app.tabs.active().carried_delta();
    assert!((kept - carried).length() < 1.0e-3, "and the piece keeps its gaps: {kept:?} from {carried:?}");

    // The mouse leaves the work area for a panel: the hand goes back to the pointer, and the piece
    // stays where it is rather than jumping to it.
    let chrome = harness.settle();
    // Any point outside the work area will do: the ribbon's own band is one, and it is always up.
    let panel_row = chrome.panels.left.map_or_else(|| egui::pos2(60.0, 60.0), |tree| tree.center());
    let panel_row = if harness.scene().contains(panel_row) { egui::pos2(60.0, 60.0) } else { panel_row };
    harness.hover(panel_row);
    assert!(!harness.scene().contains(panel_row), "the tree is outside the work area");
    assert!(harness.app.tabs.active().carry_hand().is_none(), "the hand is the pointer again");
    let kept = harness.app.tabs.active().carried_delta();
    assert!((kept - carried).length() < 1.0e-3, "the piece did not jump to it: {kept:?}");
}

#[test]
fn a_view_preset_and_home_leave_the_piece_where_it_stands() {
    // X67: the owner moved a panel, switched the view to *Front* and then pressed Home, and the panel
    // was nowhere near the pointer. A view preset is the eye's, not the hand's: the panel keeps its
    // distance to its neighbour, and the pointer is what comes back onto the panel (ADR 0156).
    let (project, product, panel) = product_with_a_side_panel();
    let mut session = engine::Session::with_directory(project, testkit::sample_catalog());
    assert!(session.open_product(product), "the fixture product opens");
    let mut harness = Harness::with_session("frame-view-preset-carry", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    // The carry is the hand's: the workspace's snaps would catch the piece at round millimetres and
    // the test is about the hand, not about them (U116).
    harness.app.tabs.active_mut().set_snap_kind(SnapKind::Centre, false);
    harness.settle();

    let body = harness.node_point(panel);
    harness.click(body, egui::PointerButton::Primary);
    let at = body + egui::vec2(70.0, -90.0);
    harness.hover(at);
    assert!(harness.app.tabs.active().holding_piece(), "the board is in hand");
    let carried = harness.app.tabs.active().carried_delta();
    let placed = harness.app.tabs.active().session.project().world_placement(panel).expect("a world frame");

    for name in ["Front", "Home"] {
        if name == "Front" {
            harness.app.tabs.active_mut().apply_view_preset(ui::display::OpeningView::Front);
        } else {
            harness.app.tabs.active_mut().home_view();
        }
        harness.settle();
        assert!(harness.app.tabs.active().holding_piece(), "{name} does not end the carry");
        let kept = harness.app.tabs.active().carried_delta();
        assert!((kept - carried).length() < 1.0e-3, "{name} moves no piece: {kept:?} from {carried:?}");
        assert_eq!(
            harness.app.tabs.active().session.project().world_placement(panel),
            Some(placed),
            "{name} leaves the board where it stands in the room"
        );

        // The hand comes back onto the piece, which is what the maker sees as "still in hand".
        harness.run(Vec::new());
        let hand =
            harness.app.tabs.active().carry_hand().unwrap_or_else(|| panic!("the carry has its own hand after {name}"));
        let back = hand - piece_middle(&harness, panel);
        assert!(back.length() < 3.0, "the hand is back in the middle of the board after {name}: {back:?}");
    }

    assert!(harness.app.tabs.active().holding_piece(), "the piece is still in hand");
}

#[test]
fn a_pan_under_a_carried_piece_leaves_the_piece_where_it_was() {
    // X67, the owner's own sequence: take a panel, pan the view a long way with the right button, and
    // come back to the panel. The pan must not move the panel — its distance to its neighbour is a
    // document value and a view gesture writes nothing — so the *pointer* is what comes back to the
    // piece, on the frame the gesture ends (ADR 0156).
    let (session, panel) = one_wide_panel();
    let mut harness = Harness::with_session("frame-pan-carry", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    harness.settle();

    let screen = |harness: &Harness| {
        let project = harness.app.tabs.active().session.project();
        let moved = project.pivot_world(panel).expect("a pivot") + harness.app.tabs.active().carried_delta();
        gizmo_math::screen_point(harness.camera(), harness.scene(), moved).expect("the piece is on screen")
    };
    let body = harness.node_point(panel);
    harness.click(body, egui::PointerButton::Primary);
    let at = body + egui::vec2(40.0, -60.0);
    harness.hover(at);
    assert!(harness.app.tabs.active().holding_piece(), "the panel is in hand");
    let carried = harness.app.tabs.active().carried_delta();
    let placed = harness.app.tabs.active().session.project().world_placement(panel).expect("a world frame");

    // A long sideways pan with the right button: the view slides under the hand, the piece does not.
    let pan = egui::vec2(420.0, 30.0);
    harness.press_at(at, egui::PointerButton::Secondary);
    for step in 1..=8 {
        harness.run(vec![egui::Event::PointerMoved(at + pan * (step as f32 / 8.0))]);
        // While the gesture runs the pointer is hidden: a cursor standing still while the world
        // slides under it is the picture of a piece being left behind, and it is not what happens.
        if step == 4 {
            // The work area draws its own square, so hiding the pointer is not drawing it: a square
            // standing still over the scene sliding under it is the picture of a piece left behind.
            assert!(harness.app.tabs.active().hides_pointer(), "the pointer is hidden for the pan");
            assert!(harness.app.shell.work_cursor.is_none(), "and its square is not drawn");
            assert_eq!(harness.cursor, egui::CursorIcon::None, "and no system arrow shows in its place");
        }
    }
    harness.release_at(at + pan, egui::PointerButton::Secondary);
    harness.run(Vec::new());
    assert!(harness.app.tabs.active().holding_piece(), "the pan leaves the move mode alone");
    let after_pan = harness.app.tabs.active().carried_delta();
    assert!((after_pan - carried).length() < 1.0e-3, "the pan carried nothing: {after_pan:?} from {carried:?}");
    assert_eq!(
        harness.app.tabs.active().session.project().world_placement(panel),
        Some(placed),
        "and it moved the panel nowhere"
    );
    // The pan slides the world under the pointer, so the piece slides on screen with it; what the
    // pan must not do is move the piece in the room, which the two lines above check.
    let apart = (screen(&harness) - (at + pan)).length();
    assert!(apart.is_finite(), "the piece is still on screen: {apart}");

    // The gesture is over, so the carry puts its own hand back on the piece: the square is drawn in
    // the middle of the board, which is where a maker reaches for it, and the piece has not moved.
    harness.run(Vec::new());
    assert!(!harness.app.tabs.active().hides_pointer(), "the pointer comes back when the gesture ends");
    assert!(harness.app.tabs.active().holding_piece(), "the panel is still in hand");
    let after_home = harness.app.tabs.active().carried_delta();
    assert!((after_home - carried).length() < 1.0e-3, "and has not moved: {after_home:?}");
    let middle = piece_middle(&harness, panel);
    let hand = harness.app.tabs.active().carry_hand().expect("the carry has its own hand after a pan");
    let back = hand - middle;
    assert!(back.length() < 3.0, "the hand is back in the middle of the board: {back:?} from {middle:?}");

    // Carrying on from there: the mouse moves the hand by its own movement, and the hand carries the
    // piece 1:1 — the pointer never jumped, so the piece does not either.
    let before = harness.app.tabs.active().carried_delta();
    harness.hover(at + pan + egui::vec2(0.0, -30.0));
    let carried_on = (harness.app.tabs.active().carried_delta() - before).length();
    assert!(carried_on > 1.0, "the mouse carries the piece again from the hand: {carried_on} mm");
}

/// The empty-space ghost passes the board its ray meets and holds at the frame's envelope (N23).
///
/// The point under the pointer cannot read the hand's place on a face square to the free axis — a
/// shelf's top names one height for the whole face — so the tool carries the part from the place
/// that face named and moves it with every millimetre the hand goes. A pointer that leaves the
/// piece keeps the ghost, so the click still creates what was drawn.
#[test]
fn the_fill_ghost_passes_the_board_it_meets_and_holds_at_the_frame_envelope() {
    // An empty scene: the sample session's loose parts stand inside a frame created at the origin,
    // and a void holding a part is refused by the niche (ADR 0106).
    let mut harness = Harness::with_session("frame-fill-passes", crate::scene_build::empty_session().expect("empty"));
    harness.settle();
    harness.app.product_card.start();
    harness.settle();
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    let (product, rect, camera) = {
        let document = harness.app.tabs.active_mut();
        let product = document.session.active_product().expect("the card opens the frame");
        // A shelf 350 mm above the frame's floor, spanning the whole footprint the card made.
        let nominal =
            model::PanelNominalSize::new(testkit::millimetres(600), testkit::millimetres(560)).expect("a shelf size");
        let placement = model::Placement::IDENTITY
            .with_translation(Length::ZERO, Length::ZERO, testkit::millimetres(350))
            .expect("a placement");
        document
            .editor
            .create_panel_sized(
                &mut document.session,
                product,
                "Shelf".to_owned(),
                nominal,
                None,
                Vec::new(),
                placement,
                Some(ClassCode::HORIZONTAL_PANEL),
            )
            .expect("the shelf lands in the open frame");
        document
            .editor
            .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, model::FillMode::Fit, Lang::En)
            .expect("the shelf fill arms inside the open frame");
        (product, document.scene_rect().expect("a scene rectangle"), document.camera())
    };
    let pointer_at = |world: geom::DVec3| {
        let pointer = gizmo_math::screen_point(camera, rect, world).expect("the frame is on screen");
        assert!(rect.contains(pointer), "{pointer:?} is on the scene");
        pointer
    };
    let ghost = |harness: &Harness| harness.app.tabs.active().fill_gizmo.ghost().cloned();

    // Under the shelf the ray meets the frame's near wall, which runs along the height, so the point
    // itself reads the hand: the part stands where the pointer is.
    harness.hover(pointer_at(geom::DVec3::new(300.0, 520.0, 250.0)));
    let below = ghost(&harness).expect("the void under the shelf takes a ghost").free_low;
    assert!((120.0..350.0).contains(&below), "the part stands under the shelf where the hand is: {below}");

    // Over the shelf's own top face the point names one height for the whole face; the hand carries
    // the part past the board instead of sticking it there.
    harness.hover(pointer_at(geom::DVec3::new(300.0, 500.0, 368.0)));
    let near = ghost(&harness).expect("the cell above the shelf takes a ghost").free_low;
    assert!(near >= 367.5, "the part jumps into the cell above the shelf: {near}");
    harness.hover(pointer_at(geom::DVec3::new(300.0, 260.0, 368.0)));
    let far = ghost(&harness).expect("the ghost stays in the cell above the shelf").free_low;
    assert!(far > near + 50.0, "the part passes the board as the hand does: {near} then {far}");
    let carried = ghost(&harness).expect("the ghost holds a measurement");

    // A pointer past the frame's envelope names no cell: the ghost holds the last part it could
    // place, and the click writes exactly that part.
    let model = model_screen_rect(&harness);
    let outside = free_scene_point(&mut harness, model);
    harness.hover(outside);
    assert_eq!(ghost(&harness).map(|ghost| ghost.placement), Some(carried.placement), "the ghost holds");
    let before = harness.revision();
    harness.click(outside, egui::PointerButton::Primary);

    let document = harness.app.tabs.active();
    let created = document.editor.selected_node().expect("the click created a part");
    let panel = document.session.project().panel(created).expect("it is a panel");
    assert_eq!(panel.placement(), carried.placement, "the click writes the ghost that was drawn");
    assert_eq!(panel.nominal_size(), carried.nominal_size, "and its size");
    assert_eq!(document.session.project().parent_of(created), Some(product), "it lands in the open frame");
    assert!(harness.revision() > before, "the click is the one write of the gesture");

    let document = harness.app.tabs.active_mut();
    document.session.undo().expect("one undo step");
    assert!(document.session.project().panel(created).is_none(), "one undo removes the part");
}

/// The screen places the eight corners of `node`'s world box would take, plus `delta`.
///
/// The bit order is the one the brackets promise: bit 0 picks the `x` maximum, bit 1 the `y` and bit
/// 2 the `z`, so a box's own bracket and its corner are read in one order in a test and in the shell.
fn box_corners(harness: &Harness, node: Id, delta: geom::DVec3) -> Vec<egui::Pos2> {
    let (low, high) = harness.app.tabs.active().session.project().world_box(node).expect("the node has a box");
    let camera = harness.camera();
    let scene = harness.scene();
    (0..8)
        .filter_map(|index| {
            let pick = |bit: usize, low: f64, high: f64| if index & bit == 0 { low } else { high };
            let corner = geom::DVec3::new(pick(1, low.x, high.x), pick(2, low.y, high.y), pick(4, low.z, high.z));
            gizmo_math::screen_point(camera, scene, corner + delta)
        })
        .collect()
}

/// The screen travel a carried piece has shown, measured from the pivot at the take.
fn carried_screen(harness: &Harness, pivot: geom::DVec3) -> egui::Vec2 {
    let delta = harness.app.tabs.active().carried_delta();
    let at =
        gizmo_math::screen_point(harness.camera(), harness.scene(), pivot + delta).expect("the pivot is on screen");
    at - gizmo_math::screen_point(harness.camera(), harness.scene(), pivot).expect("the pivot was on screen")
}

#[test]
fn a_floor_frame_keeps_the_hand_on_both_floor_directions() {
    // X75: the owner's report. A floor frame was sluggish under the hand — the cursor ran away from
    // it — because a multi-axis carry met the pointer on the camera's plane and the rule then kept
    // only the horizontal part of that delta: 120 points across the screen and 80 up moved the piece
    // 113 and 21. The plane a carried piece is met on is now the one its class allows (ADR 0164), so
    // the piece keeps the screen offset the click gave it on both directions of the floor.
    let (session, frame) = one_floor_frame("10.01.01");
    let mut harness = Harness::with_session("frame-carry-under-hand", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    harness.settle();

    let pivot = harness.app.tabs.active().session.project().pivot_world(frame).expect("a pivot");
    let body = piece_middle(&harness, frame);
    harness.click(body, egui::PointerButton::Primary);
    // The hand travels in steps, the way a person's does, and the piece keeps it at every one.
    let mut travel = egui::Vec2::ZERO;
    for step in [egui::vec2(40.0, -30.0), egui::vec2(60.0, 20.0), egui::vec2(-25.0, -45.0)] {
        travel += step;
        harness.hover(body + travel);
        let shown = carried_screen(&harness, pivot);
        assert!((shown - travel).length() < 1.5, "the frame keeps the hand at {travel:?}: it moved {shown:?}");
        let height = harness.app.tabs.active().session.project().pivot_world(frame).expect("a pivot").z;
        assert!(height.abs() < 1.0e-6, "and its class keeps it on the floor: {height}");
    }
    assert!(harness.app.tabs.active().holding_piece(), "the frame is still in hand");
    assert!(
        (harness.app.tabs.active().session.project().revision()).get() == harness.revision().get(),
        "a carry writes nothing before the click that puts it down"
    );
}

#[test]
fn a_frames_brackets_travel_with_the_piece_in_hand() {
    // X75: the level that is open brackets its box from the frame the document still holds, so a
    // carried frame left its eight corner marks at the old place. The overlay reads the drag's
    // preview now, as the renderer does, and the marks come back with the piece on Escape.
    let (mut session, frame) = one_floor_frame("10.01.01");
    assert!(session.open_product(frame), "the frame opens");
    let mut harness = Harness::with_session("frame-carry-brackets", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    harness.settle();
    assert_eq!(harness.app.tabs.active().brackets.len(), 8, "the open frame brackets its authored box");

    let body = piece_middle(&harness, frame);
    harness.click(body, egui::PointerButton::Primary);
    harness.hover(body + egui::vec2(90.0, -60.0));
    let delta = harness.app.tabs.active().carried_delta();
    assert!(delta.length() > 1.0, "the frame is in hand and moved: {delta:?}");

    let moved = box_corners(&harness, frame, delta);
    let brackets = harness.app.tabs.active().brackets.clone();
    assert_eq!(brackets.len(), moved.len(), "every corner of the box keeps its bracket");
    for (bracket, corner) in brackets.iter().zip(&moved) {
        assert!(
            (bracket.corner - *corner).length() < 0.5,
            "a bracket sits on the carried corner: {:?} vs {corner:?}",
            bracket.corner
        );
    }

    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
    let home = box_corners(&harness, frame, geom::DVec3::ZERO);
    let brackets = harness.app.tabs.active().brackets.clone();
    for (bracket, corner) in brackets.iter().zip(&home) {
        assert!(
            (bracket.corner - *corner).length() < 0.5,
            "Escape puts the marks back too: {:?} vs {corner:?}",
            bracket.corner
        );
    }
}

/// The sides of the frame's reserved box, for a test that watches a grip write nothing.
fn frame_sides(harness: &Harness, frame: Id) -> Option<[units::Length; 3]> {
    let project = harness.app.tabs.active().session.project();
    project.container(frame).and_then(|container| container.envelope()).map(|envelope| envelope.components())
}

#[test]
fn a_carried_frames_old_grip_is_no_target() {
    // X76: the frame's fourteen grip rings were read from the document, so a carried frame left them
    // behind and a press that landed in one of them started a grip drag on the piece the furniture
    // move was carrying. The grips ride with the piece now, and a piece in hand is no grip's target:
    // the press belongs to the carry.
    let (mut session, frame) = one_floor_frame("10.01.01");
    assert!(session.open_product(frame), "the frame is the one open product, the level its grips edit");
    let mut harness = Harness::with_session("frame-carry-grip", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    harness.app.shell.mode = TransformMode::Object;
    harness.settle();

    let high = harness.app.tabs.active().session.project().world_box(frame).expect("the frame has a box").1;
    let grip = gizmo_math::screen_point(harness.camera(), harness.scene(), high).expect("the top grip is on screen");
    let sides = frame_sides(&harness, frame);
    let body = piece_middle(&harness, frame);
    harness.click(body, egui::PointerButton::Primary);
    assert!(harness.app.tabs.active().holding_piece(), "the click took the frame");
    harness.hover(body + egui::vec2(70.0, -40.0));

    // The press lands in a ring of the old place and drags: the piece keeps its sides.
    harness.press_at(grip, egui::PointerButton::Primary);
    harness.hover(grip + egui::vec2(45.0, 0.0));
    harness.release_at(grip + egui::vec2(45.0, 0.0), egui::PointerButton::Primary);
    harness.settle();
    assert_eq!(frame_sides(&harness, frame), sides, "a carried piece's stale grip resizes nothing");
    assert!(harness.app.tabs.active().holding_piece(), "and the piece is still in the hand");
    harness.press(egui::Key::Escape, egui::Modifiers::NONE);
}

/// A scene holding a slab 100 x 100 x 18 and a cutter 50 x 50 x 200 that passes through it, both
/// constructive, with the cutter's id and the slab's.
fn slab_and_cutter() -> (engine::Session, Id, Id) {
    use model::{Constructive, Container, Geometry, Path, Placement, Project};
    let mm = testkit::millimetres;
    let root = Container::new(Id::new(1).expect("an id"), "Scene", Placement::IDENTITY, Vec::new());
    let mut project = Project::new(root).expect("an empty project");
    let slab = Constructive::extrude(Path::rectangle([mm(0), mm(0)], [mm(100), mm(100)]).expect("a slab"), mm(18))
        .expect("a slab");
    let cutter = Constructive::extrude(Path::rectangle([mm(0), mm(0)], [mm(50), mm(50)]).expect("a cutter"), mm(200))
        .expect("a cutter");
    let cutter_at =
        Placement::new(geom::DVec3::new(25.0, 25.0, -10.0), geom::DQuat::IDENTITY).expect("a cutter placement");
    let slab = project
        .add_object(project.root().id(), "Slab", Placement::IDENTITY, Geometry::Constructive(slab))
        .expect("the slab is added");
    let cutter = project
        .add_object(project.root().id(), "Cutter", cutter_at, Geometry::Constructive(cutter))
        .expect("the cutter is added");
    (engine::Session::with_directory(project, testkit::sample_catalog()), slab, cutter)
}

/// The same scene with the cutter standing clear of the slab, so a difference removes nothing.
fn slab_and_away_cutter() -> (engine::Session, Id, Id) {
    let (session, slab, cutter) = slab_and_cutter();
    let mut project = session.project().clone();
    let away = model::Placement::new(geom::DVec3::new(-800.0, 0.0, 0.0), geom::DQuat::IDENTITY).expect("a placement");
    project.set_node_placement(cutter, away).expect("the cutter stands away");
    (engine::Session::with_directory(project, testkit::sample_catalog()), slab, cutter)
}

/// The ids the frame draws a body for, one per drawable.
fn drawn_bodies(harness: &Harness) -> Vec<Id> {
    harness.app.tabs.active().frame().drawables().iter().map(|drawable| drawable.id).collect()
}

#[test]
fn an_armed_body_boolean_cuts_the_frame_and_the_commit_spends_the_cutter() {
    // W2: the mode appends a step that carries the cutter's shape, and the ✓ spends the cutter — so
    // the scene is one body with a hole instead of a body and the shape that cut it (ADR 0197).
    let (session, slab, cutter) = slab_and_cutter();
    let mut harness = Harness::with_session("frame-body-boolean", session);
    harness.settle();
    let revision = harness.revision();
    assert_eq!(drawn_bodies(&harness), vec![slab, cutter], "both bodies draw before the mode");

    {
        let document = harness.app.tabs.active_mut();
        // The last selected node is the primary, so the target is picked last.
        document.editor.select_nodes(vec![cutter, slab]);
        assert!(
            document.editor.arm_body_boolean(engine::ContourOp::Subtract, document.session.project()),
            "the selected body arms the mode"
        );
    }
    harness.settle();
    {
        let document = harness.app.tabs.active();
        assert!(document.editor.boolean_arm().is_some_and(ui::BooleanArm::is_ready), "the cut is on screen");
        assert_eq!(drawn_bodies(&harness), vec![slab, cutter], "a preview is drawn in place of the target");
        assert_eq!(harness.revision(), revision, "a preview is not an edit");
        assert!(!document.can_undo(), "a preview is not an undo entry");
    }

    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    harness.settle();
    let document = harness.app.tabs.active();
    assert!(document.editor.boolean_arm().is_none(), "the mode leaves with its command");
    assert_eq!(harness.revision().get(), revision.get() + 1, "one command, one revision");
    let project = document.session.project();
    let body = project.object(slab).expect("the slab").geometry().as_constructive().expect("a body");
    let step = body.booleans().next().expect("one step");
    assert_eq!(step.tool().name(), "Cutter", "the step froze the cutter's shape and its name");
    assert!(project.object(cutter).is_none(), "the cutter is spent, so one body is left");
    assert_eq!(drawn_bodies(&harness), vec![slab], "the frame draws the cut body alone");
}

#[test]
fn a_boolean_that_leaves_no_solid_is_a_diagnostic_and_not_a_frozen_frame() {
    // The owner's report: a cut whose tool missed the body left the whole picture frozen at the
    // last frame that built, so a move showed only its handles. A node the kernel refuses is a
    // diagnostic on that node and the rest of the scene keeps drawing and following the pointer
    // (geometry.md §8). W2: a marked cutter survives the operation, so it is still there to see.
    let (session, slab, cutter) = slab_and_away_cutter();
    let mut harness = Harness::with_session("frame-boolean-empty", session);
    {
        let document = harness.app.tabs.active_mut();
        document
            .session
            .dispatch(engine::OBJECT_TOOL_SET_ID, &serde_json::json!({ "object": cutter.get(), "tool": true }))
            .expect("the cutter is a tool");
        document
            .session
            .dispatch(
                engine::OBJECT_BOOLEAN_SET_ID,
                &serde_json::json!({ "object": slab.get(), "op": "difference", "tool": cutter.get() }),
            )
            .expect("the step is stored");
    }
    harness.settle();

    let document = harness.app.tabs.active();
    assert_eq!(drawn_bodies(&harness), vec![cutter], "the body that cannot be built draws nothing");
    assert_eq!(document.scene_failures.len(), 1, "the node reports its own failure");
    assert_eq!(document.scene_failures.first().expect("one failure").id, slab);
    let reported = document.last_error.clone().unwrap_or_default();
    assert!(reported.starts_with("Slab:"), "the status bar names the body: {reported}");

    // The picture is live: the cutter still follows the pointer, which is what was frozen before.
    {
        let document = harness.app.tabs.active_mut();
        let away =
            model::Placement::new(geom::DVec3::new(-900.0, 0.0, 0.0), geom::DQuat::IDENTITY).expect("a placement");
        document
            .session
            .dispatch(
                engine::node_cmds::NODE_PLACEMENT_SET_ID,
                &serde_json::json!({ "node": cutter.get(), "placement": away }),
            )
            .expect("a tool moves freely");
    }
    harness.settle();
    assert_eq!(drawn_bodies(&harness), vec![cutter], "the rest of the scene still follows an edit");

    // Dropping the step brings the body back and takes the sentence with it.
    {
        let document = harness.app.tabs.active_mut();
        document
            .session
            .dispatch(engine::OBJECT_BOOLEAN_REMOVE_ID, &serde_json::json!({ "object": slab.get(), "index": 1 }))
            .expect("the step goes");
    }
    harness.settle();
    let document = harness.app.tabs.active();
    assert_eq!(drawn_bodies(&harness), vec![slab, cutter], "the body builds again");
    assert!(document.scene_failures.is_empty(), "the scene builds");
    assert!(document.last_error.is_none(), "the sentence goes with the failure: {:?}", document.last_error);
}

#[test]
fn an_armed_boolean_whose_result_is_not_a_solid_refuses_its_enter() {
    // The preview asks the kernel the same question the ✓ will: a cut that leaves nothing has no
    // result to store, so the card drops its check and Enter keeps the mode without writing.
    let (session, slab, cutter) = slab_and_cutter();
    let mut project = session.project().clone();
    project
        .set_node_placement(
            cutter,
            model::Placement::new(geom::DVec3::new(-800.0, 0.0, 0.0), geom::DQuat::IDENTITY).expect("a placement"),
        )
        .expect("the cutter moves away");
    let session = engine::Session::with_directory(project, testkit::sample_catalog());
    let mut harness = Harness::with_session("frame-boolean-empty-armed", session);
    harness.settle();
    let revision = harness.revision();
    {
        let document = harness.app.tabs.active_mut();
        document.editor.select_nodes(vec![cutter, slab]);
        assert!(
            document.editor.arm_body_boolean(engine::ContourOp::Subtract, document.session.project()),
            "the mode arms on the body"
        );
    }
    harness.settle();

    {
        let document = harness.app.tabs.active();
        let arm = document.editor.boolean_arm().expect("the mode is armed");
        assert!(!arm.is_ready(), "a cut that leaves no solid is not a result");
        assert!(arm.refusal().is_some(), "the arm says why");
        assert_eq!(harness.revision(), revision, "nothing was written");
    }

    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    harness.settle();
    let document = harness.app.tabs.active();
    assert!(document.editor.boolean_arm().is_some(), "a refused commit keeps the mode alive");
    assert_eq!(harness.revision(), revision, "a refused commit writes nothing");
    let body = document.session.project().object(slab).expect("the slab").geometry().as_constructive().expect("a body");
    assert_eq!(body.booleans().count(), 0, "the body keeps its history");
}

/// A headless window whose root holds one closed ring object per rectangle, named `Ring 1`, `Ring
/// 2`, …, and their ids in that order.
///
/// A ring object is a drawn path, which is what a contour boolean combines (ADR 0194, revised by
/// W2): the arm's base and its operands are `Geometry::Path` and not a body.
fn rings_session(name: &str, rings: Vec<([i64; 2], [i64; 2])>) -> (Harness, Vec<Id>) {
    let mm = testkit::millimetres;
    let root = model::Container::new(Id::new(1).expect("id"), "Scene", model::Placement::IDENTITY, Vec::new());
    let mut project = model::Project::new(root).expect("a project");
    let root = project.root().id();
    let mut ids = Vec::new();
    for (index, (low, high)) in rings.into_iter().enumerate() {
        let path = model::Path::rectangle([mm(low[0]), mm(low[1])], [mm(high[0]), mm(high[1])]).expect("a ring");
        let id = project
            .add_object(root, format!("Ring {}", index + 1), model::Placement::IDENTITY, model::Geometry::Path(path))
            .expect("a drawn ring");
        ids.push(id);
    }
    let session = engine::Session::with_directory(project, testkit::sample_catalog());
    (Harness::with_session(name, session), ids)
}

/// The ink the frame last uploaded gave each ring it draws as a picked line chain.
fn chain_inks(harness: &Harness) -> Vec<(Id, Option<[f32; 4]>)> {
    harness.app.tabs.active().highlighted_chains().iter().map(|chain| (chain.id, chain.color)).collect()
}

#[test]
fn an_armed_boolean_marks_its_base_ring_and_drops_the_mark_with_the_mode() {
    // X99: the base is the frame the result is born in. A plain click on the tool replaces the
    // selection, so the base used to lose its highlight while it was still an operand, and two
    // rings looked like one selection. The base now takes an ink of its own and the operand keeps
    // the selection ink, in the frame's own shape list.
    let (mut harness, rings) =
        rings_session("frame-ring-boolean-base", vec![([0, 0], [160, 80]), ([80, 0], [240, 80])]);
    let (base, operand) = (rings[0], rings[1]);
    harness.settle();

    // Both picked, no boolean: the two rings are one selection, drawn in one ink.
    harness.app.tabs.active_mut().editor.select_nodes(vec![base, operand]);
    harness.settle();
    for (id, ink) in chain_inks(&harness) {
        assert_eq!(ink, Some(scene::SELECTION_LINE_COLOR), "ring {id:?} is picked and carries the selection ink");
    }

    let base_ink = harness.app.tabs.active().frame().boolean_base;
    assert_ne!(base_ink, scene::SELECTION_LINE_COLOR, "the two roles are two inks");

    // Arming marks the base before anything else is picked: the mode is view state the frame has to
    // follow, so the picture changes with it.
    {
        let document = harness.app.tabs.active_mut();
        document.editor.select_nodes(vec![base]);
        assert!(
            document.editor.arm_boolean(engine::ContourOp::Subtract, document.session.project()),
            "the selected ring arms the mode"
        );
    }
    harness.settle();
    let inks = chain_inks(&harness);
    assert_eq!(
        inks.iter().find(|(id, _)| *id == base).map(|(_, ink)| *ink),
        Some(Some(base_ink)),
        "arming the mode marks the base at once"
    );

    // A plain click picks the tool: the base leaves the selection, stays an operand and keeps its ink.
    harness.app.tabs.active_mut().editor.select_nodes(vec![operand]);
    harness.settle();
    let inks = chain_inks(&harness);
    assert_eq!(
        inks.iter().find(|(id, _)| *id == base).map(|(_, ink)| *ink),
        Some(Some(base_ink)),
        "the base ring takes the base ink"
    );
    assert_eq!(
        inks.iter().find(|(id, _)| *id == operand).map(|(_, ink)| *ink),
        Some(Some(scene::SELECTION_LINE_COLOR)),
        "the operand keeps the selection ink"
    );

    // Dropping the mode returns every ring to the selection ink: the mark belongs to the mode.
    harness.app.tabs.active_mut().cancel_boolean();
    harness.app.tabs.active_mut().editor.select_nodes(vec![base, operand]);
    harness.settle();
    let inks = chain_inks(&harness);
    assert_eq!(inks.len(), 2, "both rings are picked again: {inks:?}");
    for (id, ink) in inks {
        assert_eq!(ink, Some(scene::SELECTION_LINE_COLOR), "ring {id:?} keeps no base ink once the mode is gone");
    }
}

#[test]
fn an_armed_boolean_names_its_base_on_the_card() {
    // The picture marks the base and the card names it, so the words and the rings agree (X99).
    let (mut harness, rings) =
        rings_session("frame-ring-boolean-note", vec![([0, 0], [160, 80]), ([80, 0], [240, 80])]);
    let (base, operand) = (rings[0], rings[1]);
    harness.settle();
    {
        let document = harness.app.tabs.active_mut();
        document.editor.select_nodes(vec![base]);
        assert!(document.editor.arm_boolean(engine::ContourOp::Subtract, document.session.project()));
        document.editor.select_nodes(vec![operand]);
    }
    harness.settle();

    let lang = ui::language(&harness.ctx);
    let view = harness.app.tabs.active().card_view(lang).expect("a result on screen draws the card");
    let expected = Message::BooleanBase.with(lang, ["Ring 1"]);
    assert_eq!(view.note.as_deref(), Some(expected.as_str()), "the card names the base the rings mark");
}

/// A headless window whose scene root holds one drawing of the given rings, and the drawing's id.
fn drawing_session(name: &str, rings: Vec<model::Path>) -> (Harness, model::Id) {
    let root = model::Container::new(model::Id::new(1).expect("id"), "Scene", model::Placement::IDENTITY, Vec::new());
    let mut project = model::Project::new(root).expect("a project");
    let root = project.root().id();
    let set = model::Contours::new(rings).expect("a contour set");
    let drawing = project
        .add_object(root, "Union", model::Placement::IDENTITY, model::Geometry::Contours(set))
        .expect("a drawing");
    let session = engine::Session::with_directory(project, testkit::sample_catalog());
    (Harness::with_session(name, session), drawing)
}

/// The rectangle ring a drawing fixture is built from, in millimetres.
fn frame_ring(x0: i64, y0: i64, x1: i64, y1: i64) -> model::Path {
    model::Path::rectangle(
        [testkit::millimetres(x0), testkit::millimetres(y0)],
        [testkit::millimetres(x1), testkit::millimetres(y1)],
    )
    .expect("a rectangle ring")
}

#[test]
fn the_area_pull_creates_the_face_under_the_pointer_in_one_undo_step() {
    // A drawing of two nested rings: the opening inside the frame is its own area (G4).
    let (mut harness, drawing) =
        drawing_session("frame-area", vec![frame_ring(0, 0, 100, 50), frame_ring(20, 10, 60, 30)]);
    harness.settle();
    let (pointer, rect, before) = {
        let document = harness.app.tabs.active_mut();
        document.editor.select_node(Some(drawing));
        document.editor.arm_area_pull(drawing);
        let rect = document.scene_rect().expect("a drawn frame gives the scene a rectangle");
        let camera = document.camera();
        let pointer = gizmo_math::screen_point(camera, rect, geom::DVec3::new(40.0, 20.0, 0.0))
            .expect("the opening is on screen");
        assert!(rect.contains(pointer), "the press lands on the scene: {pointer:?} in {rect:?}");
        (pointer, rect, harness.revision())
    };

    harness.hover(pointer);
    let measured = harness
        .app
        .tabs
        .active()
        .editor
        .area()
        .region()
        .map(|area| area.area())
        .expect("the pointer names the opening");
    assert!((measured - 800.0).abs() < 1e-3, "the opening is 40 x 20 mm, got {measured}");
    assert!(
        harness.drawn.iter().any(|text| text.contains("Pull an area")),
        "the status bar says what the next click does"
    );

    harness.press_at(pointer, egui::PointerButton::Primary);
    harness.release_at(pointer, egui::PointerButton::Primary);
    harness.settle();

    let created = {
        let document = harness.app.tabs.active();
        let created = document.editor.selected_node().expect("the new body is selected");
        let body = document
            .session
            .project()
            .object(created)
            .and_then(|object| object.geometry().as_constructive())
            .expect("the click created a body");
        assert_eq!(
            body.contour().expect("a ring").bounds_micrometres(),
            ([20_000, 10_000], [60_000, 30_000]),
            "the body stands where the opening stood"
        );
        assert!(document.extrude_gizmo.is_active(), "the pull arrow took the body");
        assert!(document.session.project().object(drawing).is_some(), "the drawing keeps its rings");
        created
    };
    assert!(harness.revision() > before, "the click writes the body once");

    // The ✓ closes the gesture: one entry for the body and its height, and the drawing stays.
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert!(!harness.app.tabs.active().extrude_gizmo.is_active(), "the ✓ ends the pull");
    let document = harness.app.tabs.active_mut();
    document.session.undo().expect("one undo step");
    assert!(document.session.project().object(created).is_none(), "one undo removes the body");
    assert!(document.session.project().object(drawing).is_some(), "and leaves the drawing");
    assert!(!document.session.can_undo(), "the pull was one entry, not two");
    let _ = rect;
}

#[test]
fn escaping_the_area_pull_takes_the_body_back_and_leaves_no_entry() {
    let (mut harness, drawing) = drawing_session("frame-area-cancel", vec![frame_ring(0, 0, 100, 50)]);
    harness.settle();
    let pointer = {
        let document = harness.app.tabs.active_mut();
        document.editor.select_node(Some(drawing));
        document.editor.arm_area_pull(drawing);
        let rect = document.scene_rect().expect("a scene rectangle");
        let camera = document.camera();
        let pointer = gizmo_math::screen_point(camera, rect, geom::DVec3::new(50.0, 25.0, 0.0)).expect("on screen");
        assert!(rect.contains(pointer));
        pointer
    };
    let before = harness.revision();
    harness.hover(pointer);
    harness.click(pointer, egui::PointerButton::Primary);
    assert!(harness.revision() > before, "the click wrote the body");
    assert!(harness.app.tabs.active().extrude_gizmo.is_active());

    harness.press(egui::Key::Escape, egui::Modifiers::NONE);

    let document = harness.app.tabs.active();
    assert!(!document.extrude_gizmo.is_active(), "Escape drops the pull");
    assert_eq!(
        document.session.project().geometry_ids_under(document.session.project().root().id()).map(|ids| ids.len()),
        Some(1),
        "the body went with the scope: the drawing is the only geometry left"
    );
    assert!(document.session.project().object(drawing).is_some(), "the drawing is untouched");
    assert!(!document.session.can_undo(), "a cancelled pull leaves no entry");
    assert_eq!(document.editor.area_pull(), Some(drawing), "the tool stays armed for the next click");
}

#[test]
fn a_channel_call_is_answered_at_the_frame_boundary() {
    // An agent's call is taken at the top of the frame, before anything the frame derives: the
    // answer is the registry's own listing and the window's own report, and the window is exactly
    // the one the test is looking at (ADR 0187, ADR 0188).
    let mut harness = Harness::new("frame-channel");
    let (listed, listed_answer) = mcp::queued(mcp::Call::List);
    let (inspected, inspected_answer) = mcp::queued(mcp::Call::Inspect);
    harness.app.queued.push(listed);
    harness.app.queued.push(inspected);
    harness.frame();

    let listed = listed_answer.take().expect("the frame answered the list");
    assert!(!listed.failed(), "the listing is not a refusal: {:?}", listed.body());
    assert!(
        listed.body()["commands"].as_array().is_some_and(|rows| rows.len() > 50),
        "the whole registry reaches a caller: {:?}",
        listed.body()
    );
    let reported = inspected_answer.take().expect("the frame answered the report");
    let window: mcp::Window = serde_json::from_value(reported.body().clone()).expect("the report is a window");
    assert_eq!(window.tabs.len(), 1, "one scene is open in the window");
    assert_eq!(window.active, 0, "and it is the one in front");
    assert!(!window.mode.is_empty(), "the armed gesture is named: {window:?}");
    assert!(!window.busy, "an idle frame is not a gesture");
}

#[test]
fn a_call_that_lands_answers_in_the_frame_it_asks_for_with_no_input_at_all() {
    // The window paints on input, so a call from the channel used to wait for a pointer only a
    // person brings: the document moved, the tab was dirty, and the frame the person saw was the old
    // one (X111, ADR 0225). A call now rings the window the moment it is queued — the mailbox the
    // window watches is told what to do — and the frame that lands it asks for the frame that shows
    // it. Nothing in this test touches the pointer or a key.
    let folder = testkit::scratch_dir("frame-channel-landed");
    let (mut harness, _panel) = chat_fixture("frame-channel-landed-doc");
    let product = harness.app.tabs.active().session.active_product().expect("the fixture product is open");
    harness.app.tabs.active_mut().session.set_agent_mode(engine::AgentMode::Build);
    harness.settle();
    assert!(harness.app.attach_channel(&folder, &harness.ctx), "the channel opens in a folder of its own");
    assert_eq!(harness.shell_requests(), 0, "an idle window asks for no frame of its own");

    let before = harness.revision();
    let mailbox = harness.app.mailbox.as_ref().expect("the mailbox comes with the channel");
    let answering = mailbox.post(mcp::Call::Dispatch(shelf_request(product))).expect("the call queues");
    assert_eq!(harness.revision(), before, "a queued call has not been answered yet: it waits for a frame");

    harness.frame();
    let answer = answering.take().expect("the frame answered the call");
    assert!(!answer.failed(), "the batch landed: {:?}", answer.body());
    assert_ne!(harness.revision(), before, "the frame that answered the call landed it, and the window is awake");
    let created = answer.body()["created"][0].as_u64().expect("the answer names the part it created");
    let shelf = Id::new(created).expect("an id is never zero");
    assert!(
        harness.app.tabs.active().frame().drawables().iter().any(|drawable| drawable.id == shelf),
        "the scene was rebuilt in the frame the call returned, so the part is in it"
    );

    // `egui` names a frame's causes one pass later, so the request the call's own frame made is read
    // here: one ask, from the shell, with no pointer behind it.
    harness.frame();
    assert_eq!(harness.shell_requests(), 1, "the frame that landed the call asked for the frame that shows it");
}

#[test]
fn a_call_waits_for_the_gesture_and_then_lands_with_the_ids_it_created() {
    // A call that moves the document is not refused because a gesture holds the window: it waits for
    // the first frame that has the window to itself and answers with the report of what landed. A
    // refusal here would say nothing about the edit a late frame performs — the answer a caller read
    // as "nothing landed" while the document held the node (X107, ADR 0223). Reads are answered in
    // the busy frame, because a question about the document moves nothing.
    let (mut harness, _panel) = chat_fixture("frame-channel-holds");
    let product = harness.app.tabs.active().session.active_product().expect("the fixture product is open");
    // Plan mode is the state a window's document is born in, and this test is about what a call that
    // is allowed to write answers, so the person has turned the switch to build (ADR 0190).
    harness.app.tabs.active_mut().session.set_agent_mode(engine::AgentMode::Build);
    harness.settle();
    let before = harness.revision();

    harness.app.pending_question =
        Some(PendingQuestion { gesture: Gesture::Quit, question: Confirm::quit(1, Lang::En) });
    let (dispatched, answered) = mcp::queued(mcp::Call::Dispatch(shelf_request(product)));
    let (listed, listing_answer) = mcp::queued(mcp::Call::List);
    harness.app.queued.push(dispatched);
    harness.app.queued.push(listed);
    harness.frame();

    assert!(
        !listing_answer.take().expect("a read is answered in the busy frame").failed(),
        "a question about the document is not held back by a gesture in hand"
    );
    assert_eq!(harness.app.queued.len(), 1, "the dispatch waits for a frame that has the window");
    assert_eq!(harness.revision(), before, "and nothing moves under the hand that holds the window");
    assert_eq!(harness.app.tabs.active().session.active_product(), Some(product));

    // The person answers the question: the next frame has the window to itself.
    harness.app.pending_question = None;
    harness.frame();

    let answer = answered.take().expect("the frame that had the window answered the call");
    assert!(harness.app.queued.is_empty(), "nothing is left waiting");
    assert!(!answer.failed(), "the batch landed: {:?}", answer.body());
    assert_eq!(answer.body()["effect"], serde_json::json!("landed"), "{:?}", answer.body());
    let created: Vec<u64> = answer.body()["created"]
        .as_array()
        .expect("the answer lists what it created")
        .iter()
        .filter_map(serde_json::Value::as_u64)
        .collect();
    assert_eq!(created.len(), 1, "the answer names the shelf it created: {:?}", answer.body());
    let shelf = Id::new(created.first().copied().expect("one created id")).expect("an id is never zero");
    let project = harness.app.tabs.active().session.project();
    assert!(project.panel(shelf).is_some(), "the panel the answer named is in the document");
    assert_ne!(harness.revision(), before, "the frame that landed it moved the revision");
}

#[test]
fn a_call_whose_caller_gave_up_is_never_performed() {
    // A caller stops waiting at the channel's deadline. Performing the call then would change the
    // person's document for a caller that will never hear of it, and the retry that follows builds
    // the part twice (X107).
    let (mut harness, _panel) = chat_fixture("frame-channel-abandoned");
    let product = harness.app.tabs.active().session.active_product().expect("the fixture product is open");
    harness.app.tabs.active_mut().session.set_agent_mode(engine::AgentMode::Build);
    harness.settle();
    let before = harness.revision();
    let nodes = geometry_count(&harness);

    let (pending, answering) = mcp::queued(mcp::Call::Dispatch(shelf_request(product)));
    drop(answering);
    harness.app.queued.push(pending);
    harness.frame();

    assert!(harness.app.queued.is_empty(), "the dropped call leaves nothing waiting");
    assert_eq!(harness.revision(), before, "a caller that is gone moves nothing");
    assert_eq!(geometry_count(&harness), nodes, "and adds no node to the document");
}

/// How many geometry nodes the active document holds, so a test can tell a part was added.
fn geometry_count(harness: &Harness) -> usize {
    let project = harness.app.tabs.active().session.project();
    project.geometry_ids_under(project.root().id()).map(|ids| ids.len()).unwrap_or_default()
}

#[test]
fn a_refused_batch_answers_the_command_sentence_and_no_created_ids() {
    // The other half of the same promise: a call that moved nothing says so with the command's own
    // sentence and names no node, so an empty answer is never read as a landed batch (X107).
    let mut harness = Harness::new("frame-channel-refused");
    harness.app.tabs.active_mut().session.set_agent_mode(engine::AgentMode::Build);
    harness.settle();
    let before = harness.revision();
    // A panel is born only inside an open product (ADR 0098), and this session has none open.
    let request = mcp::BatchRequest {
        steps: vec![mcp::StepRequest::new(
            "panel.create",
            serde_json::json!({
                "parent": 1,
                "name": "Shelf",
                "nominal_size": {"length": 700_000, "width": 500_000},
                "material": "1-1",
                "placement": {"translation": [0, 0, 0], "rotation": [0, 0, 0, 1]},
            }),
        )],
        product: None,
        atomic: true,
        dry_run: false,
    };
    let (pending, answering) = mcp::queued(mcp::Call::Dispatch(request));
    harness.app.queued.push(pending);
    harness.frame();

    let answer = answering.take().expect("the frame answered the call");
    assert!(answer.failed(), "the command refused: {:?}", answer.body());
    assert_eq!(answer.body()["created"], serde_json::json!([]), "{:?}", answer.body());
    assert_eq!(answer.body()["effect"], serde_json::json!("rolled_back"), "{:?}", answer.body());
    let refusal = answer.body()["refusal"]["error"].as_str().expect("the command's own sentence");
    assert!(!refusal.trim().is_empty(), "a refusal says why");
    assert_eq!(harness.revision(), before, "and the document did not move");
}

/// One batch that creates a shelf inside `product`, as an agent would send it.
fn shelf_request(product: Id) -> mcp::BatchRequest {
    mcp::BatchRequest {
        steps: vec![mcp::StepRequest::new(
            "panel.create",
            serde_json::json!({
                "parent": product.get(),
                "name": "Shelf",
                "nominal_size": {"length": 700_000, "width": 500_000},
                "material": "1-1",
                "placement": {"translation": [0, 0, 0], "rotation": [0, 0, 0, 1]},
            }),
        )],
        product: None,
        atomic: true,
        dry_run: false,
    }
}

#[test]
fn a_closed_window_removes_the_token_it_published() {
    // A clean exit leaves nothing behind: a reader that cannot connect reads "no window is open
    // here", never a token nobody answers (ADR 0188).
    let folder = testkit::scratch_dir("frame-channel-token");
    let file = mcp::Channel::file(&folder);
    {
        let mut harness = Harness::new("frame-channel-window");
        assert!(harness.app.attach_channel(&folder, &harness.ctx), "the channel opens in a folder of its own");
        assert!(file.exists(), "the port and the token are published");
        harness.frame();
    }
    assert!(!file.exists(), "a window that closes removes its own token file");
}

#[test]
fn the_assistant_switch_closes_and_reopens_the_channel() {
    // The settings page is the door: the switch pressed on one frame closes or opens the channel,
    // so the token file always describes the window that is there (ADR 0188, ADR 0207).
    let mut harness = Harness::new("frame-channel-switch");
    let file = mcp::Channel::file(&testkit::scratch_dir("frame-channel-switch"));
    assert!(harness.app.settings.assistant.attach, "a fresh workspace offers the channel");

    harness.app.apply_channel(&harness.ctx);
    assert!(file.exists(), "the channel publishes its token while the switch is on");

    harness.app.settings.assistant.attach = false;
    harness.app.apply_channel(&harness.ctx);
    assert!(harness.app.channel.is_none(), "the switch closes the door");
    assert!(!file.exists(), "and the token goes with it");

    harness.app.settings.assistant.attach = true;
    harness.app.apply_channel(&harness.ctx);
    assert!(file.exists(), "turning it back on publishes a new token");
}

#[test]
fn the_settings_window_offers_the_assistant_switch() {
    // The switch is drawn where a person looks for it: its own page, named, with the sentence that
    // says what it closes (ADR 0188, ADR 0207).
    let mut harness = Harness::new("frame-channel-page");
    harness.app.settings_open = true;
    harness.app.settings_section = SettingsSection::Assistant;
    harness.settle();
    assert!(harness.drew(Message::SectionAssistant.text(Lang::En)), "the page is named: {:?}", harness.drawn);
    assert!(harness.drew(Message::AttachChannel.text(Lang::En)), "and the switch is on it: {:?}", harness.drawn);
    assert!(harness.drew(Message::AttachChannelHint.text(Lang::En)), "with the sentence that says what it is");
    assert!(harness.app.settings.assistant.attach, "a fresh workspace leaves the door open");
}

/// A headless window around one session with a product open and one panel it can rename.
///
/// The assistant's turn needs something to change, and a part is born only inside an open product
/// (ADR 0098), so the fixture builds the product and the panel the way the tool server's own test
/// does and hands back the panel's id.
fn chat_fixture(name: &str) -> (Harness, Id) {
    let (project, product) = testkit::sample_product_project();
    let mut session = engine::Session::with_directory(project, testkit::sample_catalog());
    assert!(session.open_product(product), "the fixture product opens");
    let panel = session
        .dispatch(
            "panel.create",
            &serde_json::json!({
                "parent": product.get(),
                "name": "Side",
                "nominal_size": {"length": 700_000, "width": 500_000},
                "material": "1-1",
                "placement": {"translation": [0, 0, 0], "rotation": [0, 0, 0, 1]},
            }),
        )
        .expect("the fixture panel is created")
        .created_id()
        .expect("a panel id");
    (Harness::with_session(name, session), panel)
}

/// The name the document gives `panel`, or `None` when it is gone.
fn panel_name(harness: &Harness, panel: Id) -> Option<String> {
    harness.app.tabs.active().session.project().node(panel).map(|node| node.name().to_owned())
}

/// One batch step that renames `panel`.
fn rename_step(panel: Id, name: &str) -> engine::BatchStep {
    engine::BatchStep::new("node.rename", serde_json::json!({"node": panel.get(), "name": name}))
}

/// Drives the frame until the panel's turn is over, or `frames` frames have run.
fn drive_turn(harness: &mut Harness, frames: usize) {
    for _ in 0..frames {
        harness.frame();
        if !harness.app.chat.is_busy() {
            return;
        }
    }
}

#[test]
fn a_chat_turn_lands_one_undo_entry_and_the_panel_draws_what_it_did() {
    // ADR 0187 through the real frame: the answer of one message is one entry, and the panel says so
    // in its own note line, which is the sentence a person reads instead of reading the history.
    let (mut harness, panel) = chat_fixture("frame-chat-turn");
    harness.app.chat.open = true;
    harness.app.tabs.active_mut().session.set_agent_mode(engine::AgentMode::Build);
    harness.app.chat.backend = Some(Box::new(mcp::Scripted::new(vec![
        mcp::Reply::calling(vec![rename_step(panel, "Left")]),
        mcp::Reply::saying("the side is renamed"),
    ])));

    harness.app.send_chat("rename the side".to_owned(), Lang::En);
    drive_turn(&mut harness, 24);

    assert_eq!(panel_name(&harness, panel).as_deref(), Some("Left"), "the answer landed");
    assert_eq!(
        harness.app.chat.conversation.lines().last().map(|line| line.text.as_str()),
        Some("the side is renamed"),
        "the model's own words close the turn"
    );
    harness.app.tabs.active_mut().session.undo().expect("the whole message is one undo entry");
    assert_eq!(panel_name(&harness, panel).as_deref(), Some("Side"));
    harness.app.tabs.active_mut().session.undo().expect("the fixture's creation is the entry under it");
    assert!(panel_name(&harness, panel).is_none(), "the turn left exactly one entry");
}

#[test]
fn the_panel_in_plan_mode_draws_the_plan_and_writes_nothing() {
    // ADR 0190: the first mode is plan, the answer runs with writing off, and what a person sees is
    // the same dispatch the command line's dry run measured — not prose the model imagined.
    let (mut harness, panel) = chat_fixture("frame-chat-plan");
    harness.app.chat.open = true;
    harness.app.chat.backend =
        Some(Box::new(mcp::Scripted::new(vec![mcp::Reply::calling(vec![rename_step(panel, "Left")])])));

    harness.app.send_chat("rename the side".to_owned(), Lang::En);
    drive_turn(&mut harness, 24);

    assert_eq!(harness.app.tabs.active().session.agent_mode(), engine::AgentMode::Plan, "a tab is born in plan");
    assert_eq!(panel_name(&harness, panel).as_deref(), Some("Side"), "plan wrote nothing");
    harness.app.tabs.active_mut().session.undo().expect("the fixture's creation is the only entry");
    assert!(panel_name(&harness, panel).is_none(), "plan added no entry of its own");
    assert!(harness.drawn.iter().any(|line| line.starts_with("Plan:")), "the panel drew the plan: {:?}", harness.drawn);
}

#[test]
fn a_stopped_turn_leaves_the_document_as_its_last_answer_did() {
    // The stop is between calls (ADR 0187): the second answer is never asked for, so the document
    // holds the first answer and one undo entry — never a batch cut in half.
    let (mut harness, panel) = chat_fixture("frame-chat-stop");
    harness.app.chat.open = true;
    harness.app.tabs.active_mut().session.set_agent_mode(engine::AgentMode::Build);
    harness.app.chat.backend = Some(Box::new(mcp::Scripted::new(vec![
        mcp::Reply::calling(vec![rename_step(panel, "Left")]),
        mcp::Reply::calling(vec![rename_step(panel, "Right")]),
    ])));

    harness.app.send_chat("rename twice".to_owned(), Lang::En);
    // Two frames carry the first answer: one asks the model, one takes the answer and dispatches it.
    for _ in 0..6 {
        harness.frame();
        if panel_name(&harness, panel).as_deref() == Some("Left") {
            break;
        }
    }
    assert_eq!(panel_name(&harness, panel).as_deref(), Some("Left"), "the first answer landed");
    harness.app.chat.stop();
    drive_turn(&mut harness, 12);

    assert_eq!(panel_name(&harness, panel).as_deref(), Some("Left"), "the stopped answer never ran");
    assert!(
        harness.drawn.iter().any(|line| line == Message::AssistantStoppedLine.text(Lang::En)),
        "the panel says the turn was stopped: {:?}",
        harness.drawn
    );
    harness.app.tabs.active_mut().session.undo().expect("the landed answer is one entry");
    assert_eq!(panel_name(&harness, panel).as_deref(), Some("Side"), "and it was all one entry");
}

#[test]
fn the_assistant_panel_opens_on_its_key_and_draws_its_two_modes() {
    // The panel's key is the inventory's row, and the switch a person turns is drawn with the
    // sentence that stands for it.
    let (mut harness, _panel) = chat_fixture("frame-chat-panel");
    harness.settle();
    assert!(!harness.app.chat.open, "the panel starts closed");

    harness.press(egui::Key::J, egui::Modifiers::CTRL);
    assert!(harness.app.chat.open, "Ctrl+J opens it");
    harness.settle();
    assert!(harness.drew(Message::AssistantPlanMode.text(Lang::En)), "the plan segment is drawn");
    assert!(harness.drew(Message::AssistantBuildMode.text(Lang::En)), "and the build segment beside it");
    assert!(harness.drew(Message::AssistantPlanHint.text(Lang::En)), "with the sentence plan means");
    assert!(
        harness.drew(Message::AssistantNotConfigured.text(Lang::En)),
        "and the sentence that says no model is connected"
    );
}

/// The header hint that names the assistant's cell: the panel's word and the inventory's key.
///
/// The test reads the spelling from the tables instead of writing `Ctrl+J` a second time, so a
/// remapped key moves the cell and the test with it.
fn assistant_cell_hint() -> String {
    let key = ui::inventory::key_label(ui::inventory::ASSISTANT_OPEN).expect("the panel has a key");
    crate::icons::tool_hint(Message::ShortcutAssistant.text(Lang::En), Some(key))
}

#[test]
fn the_plan_answer_names_the_key_the_header_cell_prints() {
    // The wire's sentence is English and lives in `mcp`, which may not read `ui`'s key table, so the
    // spelling of the key is asserted here — the one crate that sees both — and a remapped binding
    // fails a test instead of leaving a model to tell a person about a key that no longer exists
    // (ADR 0186, X106).
    let key = ui::inventory::key_label(ui::inventory::ASSISTANT_OPEN).expect("the panel has a key");
    assert!(mcp::PLAN_NOTE.contains(key), "the plan note names `{key}`: {}", mcp::PLAN_NOTE);
    assert!(mcp::PLAN_NOTE.contains("a person"), "and says the switch is a person's: {}", mcp::PLAN_NOTE);
}

#[test]
fn the_header_cell_opens_the_assistant_and_the_panel_draws_its_two_modes() {
    // X106: the owner could not find the plan/build switch, because the panel that holds it had no
    // entry in the interface. The header's cell is that entry: a person presses it where the window's
    // own surfaces are, and the switch is inside the panel it opens (ADR 0209, ADR 0222).
    let (mut harness, _panel) = chat_fixture("frame-chat-header-cell");
    harness.settle();
    assert!(!harness.app.chat.open, "the panel starts closed");

    let at = harness.face(&assistant_cell_hint()).unwrap_or_else(|| {
        panic!(
            "the header draws the assistant's cell: {:?}",
            harness.faces.iter().map(|(name, _)| name).collect::<Vec<_>>()
        )
    });
    harness.click(at, egui::PointerButton::Primary);
    assert!(harness.app.chat.open, "the header's cell opens the panel");
    assert!(harness.drew(Message::AssistantPlanMode.text(Lang::En)), "the plan segment is drawn: {:?}", harness.drawn);
    assert!(harness.drew(Message::AssistantBuildMode.text(Lang::En)), "the build segment is beside it");
    assert!(harness.drew(Message::AssistantPlanHint.text(Lang::En)), "and the sentence plan means");

    // The same cell closes it, so one press is one toggle whichever door asked for it.
    harness.click(at, egui::PointerButton::Primary);
    assert!(!harness.app.chat.open, "a second press closes the panel");
}

#[test]
fn the_assistant_panel_keeps_the_size_it_opens_at() {
    // X108: a person saw the panel widen by itself, a little every frame, until it filled the
    // screen. Measured: the field row left its button a guessed `available - 72` points, and the
    // Russian «Отправить» is wider than that, so the row asked the window for 9.4 more points on
    // every frame — 420, 476, 486 … 1289 on a 1280-point screen, in both modes and for any content.
    // The row now measures the button and gives the field the rest, so the width it opens at is the
    // width it keeps, in either language.
    for lang in [ui::settings::Language::English, ui::settings::Language::Russian] {
        let (mut harness, _panel) = chat_fixture("frame-chat-size");
        harness.settle();
        harness.app.settings.appearance.language = lang;
        harness.app.chat.open = true;
        harness.app.chat.backend = Some(Box::new(mcp::Scripted::new(vec![])));
        harness.app.chat.conversation.push(
            mcp::Speaker::Agent,
            "an answer long enough to wrap over several lines, which is what the transcript is for ".repeat(3),
        );
        harness.app.chat.input =
            "a message the person is typing that is longer than the field that holds it ".repeat(3);
        let id = egui::Id::unique("assistant_panel");
        let mut first = None;
        let mut last = None;
        for _ in 0..100 {
            harness.frame();
            let rect = harness.ctx.memory(|m| m.area_rect(id)).expect("the panel drew");
            first.get_or_insert(rect);
            last = Some(rect);
        }
        let (first, last) = (first.expect("the panel drew"), last.expect("the panel drew"));
        assert_eq!(first, last, "{lang:?}: the panel kept the size it opened at, first {first:?}, last {last:?}");
    }
}
