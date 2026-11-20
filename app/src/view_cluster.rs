//! The floating viewport cluster: navigation, framing, display style, layout and display settings.
//!
//! The cluster floats over the scene instead of living in a bar, so the viewport starts at the model
//! and not at a wall of labels (U40). It is anchored to the bottom centre of the viewport it belongs
//! to and follows that view when a side panel folds or the workspace splits; nothing here is
//! document data. The shell owns the cameras, the gizmos and the drawing tools, so the cluster only
//! reports the actions it cannot perform itself — framing, the projection toggle, the layout choice
//! and the display style, which belongs to the active leaf. Its own settings and the armed
//! navigation mode are view state on [`ViewCluster`].

use eframe::egui;
use scene::Projection;
use ui::display::{OpeningView, ShadingStyle, shading_caption, shading_icon, shading_label};
use ui::{icons, theme};

use crate::view_layout::{LayoutKind, MAX_LEAVES};

/// The navigation gesture a primary-button drag performs while it is armed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NavigationMode {
    /// Turn the camera about its target.
    Orbit,
    /// Move the camera across the view plane.
    Pan,
    /// Move the camera along its forward axis.
    Zoom,
}

impl NavigationMode {
    /// Every mode in the order the cluster draws them.
    pub(crate) const ALL: [Self; 3] = [Self::Orbit, Self::Pan, Self::Zoom];

    /// The short name the status bar shows.
    pub(crate) const fn name(self) -> text::Message {
        match self {
            Self::Orbit => text::Message::ShortcutOrbit,
            Self::Pan => text::Message::ShortcutPan,
            Self::Zoom => text::Message::ShortcutZoom,
        }
    }

    /// The gesture the status bar asks for once the mode is armed.
    pub(crate) const fn gesture(self) -> text::Message {
        match self {
            Self::Orbit => text::Message::GestureOrbit,
            Self::Pan => text::Message::GesturePan,
            Self::Zoom => text::Message::GestureZoom,
        }
    }

    /// The tooltip and the accessible name.
    pub(crate) const fn label(self) -> text::Message {
        match self {
            Self::Orbit => text::Message::NavOrbitLabel,
            Self::Pan => text::Message::NavPanLabel,
            Self::Zoom => text::Message::NavZoomLabel,
        }
    }

    /// The icon asset that names the gesture once it is drawn.
    pub(crate) const fn icon(self) -> icons::Icon {
        match self {
            Self::Orbit => icons::view::ORBIT,
            Self::Pan => icons::view::PAN,
            Self::Zoom => icons::view::ZOOM,
        }
    }
}

/// The cluster's view state.
///
/// The cluster keeps the switches that change what the view draws from one frame to the next; the
/// strengths of the floor grid, the outlines and the ground shadow are workspace defaults and live
/// in `ui::settings::ViewportSettings`, which the cluster links to instead of drawing
/// three sliders over the scene (U64). The display style and the ground-shadow switch are not here
/// either: both belong to the active leaf, so two views can differ (U63, U107).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ViewCluster {
    /// The navigation gesture armed for the next primary-button drag, or `None`.
    pub(crate) navigation: Option<NavigationMode>,
    /// Whether the display-settings popover is open.
    pub(crate) settings_open: bool,
    /// Whether the split-view choices are open.
    pub(crate) layout_open: bool,
    /// Whether the corner draws the on-screen display: the angles, the projection, the frame rate
    /// and the scene's cost (U42, U106).
    pub(crate) show_osd: bool,
    /// Whether the corner orientation widget draws at all (U73).
    pub(crate) show_widget: bool,
}

impl Default for ViewCluster {
    fn default() -> Self {
        Self { navigation: None, settings_open: false, layout_open: false, show_osd: false, show_widget: true }
    }
}

impl ViewCluster {
    /// Whether the corner draws its on-screen display.
    pub(crate) fn osd_visible(&self) -> bool {
        self.show_osd
    }

    /// Whether the corner orientation widget draws at all.
    pub(crate) fn widget_visible(&self) -> bool {
        self.show_widget
    }
}

/// What the cluster reads from the shell for one frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClusterView {
    /// Whether the viewport owns its navigation and projection.
    pub(crate) owns_view: bool,
    /// The projection the target view is drawing with.
    pub(crate) projection: Projection,
    /// The work plane the pointer maps onto, for the settings popover.
    pub(crate) work_plane: &'static str,
    /// The layout the workspace is in.
    pub(crate) layout: LayoutKind,
    /// Whether one view fills the whole workspace.
    pub(crate) maximized: bool,
    /// The preset the target view is in, if it has not been orbited since.
    pub(crate) preset: Option<OpeningView>,
    /// The display style of the target view; the style buttons draw it pressed.
    pub(crate) style: ShadingStyle,
}

/// What the cluster asks the shell to do; everything else it changes on its own state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClusterAction {
    /// Frame the whole scene in the target view.
    FrameAll,
    /// Return the target view to the axonometric home view.
    Home,
    /// Switch the target view between perspective and orthographic.
    ToggleProjection,
    /// Divide the workspace into this layout.
    SetLayout(LayoutKind),
    /// Expand the active view over the whole workspace, or return the tree.
    ToggleMaximize,
    /// Point the target view along a standard preset.
    SetPreset(OpeningView),
    /// Draw the target view with this display style.
    SetStyle(ShadingStyle),
    /// Open Settings on the *View* page, where the display strengths live.
    OpenSettings,
}

/// The point the cluster's bottom centre is pinned to: the bottom centre of `viewport`, raised by
/// `offset`.
///
/// The anchor is derived from the viewport rect rather than from the window, so folding a side panel
/// moves the cluster with the surface it floats over instead of leaving it over the new panel.
pub(crate) fn cluster_anchor(viewport: egui::Rect, offset: f32) -> egui::Pos2 {
    egui::pos2(viewport.center().x, viewport.bottom() - offset)
}

/// The fill of the floating cluster and its popovers at the shared chrome opacity.
///
/// The cluster sits over the scene and must stay readable even when the panels are set almost
/// transparent, so its own alpha never drops below 80 %.
pub(crate) fn cluster_fill(ui: &egui::Ui, opacity: u32) -> egui::Color32 {
    theme::at_opacity(theme::floating(ui), opacity.clamp(80, 100))
}

/// Draws the cluster over `viewport` and applies its own view-state changes.
///
/// `lift` raises the cluster by that many points, so the command bar of a previewing operation can
/// take the bottom-centre slot without the two overlapping (M71).
pub(crate) fn show(
    ui: &mut egui::Ui,
    viewport: egui::Rect,
    state: &mut ViewCluster,
    view: ClusterView,
    opacity: u32,
    lift: f32,
) -> Option<ClusterAction> {
    let mut action = None;
    let response = egui::Area::new(egui::Id::unique("view_cluster"))
        .order(egui::Order::Middle)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(cluster_anchor(viewport, theme::FLOATING_OFFSET + lift))
        .constrain_to(viewport)
        .show(ui.ctx(), |ui| {
            let frame = egui::Frame::new()
                .fill(cluster_fill(ui, opacity))
                .stroke(egui::Stroke::new(1.0, theme::line(ui)))
                .corner_radius(egui::CornerRadius::same(theme::RADIUS))
                .inner_margin(egui::Margin::symmetric(6, 4));
            frame.show(ui, |ui| {
                // The cluster floats over the viewport, so it may never grow wider than the surface
                // it covers: the wrapped row folds onto a second line in a narrow window instead of
                // being clipped by the central panel.
                ui.set_max_width((viewport.width() - 2.0 * theme::FLOATING_OFFSET).max(0.0));
                ui.horizontal_wrapped(|ui| {
                    navigation_buttons(ui, state);
                    theme::divider(ui);
                    if icons::tool_button(
                        ui,
                        icons::view::FRAME_ALL,
                        text::Message::ToolFrameAll.text(ui::language(ui.ctx())),
                        Some("F"),
                        true,
                        false,
                    )
                    .clicked()
                    {
                        action = Some(ClusterAction::FrameAll);
                    }
                    if icons::tool_button(
                        ui,
                        icons::view::HOME,
                        text::Message::HeaderHome.text(ui::language(ui.ctx())),
                        None,
                        true,
                        false,
                    )
                    .clicked()
                    {
                        action = Some(ClusterAction::Home);
                    }
                    theme::divider(ui);
                    if let Some(style) = style_buttons(ui, view.style, ui::language(ui.ctx())) {
                        action = Some(ClusterAction::SetStyle(style));
                    }
                    theme::divider(ui);
                    let open = state.layout_open;
                    if icons::tool_button(
                        ui,
                        view.layout.icon(),
                        text::Message::Layout.text(ui::language(ui.ctx())),
                        None,
                        true,
                        open,
                    )
                    .clicked()
                    {
                        state.layout_open = !state.layout_open;
                        state.settings_open = false;
                    }
                    // A single view already fills the workspace, so there is nothing to expand: the
                    // button is drawn disabled instead of entering a mode it cannot leave.
                    let (icon, label) = if view.maximized {
                        (icons::view::RESTORE, text::Message::Restore)
                    } else {
                        (icons::view::MAXIMIZE, text::Message::Expand)
                    };
                    if icons::tool_button(
                        ui,
                        icon,
                        label.text(ui::language(ui.ctx())),
                        Some("Alt+W"),
                        view.layout.leaves() > 1,
                        view.maximized,
                    )
                    .clicked()
                    {
                        action = Some(ClusterAction::ToggleMaximize);
                    }
                    let open = state.settings_open;
                    if icons::tool_button(
                        ui,
                        icons::view::SLIDERS,
                        text::Message::DisplaySettings.text(ui::language(ui.ctx())),
                        None,
                        true,
                        open,
                    )
                    .clicked()
                    {
                        state.settings_open = !state.settings_open;
                        state.layout_open = false;
                    }
                });
            });
        });
    let rect = response.response.rect;

    if state.layout_open {
        if ui.ctx().input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            state.layout_open = false;
        } else if let Some(asked) = layout_popover(ui, viewport, rect, view, opacity) {
            state.layout_open = false;
            action = Some(asked);
        }
    }
    if state.settings_open {
        if ui.ctx().input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            state.settings_open = false;
        } else if let Some(asked) = settings_popover(ui, viewport, rect, state, view, opacity) {
            action = Some(asked);
        }
    }
    action
}

/// The orbit, pan and zoom buttons; the armed one draws pressed and a second click disarms it.
fn navigation_buttons(ui: &mut egui::Ui, state: &mut ViewCluster) {
    for mode in NavigationMode::ALL {
        let on = state.navigation == Some(mode);
        if icons::tool_button_labelled(
            ui,
            mode.icon(),
            navigation_caption(mode).text(ui::language(ui.ctx())),
            mode.label().text(ui::language(ui.ctx())),
            true,
            on,
        )
        .clicked()
        {
            state.navigation = (!on).then_some(mode);
        }
    }
}

/// The short placeholder face of one navigation mode.
const fn navigation_caption(mode: NavigationMode) -> text::Message {
    match mode {
        NavigationMode::Orbit => text::Message::ShortcutOrbit,
        NavigationMode::Pan => text::Message::ShortcutPan,
        NavigationMode::Zoom => text::Message::ShortcutZoom,
    }
}

/// The five display styles as one band: states of one value are one control, not five switches.
///
/// The cells touch with no gap inside one tinted frame, and the view's own style draws pressed,
/// exactly as the transform band draws the window's mode (ADR 0113). The band names itself once,
/// because five glyphs alone read as five unrelated switches: the group's accessible name and its
/// hover line say it is this view's shading (U118).
fn style_buttons(ui: &mut egui::Ui, current: ShadingStyle, lang: text::Lang) -> Option<ShadingStyle> {
    let band = egui::Frame::new()
        .fill(theme::control(ui))
        .corner_radius(egui::CornerRadius::same(theme::RADIUS))
        .inner_margin(egui::Margin::symmetric(1, 0));
    let mut asked = None;
    let response = band.show(ui, |ui| {
        // Touching cells are what makes one band out of five states; the row's own gap stays.
        ui.spacing_mut().item_spacing.x = 0.0;
        for style in ShadingStyle::ALL {
            let on = current == style;
            if icons::tool_button_labelled(
                ui,
                shading_icon(style),
                shading_caption(style, lang),
                shading_label(style, lang),
                true,
                on,
            )
            .clicked()
            {
                asked = Some(style);
            }
        }
    });
    let name = text::Message::ShadingGroup.text(lang);
    response.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Group, ui.is_enabled(), name));
    response.response.on_hover_text(text::Message::ShadingGroupHint.text(lang));
    asked
}

/// The split-view choices above the cluster: one view, two vertical, two horizontal and four.
///
/// The active layout draws pressed, so the bar and the workspace agree without a second label. A
/// choice is always present — the button that returns to one view is the same row — so a border is
/// never the only way out of a split.
fn layout_popover(
    ui: &mut egui::Ui,
    viewport: egui::Rect,
    cluster: egui::Rect,
    view: ClusterView,
    opacity: u32,
) -> Option<ClusterAction> {
    let mut action = None;
    let anchor = egui::pos2(cluster.center().x, cluster.top() - theme::STEP);
    egui::Area::new(egui::Id::unique("view_cluster_layout"))
        .order(egui::Order::Middle)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(anchor)
        .constrain_to(viewport)
        .show(ui.ctx(), |ui| {
            let frame = egui::Frame::new()
                .fill(cluster_fill(ui, opacity))
                .stroke(egui::Stroke::new(1.0, theme::line(ui)))
                .corner_radius(egui::CornerRadius::same(theme::RADIUS))
                .inner_margin(egui::Margin::same(10));
            frame.show(ui, |ui| {
                ui.set_width(220.0);
                ui.label(egui::RichText::new(text::Message::Layout.text(ui::language(ui.ctx()))).strong());
                for kind in LayoutKind::ALL {
                    let on = view.layout == kind;
                    let label = kind.label().text(ui::language(ui.ctx()));
                    if glyph_row(ui, kind.icon(), label, on) {
                        action = Some(ClusterAction::SetLayout(kind));
                    }
                }
                ui.separator();
                ui.label(egui::RichText::new(text::Message::ViewPreset.text(ui::language(ui.ctx()))).strong());
                for preset in OpeningView::ALL {
                    if preset_row(ui, preset, view.preset == Some(preset), ui::language(ui.ctx())) {
                        action = Some(ClusterAction::SetPreset(preset));
                    }
                }
                ui.separator();
                ui.label(
                    egui::RichText::new(
                        text::Message::ViewsOfMax.with(ui::language(ui.ctx()), [view.layout.leaves(), MAX_LEAVES]),
                    )
                    .small()
                    .color(theme::text_dim(ui)),
                );
            });
        });
    action
}

/// The icon of a standard view: a screen plane with the direction the eye looks from.
pub(crate) fn preset_icon(view: OpeningView) -> icons::Icon {
    match view {
        OpeningView::Axonometric => icons::view::AXON,
        OpeningView::Front => icons::view::FRONT,
        OpeningView::Left => icons::view::LEFT,
        OpeningView::Top => icons::view::TOP,
        OpeningView::Right => icons::view::RIGHT,
        OpeningView::Bottom => icons::view::BOTTOM,
    }
}

/// One row of a popover list: the glyph, its name and the selected face.
///
/// A popover row is not a text button: the preset and split lists are where a named view is a glyph
/// the user learns, so the row draws the icon and keeps the name beside it as the label (U118).
fn glyph_row(ui: &mut egui::Ui, icon: icons::Icon, label: &str, on: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&response, on);
        ui.painter().rect(
            rect,
            egui::CornerRadius::same(theme::RADIUS),
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        let glyph =
            egui::Rect::from_center_size(egui::pos2(rect.left() + 14.0, rect.center().y), egui::Vec2::splat(16.0));
        icons::draw_icon(ui, icon, glyph, 16.0, visuals.fg_stroke.color);
        ui.painter().text(
            egui::pos2(rect.left() + 28.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(13.0),
            visuals.fg_stroke.color,
        );
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), on, label));
    }
    response.clicked()
}

/// One row of the preset list: the view's glyph, its name and the selected face.
fn preset_row(ui: &mut egui::Ui, view: OpeningView, on: bool, lang: text::Lang) -> bool {
    glyph_row(ui, preset_icon(view), view.label(lang), on)
}

/// The display-settings popover above the cluster: the angle and widget switches, the projection,
/// the contact shadow and the link to the page that owns the strengths.
fn settings_popover(
    ui: &mut egui::Ui,
    viewport: egui::Rect,
    cluster: egui::Rect,
    state: &mut ViewCluster,
    view: ClusterView,
    opacity: u32,
) -> Option<ClusterAction> {
    let mut action = None;
    let anchor = egui::pos2(cluster.center().x, cluster.top() - theme::STEP);
    egui::Area::new(egui::Id::unique("view_cluster_settings"))
        .order(egui::Order::Middle)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(anchor)
        .constrain_to(viewport)
        .show(ui.ctx(), |ui| {
            let frame = egui::Frame::new()
                .fill(cluster_fill(ui, opacity))
                .stroke(egui::Stroke::new(1.0, theme::line(ui)))
                .corner_radius(egui::CornerRadius::same(theme::RADIUS))
                .inner_margin(egui::Margin::same(10));
            frame.show(ui, |ui| {
                ui.set_width(230.0);
                ui.label(egui::RichText::new(text::Message::DisplayHeading.text(ui::language(ui.ctx()))).strong());
                let osd =
                    ui.checkbox(&mut state.show_osd, text::Message::ShowOnScreenDisplay.text(ui::language(ui.ctx())));
                osd.on_hover_text(
                    if state.osd_visible() {
                        text::Message::OnScreenDisplayOn
                    } else {
                        text::Message::OnScreenDisplayOff
                    }
                    .text(ui::language(ui.ctx())),
                );
                // The cube itself has its own switch beside the readout: a user who wants the corner
                // free for the model keeps the readout's switch alone and turns the whole widget off (U73).
                let widget = ui.checkbox(
                    &mut state.show_widget,
                    text::Message::ShowOrientationWidget.text(ui::language(ui.ctx())),
                );
                widget.on_hover_text(
                    if state.widget_visible() {
                        text::Message::OrientationWidgetOn
                    } else {
                        text::Message::OrientationWidgetOff
                    }
                    .text(ui::language(ui.ctx())),
                );
                let (icon, hint) = projection_button(view.projection);
                let toggle =
                    icons::tool_button(ui, icon, hint.text(ui::language(ui.ctx())), Some("O"), view.owns_view, false);
                if toggle
                    .on_disabled_hover_text(text::Message::AuthoredCameraOwns.text(ui::language(ui.ctx())))
                    .clicked()
                {
                    action = Some(ClusterAction::ToggleProjection);
                }
                // The ground shadow's switch is not here: it belongs to the view, so it is drawn
                // once in the View tab's Display group, beside its intensity (U107).
                ui.separator();
                // The strengths are workspace defaults, not a scene opinion: the popover links to the
                // one page that edits them instead of drawing three sliders over the model (U64).
                if ui
                    .button(text::Message::DisplayConfigure.text(ui::language(ui.ctx())))
                    .on_hover_text(text::Message::DisplayConfigureHint.text(ui::language(ui.ctx())))
                    .clicked()
                {
                    state.settings_open = false;
                    action = Some(ClusterAction::OpenSettings);
                }
                ui.separator();
                ui.label(
                    egui::RichText::new(text::Message::WorkPlane.with(ui::language(ui.ctx()), [view.work_plane]))
                        .small()
                        .color(theme::text_dim(ui)),
                );
            });
        });
    action
}

/// The icon and the hint of the projection toggle.
///
/// The icon draws the mode the view is *in*, and the hint names the switch, so the control never
/// claims a state the camera has not reached.
pub(crate) fn projection_button(projection: Projection) -> (icons::Icon, text::Message) {
    match projection {
        Projection::Perspective { .. } => (icons::view::PERSPECTIVE, text::Message::PerspectiveViewHint),
        Projection::Orthographic { .. } => (icons::view::ORTHOGRAPHIC, text::Message::OrthographicViewHint),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{drawn_text, drawn_text_rects};

    /// The rectangle the cluster floats over in these tests.
    fn viewport() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0))
    }

    /// The view a test puts in front of the cluster.
    fn cluster_view(layout: LayoutKind) -> ClusterView {
        ClusterView {
            owns_view: true,
            projection: Projection::Perspective { fov_y: 0.9 },
            work_plane: "XY",
            layout,
            maximized: false,
            preset: None,
            style: ShadingStyle::default(),
        }
    }

    /// One frame of the cluster: the action it asked for and what it drew.
    fn frame(
        ctx: &egui::Context,
        state: &mut ViewCluster,
        view: ClusterView,
        events: Vec<egui::Event>,
    ) -> (Option<ClusterAction>, egui::FullOutput) {
        let mut action = None;
        let output = ctx.run_ui(egui::RawInput { screen_rect: Some(viewport()), events, ..Default::default() }, |ui| {
            action = show(ui, viewport(), state, view, 92, 0.0);
        });
        (action, output)
    }

    /// Draws idle frames until the floating area has laid itself out where it lands.
    fn settle(ctx: &egui::Context, state: &mut ViewCluster, view: ClusterView) {
        for _ in 0..3 {
            let (_, output) = frame(ctx, state, view, Vec::new());
            output.drop_without_applying_deltas();
        }
    }

    /// A primary-button event at `pos`.
    fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Every labelled button the frame reported to the accessibility stack, with its rectangle.
    ///
    /// The tree is what a screen reader sees, so it is also the honest way to read a band of glyphs
    /// without a pointer: the names and their left-to-right order are the surface's own.
    fn buttons(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
        let update = output.platform_output.accesskit_update.as_ref().expect("the frame reported its tree");
        update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == egui::Role::Button)
            .filter_map(|(_, node)| {
                let bounds = node.bounds()?;
                let rect = egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                );
                Some((node.label()?.to_owned(), rect))
            })
            .collect()
    }

    /// The labels of the groups the frame reported to the accessibility stack.
    fn groups(output: &egui::FullOutput) -> Vec<String> {
        let update = output.platform_output.accesskit_update.as_ref().expect("the frame reported its tree");
        update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == egui::Role::Group)
            .filter_map(|(_, node)| node.label().map(str::to_owned))
            .collect()
    }

    /// The rectangles a frame painted with `fill`.
    fn filled(output: &egui::FullOutput, fill: egui::Color32) -> Vec<egui::Rect> {
        fn walk(shape: &egui::Shape, fill: egui::Color32, out: &mut Vec<egui::Rect>) {
            match shape {
                egui::Shape::Rect(rect) if rect.fill == fill => out.push(rect.rect),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, fill, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, fill, &mut out);
        }
        out
    }

    /// The textures a frame stamped inside `rect`, one per painted glyph.
    fn images_in(output: &egui::FullOutput, rect: egui::Rect) -> Vec<egui::TextureId> {
        fn walk(shape: &egui::Shape, rect: egui::Rect, out: &mut Vec<egui::TextureId>) {
            match shape {
                egui::Shape::Mesh(mesh) if rect.contains(mesh.calc_bounds().center()) => {
                    out.push(mesh.texture_id);
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, rect, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, rect, &mut out);
        }
        out
    }

    #[test]
    fn the_five_styles_are_one_named_band_and_the_view_draws_pressed() {
        // U118: five glyphs in a run between two dividers read as five switches. One tinted frame,
        // touching cells and one accessible name make them one value, answered in the style table's
        // order with the view's own style filled (ADR 0113).
        let ctx = egui::Context::default();
        ui::theme::install(&ctx);
        // A test has no clock: the floating area fades in over a fraction of a second, so the fade
        // is turned off and every frame paints the cluster's own colours.
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        ctx.enable_accesskit();
        let style = ShadingStyle::Wireframe;
        let view = ClusterView { style, ..cluster_view(LayoutKind::Single) };
        let mut state = ViewCluster::default();
        settle(&ctx, &mut state, view);

        let (action, output) = frame(&ctx, &mut state, view, Vec::new());
        assert!(action.is_none(), "a frame with no pointer asks for nothing");
        let named = groups(&output);
        assert_eq!(
            named,
            [text::Message::ShadingGroup.text(text::Lang::En)],
            "the band carries one name, and it says what it is"
        );

        let names: Vec<String> =
            ShadingStyle::ALL.iter().map(|style| shading_label(*style, text::Lang::En).to_owned()).collect();
        let reported = buttons(&output);
        let mut cells: Vec<(&str, egui::Rect)> = reported
            .iter()
            .filter(|(label, _)| names.contains(label))
            .map(|(label, rect)| (label.as_str(), *rect))
            .collect();
        assert_eq!(cells.len(), ShadingStyle::ALL.len(), "the band draws five cells");
        cells.sort_by(|left, right| left.1.left().total_cmp(&right.1.left()));
        let order: Vec<&str> = cells.iter().map(|(label, _)| *label).collect();
        assert_eq!(
            order,
            names.iter().map(String::as_str).collect::<Vec<_>>(),
            "the band runs in the style table's order"
        );
        for pair in cells.windows(2) {
            assert!((pair[1].1.left() - pair[0].1.right()).abs() < 0.01, "the cells touch with no gap between them");
        }
        let control = ctx.global_style().visuals.widgets.inactive.bg_fill;
        let frames: Vec<egui::Rect> = filled(&output, control)
            .into_iter()
            .filter(|rect| cells.iter().all(|(_, cell)| rect.contains(cell.center())))
            .collect();
        assert_eq!(frames.len(), 1, "the five cells sit inside one tinted frame");

        let pressed = filled(&output, ctx.global_style().visuals.selection.bg_fill);
        assert_eq!(pressed.len(), 1, "one cell of the band draws pressed");
        let current = ShadingStyle::ALL.iter().position(|entry| *entry == style).expect("the view's style is a style");
        assert!(
            (pressed[0].center() - cells[current].1.center()).length() < 1.0,
            "the pressed cell is the style the view draws with"
        );

        // The band answers with its own style, and only when a cell is clicked: the cells are one
        // value, so the action carries the choice the cell names and never an index.
        let target = cells[ShadingStyle::ALL.len() - 1].1.center();
        let (_, moved) = frame(&ctx, &mut state, view, vec![egui::Event::PointerMoved(target)]);
        moved.drop_without_applying_deltas();
        let (_, down) = frame(&ctx, &mut state, view, vec![pointer(target, true)]);
        down.drop_without_applying_deltas();
        let (action, up) = frame(&ctx, &mut state, view, vec![pointer(target, false)]);
        up.drop_without_applying_deltas();
        assert_eq!(
            action,
            Some(ClusterAction::SetStyle(ShadingStyle::Translucent)),
            "a cell asks for the style it names"
        );
    }

    #[test]
    fn every_split_choice_draws_its_glyph_in_the_popover_and_in_the_cell() {
        // U118: the four splits shared one generic `layout` frame, so neither the popover nor the
        // cluster's cell said which split was in force. Each choice owns its drawing, the popover
        // pairs it with the name, and the cell shows the layout the workspace is in.
        let ctx = egui::Context::default();
        ui::theme::install(&ctx);
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        ctx.enable_accesskit();
        let mut cell_glyphs = std::collections::HashSet::new();
        let mut state = ViewCluster::default();
        for kind in LayoutKind::ALL {
            let view = cluster_view(kind);
            settle(&ctx, &mut state, view);
            let (_, output) = frame(&ctx, &mut state, view, Vec::new());
            let cell = buttons(&output)
                .into_iter()
                .find(|(label, _)| label == text::Message::Layout.text(text::Lang::En))
                .map(|(_, rect)| rect)
                .expect("the cluster draws its layout cell");
            let glyphs = images_in(&output, cell);
            assert_eq!(glyphs.len(), 1, "{kind:?} draws one glyph in the layout cell");
            assert!(cell_glyphs.insert(glyphs[0]), "{kind:?} draws the glyph of an earlier split");
            output.drop_without_applying_deltas();
        }
        assert_eq!(cell_glyphs.len(), LayoutKind::ALL.len(), "the four splits own four glyphs");

        // The popover pairs each name with its glyph, the way the preset list does.
        let mut state = ViewCluster { layout_open: true, ..ViewCluster::default() };
        let view = cluster_view(LayoutKind::Single);
        settle(&ctx, &mut state, view);
        let (_, output) = frame(&ctx, &mut state, view, Vec::new());
        let rects = drawn_text_rects(&output.shapes);
        for kind in LayoutKind::ALL {
            let label = kind.label().text(text::Lang::En);
            let (row, _) = rects.iter().find(|(_, text)| text == label).expect("the popover names every split");
            // The row paints the glyph 14 points left of its name, centred on the row.
            let glyph =
                egui::Rect::from_center_size(egui::pos2(row.left() - 14.0, row.center().y), egui::Vec2::splat(2.0));
            assert_eq!(images_in(&output, glyph).len(), 1, "{kind:?} draws its glyph beside its name");
        }
        output.drop_without_applying_deltas();
    }

    #[test]
    fn every_view_preset_has_its_own_drawn_glyph() {
        let mut seen = std::collections::HashSet::new();
        for view in OpeningView::ALL {
            let icon = preset_icon(view);
            assert!(icon.is_drawn(), "{view:?} still draws a placeholder");
            assert!(seen.insert(icon.uri()), "{view:?} shares its glyph with another preset");
        }
        assert_eq!(seen.len(), OpeningView::ALL.len(), "the six presets own six glyphs");
    }

    #[test]
    fn the_display_popover_keeps_the_switches_and_links_to_the_settings_page() {
        // The strengths moved to Settings › View, so the popover keeps its switches and offers the
        // one link that reaches them; the triangle diagnostic left the cluster with the sliders (U64).
        let ctx = egui::Context::default();
        ui::theme::install(&ctx);
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0))),
            ..Default::default()
        };
        let mut state = ViewCluster { settings_open: true, ..ViewCluster::default() };
        let view = ClusterView {
            owns_view: true,
            projection: Projection::Perspective { fov_y: 0.9 },
            work_plane: "XY",
            layout: LayoutKind::Single,
            maximized: false,
            preset: None,
            style: ShadingStyle::default(),
        };
        // An area is drawn invisibly on its sizing pass, so the second frame is the one that shows it.
        for _ in 0..2 {
            let output = ctx.run_ui(input.clone(), |ui| {
                show(ui, viewport, &mut state, view, 92, 0.0);
            });
            output.drop_without_applying_deltas();
        }
        let output = ctx.run_ui(input.clone(), |ui| {
            show(ui, viewport, &mut state, view, 92, 0.0);
        });
        let texts = drawn_text(&output.shapes);
        output.drop_without_applying_deltas();
        assert!(
            texts.iter().any(|text| text.contains("Configure…")),
            "the popover links to the settings page: {texts:?}"
        );
        assert!(
            !texts.iter().any(|text| text.contains("triangle edge")),
            "the diagnostic left the cluster with the strength sliders: {texts:?}"
        );
        // The shadow left for the View tab with its switch and its intensity (U107), and the
        // triangle diagnostic left the cluster with the sliders (U64).
        assert!(
            !texts.iter().any(|text| text.contains("Contact shadows")),
            "the shadow switch is the View tab's alone: {texts:?}"
        );
    }
}
