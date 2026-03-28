use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText};
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

use crate::canvas::{
    CanvasController, CanvasToolKind, CommittedDocumentEdit, DocumentEditMode, ToolSettings,
    color32_from_rgba, rgba_from_color32,
};
use crate::fonts::install_japanese_fonts;
use crate::model::{
    AlignmentKind, DistributionKind, DocumentHistory, GuideAxis, LayerId, PaintDocument,
    PaintElement, RgbaColor, StackOrderCommand,
};
use crate::storage::{
    ExportedImage, LoadedDocument, PngExportKind, SavedDocument, StorageError, StorageFacade,
};

const MIN_BRUSH_WIDTH: f32 = 1.0;
const MAX_BRUSH_WIDTH: f32 = 48.0;
const GRID_SPACING_PRESETS: [f32; 6] = [16.0, 24.0, 32.0, 48.0, 64.0, 96.0];
const GRID_SPACING_STEP: f32 = 8.0;
const TOOL_BUTTON_HEIGHT: f32 = 44.0;
const COLOR_SWATCH_SIZE: f32 = 36.0;
const LAYER_ACTION_BUTTON_HEIGHT: f32 = 34.0;
const LAYER_CHIP_BUTTON_WIDTH: f32 = 56.0;
const RECENT_COLOR_LIMIT: usize = 12;
const APP_UI_STATE_KEY: &str = "paint_app_ui_state";

const QUICK_PALETTE: [RgbaColor; 12] = [
    RgbaColor::from_rgba(37, 37, 41, 255),
    RgbaColor::from_rgba(255, 255, 255, 255),
    RgbaColor::from_rgba(220, 64, 64, 255),
    RgbaColor::from_rgba(255, 143, 0, 255),
    RgbaColor::from_rgba(255, 199, 64, 255),
    RgbaColor::from_rgba(76, 175, 80, 255),
    RgbaColor::from_rgba(0, 150, 136, 255),
    RgbaColor::from_rgba(33, 150, 243, 255),
    RgbaColor::from_rgba(63, 81, 181, 255),
    RgbaColor::from_rgba(156, 39, 176, 255),
    RgbaColor::from_rgba(121, 85, 72, 255),
    RgbaColor::from_rgba(96, 125, 139, 255),
];

fn shortcut_undo() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)
}

fn shortcut_redo() -> KeyboardShortcut {
    KeyboardShortcut::new(
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
        Key::Z,
    )
}

fn shortcut_redo_alt() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Y)
}

fn shortcut_save() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::S)
}

fn shortcut_load() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::O)
}

fn shortcut_export_png() -> KeyboardShortcut {
    KeyboardShortcut::new(
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
        Key::E,
    )
}

fn shortcut_group() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::G)
}

fn shortcut_ungroup() -> KeyboardShortcut {
    KeyboardShortcut::new(
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
        Key::G,
    )
}

fn shortcut_zoom_in() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus)
}

fn shortcut_zoom_in_alt() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Equals)
}

fn shortcut_zoom_out() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus)
}

fn shortcut_reset_view() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0)
}

#[derive(Clone)]
struct StatusMessage {
    kind: StatusKind,
    text: String,
}

#[derive(Clone, Copy)]
enum StatusKind {
    Info,
    Error,
}

impl StatusMessage {
    fn info(text: impl Into<String>) -> Self {
        Self {
            kind: StatusKind::Info,
            text: text.into(),
        }
    }

    fn error(text: impl Into<String>) -> Self {
        Self {
            kind: StatusKind::Error,
            text: text.into(),
        }
    }

    fn rich_text(&self) -> RichText {
        let color = match self.kind {
            StatusKind::Info => egui::Color32::from_gray(70),
            StatusKind::Error => egui::Color32::from_rgb(160, 40, 40),
        };

        RichText::new(&self.text).small().color(color)
    }
}

#[cfg(target_arch = "wasm32")]
struct PendingWebStorageTask {
    label: &'static str,
    slot: Rc<RefCell<Option<Result<WebStorageResult, StorageError>>>>,
}

#[cfg(target_arch = "wasm32")]
enum WebStorageResult {
    Saved(SavedDocument),
    Loaded(LoadedDocument),
    Exported {
        image: ExportedImage,
        kind: PngExportKind,
    },
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct UiStatePersistence {
    tutorial_dismissed: bool,
    recent_colors: Vec<RgbaColor>,
}

#[derive(Clone, Default)]
struct TutorialOverlayState {
    visible: bool,
    step_index: usize,
}

#[derive(Clone, Copy)]
struct TutorialStepContent {
    title: &'static str,
    body: &'static str,
    action: &'static str,
}

#[derive(Clone, Copy)]
struct ToolWidthSettings {
    draw_width: f32,
    eraser_width: f32,
}

impl Default for ToolWidthSettings {
    fn default() -> Self {
        Self {
            draw_width: 6.0,
            eraser_width: 12.0,
        }
    }
}

impl ToolWidthSettings {
    fn for_tool(self, tool: CanvasToolKind) -> f32 {
        match tool {
            CanvasToolKind::Eraser => self.eraser_width,
            _ => self.draw_width,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorTarget {
    Stroke,
    Fill,
}

impl ColorTarget {
    const fn label(self) -> &'static str {
        match self {
            Self::Stroke => "線色",
            Self::Fill => "塗り色",
        }
    }
}

#[derive(Clone, Copy)]
struct ToolColorSettings {
    stroke_color: RgbaColor,
    fill_color: RgbaColor,
    fill_enabled: bool,
    quick_color_target: ColorTarget,
}

impl Default for ToolColorSettings {
    fn default() -> Self {
        Self {
            stroke_color: RgbaColor::charcoal(),
            fill_color: RgbaColor::from_rgba(255, 199, 64, 180),
            fill_enabled: false,
            quick_color_target: ColorTarget::Stroke,
        }
    }
}

pub struct PaintApp {
    history: DocumentHistory,
    canvas: CanvasController,
    storage: StorageFacade,
    active_tool: CanvasToolKind,
    previous_non_picker_tool: CanvasToolKind,
    multi_select_mode: bool,
    finger_draw_enabled: bool,
    tool_colors: ToolColorSettings,
    tool_widths: ToolWidthSettings,
    status_message: StatusMessage,
    document_name: String,
    saved_snapshot: PaintDocument,
    layer_name_draft: String,
    layer_name_draft_for: Option<LayerId>,
    show_help: bool,
    ui_state: UiStatePersistence,
    ui_state_dirty: bool,
    tutorial: TutorialOverlayState,
    #[cfg(target_arch = "wasm32")]
    pending_web_task: Option<PendingWebStorageTask>,
}

impl Default for PaintApp {
    fn default() -> Self {
        let storage = StorageFacade::new();
        let document = PaintDocument::default();
        Self {
            history: DocumentHistory::new(document.clone()),
            canvas: CanvasController::default(),
            storage,
            active_tool: CanvasToolKind::Brush,
            previous_non_picker_tool: CanvasToolKind::Brush,
            multi_select_mode: false,
            finger_draw_enabled: false,
            tool_colors: ToolColorSettings::default(),
            tool_widths: ToolWidthSettings::default(),
            status_message: StatusMessage::info(
                "描く準備ができました。ペンや図形ツールを選んで、必要ならヘルプを開いてください。",
            ),
            document_name: storage.suggested_file_name().to_owned(),
            saved_snapshot: document,
            layer_name_draft: "レイヤー 1".to_owned(),
            layer_name_draft_for: Some(1),
            show_help: false,
            ui_state: UiStatePersistence::default(),
            ui_state_dirty: false,
            tutorial: TutorialOverlayState::default(),
            #[cfg(target_arch = "wasm32")]
            pending_web_task: None,
        }
    }
}

impl PaintApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_japanese_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let ui_state = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, APP_UI_STATE_KEY))
            .unwrap_or_default();
        let mut app = Self {
            ui_state,
            ..Self::default()
        };
        app.ui_state.recent_colors.truncate(RECENT_COLOR_LIMIT);
        app.tutorial.visible = !app.ui_state.tutorial_dismissed;
        if app.tutorial.visible {
            app.status_message =
                StatusMessage::info("最初の操作はミニチュートリアルで確認できます。");
        }
        app
    }

    fn document(&self) -> &PaintDocument {
        self.history.current()
    }

    fn is_dirty(&self) -> bool {
        &self.saved_snapshot != self.document()
    }

    fn set_info(&mut self, text: impl Into<String>) {
        self.status_message = StatusMessage::info(text);
    }

    fn set_error(&mut self, text: impl Into<String>) {
        self.status_message = StatusMessage::error(text);
    }

    fn set_active_tool(&mut self, tool: CanvasToolKind, announce: bool) {
        if self.active_tool == tool {
            return;
        }

        if tool != CanvasToolKind::Select && self.multi_select_mode {
            self.multi_select_mode = false;
        }
        if tool != CanvasToolKind::Eyedropper {
            self.previous_non_picker_tool = tool;
        }
        self.active_tool = tool;
        if announce {
            self.set_info(tool_switch_message(tool));
        }
    }

    fn tool_settings(&self) -> ToolSettings {
        ToolSettings {
            tool: self.active_tool,
            stroke_color: self.tool_colors.stroke_color,
            fill_color: self
                .tool_colors
                .fill_enabled
                .then_some(self.tool_colors.fill_color),
            width: self.active_tool_width(),
            multi_select_mode: self.multi_select_mode,
            finger_draw_enabled: self.finger_draw_enabled,
        }
    }

    fn active_tool_width(&self) -> f32 {
        self.tool_widths.for_tool(self.active_tool)
    }

    fn tool_uses_fill(&self) -> bool {
        matches!(
            self.active_tool,
            CanvasToolKind::Rectangle | CanvasToolKind::Ellipse | CanvasToolKind::Eyedropper
        )
    }

    fn push_recent_color(&mut self, color: RgbaColor) {
        if self.ui_state.recent_colors.first().copied() == Some(color) {
            return;
        }

        self.ui_state
            .recent_colors
            .retain(|stored| *stored != color);
        self.ui_state.recent_colors.insert(0, color);
        self.ui_state.recent_colors.truncate(RECENT_COLOR_LIMIT);
        self.ui_state_dirty = true;
    }

    fn apply_color_to_target(&mut self, color: RgbaColor, announce: impl Into<String>) {
        match self.tool_colors.quick_color_target {
            ColorTarget::Stroke => {
                self.tool_colors.stroke_color = color;
            }
            ColorTarget::Fill => {
                self.tool_colors.fill_color = color;
                self.tool_colors.fill_enabled = true;
            }
        }
        self.push_recent_color(color);
        self.set_info(announce);
    }

    fn apply_quick_color(&mut self, color: RgbaColor, source: &str) {
        let target = self.tool_colors.quick_color_target;
        self.apply_color_to_target(
            color,
            format!("{source}から{}を更新しました。", target.label()),
        );
    }

    fn apply_picked_color(&mut self, color: RgbaColor) {
        let target = self.tool_colors.quick_color_target;
        self.apply_color_to_target(color, format!("スポイトで{}を拾いました。", target.label()));

        if self.active_tool == CanvasToolKind::Eyedropper {
            let return_tool = self.previous_non_picker_tool;
            self.set_active_tool(self.previous_non_picker_tool, false);
            self.set_info(format!(
                "スポイトで{}を拾いました。{} に戻ります。",
                target.label(),
                return_tool.label()
            ));
        }
    }

    fn set_multi_select_mode(&mut self, enabled: bool) {
        if self.multi_select_mode == enabled {
            return;
        }

        self.multi_select_mode = enabled;
        if enabled {
            self.active_tool = CanvasToolKind::Select;
            self.set_info("複数選択モードをオンにしました。タップで追加や解除ができます。");
        } else {
            self.set_info("複数選択モードをオフにしました。通常の選択と移動に戻ります。");
        }
    }

    fn exit_multi_select_mode_for_editing(&mut self) {
        self.multi_select_mode = false;
        self.active_tool = CanvasToolKind::Select;
        self.set_info("複数選択を保ったまま通常の移動に戻しました。");
    }

    fn set_finger_draw_enabled(&mut self, enabled: bool) {
        if self.finger_draw_enabled == enabled {
            return;
        }

        self.finger_draw_enabled = enabled;
        if enabled {
            self.set_info(
                "指でも描画できるようにしました。タブレットでは必要に応じてオフに戻せます。",
            );
        } else {
            self.set_info("指の既定動作をビュー操作と長押し選択に戻しました。ペンやマウスはそのまま描画できます。");
        }
    }

    fn sync_layer_name_draft(&mut self) {
        let Some((layer_id, layer_name)) = self
            .document()
            .active_layer()
            .map(|layer| (layer.id, layer.name.clone()))
        else {
            self.layer_name_draft_for = None;
            self.layer_name_draft.clear();
            return;
        };

        if self.layer_name_draft_for != Some(layer_id) {
            self.layer_name_draft_for = Some(layer_id);
            self.layer_name_draft = layer_name;
        }
    }

    fn persist_ui_state_if_needed(&mut self, frame: &mut eframe::Frame) {
        if !self.ui_state_dirty {
            return;
        }

        if let Some(storage) = frame.storage_mut() {
            eframe::set_value(storage, APP_UI_STATE_KEY, &self.ui_state);
            self.ui_state_dirty = false;
        }
    }

    fn open_tutorial(&mut self) {
        self.show_help = false;
        self.tutorial.visible = true;
        self.tutorial.step_index = 0;
        self.set_info("ミニチュートリアルを開きました。短く流れを確認できます。");
    }

    fn close_tutorial(&mut self, completed: bool) {
        self.tutorial.visible = false;
        self.ui_state.tutorial_dismissed = true;
        self.ui_state_dirty = true;
        self.set_info(if completed {
            "チュートリアルを閉じました。必要ならヘルプからもう一度開けます。"
        } else {
            "チュートリアルをスキップしました。必要ならヘルプから開けます。"
        });
    }

    fn apply_tool_button_selection(&mut self, tool: CanvasToolKind) {
        if tool == CanvasToolKind::Select && self.multi_select_mode {
            self.exit_multi_select_mode_for_editing();
        } else {
            self.set_active_tool(tool, true);
        }
    }

    fn show_file_summary(&self, ui: &mut egui::Ui) {
        let dirty_suffix = if self.is_dirty() {
            "未保存の変更あり"
        } else {
            "保存済み"
        };
        ui.label(RichText::new("ドキュメント").strong());
        ui.label(self.document_name.as_str());
        ui.small(dirty_suffix);
    }

    fn show_tools(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 8.0;

        ui.horizontal(|ui| {
            ui.heading("ツール");
            let response = help_icon_button(
                ui,
                "描く / 選ぶ / 動かすツールを切り替えます。詳しい流れはヘルプやチュートリアルで確認できます。",
            );
            if response.clicked() {
                self.set_info("ツールを切り替えて、現在のレイヤーを描いたり編集したりできます。");
            }
        });
        ui.small("よく使う操作を上にまとめています。下へスクロールすると詳細設定も開けます。");
        ui.add_space(8.0);

        ui.group(|ui| {
            ui.label(RichText::new("今の状態").strong());
            ui.small(format!("道具: {}", self.active_tool.label()));
            if matches!(
                self.active_tool,
                CanvasToolKind::Brush | CanvasToolKind::Pencil | CanvasToolKind::Marker
            ) {
                ui.small(format!("描き味: {}", brush_kind_summary(self.active_tool)));
            }
            if let Some(active_layer) = self.document().active_layer() {
                ui.small(format!("レイヤー: {}", active_layer.name));
            }
            ui.small(self.canvas.selection_summary(self.document()));
            ui.horizontal_wrapped(|ui| {
                ui.small("線色");
                let stroke_response = color_swatch_button(
                    ui,
                    self.tool_colors.stroke_color,
                    self.tool_colors.quick_color_target == ColorTarget::Stroke,
                    "線色を編集対象にします。",
                );
                if stroke_response.clicked() {
                    self.tool_colors.quick_color_target = ColorTarget::Stroke;
                }
                if self.tool_colors.fill_enabled || self.tool_uses_fill() {
                    ui.small("塗り色");
                    let fill_response = color_swatch_button(
                        ui,
                        self.tool_colors.fill_color,
                        self.tool_colors.quick_color_target == ColorTarget::Fill,
                        "塗り色を編集対象にします。",
                    );
                    if fill_response.clicked() {
                        self.tool_colors.quick_color_target = ColorTarget::Fill;
                    }
                }
            });
        });
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label(RichText::new("タブレット向け").strong());
            let response = help_icon_button(
                ui,
                "キーボードなしで複数選択したり、指で描く / パンするための切り替えです。",
            );
            if response.clicked() {
                self.set_info(
                    "タブレットでは複数選択モード、手のひら、指でも描くを切り替えて使えます。",
                );
            }
        });
        let multi_select_response = ui
            .add_sized(
                [ui.available_width(), TOOL_BUTTON_HEIGHT],
                egui::Button::new("複数選択モード").selected(self.multi_select_mode),
            )
            .on_hover_text("タップ / クリックで追加選択や解除をします。");
        if multi_select_response.clicked() {
            self.set_multi_select_mode(!self.multi_select_mode);
        }
        if self.multi_select_mode && self.canvas.selection_count() > 0 {
            let keep_selection_response = ui
                .add_sized(
                    [ui.available_width(), TOOL_BUTTON_HEIGHT],
                    egui::Button::new("選択を保ったまま移動へ"),
                )
                .on_hover_text("複数選択を保ったまま通常のドラッグ移動に戻します。");
            if keep_selection_response.clicked() {
                self.exit_multi_select_mode_for_editing();
            }
        }
        let finger_draw_response = ui
            .add_sized(
                [ui.available_width(), TOOL_BUTTON_HEIGHT],
                egui::Button::new("指でも描く").selected(self.finger_draw_enabled),
            )
            .on_hover_text("指でもそのまま描画できるようにします。");
        if finger_draw_response.clicked() {
            self.set_finger_draw_enabled(!self.finger_draw_enabled);
        }
        ui.add_space(8.0);

        for tool in [
            CanvasToolKind::Select,
            CanvasToolKind::Pan,
            CanvasToolKind::Brush,
            CanvasToolKind::Pencil,
            CanvasToolKind::Marker,
            CanvasToolKind::Eyedropper,
            CanvasToolKind::Rectangle,
            CanvasToolKind::Ellipse,
            CanvasToolKind::Line,
            CanvasToolKind::Eraser,
        ] {
            let is_selected = self.active_tool == tool;
            let response = ui
                .add_sized(
                    [ui.available_width(), TOOL_BUTTON_HEIGHT],
                    egui::Button::new(tool.label()).selected(is_selected),
                )
                .on_hover_text(tool_button_tooltip(tool));
            let can_activate =
                !is_selected || (tool == CanvasToolKind::Select && self.multi_select_mode);
            if response.clicked() && can_activate {
                self.apply_tool_button_selection(tool);
            }
        }

        ui.add_space(12.0);
        ui.label(RichText::new("描画ツール設定").strong());
        match self.active_tool {
            CanvasToolKind::Brush
            | CanvasToolKind::Pencil
            | CanvasToolKind::Marker
            | CanvasToolKind::Eyedropper
            | CanvasToolKind::Rectangle
            | CanvasToolKind::Ellipse
            | CanvasToolKind::Line => {
                ui.small(format!(
                    "今のツールは描く太さ {:.1}px を使います。",
                    self.tool_widths.draw_width
                ));
            }
            CanvasToolKind::Eraser => {
                ui.small(format!(
                    "今のツールは消しゴム太さ {:.1}px を使います。",
                    self.tool_widths.eraser_width
                ));
            }
            CanvasToolKind::Select | CanvasToolKind::Pan => {
                ui.small("描く太さは描画ツールと図形、消しゴム太さは消しゴムに使います。");
            }
        }

        ui.label(RichText::new("色と不透明度").strong());
        ui.small("線色は描画と図形、塗り色は四角形と楕円に使います。");

        ui.horizontal_wrapped(|ui| {
            ui.label("色の反映先");
            ui.selectable_value(
                &mut self.tool_colors.quick_color_target,
                ColorTarget::Stroke,
                "線色",
            );
            ui.selectable_value(
                &mut self.tool_colors.quick_color_target,
                ColorTarget::Fill,
                "塗り色",
            );
        });
        ui.horizontal_wrapped(|ui| {
            let stroke_selected = self.tool_colors.quick_color_target == ColorTarget::Stroke;
            let fill_selected = self.tool_colors.quick_color_target == ColorTarget::Fill;
            let stroke_response = color_swatch_button(
                ui,
                self.tool_colors.stroke_color,
                stroke_selected,
                "線色を編集対象にします。",
            );
            if stroke_response.clicked() {
                self.tool_colors.quick_color_target = ColorTarget::Stroke;
            }
            ui.small("線色");

            let fill_response = color_swatch_button(
                ui,
                self.tool_colors.fill_color,
                fill_selected,
                "塗り色を編集対象にします。",
            );
            if fill_response.clicked() {
                self.tool_colors.quick_color_target = ColorTarget::Fill;
            }
            ui.small("塗り色");
        });
        ui.small("スポイト、最近使った色、簡易パレットはここで選んだ色へ入ります。");

        ui.label("線色");
        let mut stroke_color = color32_from_rgba(self.tool_colors.stroke_color);
        if ui.color_edit_button_srgba(&mut stroke_color).changed() {
            let color = rgba_from_color32(stroke_color);
            self.tool_colors.stroke_color = color;
            self.push_recent_color(color);
            self.set_info("線色を変更しました。");
        }
        let mut stroke_opacity = alpha_percent(self.tool_colors.stroke_color);
        if ui
            .add(egui::Slider::new(&mut stroke_opacity, 0..=100).suffix("%"))
            .on_hover_text("描画ツールや図形の線の不透明度を変えます。")
            .changed()
        {
            self.tool_colors.stroke_color =
                set_alpha_percent(self.tool_colors.stroke_color, stroke_opacity);
            self.push_recent_color(self.tool_colors.stroke_color);
            self.set_info(format!(
                "線の不透明度を {}% に変更しました。",
                stroke_opacity
            ));
        }

        let mut fill_enabled = self.tool_colors.fill_enabled;
        if ui
            .checkbox(&mut fill_enabled, "塗りを使う")
            .on_hover_text("四角形や楕円の内側を塗ります。")
            .changed()
        {
            self.tool_colors.fill_enabled = fill_enabled;
            self.set_info(if fill_enabled {
                "図形の塗りをオンにしました。".to_owned()
            } else {
                "図形の塗りをオフにしました。".to_owned()
            });
        }
        ui.add_enabled_ui(self.tool_colors.fill_enabled, |ui| {
            ui.label("塗り色");
            let mut fill_color = color32_from_rgba(self.tool_colors.fill_color);
            if ui.color_edit_button_srgba(&mut fill_color).changed() {
                let color = rgba_from_color32(fill_color);
                self.tool_colors.fill_color = color;
                self.push_recent_color(color);
                self.set_info("塗り色を変更しました。");
            }
            let mut fill_opacity = alpha_percent(self.tool_colors.fill_color);
            if ui
                .add(egui::Slider::new(&mut fill_opacity, 0..=100).suffix("%"))
                .on_hover_text("四角形や楕円の塗りの不透明度を変えます。")
                .changed()
            {
                self.tool_colors.fill_color =
                    set_alpha_percent(self.tool_colors.fill_color, fill_opacity);
                self.push_recent_color(self.tool_colors.fill_color);
                self.set_info(format!(
                    "塗りの不透明度を {}% に変更しました。",
                    fill_opacity
                ));
            }
        });
        if !self.tool_uses_fill() {
            ui.small("塗り色は四角形と楕円で使います。");
        }

        ui.add_space(6.0);
        ui.label(RichText::new("最近使った色").strong());
        if self.ui_state.recent_colors.is_empty() {
            ui.small("色を変更すると、ここからすぐ呼び戻せます。");
        } else {
            let recent_colors = self.ui_state.recent_colors.clone();
            ui.horizontal_wrapped(|ui| {
                for color in recent_colors {
                    let response = color_swatch_button(
                        ui,
                        color,
                        false,
                        "最近使った色を現在の反映先へ入れます。",
                    );
                    if response.clicked() {
                        self.apply_quick_color(color, "最近使った色");
                    }
                }
            });
        }

        ui.add_space(6.0);
        ui.label(RichText::new("よく使う色").strong());
        ui.horizontal_wrapped(|ui| {
            for color in QUICK_PALETTE {
                let response =
                    color_swatch_button(ui, color, false, "よく使う色を現在の反映先へ入れます。");
                if response.clicked() {
                    self.apply_quick_color(color, "簡易パレット");
                }
            }
        });

        ui.add_space(8.0);
        ui.label("描く太さ");
        if ui
            .add(egui::Slider::new(
                &mut self.tool_widths.draw_width,
                MIN_BRUSH_WIDTH..=MAX_BRUSH_WIDTH,
            ))
            .on_hover_text("ペン、えんぴつ、マーカー、四角形、楕円、直線の太さを変えます。")
            .changed()
        {
            self.set_info(format!(
                "描く太さを {:.1}px に変更しました。描画ツールと図形に使います。",
                self.tool_widths.draw_width
            ));
        }
        ui.small(format!(
            "{:.1}px · ペン / えんぴつ / マーカー / 四角形 / 楕円 / 直線",
            self.tool_widths.draw_width
        ));

        ui.label("消しゴム太さ");
        if ui
            .add(egui::Slider::new(
                &mut self.tool_widths.eraser_width,
                MIN_BRUSH_WIDTH..=MAX_BRUSH_WIDTH,
            ))
            .on_hover_text("消しゴムで消す太さを変えます。")
            .changed()
        {
            self.set_info(format!(
                "消しゴム太さを {:.1}px に変更しました。",
                self.tool_widths.eraser_width
            ));
        }
        ui.small(format!("{:.1}px · 消しゴム", self.tool_widths.eraser_width));

        ui.separator();
        ui.label(RichText::new("現在のモード").strong());
        ui.small(format!("ツール: {}", self.active_tool.label()));
        if matches!(
            self.active_tool,
            CanvasToolKind::Brush | CanvasToolKind::Pencil | CanvasToolKind::Marker
        ) {
            ui.small(format!("描き味: {}", brush_kind_summary(self.active_tool)));
        }
        ui.small(format!(
            "複数選択モード {} / 指でも描く {}",
            on_off_label(self.multi_select_mode),
            on_off_label(self.finger_draw_enabled),
        ));

        ui.separator();
        self.show_file_summary(ui);
        ui.add_space(8.0);
        ui.label(RichText::new("キャンバス").strong());
        ui.label(format!(
            "{:.0} x {:.0}px",
            self.document().canvas_size.width,
            self.document().canvas_size.height
        ));
        ui.label(format!(
            "要素数: 全体 {} / 現在のレイヤー {}",
            self.document().total_element_count(),
            self.document().element_count()
        ));
        ui.label(format!("ズーム: {}", self.canvas.zoom_label()));
        if let Some(active_layer) = self.document().active_layer() {
            ui.label(format!("現在のレイヤー: {}", active_layer.name));
        }
        ui.small("詳しい操作はツールチップかヘルプで確認できます。");

        ui.separator();
        self.show_canvas_aids(ui);

        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("保存と書き出し").strong());
            let response = help_icon_button(
                ui,
                "JSON保存は再編集用、PNG書き出しは共有用です。上部バーから使えます。",
            );
            if response.clicked() {
                self.set_info(
                    "JSON保存は続きから再編集、PNG書き出しは画像として共有したいときに使います。",
                );
            }
        });
        ui.small("JSONは再編集用、PNGは共有用です。");
        ui.small("PNG書き出しは背景あり、透過PNGは透明背景で保存します。");
        ui.small(self.storage.storage_strategy_summary());
    }

    fn show_canvas_aids(&mut self, ui: &mut egui::Ui) {
        #[derive(Clone, Copy)]
        enum AidAction {
            ToggleRulersVisible,
            ToggleGridVisible,
            ToggleGridSnap,
            SetGridSpacing(f32),
            ToggleSmartGuidesVisible,
            ToggleGuidesVisible,
            ToggleGuidesSnap,
            AddGuide(GuideAxis),
            RemoveGuide(usize),
        }

        let has_canvas_interaction = self.canvas.has_active_interaction();
        let grid = self.document().grid();
        let rulers_visible = self.document().rulers().visible;
        let smart_guides_visible = self.document().smart_guides().visible;
        let guides_visible = self.document().guides().visible;
        let guides_snap = self.document().guides().snap_enabled;
        let guides: Vec<_> = self
            .document()
            .guides()
            .lines
            .iter()
            .copied()
            .enumerate()
            .collect();
        let mut pending_action = None;

        ui.horizontal(|ui| {
            ui.label(RichText::new("配置補助").strong());
            let response = help_icon_button(
                ui,
                "グリッド、ガイド、ルーラー、スナップの切り替えです。細かい位置合わせに使います。",
            );
            if response.clicked() {
                self.set_info(
                    "配置補助ではグリッド、ガイド、ルーラー、スマートガイドを切り替えられます。",
                );
            }
        });
        ui.small(format!(
            "ルーラー: {} · グリッド: {:.0}px · スマートガイド: {} · ガイド: {}本",
            if rulers_visible {
                "表示"
            } else {
                "非表示"
            },
            grid.spacing,
            if smart_guides_visible {
                "オン"
            } else {
                "オフ"
            },
            guides.len(),
        ));

        ui.add_enabled_ui(!has_canvas_interaction, |ui| {
            ui.horizontal_wrapped(|ui| {
                let mut show_rulers = rulers_visible;
                let rulers_response = ui
                    .checkbox(&mut show_rulers, "ルーラーを表示")
                    .on_hover_text("キャンバスの上端と左端に目盛りを表示します。");
                if rulers_response.changed() {
                    pending_action = Some(AidAction::ToggleRulersVisible);
                }

                let mut show_smart_guides = smart_guides_visible;
                let smart_guides_response = ui
                    .checkbox(&mut show_smart_guides, "スマートガイド")
                    .on_hover_text("移動中に他の要素へそろう位置を線で示します。");
                if smart_guides_response.changed() {
                    pending_action = Some(AidAction::ToggleSmartGuidesVisible);
                }
            });

            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                let mut show_grid = grid.visible;
                let grid_response = ui
                    .checkbox(&mut show_grid, "グリッドを表示")
                    .on_hover_text("キャンバスに方眼を表示します。");
                if grid_response.changed() {
                    pending_action = Some(AidAction::ToggleGridVisible);
                }

                let mut snap_grid = grid.snap_enabled;
                let snap_grid_response = ui
                    .checkbox(&mut snap_grid, "グリッドに吸着")
                    .on_hover_text("移動やリサイズの位置をグリッドに合わせます。");
                if snap_grid_response.changed() {
                    pending_action = Some(AidAction::ToggleGridSnap);
                }

                ui.label(RichText::new(format!("{:.0}px", grid.spacing)).monospace());
                if ui
                    .small_button("-")
                    .on_hover_text("グリッド間隔を細かくします。")
                    .clicked()
                {
                    pending_action = Some(AidAction::SetGridSpacing(
                        (grid.spacing - GRID_SPACING_STEP).max(GRID_SPACING_STEP),
                    ));
                }
                if ui
                    .small_button("+")
                    .on_hover_text("グリッド間隔を広げます。")
                    .clicked()
                {
                    pending_action =
                        Some(AidAction::SetGridSpacing(grid.spacing + GRID_SPACING_STEP));
                }
            });

            ui.horizontal_wrapped(|ui| {
                ui.small("間隔プリセット:");
                for preset in GRID_SPACING_PRESETS {
                    let is_current = (grid.spacing - preset).abs() < 0.1;
                    let response = ui
                        .selectable_label(is_current, format!("{preset:.0}px"))
                        .on_hover_text("グリッド間隔をこの値に切り替えます。");
                    if response.clicked() {
                        pending_action = Some(AidAction::SetGridSpacing(preset));
                    }
                }
            });

            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                let mut show_guides = guides_visible;
                let guides_response = ui
                    .checkbox(&mut show_guides, "ガイドを表示")
                    .on_hover_text("追加した横ガイド / 縦ガイドを表示します。");
                if guides_response.changed() {
                    pending_action = Some(AidAction::ToggleGuidesVisible);
                }

                let mut snap_guides = guides_snap;
                let snap_guides_response = ui
                    .checkbox(&mut snap_guides, "ガイドに吸着")
                    .on_hover_text("移動やリサイズの位置をガイドに合わせます。");
                if snap_guides_response.changed() {
                    pending_action = Some(AidAction::ToggleGuidesSnap);
                }

                ui.small(format!("{}本", guides.len()));
            });

            ui.horizontal(|ui| {
                if ui
                    .button("横ガイド追加")
                    .on_hover_text("横方向のガイドを 1 本追加します。")
                    .clicked()
                {
                    pending_action = Some(AidAction::AddGuide(GuideAxis::Horizontal));
                }
                if ui
                    .button("縦ガイド追加")
                    .on_hover_text("縦方向のガイドを 1 本追加します。")
                    .clicked()
                {
                    pending_action = Some(AidAction::AddGuide(GuideAxis::Vertical));
                }
            });
        });

        if has_canvas_interaction {
            ui.small("編集中は配置補助の切り替えを一時停止します。");
        }

        if guides.is_empty() {
            ui.small("ガイドはまだありません。必要なら追加できます。");
        } else {
            for (index, guide) in guides {
                ui.horizontal(|ui| {
                    ui.small(format!("{} {:.0}px", guide.axis.label(), guide.position));
                    if ui
                        .add_enabled(!has_canvas_interaction, egui::Button::new("削除"))
                        .on_hover_text("このガイドを削除します。")
                        .clicked()
                    {
                        pending_action = Some(AidAction::RemoveGuide(index));
                    }
                });
            }
        }

        if let Some(action) = pending_action {
            match action {
                AidAction::ToggleRulersVisible => self.toggle_rulers_visibility(),
                AidAction::ToggleGridVisible => self.toggle_grid_visibility(),
                AidAction::ToggleGridSnap => self.toggle_grid_snap(),
                AidAction::SetGridSpacing(spacing) => self.set_grid_spacing(spacing),
                AidAction::ToggleSmartGuidesVisible => self.toggle_smart_guides_visibility(),
                AidAction::ToggleGuidesVisible => self.toggle_guides_visibility(),
                AidAction::ToggleGuidesSnap => self.toggle_guides_snap(),
                AidAction::AddGuide(axis) => self.add_guide(axis),
                AidAction::RemoveGuide(index) => self.remove_guide(index),
            }
        }
    }

    fn show_layers(&mut self, ui: &mut egui::Ui) {
        #[derive(Clone, Copy)]
        enum LayerAction {
            Add,
            DuplicateActive,
            DeleteActive,
            RenameActive,
            SetActive(LayerId),
            ToggleVisibility(LayerId),
            ToggleLocked(LayerId),
            MoveUp(LayerId),
            MoveDown(LayerId),
            MoveSelectionTo(LayerId),
            DuplicateSelectionTo(LayerId),
        }

        self.sync_layer_name_draft();
        ui.spacing_mut().item_spacing.y = 8.0;

        let layer_count = self.document().layer_count();
        let active_layer_id = self.document().active_layer_id();
        let selection_count = self.canvas.selection_count();
        let has_canvas_interaction = self.canvas.has_active_interaction();
        let layers: Vec<_> = self
            .document()
            .layers()
            .iter()
            .enumerate()
            .map(|(index, layer)| {
                (
                    index,
                    layer.id,
                    layer.name.clone(),
                    layer.visible,
                    layer.locked,
                    layer.elements.len(),
                )
            })
            .collect();
        let active_layer_state = self.document().active_layer().map(|layer| {
            (
                layer.id,
                layer.name.clone(),
                layer.visible,
                layer.locked,
                layer.elements.len(),
            )
        });
        let mut pending_action = None;

        ui.horizontal(|ui| {
            ui.heading("レイヤー");
            let response = help_icon_button(
                ui,
                "現在のレイヤーだけ編集できます。非表示は書き出しに含まれず、ロック中は編集できません。",
            );
            if response.clicked() {
                self.set_info("レイヤーでは表示、ロック、順序、レイヤー間の移動 / 複製を管理できます。");
            }
        });
        ui.small("現在のレイヤーだけ描いたり編集したりできます。");
        ui.add_space(8.0);

        if let Some((_, active_name, visible, locked, element_count)) = &active_layer_state {
            ui.group(|ui| {
                ui.label(RichText::new("作業レイヤー").strong());
                ui.label(RichText::new(active_name).strong().size(15.0));
                ui.horizontal_wrapped(|ui| {
                    ui.small(format!("{element_count}個"));
                    layer_status_chip(ui, "作業中", true);
                    layer_status_chip(ui, "表示", *visible);
                    layer_status_chip(ui, "ロック", *locked);
                });
            });
            ui.add_space(8.0);
        }

        let action_width = ((ui.available_width() - 16.0) / 3.0).max(64.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_sized(
                    [action_width, LAYER_ACTION_BUTTON_HEIGHT],
                    egui::Button::new("追加"),
                )
                .on_hover_text("新しいレイヤーを追加して、作業レイヤーに切り替えます。")
                .clicked()
            {
                pending_action = Some(LayerAction::Add);
            }

            if ui
                .add_sized(
                    [action_width, LAYER_ACTION_BUTTON_HEIGHT],
                    egui::Button::new("複製"),
                )
                .on_hover_text("現在のレイヤーを複製して、コピー側を作業レイヤーにします。")
                .clicked()
            {
                pending_action = Some(LayerAction::DuplicateActive);
            }

            if ui
                .add_enabled(
                    layer_count > 1,
                    egui::Button::new("削除")
                        .min_size(egui::vec2(action_width, LAYER_ACTION_BUTTON_HEIGHT)),
                )
                .on_hover_text("現在のレイヤーを削除します。最低 1 レイヤーは残ります。")
                .clicked()
            {
                pending_action = Some(LayerAction::DeleteActive);
            }
        });

        if selection_count > 0 {
            ui.small(format!(
                "{selection_count} 個選択中です。移動先のレイヤーで「ここへ移動」または「ここへ複製」を使えます。"
            ));
        }

        ui.separator();

        let total_layers = layers.len();
        for (index, layer_id, name, visible, locked, element_count) in layers.into_iter().rev() {
            let is_active = layer_id == active_layer_id;
            let can_receive_selection =
                selection_count > 0 && !has_canvas_interaction && layer_id != active_layer_id;
            let active_fill = ui.visuals().selection.bg_fill.linear_multiply(0.22);
            let frame = egui::Frame::group(ui.style())
                .fill(if is_active {
                    active_fill
                } else {
                    ui.visuals().faint_bg_color
                })
                .stroke(if is_active {
                    ui.visuals().selection.stroke
                } else {
                    ui.visuals().widgets.noninteractive.bg_stroke
                })
                .inner_margin(egui::Margin::same(10));

            frame.show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
                ui.horizontal(|ui| {
                    let active_response = ui
                        .selectable_label(
                            is_active,
                            RichText::new(name.as_str()).strong().size(if is_active {
                                15.0
                            } else {
                                14.0
                            }),
                        )
                        .on_hover_text("このレイヤーを作業レイヤーにします。");
                    if active_response.clicked() {
                        pending_action = Some(LayerAction::SetActive(layer_id));
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    layer_status_chip(ui, "作業中", is_active);
                    layer_status_chip(ui, "表示", visible);
                    layer_status_chip(ui, "ロック", locked);
                    ui.small(format!("{element_count}個"));
                    if index + 1 == total_layers {
                        ui.small("最前面");
                    } else if index == 0 {
                        ui.small("最背面");
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_sized(
                            [LAYER_CHIP_BUTTON_WIDTH, LAYER_ACTION_BUTTON_HEIGHT],
                            egui::Button::new("表示").selected(visible),
                        )
                        .on_hover_text(if visible {
                            "このレイヤーを隠します。"
                        } else {
                            "このレイヤーを再表示します。"
                        })
                        .clicked()
                    {
                        pending_action = Some(LayerAction::ToggleVisibility(layer_id));
                    }

                    if ui
                        .add_sized(
                            [LAYER_CHIP_BUTTON_WIDTH, LAYER_ACTION_BUTTON_HEIGHT],
                            egui::Button::new("ロック").selected(locked),
                        )
                        .on_hover_text(if locked {
                            "このレイヤーを編集できるように戻します。"
                        } else {
                            "このレイヤーを表示だけにして編集不可にします。"
                        })
                        .clicked()
                    {
                        pending_action = Some(LayerAction::ToggleLocked(layer_id));
                    }

                    if ui
                        .add_sized(
                            [LAYER_CHIP_BUTTON_WIDTH, LAYER_ACTION_BUTTON_HEIGHT],
                            egui::Button::new("上へ"),
                        )
                        .on_hover_text("レイヤー順をひとつ上げます。")
                        .clicked()
                    {
                        pending_action = Some(LayerAction::MoveUp(layer_id));
                    }
                    if ui
                        .add_sized(
                            [LAYER_CHIP_BUTTON_WIDTH, LAYER_ACTION_BUTTON_HEIGHT],
                            egui::Button::new("下へ"),
                        )
                        .on_hover_text("レイヤー順をひとつ下げます。")
                        .clicked()
                    {
                        pending_action = Some(LayerAction::MoveDown(layer_id));
                    }
                });

                if can_receive_selection {
                    ui.horizontal_wrapped(|ui| {
                        let can_drop_here = visible && !locked;
                        if ui
                            .add_enabled(can_drop_here, egui::Button::new("ここへ移動"))
                            .on_hover_text("選択中の要素をこのレイヤーへ移します。")
                            .clicked()
                        {
                            pending_action = Some(LayerAction::MoveSelectionTo(layer_id));
                        }
                        if ui
                            .add_enabled(can_drop_here, egui::Button::new("ここへ複製"))
                            .on_hover_text("選択中の要素をこのレイヤーへ複製します。")
                            .clicked()
                        {
                            pending_action = Some(LayerAction::DuplicateSelectionTo(layer_id));
                        }
                    });
                }
            });
            ui.add_space(4.0);
        }

        ui.separator();
        ui.label(RichText::new("現在のレイヤー名").strong());
        let rename_response = ui.text_edit_singleline(&mut self.layer_name_draft);
        let rename_on_enter =
            rename_response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
        if ui
            .button("レイヤー名を変更")
            .on_hover_text("現在のレイヤー名を更新します。")
            .clicked()
            || rename_on_enter
        {
            pending_action = Some(LayerAction::RenameActive);
        }

        if let Some((_, active_name, visible, locked, _)) = active_layer_state {
            if !visible {
                ui.small(format!(
                    "{active_name} は非表示です。表示も書き出しもされません。"
                ));
            } else if locked {
                ui.small(format!(
                    "{active_name} はロック中です。表示はされますが、選択や編集はできません。"
                ));
            }
        }

        if let Some(action) = pending_action {
            match action {
                LayerAction::Add => self.add_layer(),
                LayerAction::DuplicateActive => self.duplicate_active_layer(),
                LayerAction::DeleteActive => self.delete_active_layer(),
                LayerAction::RenameActive => self.rename_active_layer(),
                LayerAction::SetActive(layer_id) => self.set_active_layer(layer_id),
                LayerAction::ToggleVisibility(layer_id) => self.toggle_layer_visibility(layer_id),
                LayerAction::ToggleLocked(layer_id) => self.toggle_layer_locked(layer_id),
                LayerAction::MoveUp(layer_id) => self.move_layer_up(layer_id),
                LayerAction::MoveDown(layer_id) => self.move_layer_down(layer_id),
                LayerAction::MoveSelectionTo(layer_id) => self.move_selection_to_layer(layer_id),
                LayerAction::DuplicateSelectionTo(layer_id) => {
                    self.duplicate_selection_to_layer(layer_id);
                }
            }
        }
    }

    fn show_actions(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let has_canvas_interaction = self.canvas.has_active_interaction();
        let selection_count = self.canvas.selection_count();
        let can_undo = has_canvas_interaction || self.history.can_undo();
        let can_redo = !has_canvas_interaction && self.history.can_redo();
        let can_clear = has_canvas_interaction || self.document().has_elements();
        let can_file_io = !has_canvas_interaction && !self.has_pending_storage_task();
        let can_adjust_view = !has_canvas_interaction;
        let can_reorder = !has_canvas_interaction && selection_count >= 1;
        let can_align = !has_canvas_interaction && selection_count >= 2;
        let can_group = !has_canvas_interaction && selection_count >= 2;
        let can_ungroup =
            !has_canvas_interaction && self.canvas.selection_contains_group(self.document());
        let can_distribute = !has_canvas_interaction && selection_count >= 3;

        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(can_undo, egui::Button::new("元に戻す"))
                    .on_hover_text("ひとつ前の編集に戻します。")
                    .clicked()
                {
                    self.perform_undo();
                }

                if ui
                    .add_enabled(can_redo, egui::Button::new("やり直す"))
                    .on_hover_text("元に戻した編集をもう一度適用します。")
                    .clicked()
                {
                    self.perform_redo();
                }

                if ui
                    .add_enabled(can_clear, egui::Button::new("クリア"))
                    .on_hover_text("作品全体を消去します。")
                    .clicked()
                {
                    self.perform_clear();
                }

                ui.separator();

                if ui
                    .add_enabled(can_file_io, egui::Button::new("JSON保存"))
                    .on_hover_text("再編集できる JSON を保存します。")
                    .clicked()
                {
                    self.save_document(ctx);
                }

                if ui
                    .add_enabled(can_file_io, egui::Button::new("JSONを開く"))
                    .on_hover_text("保存した JSON を開いて続きを編集します。")
                    .clicked()
                {
                    self.load_document(ctx);
                }

                if ui
                    .add_enabled(can_file_io, egui::Button::new("PNG書き出し"))
                    .on_hover_text("共有しやすい PNG 画像を書き出します。")
                    .clicked()
                {
                    self.export_png(ctx, PngExportKind::Opaque);
                }

                if ui
                    .add_enabled(can_file_io, egui::Button::new("透過PNG"))
                    .on_hover_text("背景を透明にした PNG 素材を書き出します。")
                    .clicked()
                {
                    self.export_png(ctx, PngExportKind::Transparent);
                }

                ui.separator();

                ui.add_enabled_ui(can_align, |ui| {
                    ui.menu_button("整列", |ui| {
                        for alignment in [
                            AlignmentKind::Left,
                            AlignmentKind::HorizontalCenter,
                            AlignmentKind::Right,
                            AlignmentKind::Top,
                            AlignmentKind::VerticalCenter,
                            AlignmentKind::Bottom,
                        ] {
                            if ui.button(alignment.label()).clicked() {
                                self.apply_alignment(alignment);
                            }
                        }
                    });
                });

                if ui
                    .add_enabled(can_group, egui::Button::new("グループ化"))
                    .on_hover_text("複数選択をひとまとまりにします。")
                    .clicked()
                {
                    self.apply_group();
                }

                if ui
                    .add_enabled(can_ungroup, egui::Button::new("グループ解除"))
                    .on_hover_text("選択中のグループを 1 段だけ展開します。")
                    .clicked()
                {
                    self.apply_ungroup();
                }

                ui.add_enabled_ui(can_distribute, |ui| {
                    ui.menu_button("等間隔", |ui| {
                        for distribution in
                            [DistributionKind::Horizontal, DistributionKind::Vertical]
                        {
                            if ui.button(distribution.label()).clicked() {
                                self.apply_distribution(distribution);
                            }
                        }
                    });
                });

                ui.add_enabled_ui(can_reorder, |ui| {
                    ui.menu_button("重なり順", |ui| {
                        for command in [
                            StackOrderCommand::BringToFront,
                            StackOrderCommand::BringForward,
                            StackOrderCommand::SendBackward,
                            StackOrderCommand::SendToBack,
                        ] {
                            if ui.button(command.label()).clicked() {
                                self.apply_stack_order(command);
                            }
                        }
                    });
                });

                ui.separator();

                if ui
                    .add_enabled(can_adjust_view, egui::Button::new("-"))
                    .on_hover_text("表示を少し縮小します。")
                    .clicked()
                {
                    self.zoom_out();
                }

                ui.label(RichText::new(self.canvas.zoom_label()).monospace());

                if ui
                    .add_enabled(can_adjust_view, egui::Button::new("+"))
                    .on_hover_text("表示を少し拡大します。")
                    .clicked()
                {
                    self.zoom_in();
                }

                if ui
                    .add_enabled(can_adjust_view, egui::Button::new("表示をリセット"))
                    .on_hover_text("ズームと表示位置を初期状態へ戻します。")
                    .clicked()
                {
                    self.reset_view();
                }

                ui.separator();
                if ui
                    .button(if self.show_help {
                        "ヘルプを閉じる"
                    } else {
                        "ヘルプ"
                    })
                    .on_hover_text("操作説明やチュートリアル再表示を開きます。")
                    .clicked()
                {
                    self.show_help = !self.show_help;
                }
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("状態").small().strong());
                ui.add_sized(
                    [ui.available_width(), 18.0],
                    egui::Label::new(self.status_message.rich_text()).truncate(),
                );
            });
        });
    }

    fn show_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_help {
            return;
        }

        let mut open = self.show_help;
        let mut reopen_tutorial = false;
        egui::Window::new("かんたんヘルプ")
            .open(&mut open)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.label(RichText::new("短く確認する").strong());
                ui.small("描く: ペン / えんぴつ / マーカーか図形ツールを選んでドラッグします。");
                ui.small("色: スポイト、最近使った色、簡易パレットで線色や塗り色をすぐ使い回せます。");
                ui.small("選ぶ: 選択ツールで移動や変形、複数選択でまとめて整理できます。");
                ui.small("パンとズーム: 手のひら、Space+Drag、2本指ドラッグ、ピンチが使えます。");
                ui.small("保存: JSON保存は再編集用、PNG書き出しは共有用、透過PNGは素材用です。");
                ui.small("レイヤー: 右側で現在のレイヤー、表示、ロックを切り替えます。");
                ui.small("左パネル: 下までスクロールすると配置補助や保存メモが見られます。");
                #[cfg(target_arch = "wasm32")]
                ui.small("Web版: JSON保存 と PNG書き出し / 透過PNG はダウンロード、JSONを開く はファイル選択です。");

                ui.add_space(8.0);
                ui.label(RichText::new("ショートカット").strong());
                ui.small("元に戻す: Ctrl/Cmd+Z · やり直す: Ctrl/Cmd+Shift+Z または Ctrl/Cmd+Y");
                ui.small("JSON保存: Ctrl/Cmd+S · JSONを開く: Ctrl/Cmd+O · PNG書き出し: Ctrl/Cmd+Shift+E");
                ui.small("ツール: V 選択 · H 手のひら · B ペン · N えんぴつ · M マーカー · I スポイト · R 四角形 · O 楕円 · L 直線 · E 消しゴム");

                ui.add_space(10.0);
                if ui.button("チュートリアルをもう一度見る").clicked() {
                    reopen_tutorial = true;
                }
            });

        self.show_help = open;
        if reopen_tutorial {
            self.open_tutorial();
        }
    }

    fn show_tutorial_window(&mut self, ctx: &egui::Context) {
        if !self.tutorial.visible {
            return;
        }

        #[derive(Clone, Copy)]
        enum TutorialAction {
            None,
            Back,
            Next,
            Skip,
            Complete,
        }

        let step_count = tutorial_step_count();
        let step_index = self.tutorial.step_index.min(step_count.saturating_sub(1));
        let step = tutorial_step(step_index);
        let mut open = self.tutorial.visible;
        let mut pending_action = TutorialAction::None;

        egui::Window::new("ミニチュートリアル")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(format!("{} / {}", step_index + 1, step_count)).small());
                ui.heading(step.title);
                ui.label(step.body);
                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.small("次にやること");
                    ui.label(step.action);
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("スキップ").clicked() {
                        pending_action = TutorialAction::Skip;
                    }
                    if ui
                        .add_enabled(step_index > 0, egui::Button::new("前へ"))
                        .clicked()
                    {
                        pending_action = TutorialAction::Back;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if step_index + 1 == step_count {
                            "完了"
                        } else {
                            "次へ"
                        };
                        if ui.button(label).clicked() {
                            pending_action = if step_index + 1 == step_count {
                                TutorialAction::Complete
                            } else {
                                TutorialAction::Next
                            };
                        }
                    });
                });
            });

        if !open && matches!(pending_action, TutorialAction::None) {
            pending_action = TutorialAction::Skip;
        }

        match pending_action {
            TutorialAction::None => {
                self.tutorial.visible = open;
            }
            TutorialAction::Back => {
                self.tutorial.step_index = self.tutorial.step_index.saturating_sub(1);
            }
            TutorialAction::Next => {
                self.tutorial.step_index =
                    (self.tutorial.step_index + 1).min(step_count.saturating_sub(1));
            }
            TutorialAction::Skip => self.close_tutorial(false),
            TutorialAction::Complete => self.close_tutorial(true),
        }
    }

    fn perform_undo(&mut self) {
        if self.canvas.discard_active_interaction() {
            self.set_info("進行中の操作を取り消しました。");
        } else if self.history.undo() {
            self.canvas.clear_selection();
            self.sync_layer_name_draft();
            self.set_info("ひとつ前に戻しました。");
        }
    }

    fn perform_redo(&mut self) {
        if self.history.redo() {
            self.canvas.clear_selection();
            self.sync_layer_name_draft();
            self.set_info("やり直しました。");
        }
    }

    fn perform_clear(&mut self) {
        let discarded = self.canvas.discard_active_interaction();
        if self.history.clear() {
            self.canvas.clear_selection();
            self.sync_layer_name_draft();
            self.set_info("キャンバスをクリアしました。");
        } else if discarded {
            self.set_info("進行中の操作を取り消しました。");
        }
    }

    fn zoom_in(&mut self) {
        let canvas_size = self.document().canvas_size;
        if self.canvas.zoom_in(canvas_size) {
            self.set_info(format!(
                "ズームを {} にしました。",
                self.canvas.zoom_label()
            ));
        }
    }

    fn zoom_out(&mut self) {
        let canvas_size = self.document().canvas_size;
        if self.canvas.zoom_out(canvas_size) {
            self.set_info(format!(
                "ズームを {} にしました。",
                self.canvas.zoom_label()
            ));
        }
    }

    fn reset_view(&mut self) {
        let canvas_size = self.document().canvas_size;
        self.canvas.request_view_reset();
        let _ = self.canvas.reset_view(canvas_size);
        self.set_info("表示をキャンバス全体に戻しました。");
    }

    fn commit_element(&mut self, element: PaintElement) {
        let label = element.kind_label().to_owned();
        if self.history.commit_element(element) {
            self.canvas.clear_selection();
            self.set_info(format!("{label}を追加しました。"));
        }
    }

    fn apply_document_edit(&mut self, edit: CommittedDocumentEdit) {
        let selection_layer_id = edit.document.active_layer_id();
        if self.history.replace_document(edit.document) {
            self.canvas
                .set_selection_indices(selection_layer_id, edit.selection_indices);
            let message = match edit.mode {
                DocumentEditMode::Move => "選択中の要素を移動しました。",
                DocumentEditMode::Resize => {
                    if self.canvas.selection_count() > 1 {
                        "選択中の要素のサイズを変更しました。"
                    } else {
                        "選択中の図形のサイズを変更しました。"
                    }
                }
                DocumentEditMode::Rotate => {
                    if self.canvas.selection_count() > 1 {
                        "選択中の要素を回転しました。"
                    } else {
                        "選択中の図形を回転しました。"
                    }
                }
                DocumentEditMode::Guide => "ガイドを移動しました。",
                DocumentEditMode::Group => "選択中の要素をグループ化しました。",
                DocumentEditMode::Ungroup => "選択中のグループを解除しました。",
                DocumentEditMode::Align(alignment) => match alignment {
                    AlignmentKind::Left => "左揃えにしました。",
                    AlignmentKind::HorizontalCenter => "横中央揃えにしました。",
                    AlignmentKind::Right => "右揃えにしました。",
                    AlignmentKind::Top => "上揃えにしました。",
                    AlignmentKind::VerticalCenter => "縦中央揃えにしました。",
                    AlignmentKind::Bottom => "下揃えにしました。",
                },
                DocumentEditMode::Distribute(distribution) => match distribution {
                    DistributionKind::Horizontal => "横方向に等間隔配置しました。",
                    DistributionKind::Vertical => "縦方向に等間隔配置しました。",
                },
                DocumentEditMode::Reorder(command) => match command {
                    StackOrderCommand::BringToFront => "選択中の要素を最前面へ移動しました。",
                    StackOrderCommand::SendToBack => "選択中の要素を最背面へ移動しました。",
                    StackOrderCommand::BringForward => "選択中の要素を一つ前面へ移動しました。",
                    StackOrderCommand::SendBackward => "選択中の要素を一つ背面へ移動しました。",
                },
            };
            self.set_info(message);
            self.sync_layer_name_draft();
        }
    }

    fn apply_alignment(&mut self, alignment: AlignmentKind) {
        let document = self.document().clone();
        if let Some(edit) = self.canvas.apply_alignment(&document, alignment) {
            self.apply_document_edit(edit);
        }
    }

    fn apply_group(&mut self) {
        let document = self.document().clone();
        if let Some(edit) = self.canvas.apply_group(&document) {
            self.apply_document_edit(edit);
        }
    }

    fn apply_ungroup(&mut self) {
        let document = self.document().clone();
        if let Some(edit) = self.canvas.apply_ungroup(&document) {
            self.apply_document_edit(edit);
        }
    }

    fn apply_distribution(&mut self, distribution: DistributionKind) {
        let document = self.document().clone();
        if let Some(edit) = self.canvas.apply_distribution(&document, distribution) {
            self.apply_document_edit(edit);
        }
    }

    fn apply_stack_order(&mut self, command: StackOrderCommand) {
        let document = self.document().clone();
        if let Some(edit) = self.canvas.apply_stack_order(&document, command) {
            self.apply_document_edit(edit);
        }
    }

    fn apply_layer_document_change(&mut self, document: PaintDocument, message: impl Into<String>) {
        if self.history.replace_document(document) {
            self.canvas.discard_active_interaction();
            self.canvas.clear_selection();
            self.sync_layer_name_draft();
            self.set_info(message);
        }
    }

    fn apply_document_configuration_change(
        &mut self,
        document: PaintDocument,
        message: impl Into<String>,
    ) {
        if self.history.replace_document(document) {
            self.canvas.discard_active_interaction();
            self.sync_layer_name_draft();
            self.set_info(message);
        }
    }

    fn apply_layer_selection_document_change(
        &mut self,
        document: PaintDocument,
        selection_layer_id: LayerId,
        selection_indices: Vec<usize>,
        message: impl Into<String>,
    ) {
        if self.history.replace_document(document) {
            self.canvas.discard_active_interaction();
            self.canvas
                .set_selection_indices(selection_layer_id, selection_indices);
            self.sync_layer_name_draft();
            self.set_info(message);
        }
    }

    fn set_active_layer(&mut self, layer_id: LayerId) {
        if self.history.set_active_layer(layer_id) {
            self.canvas.discard_active_interaction();
            self.canvas.clear_selection();
            self.sync_layer_name_draft();
            if let Some(layer) = self.document().active_layer() {
                self.set_info(format!("作業レイヤーを {} に切り替えました。", layer.name));
            }
        }
    }

    fn add_layer(&mut self) {
        let document = self.document().clone();
        let (next, layer_id) = document.add_layer_document();
        let layer_name = next
            .layer(layer_id)
            .map(|layer| layer.name.clone())
            .unwrap_or_else(|| "新しいレイヤー".to_owned());
        self.apply_layer_document_change(next, format!("{layer_name} を追加しました。"));
    }

    fn duplicate_active_layer(&mut self) {
        let document = self.document().clone();
        if let Some((next, layer_id)) = document.duplicate_active_layer_document() {
            let layer_name = next
                .layer(layer_id)
                .map(|layer| layer.name.clone())
                .unwrap_or_else(|| "レイヤーのコピー".to_owned());
            self.apply_layer_document_change(next, format!("{layer_name} を追加しました。"));
        }
    }

    fn delete_active_layer(&mut self) {
        let document = self.document().clone();
        if let Some((next, next_active)) = document.delete_active_layer_document() {
            let next_name = next
                .layer(next_active)
                .map(|layer| layer.name.clone())
                .unwrap_or_else(|| "残りのレイヤー".to_owned());
            self.apply_layer_document_change(
                next,
                format!("現在のレイヤーを削除しました。{next_name} が作業レイヤーです。"),
            );
        }
    }

    fn rename_active_layer(&mut self) {
        let Some(active_layer) = self.document().active_layer() else {
            return;
        };
        let document = self.document().clone();
        if let Some(next) = document.renamed_layer_document(active_layer.id, &self.layer_name_draft)
        {
            self.apply_layer_document_change(
                next,
                format!(
                    "レイヤー名を {} に変更しました。",
                    self.layer_name_draft.trim()
                ),
            );
        }
    }

    fn toggle_layer_visibility(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_layer_visibility_document(layer_id) {
            let message = next
                .layer(layer_id)
                .map(|layer| {
                    if layer.visible {
                        format!("{} を表示しました。", layer.name)
                    } else {
                        format!("{} を非表示にしました。", layer.name)
                    }
                })
                .unwrap_or_else(|| "レイヤーの表示状態を変更しました。".to_owned());
            self.apply_layer_document_change(next, message);
        }
    }

    fn toggle_layer_locked(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_layer_locked_document(layer_id) {
            let message = next
                .layer(layer_id)
                .map(|layer| {
                    if layer.locked {
                        format!("{} をロックしました。", layer.name)
                    } else {
                        format!("{} のロックを解除しました。", layer.name)
                    }
                })
                .unwrap_or_else(|| "レイヤーのロック状態を変更しました。".to_owned());
            self.apply_layer_document_change(next, message);
        }
    }

    fn move_layer_up(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if let Some(next) = document.moved_layer_up_document(layer_id) {
            self.apply_layer_document_change(next, "レイヤーを上へ移動しました。");
        }
    }

    fn move_layer_down(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if let Some(next) = document.moved_layer_down_document(layer_id) {
            self.apply_layer_document_change(next, "レイヤーを下へ移動しました。");
        }
    }

    fn toggle_rulers_visibility(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_rulers_visibility_document() {
            let message = if next.rulers().visible {
                "ルーラーを表示しました。"
            } else {
                "ルーラーを非表示にしました。"
            };
            self.apply_document_configuration_change(next, message);
        }
    }

    fn toggle_grid_visibility(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_grid_visibility_document() {
            let message = if next.grid().visible {
                "グリッドを表示しました。"
            } else {
                "グリッドを非表示にしました。"
            };
            self.apply_document_configuration_change(next, message);
        }
    }

    fn toggle_grid_snap(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_grid_snap_document() {
            let message = if next.grid().snap_enabled {
                "グリッド吸着をオンにしました。"
            } else {
                "グリッド吸着をオフにしました。"
            };
            self.apply_document_configuration_change(next, message);
        }
    }

    fn set_grid_spacing(&mut self, spacing: f32) {
        let document = self.document().clone();
        if let Some(next) = document.set_grid_spacing_document(spacing) {
            let applied_spacing = next.grid().spacing;
            self.apply_document_configuration_change(
                next,
                format!("グリッド間隔を {:.0}px に設定しました。", applied_spacing),
            );
        }
    }

    fn toggle_smart_guides_visibility(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_smart_guides_visibility_document() {
            let message = if next.smart_guides().visible {
                "スマートガイドをオンにしました。"
            } else {
                "スマートガイドをオフにしました。"
            };
            self.apply_document_configuration_change(next, message);
        }
    }

    fn toggle_guides_visibility(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_guides_visibility_document() {
            let message = if next.guides().visible {
                "ガイドを表示しました。"
            } else {
                "ガイドを非表示にしました。"
            };
            self.apply_document_configuration_change(next, message);
        }
    }

    fn toggle_guides_snap(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_guides_snap_document() {
            let message = if next.guides().snap_enabled {
                "ガイド吸着をオンにしました。"
            } else {
                "ガイド吸着をオフにしました。"
            };
            self.apply_document_configuration_change(next, message);
        }
    }

    fn add_guide(&mut self, axis: GuideAxis) {
        let document = self.document().clone();
        let position = self.suggested_guide_position(axis);
        if let Some(next) = document.add_guide_document(axis, position) {
            self.apply_document_configuration_change(
                next,
                format!(
                    "{}ガイドを {:.0}px に追加しました。",
                    axis.label(),
                    position
                ),
            );
        }
    }

    fn remove_guide(&mut self, index: usize) {
        let document = self.document().clone();
        let Some(guide) = document.guides().lines.get(index).copied() else {
            return;
        };
        if let Some(next) = document.remove_guide_document(index) {
            self.apply_document_configuration_change(
                next,
                format!(
                    "{}ガイド（{:.0}px）を削除しました。",
                    guide.axis.label(),
                    guide.position
                ),
            );
        }
    }

    fn suggested_guide_position(&self, axis: GuideAxis) -> f32 {
        let selection_indices = self.canvas.selection_indices();
        let selection_center = self
            .document()
            .selection_bounds(selection_indices)
            .map(|bounds| bounds.center());
        match axis {
            GuideAxis::Horizontal => selection_center
                .map(|center| center.y)
                .unwrap_or(self.document().canvas_size.height * 0.5),
            GuideAxis::Vertical => selection_center
                .map(|center| center.x)
                .unwrap_or(self.document().canvas_size.width * 0.5),
        }
    }

    fn move_selection_to_layer(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if self.canvas.selection_layer_id() != Some(document.active_layer_id()) {
            return;
        }
        let selection_indices = self.canvas.selection_indices().to_vec();
        if selection_indices.is_empty() {
            return;
        }

        let Some(destination) = document.layer(layer_id) else {
            return;
        };
        if !destination.is_editable() {
            self.set_error(format!(
                "{} は表示中かつロック解除されていないと、要素を移動できません。",
                destination.name
            ));
            return;
        }

        if let Some((next, next_selection)) =
            document.moved_selection_to_layer_document(&selection_indices, layer_id)
        {
            let moved_count = next_selection.len();
            let destination_name = next
                .layer(layer_id)
                .map(|layer| layer.name.clone())
                .unwrap_or_else(|| "移動先レイヤー".to_owned());
            let message = if moved_count == 1 {
                format!("選択中の要素を {destination_name} へ移動しました。")
            } else {
                format!("選択中の {moved_count} 個の要素を {destination_name} へ移動しました。")
            };
            self.apply_layer_selection_document_change(next, layer_id, next_selection, message);
        }
    }

    fn duplicate_selection_to_layer(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if self.canvas.selection_layer_id() != Some(document.active_layer_id()) {
            return;
        }
        let selection_indices = self.canvas.selection_indices().to_vec();
        if selection_indices.is_empty() {
            return;
        }

        let Some(destination) = document.layer(layer_id) else {
            return;
        };
        if !destination.is_editable() {
            self.set_error(format!(
                "{} は表示中かつロック解除されていないと、要素を複製できません。",
                destination.name
            ));
            return;
        }

        if let Some((next, next_selection)) =
            document.duplicated_selection_to_layer_document(&selection_indices, layer_id)
        {
            let duplicated_count = next_selection.len();
            let destination_name = next
                .layer(layer_id)
                .map(|layer| layer.name.clone())
                .unwrap_or_else(|| "複製先レイヤー".to_owned());
            let message = if duplicated_count == 1 {
                format!("選択中の要素を {destination_name} へ複製しました。")
            } else {
                format!(
                    "選択中の {duplicated_count} 個の要素を {destination_name} へ複製しました。"
                )
            };
            self.apply_layer_selection_document_change(next, layer_id, next_selection, message);
        }
    }

    fn finish_save(&mut self, saved: SavedDocument) {
        self.document_name = saved.file_name;
        self.saved_snapshot = self.document().clone();
        self.set_info(format!(
            "再編集用の JSON を {} として保存しました。",
            self.document_name
        ));
    }

    fn finish_load(&mut self, loaded: LoadedDocument) {
        self.canvas.discard_active_interaction();
        self.canvas.clear_selection();
        self.canvas.request_view_reset();
        self.history.replace_document(loaded.document.clone());
        self.document_name = loaded.file_name;
        self.saved_snapshot = loaded.document;
        self.sync_layer_name_draft();
        self.set_info(format!("{} を開きました。", self.document_name));
    }

    fn finish_export(&mut self, exported: ExportedImage, kind: PngExportKind) {
        self.set_info(match kind {
            PngExportKind::Opaque => {
                format!("PNG を {} として書き出しました。", exported.file_name)
            }
            PngExportKind::Transparent => {
                format!("透過PNG を {} として書き出しました。", exported.file_name)
            }
        });
    }

    fn storage_action_title(action: &'static str) -> &'static str {
        match action {
            "save" => "JSON保存",
            "load" => "JSONを開く",
            "export" => "PNG書き出し",
            "export-transparent" => "透過PNG",
            _ => "ファイル操作",
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn storage_pending_message(action: &'static str) -> &'static str {
        match action {
            "save" => {
                "ブラウザの保存ダイアログを開きました。再編集用 JSON の保存先を選んでください。"
            }
            "load" => "ブラウザのファイル選択を開きました。.paint.json を選んでください。",
            "export" => "ブラウザで PNG ダウンロードを準備しています...",
            "export-transparent" => "ブラウザで透過PNGダウンロードを準備しています...",
            _ => "ブラウザのファイル操作を待っています...",
        }
    }

    fn storage_error_message(action: &'static str, error: &StorageError) -> String {
        let label = Self::storage_action_title(action);
        match error {
            StorageError::Cancelled => format!("{label}をキャンセルしました。"),
            StorageError::EmptyFile => {
                format!("{label}できませんでした。選択したファイルは空です。")
            }
            StorageError::UnsupportedFormat(_) => {
                format!(
                    "{label}できませんでした。このアプリの .paint.json ファイルを選んでください。"
                )
            }
            StorageError::UnsupportedVersion(_) => {
                format!(
                    "{label}できませんでした。このドキュメントのバージョンにはまだ対応していません。"
                )
            }
            StorageError::Deserialize(_) => {
                format!("{label}できませんでした。お絵かきドキュメントとして読み込めませんでした。")
            }
            StorageError::Serialize(_) => {
                format!("{label}できませんでした。ファイルの準備中に失敗しました。")
            }
            StorageError::Render(_) => {
                format!("{label}できませんでした。キャンバスの描画中に失敗しました。")
            }
            StorageError::Io(details) => format!("{label}できませんでした: {details}"),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn report_storage_result(&mut self, action: &'static str, result: Result<(), StorageError>) {
        if let Err(error) = result {
            match &error {
                StorageError::Cancelled => {
                    self.set_info(Self::storage_error_message(action, &error));
                }
                _ => {
                    self.set_error(Self::storage_error_message(action, &error));
                }
            }
        }
    }

    fn save_document(&mut self, _ctx: &egui::Context) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let result = self
                .storage
                .save_document_via_dialog(self.document(), &self.document_name)
                .map(|saved| self.finish_save(saved));
            self.report_storage_result("save", result);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let slot = Rc::new(RefCell::new(None));
            let task_slot = slot.clone();
            let storage = self.storage;
            let document = self.document().clone();
            let suggested_name = self.document_name.clone();
            let ctx = _ctx.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let result = storage
                    .save_document_via_dialog(&document, &suggested_name)
                    .await
                    .map(WebStorageResult::Saved);
                *task_slot.borrow_mut() = Some(result);
                ctx.request_repaint();
            });

            self.pending_web_task = Some(PendingWebStorageTask {
                label: "save",
                slot,
            });
            self.set_info(Self::storage_pending_message("save"));
        }
    }

    fn load_document(&mut self, _ctx: &egui::Context) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let result = self
                .storage
                .load_document_via_dialog()
                .map(|loaded| self.finish_load(loaded));
            self.report_storage_result("load", result);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let slot = Rc::new(RefCell::new(None));
            let task_slot = slot.clone();
            let storage = self.storage;
            let ctx = _ctx.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let result = storage
                    .load_document_via_dialog()
                    .await
                    .map(WebStorageResult::Loaded);
                *task_slot.borrow_mut() = Some(result);
                ctx.request_repaint();
            });

            self.pending_web_task = Some(PendingWebStorageTask {
                label: "load",
                slot,
            });
            self.set_info(Self::storage_pending_message("load"));
        }
    }

    fn export_png(&mut self, _ctx: &egui::Context, kind: PngExportKind) {
        let suggested_name = self
            .storage
            .suggested_png_file_name_for_kind(&self.document_name, kind);
        let pending_label = match kind {
            PngExportKind::Opaque => "export",
            PngExportKind::Transparent => "export-transparent",
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            let result = self
                .storage
                .export_png_via_dialog_with_kind(self.document(), &suggested_name, kind)
                .map(|exported| self.finish_export(exported, kind));
            self.report_storage_result(pending_label, result);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let slot = Rc::new(RefCell::new(None));
            let task_slot = slot.clone();
            let storage = self.storage;
            let document = self.document().clone();
            let ctx = _ctx.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let result = storage
                    .export_png_via_dialog_with_kind(&document, &suggested_name, kind)
                    .await
                    .map(|image| WebStorageResult::Exported { image, kind });
                *task_slot.borrow_mut() = Some(result);
                ctx.request_repaint();
            });

            self.pending_web_task = Some(PendingWebStorageTask {
                label: pending_label,
                slot,
            });
            self.set_info(Self::storage_pending_message(pending_label));
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let has_canvas_interaction = self.canvas.has_active_interaction();

        let redo_pressed = ctx.input_mut(|input| input.consume_shortcut(&shortcut_redo()))
            || ctx.input_mut(|input| input.consume_shortcut(&shortcut_redo_alt()));
        if redo_pressed && !has_canvas_interaction && self.history.can_redo() {
            self.perform_redo();
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_undo()))
            && (has_canvas_interaction || self.history.can_undo())
        {
            self.perform_undo();
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_save()))
            && !has_canvas_interaction
            && !self.has_pending_storage_task()
        {
            self.save_document(ctx);
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_load()))
            && !has_canvas_interaction
            && !self.has_pending_storage_task()
        {
            self.load_document(ctx);
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_export_png()))
            && !has_canvas_interaction
            && !self.has_pending_storage_task()
        {
            self.export_png(ctx, PngExportKind::Opaque);
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_group()))
            && !has_canvas_interaction
            && self.canvas.selection_count() >= 2
        {
            self.apply_group();
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_ungroup()))
            && !has_canvas_interaction
            && self.canvas.selection_contains_group(self.document())
        {
            self.apply_ungroup();
        }

        let zoom_in_pressed = ctx.input_mut(|input| input.consume_shortcut(&shortcut_zoom_in()))
            || ctx.input_mut(|input| input.consume_shortcut(&shortcut_zoom_in_alt()));
        if zoom_in_pressed && !has_canvas_interaction {
            self.zoom_in();
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_zoom_out()))
            && !has_canvas_interaction
        {
            self.zoom_out();
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_reset_view()))
            && !has_canvas_interaction
        {
            self.reset_view();
        }

        if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
            && self.canvas.discard_active_interaction()
        {
            self.set_info("進行中の操作をキャンセルしました。");
        }

        if !has_canvas_interaction {
            self.handle_tool_shortcuts(ctx);
        }
    }

    fn handle_tool_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::V)) {
            self.apply_tool_button_selection(CanvasToolKind::Select);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::H)) {
            self.set_active_tool(CanvasToolKind::Pan, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::B)) {
            self.set_active_tool(CanvasToolKind::Brush, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::N)) {
            self.set_active_tool(CanvasToolKind::Pencil, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::M)) {
            self.set_active_tool(CanvasToolKind::Marker, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::I)) {
            self.set_active_tool(CanvasToolKind::Eyedropper, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::R)) {
            self.set_active_tool(CanvasToolKind::Rectangle, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::O)) {
            self.set_active_tool(CanvasToolKind::Ellipse, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::L)) {
            self.set_active_tool(CanvasToolKind::Line, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::E)) {
            self.set_active_tool(CanvasToolKind::Eraser, true);
        }
    }

    fn has_pending_storage_task(&self) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            self.pending_web_task.is_some()
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            false
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn poll_web_storage_task(&mut self) {
        let Some(task) = self.pending_web_task.take() else {
            return;
        };

        let maybe_result = task.slot.borrow_mut().take();
        if let Some(result) = maybe_result {
            match result {
                Ok(WebStorageResult::Saved(saved)) => self.finish_save(saved),
                Ok(WebStorageResult::Loaded(loaded)) => self.finish_load(loaded),
                Ok(WebStorageResult::Exported { image, kind }) => self.finish_export(image, kind),
                Err(error) => match &error {
                    StorageError::Cancelled => {
                        self.set_info(Self::storage_error_message(task.label, &error));
                    }
                    _ => {
                        self.set_error(Self::storage_error_message(task.label, &error));
                    }
                },
            }
        } else {
            self.pending_web_task = Some(task);
        }
    }
}

impl eframe::App for PaintApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        #[cfg(target_arch = "wasm32")]
        self.poll_web_storage_task();

        self.canvas.sync_with_document(self.history.current());
        self.handle_shortcuts(ctx);

        egui::SidePanel::left("tools_panel")
            .resizable(false)
            .default_width(220.0)
            .min_width(220.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.show_tools(ui));
            });

        egui::SidePanel::right("layers_panel")
            .resizable(false)
            .default_width(240.0)
            .min_width(240.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.show_layers(ui));
            });

        egui::TopBottomPanel::top("actions_panel")
            .resizable(false)
            .show(ctx, |ui| self.show_actions(ui, ctx));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            let tool_settings = self.tool_settings();
            let output = self.canvas.show(ui, self.history.current(), tool_settings);

            if output.needs_repaint {
                ctx.request_repaint();
            }

            if let Some(color) = output.picked_color {
                self.apply_picked_color(color);
            }

            if let Some(tool) = output.requested_tool
                && self.active_tool != tool
            {
                self.active_tool = tool;
                self.set_info(
                    "長押しから選択に入りました。続けてドラッグして移動や編集ができます。",
                );
            }

            if let Some(edit) = output.committed_edit {
                self.apply_document_edit(edit);
            }

            if let Some(element) = output.committed_element {
                self.commit_element(element);
            }
        });

        self.show_tutorial_window(ctx);
        self.show_help_window(ctx);
        self.persist_ui_state_if_needed(frame);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, APP_UI_STATE_KEY, &self.ui_state);
    }
}

fn on_off_label(value: bool) -> &'static str {
    if value { "オン" } else { "オフ" }
}

fn alpha_percent(color: RgbaColor) -> u8 {
    ((color.a as f32 / 255.0) * 100.0).round().clamp(0.0, 100.0) as u8
}

fn set_alpha_percent(color: RgbaColor, percent: u8) -> RgbaColor {
    let alpha = ((percent as f32 / 100.0) * 255.0).round().clamp(0.0, 255.0) as u8;
    RgbaColor::from_rgba(color.r, color.g, color.b, alpha)
}

fn brush_kind_summary(tool: CanvasToolKind) -> &'static str {
    match tool {
        CanvasToolKind::Brush => "くっきり描けるペン",
        CanvasToolKind::Pencil => "やや薄く軽いえんぴつ",
        CanvasToolKind::Marker => "重ねやすい半透明マーカー",
        _ => "",
    }
}

fn color_swatch_button(
    ui: &mut egui::Ui,
    color: RgbaColor,
    selected: bool,
    hover_text: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(COLOR_SWATCH_SIZE, COLOR_SWATCH_SIZE),
        egui::Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        let outer_rounding = 6.0;
        let inner_rect = rect.shrink(2.0);
        let inner_rounding = 4.0;
        let checker_light = egui::Color32::from_gray(246);
        let checker_dark = egui::Color32::from_gray(226);
        let fill = color32_from_rgba(color);

        ui.painter()
            .rect_filled(rect, outer_rounding, checker_light);

        let half_w = inner_rect.width() * 0.5;
        let half_h = inner_rect.height() * 0.5;
        for row in 0..2 {
            for col in 0..2 {
                let tile = egui::Rect::from_min_size(
                    egui::pos2(
                        inner_rect.min.x + half_w * col as f32,
                        inner_rect.min.y + half_h * row as f32,
                    ),
                    egui::vec2(half_w, half_h),
                );
                let tile_color = if (row + col) % 2 == 0 {
                    checker_light
                } else {
                    checker_dark
                };
                ui.painter().rect_filled(tile, 0.0, tile_color);
            }
        }

        ui.painter().rect_filled(inner_rect, inner_rounding, fill);
        ui.painter().rect_stroke(
            rect,
            outer_rounding,
            if selected {
                ui.visuals().selection.stroke
            } else {
                ui.visuals().widgets.inactive.bg_stroke
            },
            egui::StrokeKind::Outside,
        );
    }

    response.on_hover_text(hover_text)
}

fn tool_switch_message(tool: CanvasToolKind) -> String {
    format!(
        "{} に切り替えました。{}",
        tool.label(),
        tool_button_tooltip(tool)
    )
}

fn help_icon_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new("?").small())
            .min_size(egui::vec2(22.0, 22.0))
            .frame(false),
    )
    .on_hover_text(text)
}

fn layer_status_chip(ui: &mut egui::Ui, label: &str, active: bool) {
    let text = if active {
        RichText::new(label)
            .small()
            .strong()
            .color(ui.visuals().selection.stroke.color)
    } else {
        RichText::new(label)
            .small()
            .color(ui.visuals().weak_text_color())
    };
    ui.label(text);
}

fn tool_button_tooltip(tool: CanvasToolKind) -> &'static str {
    match tool {
        CanvasToolKind::Select => "選ぶ・動かす・変形するツールです。",
        CanvasToolKind::Pan => "キャンバスをドラッグして移動します。",
        CanvasToolKind::Brush => "はっきり描けるペンです。",
        CanvasToolKind::Pencil => "少し軽いタッチのえんぴつです。",
        CanvasToolKind::Marker => "重ねやすい半透明のマーカーです。",
        CanvasToolKind::Eyedropper => "見えている色を拾って線色や塗り色に使います。",
        CanvasToolKind::Eraser => "背景色でなぞって消します。",
        CanvasToolKind::Rectangle => "四角形の線と塗りを描きます。",
        CanvasToolKind::Ellipse => "楕円の線と塗りを描きます。",
        CanvasToolKind::Line => "始点から終点まで直線を描きます。",
    }
}

fn tutorial_step_count() -> usize {
    4
}

fn tutorial_step(step_index: usize) -> TutorialStepContent {
    match step_index {
        0 => TutorialStepContent {
            title: "まずは 1 つ描いてみましょう",
            body: "ペン、えんぴつ、マーカーか図形ツールを選んで、キャンバスをドラッグします。線色、塗り色、不透明度を少し変えるだけでも印象が変わります。",
            action: "左の「ペン」「えんぴつ」「マーカー」「四角形」「楕円」「直線」のどれかを選んで、中央でドラッグしてみます。",
        },
        1 => TutorialStepContent {
            title: "選んで動かせます",
            body: "選択ツールに切り替えると、要素をクリックして動かしたり、図形ならサイズ変更や回転もできます。",
            action: "「選択」に切り替えて、描いた要素をクリックしてみます。ドラッグで移動、ハンドルで変形できます。",
        },
        2 => TutorialStepContent {
            title: "複数選択でまとめて整理できます",
            body: "Shift+Click やドラッグ選択で複数選べます。タブレットでは「複数選択モード」を使うと、タップだけで追加選択できます。",
            action: "複数選んだら、整列・等間隔・グループ化やドラッグ移動を試してみます。",
        },
        _ => TutorialStepContent {
            title: "保存方法は 2 つです",
            body: "JSON保存 は続きから再編集したいとき用、PNG書き出し は画像共有用、透過PNG は素材向けです。迷ったらヘルプからもう一度見直せます。",
            action: "上部バーの「JSON保存」「JSONを開く」「PNG書き出し」「透過PNG」を覚えておけば、ひとまず困りません。",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{CanvasToolKind, ColorTarget, PaintApp, RECENT_COLOR_LIMIT};
    use crate::model::RgbaColor;

    #[test]
    fn multi_select_mode_switches_to_select_tool() {
        let mut app = PaintApp::default();
        app.set_active_tool(CanvasToolKind::Brush, false);

        app.set_multi_select_mode(true);

        assert!(app.multi_select_mode);
        assert_eq!(app.active_tool, CanvasToolKind::Select);
    }

    #[test]
    fn switching_to_non_select_tool_turns_off_multi_select_mode() {
        let mut app = PaintApp::default();
        app.set_multi_select_mode(true);

        app.set_active_tool(CanvasToolKind::Brush, false);

        assert!(!app.multi_select_mode);
        assert_eq!(app.active_tool, CanvasToolKind::Brush);
    }

    #[test]
    fn select_button_path_exits_multi_select_mode() {
        let mut app = PaintApp::default();
        app.set_multi_select_mode(true);

        app.apply_tool_button_selection(CanvasToolKind::Select);

        assert!(!app.multi_select_mode);
        assert_eq!(app.active_tool, CanvasToolKind::Select);
    }

    #[test]
    fn eraser_uses_its_own_width_setting() {
        let mut app = PaintApp::default();
        app.tool_widths.draw_width = 5.0;
        app.tool_widths.eraser_width = 19.0;
        app.set_active_tool(CanvasToolKind::Eraser, false);

        assert_eq!(app.tool_settings().width, 19.0);
    }

    #[test]
    fn shape_tools_reuse_draw_width_setting() {
        let mut app = PaintApp::default();
        app.tool_widths.draw_width = 9.0;
        app.tool_widths.eraser_width = 23.0;
        app.set_active_tool(CanvasToolKind::Rectangle, false);

        assert_eq!(app.tool_settings().width, 9.0);
    }

    #[test]
    fn pencil_and_marker_reuse_draw_width_setting() {
        let mut app = PaintApp::default();
        app.tool_widths.draw_width = 11.0;
        app.tool_widths.eraser_width = 21.0;

        app.set_active_tool(CanvasToolKind::Pencil, false);
        assert_eq!(app.tool_settings().width, 11.0);

        app.set_active_tool(CanvasToolKind::Marker, false);
        assert_eq!(app.tool_settings().width, 11.0);
    }

    #[test]
    fn recent_colors_dedupe_and_truncate() {
        let mut app = PaintApp::default();
        let color_a = RgbaColor::from_rgba(10, 20, 30, 255);
        let color_b = RgbaColor::from_rgba(40, 50, 60, 255);

        app.push_recent_color(color_a);
        app.push_recent_color(color_b);
        app.push_recent_color(color_a);

        assert_eq!(app.ui_state.recent_colors[0], color_a);
        assert_eq!(app.ui_state.recent_colors[1], color_b);

        for index in 0..(RECENT_COLOR_LIMIT + 3) {
            app.push_recent_color(RgbaColor::from_rgba(index as u8, 0, 0, 255));
        }

        assert_eq!(app.ui_state.recent_colors.len(), RECENT_COLOR_LIMIT);
    }

    #[test]
    fn quick_color_target_fill_enables_fill() {
        let mut app = PaintApp::default();
        app.tool_colors.fill_enabled = false;
        app.tool_colors.quick_color_target = ColorTarget::Fill;

        app.apply_quick_color(RgbaColor::from_rgba(120, 90, 60, 180), "最近使った色");

        assert!(app.tool_colors.fill_enabled);
        assert_eq!(
            app.tool_colors.fill_color,
            RgbaColor::from_rgba(120, 90, 60, 180)
        );
    }
}
