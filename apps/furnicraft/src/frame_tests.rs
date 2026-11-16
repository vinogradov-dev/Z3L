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

use crate::gizmo_math::SnapTarget;
use eframe::egui;
use furnicraft_geom::DVec2;
use furnicraft_model::{ClassCode, ContourSegment, Envelope, Id};
use furnicraft_scene::Camera;
use furnicraft_text::{Lang, Message};
use furnicraft_ui::settings::{MAX_HANDLE_POINTS, MAX_RING_PERCENT, MIN_HANDLE_POINTS, MIN_RING_PERCENT};
use furnicraft_ui::{DrawIntent, DrawTool, FieldParam, MeasureStage};
use furnicraft_units::Length;

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
    fn with_session(name: &str, session: furnicraft_engine::Session) -> Self {
        let path = furnicraft_testkit::scratch_dir(name).join("settings.json");
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
        output.textures_delta.clear();
        chrome.expect("the frame drew the interface")
    }

    /// Runs one idle frame, with no input at all.
    fn frame(&mut self) -> Chrome {
        self.run(Vec::new())
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
    fn revision(&self) -> furnicraft_model::Revision {
        self.app.tabs.active().session.project().revision()
    }

    /// The nodes the viewport selection holds.
    fn selected(&self) -> Vec<Id> {
        self.app.tabs.active().editor.selected_nodes().to_vec()
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
        let local = project.panel(node).map_or(furnicraft_geom::DVec3::ZERO, |panel| {
            let size = panel.nominal_size();
            furnicraft_geom::DVec3::new(
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
    fn mode_cell(&self, caption: &str) -> egui::Pos2 {
        let below_scene = self.scene().bottom();
        self.drawn_rects
            .iter()
            .find(|(rect, text)| text == caption && rect.center().y > below_scene)
            .map(|(rect, _)| rect.center())
            .unwrap_or_else(|| panic!("the status bar drew no cell named {caption}"))
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
fn the_scene_rectangle_and_its_camera_hold_in_every_panel_and_ribbon_state() {
    // The regression U72 fixed: a fold, a hide or a restore moved the scene rectangle and therefore
    // the camera aspect, so the model panned when a panel was folded. Only the frame can prove it:
    // the rectangle the viewport really used, and the camera it really drew with.
    let mut harness = Harness::new("frame-scene-rect");
    harness.frame();
    let mut scenes = Vec::new();
    for ribbon_open in [true, false] {
        if !ribbon_open {
            harness.app.handle_header_action(HeaderAction::ToggleRibbon, Lang::En);
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
    assert!(list.width() >= furnicraft_ui::toolbars::list::WIDTH, "the popup is drawn at the approved width");

    // Four points across the open list: the header, a row's name, its key and its pin. Every one of
    // them is the list, so every one of them shows the list's cursor.
    let row =
        list.top() + furnicraft_ui::toolbars::list::HEADER_HEIGHT + furnicraft_ui::toolbars::list::ROW_HEIGHT / 2.0;
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

    harness.app.handle_header_action(HeaderAction::Settings, Lang::En);
    harness.settle();
    assert!(harness.drew(Message::SettingsTitle.text(Lang::En)), "the Settings window is up: {:?}", harness.drawn);

    harness.press(egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(harness.revision(), edited, "Ctrl+Z under a modal undoes nothing");
    assert!(harness.app.tabs.active().can_undo(), "the edit is still there to undo");
    harness.press(egui::Key::F, egui::Modifiers::NONE);
    assert_eq!(harness.camera(), orbited, "F under a modal does not reframe");
    harness.press(egui::Key::R, egui::Modifiers::NONE);
    assert!(harness.app.tabs.active().editor.drawing().is_none(), "and no tool starts under it");
    harness.press(egui::Key::S, egui::Modifiers::CTRL);
    assert!(
        !harness.drew(Message::SaveSceneAsTitle.text(Lang::En)),
        "Ctrl+S under a modal opens no save dialog: {:?}",
        harness.drawn
    );

    // The gate is the modal and not a dead key: with it closed, the same press undoes the edit.
    harness.app.settings_open = false;
    harness.settle();
    harness.press(egui::Key::Z, egui::Modifiers::CTRL);
    assert!(!harness.app.tabs.active().can_undo(), "the undo ran once the modal was gone");
    assert!(harness.app.tabs.active().can_redo(), "and one edit was one undo entry");
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
fn the_grid_key_turns_the_running_drafts_grid_and_nothing_else() {
    // The key is read with the shell's table and its effect belongs to the draft in front: outside a
    // draft the shell asks for it and the document changes nothing (U96).
    let mut harness = Harness::new("frame-grid-key");
    harness.settle();
    let idle = harness.app.tabs.active().draw_gizmo.grid();
    harness.press(egui::Key::G, egui::Modifiers::NONE);
    assert_eq!(harness.app.tabs.active().draw_gizmo.grid(), idle, "no draft, no snap to turn");

    harness.app.tabs.active_mut().editor.start_drawing(furnicraft_ui::DrawTool::Line);
    harness.settle();
    let revision = harness.revision();
    harness.press(egui::Key::G, egui::Modifiers::NONE);
    assert_ne!(harness.app.tabs.active().draw_gizmo.grid(), idle, "the key turned the draft's grid");
    assert_eq!(harness.revision(), revision, "the grid is view state");
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
    harness.app.handle_header_action(HeaderAction::ToggleRibbon, Lang::En);
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
fn one_wide_panel() -> (furnicraft_engine::Session, Id) {
    let mm = furnicraft_testkit::millimetres;
    let root = furnicraft_model::Container::new(
        Id::new(1).expect("an id"),
        "Root",
        furnicraft_model::Placement::IDENTITY,
        Vec::new(),
    );
    let mut project = furnicraft_model::Project::new(root).expect("an empty project");
    let size = furnicraft_model::PanelNominalSize::new(mm(2000), mm(1500)).expect("a panel size");
    let panel = project
        .add_panel(project.root().id(), "Board", size, None, mm(18), furnicraft_model::Placement::IDENTITY)
        .expect("a panel at the origin");
    (furnicraft_engine::Session::with_catalog(project, furnicraft_testkit::sample_catalog()), panel)
}

/// A point on a translate handle of `node`, with the screen direction the handle points in.
///
/// The point sits past the dead zone around the origin and well inside the scene rectangle, so the
/// same point can be a handle in `Move` and a press in `Select`; picking the axis by where it lands
/// keeps the fixture's camera out of the test.
fn handle_point(harness: &Harness, node: Id) -> (egui::Pos2, egui::Vec2) {
    let pivot = harness.app.tabs.active().session.project().pivot_world(node).expect("the part has a pivot");
    let rect = harness.scene();
    let camera = harness.camera();
    let length = harness.app.settings.gizmo.handle_length();
    let origin = gizmo_math::screen_point(camera, rect, pivot).expect("the pivot is on screen");
    let spec = gizmo_math::HandleSpec { axes: gizmo_math::ALL_AXES, length };
    gizmo_math::axis_handles(camera, rect, pivot, origin, spec)
        .into_iter()
        .map(|handle| (origin + handle.direction * (length * 0.6), handle.direction))
        .find(|(at, _)| rect.contains(*at))
        .expect("a handle runs inside the scene rectangle")
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
        harness.drew(Message::Select.text(Lang::En)) && harness.drew(Message::Rotate.text(Lang::En)),
        "the bar offers Select beside the two gestures"
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

    // The bar's own cell changes the mode, and the same drag now moves the part.
    let cell = harness.mode_cell(Message::Move.text(Lang::En));
    harness.click(cell, egui::PointerButton::Primary);
    assert_eq!(harness.app.shell.mode, TransformMode::Move, "the bar's cell changes the mode");
    let revision = harness.revision();
    harness.drag(at, direction * 40.0, egui::PointerButton::Primary);
    assert_eq!(harness.revision().get(), revision.get() + 1, "Move draws the handle, so the press starts one drag");
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
    let draft = furnicraft_ui::product_card::ProductDraft {
        envelope: Envelope::new(
            furnicraft_testkit::millimetres(600),
            furnicraft_testkit::millimetres(560),
            furnicraft_testkit::millimetres(720),
        )
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
    let draft = furnicraft_ui::product_card::ProductDraft {
        envelope: Envelope::new(
            furnicraft_testkit::millimetres(600),
            furnicraft_testkit::millimetres(560),
            furnicraft_testkit::millimetres(720),
        )
        .expect("a valid box"),
        class: ClassCode::GENERAL_PRODUCT,
    };
    harness.app.tabs.active_mut().create_product(draft, Lang::En).expect("create the frame");
    let product = harness.app.tabs.active().session.active_product().expect("the frame is open");
    let before = harness.revision();

    // The cell lives on the Furniture tab, and a fresh workspace opens another one.
    if let Some(tab) = furnicraft_ui::toolbars::spec::tab("furniture") {
        harness.app.tabs.active_mut().editor.set_ribbon_tab(tab);
    }
    harness.settle();
    let hint = Message::PanelContourDraw.with(Lang::En, [Message::ToolContourPanel.text(Lang::En)]);
    let cell = harness.face(&hint).expect("the contour cell is on the strip");
    harness.click(cell, egui::PointerButton::Primary);
    assert_eq!(
        harness.app.tabs.active().editor.drawing_intent(),
        furnicraft_ui::DrawIntent::Panel,
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
fn collect_panels<'a>(children: &'a [furnicraft_model::Node], panels: &mut Vec<&'a furnicraft_model::Panel>) {
    for child in children {
        match child {
            furnicraft_model::Node::Panel(panel) => panels.push(panel),
            furnicraft_model::Node::Container(container) => collect_panels(container.children(), panels),
            furnicraft_model::Node::Object(_)
            | furnicraft_model::Node::Camera(_)
            | furnicraft_model::Node::Light(_) => {}
        }
    }
}

/// A product of 600 x 560 x 720 mm holding one 500 x 400 mm side panel, and both ids.
fn product_with_a_side_panel() -> (furnicraft_model::Project, Id, Id) {
    let mm = furnicraft_testkit::millimetres;
    let root = furnicraft_model::Container::new(
        Id::new(1).expect("an id"),
        "Root",
        furnicraft_model::Placement::IDENTITY,
        Vec::new(),
    );
    let mut project = furnicraft_model::Project::new(root).expect("an empty project");
    let envelope = Envelope::new(mm(600), mm(560), mm(720)).expect("a box");
    let product = project
        .add_product(
            project.root().id(),
            "Cabinet",
            ClassCode::GENERAL_PRODUCT,
            envelope,
            furnicraft_model::Placement::IDENTITY,
        )
        .expect("a product");
    let size = furnicraft_model::PanelNominalSize::new(mm(500), mm(400)).expect("a panel size");
    let panel = project
        .add_panel(product, "Side", size, None, mm(18), furnicraft_model::Placement::IDENTITY)
        .expect("a part inside the product");
    (project, product, panel)
}

#[test]
fn a_click_inside_an_open_product_selects_the_part_under_the_pointer() {
    // X38: the tree selected the part and the viewport selected the whole product, because the
    // product's openness is the session's view state and the pick rule read only the editor's open
    // set. Once a level is open, a click has to reach the part the pointer is on.
    let (project, product, panel) = product_with_a_side_panel();
    let mut session = furnicraft_engine::Session::with_catalog(project, furnicraft_testkit::sample_catalog());
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
        let centre = world.transform_point(furnicraft_geom::DVec3::new(250.0, 200.0, thickness * 0.5));
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
    let mut session = furnicraft_engine::Session::with_catalog(project, furnicraft_testkit::sample_catalog());
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
    use furnicraft_model::{NodeQuery, StructuralKind};

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
    let mut session = furnicraft_engine::Session::with_catalog(project, furnicraft_testkit::sample_catalog());
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
    // The frame is created and edited like a box (ADR 0101): the same base rectangle and height
    // pull, and the ✓ dispatches `product.create` instead of `object.primitive.create`.
    let mut harness = Harness::new("frame-drawn-product");
    harness.settle();
    let before = harness.revision();
    let mm = furnicraft_testkit::millimetres;
    {
        let document = harness.app.tabs.active_mut();
        document.editor.arm_box(furnicraft_ui::BoxOwner::Product);
        assert!(!document.editor.click_box([mm(600), mm(560)]).expect("the anchor"), "the first click only anchors");
        assert!(document.editor.click_box([mm(0), mm(0)]).expect("the base"), "the second fixes the base");
        document.editor.pull_box_height(mm(720)).expect("the pull");
        let ghost = document.editor.box_draft().and_then(furnicraft_ui::BoxDraft::sizes).expect("the ghost sizes");
        assert_eq!(ghost, [mm(600), mm(560), mm(720)]);
        document.finish_box(Lang::En).expect("the ✓ commits the drawn frame");
    }
    let document = harness.app.tabs.active();
    let frame = document.session.active_product().expect("the drawn frame is opened for its parts");
    let container = document.session.project().container(frame).expect("a container");
    assert!(container.is_product(), "a drawn frame is a product");
    let sides = container.envelope().expect("an envelope").components();
    assert_eq!(sides, [mm(600), mm(560), mm(720)], "the frame keeps exactly the ghost's three sides");
    assert_eq!(container.class(), ClassCode::GENERAL_PRODUCT, "the drawn gesture takes the placeholder class");
    assert_eq!(harness.revision().get(), before.get() + 1, "the whole gesture is one command");
    assert_eq!(document.editor.selected_nodes(), [frame], "the new frame is selected, so its grips show");
    assert_eq!(document.editor.creation_target(document.session.project()), frame, "the next part lands inside");
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
            gizmo_math::screen_point(camera, rect, world.transform_point(furnicraft_geom::DVec3::new(x, y, 0.0)))
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
        let size = furnicraft_geom::DVec3::new(
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
            .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, furnicraft_model::FillMode::Fit, Lang::En)
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

/// A harness whose open product holds two posts standing at an angle, plus the pointer at the
/// void's centre and the product's id.
///
/// The posts' inner faces run from `(100, 0)` to `(200, 560)` and from `(500, 0)` to `(400, 560)`,
/// so the free section is a trapezoid: 400 mm wide at the floor and 200 mm wide at the back.
fn splayed_shelf_harness(name: &str) -> (Harness, furnicraft_model::Id, egui::Pos2) {
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
        let rotation = furnicraft_geom::DQuat::from_rotation_z(normal.1.atan2(normal.0))
            * furnicraft_geom::DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2);
        let translation = furnicraft_geom::DVec3::new(start_x, start_y, 720.0)
            - furnicraft_geom::DVec3::new(normal.0, normal.1, 0.0) * 18.0;
        let placement = furnicraft_model::Placement::new(translation, rotation).expect("a placement");
        let nominal = furnicraft_model::PanelNominalSize::new(
            furnicraft_testkit::millimetres(720),
            furnicraft_testkit::millimetres(1200),
        )
        .expect("a post size");
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
    let size = furnicraft_geom::DVec3::new(
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
fn leaned_shelf_harness(name: &str) -> (Harness, furnicraft_model::Id, egui::Pos2) {
    let mut harness = Harness::with_session(name, crate::scene_build::empty_session().expect("empty"));
    harness.settle();
    harness.app.product_card.start();
    harness.settle();
    harness.press(egui::Key::Enter, egui::Modifiers::NONE);
    let document = harness.app.tabs.active_mut();
    let product = document.session.active_product().expect("the card opens the frame");
    let wall = furnicraft_geom::DQuat::from_rotation_z(-std::f64::consts::FRAC_PI_2)
        * furnicraft_geom::DQuat::from_rotation_y(-std::f64::consts::FRAC_PI_2);
    let rotation = furnicraft_geom::DQuat::from_rotation_x(10.0_f64.to_radians()) * wall;
    let placement =
        furnicraft_model::Placement::new(furnicraft_geom::DVec3::new(0.0, 560.0, 0.0), rotation).expect("a placement");
    let nominal = furnicraft_model::PanelNominalSize::new(
        furnicraft_testkit::millimetres(1600),
        furnicraft_testkit::millimetres(600),
    )
    .expect("a wall size");
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
    let size = furnicraft_geom::DVec3::new(
        envelope.x().millimetres_f64(),
        envelope.y().millimetres_f64(),
        envelope.z().millimetres_f64(),
    );
    // A point inside the void: the wall's face is at 560 - z tan 10, so 300 mm back is free.
    let inside = furnicraft_geom::DVec3::new(size.x * 0.5, size.y * 0.5, size.z * 0.5);
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
        .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, furnicraft_model::FillMode::Shape, Lang::En)
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
        .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, furnicraft_model::FillMode::Shape, Lang::En)
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
        .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, furnicraft_model::FillMode::Fit, Lang::En)
        .expect("the shelf fill arms inside the open frame");
    let before = harness.revision();
    harness.hover(pointer);
    harness.press_at(pointer, egui::PointerButton::Primary);
    let planned = harness.app.tabs.active().fill_gizmo.ghost().cloned().expect("the splayed void took a ghost");
    assert!(planned.outline.contour().is_none(), "a fit is a standard rectangle");
    assert_eq!(planned.nominal_size.length(), furnicraft_testkit::millimetres(200), "the narrowest width fits");
    assert_eq!(planned.nominal_size.width(), furnicraft_testkit::millimetres(560), "and the whole depth");
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
        let size = furnicraft_geom::DVec3::new(
            envelope.x().millimetres_f64(),
            envelope.y().millimetres_f64(),
            envelope.z().millimetres_f64(),
        );
        let placement = project.world_placement(product).expect("a frame");
        let centre = placement.transform_point(size * 0.5);
        // A point inside the void, off the centre line the two distance fields sit on, so the
        // creating click never lands on a plate.
        let away = placement.transform_point(furnicraft_geom::DVec3::new(size.x * 0.2, size.y * 0.2, size.z * 0.5));
        let rect = document.scene_rect().expect("a drawn frame gives the scene a rectangle");
        let camera = document.camera();
        let pointer = gizmo_math::screen_point(camera, rect, centre).expect("the frame is on screen");
        let away = gizmo_math::screen_point(camera, rect, away).expect("the left face is on screen");
        assert!(rect.contains(pointer) && rect.contains(away), "{away:?} in {rect:?}");
        document
            .editor
            .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, furnicraft_model::FillMode::Fit, Lang::En)
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
    let on_cluster = egui::pos2(scene.center().x, scene.bottom() - furnicraft_ui::theme::FLOATING_OFFSET - 8.0);
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

    // An armed primitive and a box do the same.
    harness.app.tabs.active_mut().editor.arm_box(furnicraft_ui::BoxOwner::Cuboid);
    harness.hover(centre);
    assert_eq!(harness.cursor, egui::CursorIcon::None, "a primitive tool keeps the shell's square");
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
    harness.app.settings.pointer.cursor_size = furnicraft_ui::settings::MIN_CURSOR_POINTS;
    harness.hover(centre);
    let small = harness.app.shell.work_cursor.expect("the scene draws its cursor");
    let smallest = f32::from(u16::try_from(furnicraft_ui::settings::MIN_CURSOR_POINTS).expect("a small constant"));
    assert_eq!(small.square.width(), smallest, "the setting is the square's side, in points");
    assert_eq!(small.square.center(), centre, "the smallest square is still centred");

    harness.app.settings.pointer.cursor_size = furnicraft_ui::settings::MAX_CURSOR_POINTS;
    harness.app.settings.pointer.crosshair = true;
    let view = harness.scene();
    harness.hover(view.center());
    let large = harness.app.shell.work_cursor.expect("the scene draws its cursor");
    let largest = f32::from(u16::try_from(furnicraft_ui::settings::MAX_CURSOR_POINTS).expect("a small constant"));
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
fn a_box_shows_its_height_in_the_card_and_the_inspector_and_they_never_disagree() {
    let mut harness = Harness::with_session("frame-tool-card-box", crate::scene_build::empty_session().expect("empty"));
    harness.settle();
    let mm = furnicraft_testkit::millimetres;
    {
        let document = harness.app.tabs.active_mut();
        document.editor.arm_box(furnicraft_ui::BoxOwner::Cuboid);
        assert!(!document.editor.click_box([mm(0), mm(0)]).expect("the anchor"), "the first click only anchors");
        assert!(document.editor.click_box([mm(300), mm(200)]).expect("the base"), "the second fixes the base");
        document.editor.pull_box_height(mm(120)).expect("the pull");
    }
    let chrome = harness.settle();
    let document = harness.app.tabs.active();
    let card = document.tool_card_rect().expect("the height pull draws its card");
    let in_card = texts_inside(&harness, card);
    assert!(in_card.iter().any(|text| text == "120"), "the card shows the pulled height: {in_card:?}");
    assert!(in_card.iter().any(|text| text == "Box height"), "and names what it sizes: {in_card:?}");
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
        let size = furnicraft_geom::DVec3::new(
            envelope.x().millimetres_f64(),
            envelope.y().millimetres_f64(),
            envelope.z().millimetres_f64(),
        );
        let centre = project.world_placement(product).expect("a frame").transform_point(size * 0.5);
        let rect = document.scene_rect().expect("a drawn frame gives the scene a rectangle");
        let pointer = gizmo_math::screen_point(document.camera(), rect, centre).expect("the frame is on screen");
        document
            .editor
            .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, furnicraft_model::FillMode::Fit, Lang::En)
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
fn one_body() -> (furnicraft_engine::Session, Id) {
    use furnicraft_model::{Constructive, Container, Geometry, Path, Placement, Project};
    let mm = furnicraft_testkit::millimetres;
    let root = Container::new(Id::new(1).expect("an id"), "Scene", Placement::IDENTITY, Vec::new());
    let mut project = Project::new(root).expect("an empty project");
    let ring = Path::rectangle([mm(0), mm(0)], [mm(600), mm(400)]).expect("a rectangle");
    let body = Constructive::extrude(ring, mm(18)).expect("a body");
    let id = project
        .add_object(project.root().id(), "Body", Placement::IDENTITY, Geometry::Constructive(body))
        .expect("the body is added");
    (furnicraft_engine::Session::with_catalog(project, furnicraft_testkit::sample_catalog()), id)
}

#[test]
fn a_pull_shows_its_height_in_the_card_and_the_inspector_and_they_never_disagree() {
    let (session, body) = one_body();
    let mut harness = Harness::with_session("frame-tool-card-pull", session);
    harness.settle();
    {
        let document = harness.app.tabs.active_mut();
        document.editor.select_node(Some(body));
        document.extrude_gizmo.start(body, furnicraft_testkit::millimetres(40));
    }
    harness.run(vec![egui::Event::PointerMoved(harness.scene().center())]);
    let chrome = harness.settle();
    let document = harness.app.tabs.active();
    let card = document.tool_card_rect().expect("the pull draws its card");
    let in_card = texts_inside(&harness, card);
    assert!(in_card.iter().any(|text| text == "40"), "the card shows the pulled height: {in_card:?}");
    assert_the_card_and_the_inspector_agree(&harness, &chrome);
}

#[test]
fn a_press_inside_the_card_is_chrome_and_never_a_pick_or_a_finish() {
    let (session, body) = one_body();
    let mut harness = Harness::with_session("frame-tool-card-chrome", session);
    harness.settle();
    {
        let document = harness.app.tabs.active_mut();
        document.editor.select_node(Some(body));
        document.extrude_gizmo.start(body, furnicraft_testkit::millimetres(40));
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
    let mm = furnicraft_testkit::millimetres;
    {
        let document = harness.app.tabs.active_mut();
        document.editor.arm_box(furnicraft_ui::BoxOwner::Cuboid);
        assert!(!document.editor.click_box([mm(0), mm(0)]).expect("the anchor"), "the first click only anchors");
        assert!(document.editor.click_box([mm(300), mm(200)]).expect("the base"));
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
        matches!(harness.app.tabs.active().measure_gizmo.hover().map(|hover| hover.target), Some(SnapTarget::Grid)),
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
    let draft = furnicraft_ui::product_card::ProductDraft {
        envelope: Envelope::new(
            furnicraft_testkit::millimetres(600),
            furnicraft_testkit::millimetres(560),
            furnicraft_testkit::millimetres(720),
        )
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
        let size = furnicraft_geom::DVec3::new(
            envelope.x().millimetres_f64(),
            envelope.y().millimetres_f64(),
            envelope.z().millimetres_f64(),
        );
        let centre = project.world_placement(product).expect("a frame").transform_point(size * 0.5);
        let rect = document.scene_rect().expect("a drawn frame gives the scene a rectangle");
        gizmo_math::screen_point(document.camera(), rect, centre).expect("the frame is on screen")
    };

    // The cell lives on the Furniture tab, and a fresh workspace opens the Sketch tab.
    if let Some(tab) = furnicraft_ui::toolbars::spec::tab("furniture") {
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
            .arm_fill(&document.session, ClassCode::HORIZONTAL_PANEL, furnicraft_model::FillMode::Fit, Lang::En)
            .expect("the shelf fill arms inside the open frame");
    }
    harness.hover(pointer);
    let ghost = harness.app.tabs.active().fill_gizmo.ghost().cloned().expect("the fill plans a part");
    assert_eq!(ghost.niche, measured, "one pointer, one cell: the fill reads the niche tool's value");
    assert_eq!(harness.revision(), revision, "and the fill wrote nothing before its click");
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
    assert_eq!(hover.target, SnapTarget::Surface, "the panel face captures the pointer");
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
    let local = inverse.transform_point(furnicraft_geom::DVec3::new(
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
fn the_ruler_measures_the_board_inside_a_product_not_its_reserved_box() {
    // The reserved box is a probe surface (ADR 0103) and the first thing the ray meets from
    // outside, so the ruler steps past its near face: the person points at the board inside.
    let (project, product, panel) = product_with_a_side_panel();
    let mut session = furnicraft_engine::Session::with_catalog(project, furnicraft_testkit::sample_catalog());
    assert!(session.open_product(product), "the fixture product opens");
    let mut harness = Harness::with_session("frame-measure-product", session);
    harness.settle();
    harness.app.tabs.active_mut().frame_all();
    harness.settle();

    let at = harness.node_point(panel);
    harness.press(egui::Key::M, egui::Modifiers::NONE);
    harness.hover(at);
    let hover = harness.app.tabs.active().measure_gizmo.hover().expect("the pointer resolves a point");
    assert_eq!(hover.target, SnapTarget::Surface, "the board captures the pointer, not the envelope");
    harness.click(at, egui::PointerButton::Primary);

    let document = harness.app.tabs.active();
    let first = document.editor.measure().and_then(furnicraft_ui::Measure::first).expect("the point is stored");
    let project = document.session.project();
    let piece = project.panel(panel).expect("the node is a panel");
    let world = project.world_placement(panel).expect("the panel has a world frame");
    let local = world.inverse().expect("a rigid placement inverts").transform_point(furnicraft_geom::DVec3::new(
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
    harness.app.tabs.active_mut().editor.start_drawing(furnicraft_ui::DrawTool::Line);
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
    let at =
        world.transform_point(furnicraft_geom::DVec3::new(start[0].millimetres_f64(), start[1].millimetres_f64(), 0.0));
    let anchor_at = furnicraft_geom::DVec3::new(anchor[0].millimetres_f64(), anchor[1].millimetres_f64(), 0.0);
    assert!((at - anchor_at).length() < 0.01, "the line starts where the first click placed it: {at:?}");
    assert_eq!(harness.revision().get(), before.get() + 1, "one click, one point, one path, one undo entry");
    assert!(document.can_undo());
}

#[test]
fn a_typed_number_outlives_the_pointer_and_escape_gives_the_keys_back() {
    // A typed number is a promise: the pointer and its snap cannot move it. Escape drops the promise
    // first and ends the tool second, so one key never means two things in one frame (ADR 0126).
    let mut harness = Harness::new("frame-sketch-lock");
    harness.settle();
    let before = harness.revision();
    harness.app.tabs.active_mut().editor.start_drawing(furnicraft_ui::DrawTool::Line);
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
fn the_polyline_segment_kind_comes_from_the_right_click_menu() {
    // Tab belongs to the plates now, so a polyline's line/arc switch moved to the right-click menu;
    // the menu is a chrome surface, so choosing a row never places a point behind it (ADR 0126,
    // 0128).
    let mut harness = Harness::new("frame-sketch-menu");
    harness.settle();
    harness.app.tabs.active_mut().editor.start_drawing(furnicraft_ui::DrawTool::Polyline);
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
    harness.app.tabs.active_mut().editor.start_drawing(furnicraft_ui::DrawTool::CircleDiameter);
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
    let at = world.transform_point(furnicraft_geom::DVec3::ZERO);
    assert!(
        (at - furnicraft_geom::DVec3::new(100.0, 100.0, 0.0)).length() < 0.01,
        "centred where the plates said: {at:?}"
    );
    assert_eq!(harness.revision().get(), before.get() + 1, "one circle, one undo entry");
}

#[test]
fn an_arc_from_a_typed_radius_takes_the_side_the_pointer_chose() {
    // C4: an arc from its start, end and radius. The two ends are typed, so the chord is exact; the
    // radius is typed and the pointer only names the side the centre sits on.
    let mut harness = Harness::new("frame-sketch-arc-radius");
    harness.settle();
    harness.app.tabs.active_mut().editor.start_drawing(furnicraft_ui::DrawTool::ArcByRadius);
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
        let at = world.transform_point(furnicraft_geom::DVec3::new(point.x, point.y, 0.0));
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
    harness.app.tabs.active_mut().editor.start_drawing(furnicraft_ui::DrawTool::ArcByRadius);
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
    harness.app.tabs.active_mut().editor.start_drawing(furnicraft_ui::DrawTool::PolygonApothem);
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
    harness.app.tabs.active_mut().editor.start_drawing(furnicraft_ui::DrawTool::PolygonByEdge);
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
    harness.app.tabs.active_mut().editor.start_drawing(furnicraft_ui::DrawTool::RectangleTilted);
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
    harness.app.tabs.active_mut().editor.start_drawing(furnicraft_ui::DrawTool::RegularPolygon);
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
fn ring_vertices(path: &furnicraft_model::Path, world: furnicraft_model::Placement) -> Vec<DVec2> {
    path.entries()
        .iter()
        .filter_map(|entry| entry.segment().start().ok())
        .map(|point| {
            let at = world.transform_point(furnicraft_geom::DVec3::new(
                point[0].millimetres_f64(),
                point[1].millimetres_f64(),
                0.0,
            ));
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
fn one_sketch_path() -> (furnicraft_engine::Session, Id) {
    let mm = furnicraft_testkit::millimetres;
    let mut project = furnicraft_testkit::sample_project();
    let root = project.root().id();
    let path = furnicraft_model::Path::new(
        vec![
            furnicraft_model::ContourSegment::line([mm(0), mm(0)], [mm(120), mm(0)]).expect("the first side"),
            furnicraft_model::ContourSegment::line([mm(120), mm(0)], [mm(120), mm(80)]).expect("the second side"),
        ],
        false,
    )
    .expect("an open chain");
    let object = project
        .add_object(root, "Sketch", furnicraft_model::Placement::IDENTITY, furnicraft_model::Geometry::Path(path))
        .expect("a path object");
    (furnicraft_engine::Session::with_catalog(project, furnicraft_testkit::sample_catalog()), object)
}

/// A headless frame in contour edit mode on the open chain, framed so its corners are grabbable.
///
/// The mode is entered the way the inspector card enters it, and the chain's first corner is the one
/// every latch test moves.
fn sketching(name: &str) -> (Harness, Id, furnicraft_model::PathCorner) {
    let (session, object) = one_sketch_path();
    let mut harness = Harness::with_session(name, session);
    harness.settle();
    let corner = {
        let document = harness.app.tabs.active();
        let path = document.session.project().object_path(object).expect("the path");
        furnicraft_model::PathCorner::Start(path.id_at(0).expect("the first segment"))
    };
    {
        let document = harness.app.tabs.active_mut();
        document.editor.select_node(Some(object));
        assert!(
            document.editor.start_contour_edit(document.session.project(), furnicraft_ui::ContourTarget::Path(object)),
            "an open chain is editable"
        );
    }
    harness.app.tabs.active_mut().frame_all();
    harness.settle();
    (harness, object, corner)
}

/// The screen point of one point of a node's own plane, in millimetres.
fn node_point_mm(harness: &Harness, node: Id, local: [f64; 2]) -> egui::Pos2 {
    let document = harness.app.tabs.active();
    let world = document.session.project().world_placement(node).expect("the node has a world frame");
    let rect = document.scene_rect().expect("a settled frame gives the scene a rectangle");
    let at = furnicraft_geom::DVec3::new(local[0], local[1], 0.0);
    gizmo_math::screen_point(document.camera(), rect, world.transform_point(at)).expect("the point is on screen")
}

/// The point of a node's own plane under a viewport position, in the units the gizmo resolves.
fn node_point_under(harness: &Harness, node: Id, pointer: egui::Pos2) -> [Length; 2] {
    let document = harness.app.tabs.active();
    let project = document.session.project();
    let placement = project.world_placement(node).expect("the node has a world frame");
    let rect = document.scene_rect().expect("a settled frame gives the scene a rectangle");
    let ray = gizmo_math::ray_to(pointer, rect, document.camera());
    let origin = placement.transform_point(furnicraft_geom::DVec3::ZERO);
    let normal = placement.transform_vector(furnicraft_geom::DVec3::Z);
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
fn drafted_corner(harness: &Harness, corner: furnicraft_model::PathCorner) -> [Length; 2] {
    harness
        .app
        .tabs
        .active()
        .editor
        .contour_edit()
        .expect("a live contour edit")
        .draft()
        .grip_position(furnicraft_ui::ContourGrip::Corner(corner))
        .expect("the corner is on the draft")
}

/// The corner the document stores.
fn stored_corner(harness: &Harness, object: Id, corner: furnicraft_model::PathCorner) -> [Length; 2] {
    let path = harness.app.tabs.active().session.project().object_path(object).expect("the path");
    furnicraft_ui::ContourDraft::Chain(path.clone())
        .grip_position(furnicraft_ui::ContourGrip::Corner(corner))
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
fn sketch_tools() -> [DrawTool; 15] {
    [
        DrawTool::Line,
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
        DrawTool::RegularPolygon,
        DrawTool::PolygonApothem,
        DrawTool::PolygonByEdge,
    ]
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
            !rect.expand(furnicraft_ui::CURSOR_CLEARANCE - 1.0).contains(pointer),
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
    for tool in sketch_tools() {
        let need = tool.points().unwrap_or(3);
        for placed in 0..need {
            arm_sketch(&mut harness, tool);
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
