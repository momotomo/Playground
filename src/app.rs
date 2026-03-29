use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText};
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

use crate::canvas::{
    CanvasController, CanvasMessageKind, CanvasToolKind, CommittedDocumentEdit, DocumentEditMode,
    ToolSettings, color32_from_rgba, rgba_from_color32,
};
use crate::fill::FillTolerancePreset;
use crate::fonts::install_japanese_fonts;
use crate::model::{
    AlignmentKind, DistributionKind, DocumentHistory, FillElement, GuideAxis, LayerId,
    PaintDocument, PaintElement, RgbaColor, ShapeElement, ShapeKind, StackOrderCommand, Stroke,
};
use crate::storage::{
    ExportedImage, ExportedVectorGraphic, LoadedDocument, PngExportKind, SavedDocument,
    StorageError, StorageFacade,
};

const MIN_BRUSH_WIDTH: f32 = 1.0;
const MAX_BRUSH_WIDTH: f32 = 48.0;
const GRID_SPACING_PRESETS: [f32; 6] = [16.0, 24.0, 32.0, 48.0, 64.0, 96.0];
const GRID_SPACING_STEP: f32 = 8.0;
const TOOL_BUTTON_HEIGHT: f32 = 44.0;
const COLOR_SWATCH_SIZE: f32 = 36.0;
const LAYER_ACTION_BUTTON_HEIGHT: f32 = 38.0;
const LAYER_CHIP_BUTTON_WIDTH: f32 = 72.0;
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
    ExportedSvg(ExportedVectorGraphic),
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct UiStatePersistence {
    tutorial_dismissed: bool,
    recent_colors: Vec<RgbaColor>,
    bucket_fill_tolerance: FillTolerancePreset,
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

#[derive(Clone, Copy)]
enum ShapeStyleTarget {
    SelectedShape,
    SelectedShapes,
    NewShape,
}

#[derive(Clone, Copy)]
struct ShapeStyleContext {
    target: ShapeStyleTarget,
    kind: ShapeKind,
    stroke_color: RgbaColor,
    fill_color: Option<RgbaColor>,
    width: f32,
    shape_count: usize,
    total_selection_count: usize,
    fill_supported_count: usize,
    fill_enabled_count: usize,
}

impl ShapeStyleContext {
    fn is_selected_shape(self) -> bool {
        matches!(self.target, ShapeStyleTarget::SelectedShape)
    }

    fn is_selected_shapes(self) -> bool {
        matches!(self.target, ShapeStyleTarget::SelectedShapes)
    }

    fn is_selection_target(self) -> bool {
        matches!(
            self.target,
            ShapeStyleTarget::SelectedShape | ShapeStyleTarget::SelectedShapes
        )
    }

    fn supports_fill(self) -> bool {
        self.fill_supported_count > 0
    }

    fn fill_enabled(self) -> bool {
        self.supports_fill() && self.fill_enabled_count == self.fill_supported_count
    }

    fn has_any_fill(self) -> bool {
        self.fill_enabled_count > 0
    }

    fn selection_label(self) -> String {
        match self.target {
            ShapeStyleTarget::SelectedShape => "選択中".to_owned(),
            ShapeStyleTarget::SelectedShapes => {
                if self.total_selection_count == self.shape_count {
                    format!("{}個の図形", self.shape_count)
                } else {
                    format!(
                        "{}個選択 / 図形{}個",
                        self.total_selection_count, self.shape_count
                    )
                }
            }
            ShapeStyleTarget::NewShape => "次に描く図形".to_owned(),
        }
    }

    fn kind_summary_label(self) -> String {
        match self.target {
            ShapeStyleTarget::SelectedShape | ShapeStyleTarget::NewShape => {
                self.kind.label().to_owned()
            }
            ShapeStyleTarget::SelectedShapes => format!("図形 {}個", self.shape_count),
        }
    }

    fn fill_scope_note(self) -> Option<&'static str> {
        (self.supports_fill() && self.fill_supported_count < self.shape_count)
            .then_some("直線には塗りを適用せず、四角形 / 楕円だけに反映します。")
    }

    fn reflection_chip_label(self) -> Option<String> {
        match self.target {
            ShapeStyleTarget::SelectedShape => Some("選択中へ反映".to_owned()),
            ShapeStyleTarget::SelectedShapes => {
                Some(if self.total_selection_count == self.shape_count {
                    format!("図形{}個へ反映", self.shape_count)
                } else {
                    format!(
                        "選択{}個中 図形{}個へ反映",
                        self.total_selection_count, self.shape_count
                    )
                })
            }
            ShapeStyleTarget::NewShape => None,
        }
    }

    fn stroke_summary_label(self) -> String {
        format!(
            "線 {}% / {:.1}px",
            alpha_percent(self.stroke_color),
            self.width
        )
    }

    fn fill_summary_label(self) -> String {
        if !self.supports_fill() || !self.has_any_fill() {
            "塗りなし".to_owned()
        } else if self.fill_enabled() {
            self.fill_color
                .map(|fill| format!("塗り {}%", alpha_percent(fill)))
                .unwrap_or_else(|| "塗りあり".to_owned())
        } else {
            "一部に塗り".to_owned()
        }
    }

    fn edit_summary(self) -> String {
        match self.target {
            ShapeStyleTarget::SelectedShape => "選択中の図形へすぐ反映されます。".to_owned(),
            ShapeStyleTarget::SelectedShapes => {
                if self.total_selection_count == self.shape_count {
                    format!("選択中の図形{}個へまとめて反映されます。", self.shape_count)
                } else {
                    format!(
                        "選択中{}個のうち図形{}個へまとめて反映されます。",
                        self.total_selection_count, self.shape_count
                    )
                }
            }
            ShapeStyleTarget::NewShape => "次に描く図形へ反映されます。".to_owned(),
        }
    }

    fn paint_mode_label(self) -> &'static str {
        if !self.supports_fill() || !self.has_any_fill() {
            "線だけ"
        } else if self.fill_enabled() {
            "線と塗り"
        } else {
            "一部に塗り"
        }
    }
}

#[derive(Clone, Copy)]
struct SelectionPaintContext {
    stroke_count: usize,
    fill_count: usize,
    total_selection_count: usize,
    stroke_color: Option<RgbaColor>,
    stroke_width: Option<f32>,
    fill_color: Option<RgbaColor>,
}

impl SelectionPaintContext {
    fn has_strokes(self) -> bool {
        self.stroke_count > 0
    }

    fn has_fills(self) -> bool {
        self.fill_count > 0
    }

    fn selection_label(self) -> String {
        match (self.stroke_count, self.fill_count) {
            (stroke_count, 0) => format!("線 {stroke_count}個"),
            (0, fill_count) => format!("塗り {fill_count}個"),
            (stroke_count, fill_count) => format!("線 {stroke_count}個 / 塗り {fill_count}個"),
        }
    }

    fn reflection_chip_label(self) -> String {
        if self.total_selection_count == self.stroke_count + self.fill_count {
            self.selection_label()
        } else {
            format!("選択{}個へ反映", self.total_selection_count)
        }
    }

    fn edit_summary(self) -> &'static str {
        match (self.has_strokes(), self.has_fills()) {
            (true, true) => "選択中の線と塗りへすぐ反映されます。",
            (true, false) => "選択中の線へすぐ反映されます。",
            (false, true) => "選択中の塗りへすぐ反映されます。",
            (false, false) => "選択中の要素へすぐ反映されます。",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SelectionArrangeContext {
    selection_count: usize,
    can_reorder: bool,
    can_align: bool,
    can_distribute: bool,
}

impl SelectionArrangeContext {
    fn from_state(selection_count: usize, has_canvas_interaction: bool) -> Self {
        Self {
            selection_count,
            can_reorder: !has_canvas_interaction && selection_count >= 1,
            can_align: !has_canvas_interaction && selection_count >= 2,
            can_distribute: !has_canvas_interaction && selection_count >= 3,
        }
    }

    fn show_panel(self) -> bool {
        self.can_align
    }

    fn summary_chip_label(self, compact: bool) -> Option<String> {
        if !self.show_panel() {
            None
        } else if self.can_distribute {
            Some(if compact {
                "整列・等間隔".to_owned()
            } else {
                "整列・等間隔・順序".to_owned()
            })
        } else if compact {
            Some("整列・順序".to_owned())
        } else {
            Some("整列・重なり順".to_owned())
        }
    }

    fn panel_status_label(self) -> String {
        if self.can_distribute {
            format!("{}個選択中 · 整列 / 等間隔", self.selection_count)
        } else {
            format!("{}個選択中 · 整列 / 順序", self.selection_count)
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
                "まずは左のペンか図形ツールで 1 つ描いてみましょう。困ったらヘルプを開けます。",
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
            app.status_message = StatusMessage::info(
                "最初は「描く → 選ぶ → 保存」の流れをミニチュートリアルで確認できます。",
            );
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
        let fill_color = if self.active_tool == CanvasToolKind::Bucket {
            Some(self.tool_colors.fill_color)
        } else {
            self.tool_colors
                .fill_enabled
                .then_some(self.tool_colors.fill_color)
        };
        ToolSettings {
            tool: self.active_tool,
            stroke_color: self.tool_colors.stroke_color,
            fill_color,
            width: self.active_tool_width(),
            fill_tolerance: self.ui_state.bucket_fill_tolerance,
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
            CanvasToolKind::Rectangle
                | CanvasToolKind::Ellipse
                | CanvasToolKind::Eyedropper
                | CanvasToolKind::Bucket
        )
    }

    fn selected_shape_edit_target(&self) -> Option<(usize, ShapeElement)> {
        if self.active_tool != CanvasToolKind::Select {
            return None;
        }

        let selected_shapes = self.selected_shape_edit_targets();
        let [(index, shape)] = selected_shapes.as_slice() else {
            return None;
        };

        Some((*index, *shape))
    }

    fn selected_shape_edit_targets(&self) -> Vec<(usize, ShapeElement)> {
        if self.active_tool != CanvasToolKind::Select {
            return Vec::new();
        }

        self.canvas
            .selection_indices()
            .iter()
            .filter_map(|index| match self.document().element(*index).cloned()? {
                PaintElement::Shape(shape) => Some((*index, shape)),
                _ => None,
            })
            .collect()
    }

    fn selected_stroke_edit_targets(&self) -> Vec<(usize, Stroke)> {
        if self.active_tool != CanvasToolKind::Select {
            return Vec::new();
        }

        self.canvas
            .selection_indices()
            .iter()
            .filter_map(|index| match self.document().element(*index).cloned()? {
                PaintElement::Stroke(stroke) => Some((*index, stroke)),
                _ => None,
            })
            .collect()
    }

    fn selected_fill_edit_targets(&self) -> Vec<(usize, FillElement)> {
        if self.active_tool != CanvasToolKind::Select {
            return Vec::new();
        }

        self.canvas
            .selection_indices()
            .iter()
            .filter_map(|index| match self.document().element(*index).cloned()? {
                PaintElement::Fill(fill) => Some((*index, fill)),
                _ => None,
            })
            .collect()
    }

    fn current_shape_style_context(&self) -> Option<ShapeStyleContext> {
        let selected_shapes = self.selected_shape_edit_targets();
        let total_selection_count = self.canvas.selection_count();

        if let [(_, shape)] = selected_shapes.as_slice()
            && total_selection_count == 1
        {
            return Some(ShapeStyleContext {
                target: ShapeStyleTarget::SelectedShape,
                kind: shape.kind,
                stroke_color: shape.color,
                fill_color: shape.effective_fill_color(),
                width: shape.width,
                shape_count: 1,
                total_selection_count: 1,
                fill_supported_count: usize::from(shape.kind.supports_fill()),
                fill_enabled_count: usize::from(shape.effective_fill_color().is_some()),
            });
        }

        if let Some((_, representative)) = selected_shapes.first().copied() {
            let fill_supported_count = selected_shapes
                .iter()
                .filter(|(_, shape)| shape.kind.supports_fill())
                .count();
            let fill_enabled_count = selected_shapes
                .iter()
                .filter(|(_, shape)| shape.effective_fill_color().is_some())
                .count();
            return Some(ShapeStyleContext {
                target: ShapeStyleTarget::SelectedShapes,
                kind: representative.kind,
                stroke_color: representative.color,
                fill_color: representative.effective_fill_color(),
                width: representative.width,
                shape_count: selected_shapes.len(),
                total_selection_count,
                fill_supported_count,
                fill_enabled_count,
            });
        }

        let kind = self.active_tool.shape_kind()?;
        Some(ShapeStyleContext {
            target: ShapeStyleTarget::NewShape,
            kind,
            stroke_color: self.tool_colors.stroke_color,
            fill_color: self
                .tool_colors
                .fill_enabled
                .then_some(self.tool_colors.fill_color),
            width: self.tool_widths.draw_width,
            shape_count: 1,
            total_selection_count: 0,
            fill_supported_count: usize::from(kind.supports_fill()),
            fill_enabled_count: usize::from(kind.supports_fill() && self.tool_colors.fill_enabled),
        })
    }

    fn current_selection_paint_context(&self) -> Option<SelectionPaintContext> {
        if self.active_tool != CanvasToolKind::Select {
            return None;
        }

        let selected_shapes = self.selected_shape_edit_targets();
        if !selected_shapes.is_empty() {
            return None;
        }

        let selected_strokes = self.selected_stroke_edit_targets();
        let selected_fills = self.selected_fill_edit_targets();
        let total_selection_count = self.canvas.selection_count();
        if total_selection_count == 0
            || selected_strokes.len() + selected_fills.len() != total_selection_count
        {
            return None;
        }

        Some(SelectionPaintContext {
            stroke_count: selected_strokes.len(),
            fill_count: selected_fills.len(),
            total_selection_count,
            stroke_color: selected_strokes.first().map(|(_, stroke)| stroke.color),
            stroke_width: selected_strokes.first().map(|(_, stroke)| stroke.width),
            fill_color: selected_fills.first().map(|(_, fill)| fill.color),
        })
    }

    fn replace_selected_shape(
        &mut self,
        update: impl FnOnce(ShapeElement) -> ShapeElement,
        message: impl Into<String>,
    ) -> bool {
        let message = message.into();
        let Some((index, shape)) = self.selected_shape_edit_target() else {
            return false;
        };

        let next_shape = update(shape);
        if next_shape == shape {
            return false;
        }

        let mut document = self.document().clone();
        if !document.replace_element(index, PaintElement::Shape(next_shape)) {
            return false;
        }

        let active_layer_id = document.active_layer_id();
        if self.history.replace_document(document) {
            self.canvas
                .set_selection_indices(active_layer_id, vec![index]);
            self.set_info(message);
            true
        } else {
            false
        }
    }

    fn replace_selected_shapes(
        &mut self,
        update: impl Fn(ShapeElement) -> ShapeElement,
        message: impl Into<String>,
    ) -> usize {
        let message = message.into();
        let targets = self.selected_shape_edit_targets();
        if targets.is_empty() {
            return 0;
        }

        let replacements: Vec<_> = targets
            .iter()
            .filter_map(|(index, shape)| {
                let next_shape = update(*shape);
                (next_shape != *shape).then_some((*index, PaintElement::Shape(next_shape)))
            })
            .collect();
        if replacements.is_empty() {
            return 0;
        }

        let mut document = self.document().clone();
        let active_layer_id = document.active_layer_id();
        let selection_indices = self.canvas.selection_indices().to_vec();
        if !document.replace_elements(&replacements) {
            return 0;
        }

        if self.history.replace_document(document) {
            self.canvas
                .set_selection_indices(active_layer_id, selection_indices);
            self.set_info(message);
            replacements.len()
        } else {
            0
        }
    }

    fn replace_selected_strokes(
        &mut self,
        update: impl Fn(Stroke) -> Stroke,
        message: impl Into<String>,
    ) -> usize {
        let message = message.into();
        let targets = self.selected_stroke_edit_targets();
        if targets.is_empty() {
            return 0;
        }

        let replacements: Vec<_> = targets
            .iter()
            .filter_map(|(index, stroke)| {
                let next_stroke = update(stroke.clone());
                (next_stroke != *stroke).then_some((*index, PaintElement::Stroke(next_stroke)))
            })
            .collect();
        if replacements.is_empty() {
            return 0;
        }

        let mut document = self.document().clone();
        let active_layer_id = document.active_layer_id();
        let selection_indices = self.canvas.selection_indices().to_vec();
        if !document.replace_elements(&replacements) {
            return 0;
        }

        if self.history.replace_document(document) {
            self.canvas
                .set_selection_indices(active_layer_id, selection_indices);
            self.set_info(message);
            replacements.len()
        } else {
            0
        }
    }

    fn replace_selected_fills(
        &mut self,
        update: impl Fn(FillElement) -> FillElement,
        message: impl Into<String>,
    ) -> usize {
        let message = message.into();
        let targets = self.selected_fill_edit_targets();
        if targets.is_empty() {
            return 0;
        }

        let replacements: Vec<_> = targets
            .iter()
            .filter_map(|(index, fill)| {
                let next_fill = update(fill.clone());
                (next_fill != *fill).then_some((*index, PaintElement::Fill(next_fill)))
            })
            .collect();
        if replacements.is_empty() {
            return 0;
        }

        let mut document = self.document().clone();
        let active_layer_id = document.active_layer_id();
        let selection_indices = self.canvas.selection_indices().to_vec();
        if !document.replace_elements(&replacements) {
            return 0;
        }

        if self.history.replace_document(document) {
            self.canvas
                .set_selection_indices(active_layer_id, selection_indices);
            self.set_info(message);
            replacements.len()
        } else {
            0
        }
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
        let announce = announce.into();
        let mut applied_message = None;
        match self.tool_colors.quick_color_target {
            ColorTarget::Stroke => {
                self.tool_colors.stroke_color = color;
                let selected_shapes = self.selected_shape_edit_targets();
                if selected_shapes.len() > 1 {
                    let changed = self.replace_selected_shapes(
                        |selected| ShapeElement { color, ..selected },
                        format!(
                            "選択中の図形{}個の線色を変更しました。",
                            selected_shapes.len()
                        ),
                    );
                    if changed > 0 {
                        applied_message =
                            Some(format!("選択中の図形{}個にも反映しました。", changed));
                    }
                } else if let Some((_, shape)) = selected_shapes.first().copied()
                    && self.replace_selected_shape(
                        |selected| ShapeElement { color, ..selected },
                        format!("選択中の{}の線色を変更しました。", shape.kind.label()),
                    )
                {
                    applied_message =
                        Some(format!("選択中の{}にも反映しました。", shape.kind.label()));
                } else {
                    let selected_strokes = self.selected_stroke_edit_targets();
                    if !selected_strokes.is_empty() {
                        let changed = self.replace_selected_strokes(
                            |mut selected| {
                                selected.color = color;
                                selected
                            },
                            format!("選択中の線{}個の色を変更しました。", selected_strokes.len()),
                        );
                        if changed > 0 {
                            applied_message =
                                Some(format!("選択中の線{}個にも反映しました。", changed));
                        }
                    }
                }
            }
            ColorTarget::Fill => {
                self.tool_colors.fill_color = color;
                self.tool_colors.fill_enabled = true;
                let selected_shapes = self.selected_shape_edit_targets();
                let fill_target_count = selected_shapes
                    .iter()
                    .filter(|(_, shape)| shape.kind.supports_fill())
                    .count();
                if selected_shapes.len() > 1 && fill_target_count > 0 {
                    let changed = self.replace_selected_shapes(
                        |selected| selected.with_fill_color(Some(color)),
                        format!(
                            "選択中の図形{}個の塗り色を変更しました。",
                            fill_target_count
                        ),
                    );
                    if changed > 0 {
                        applied_message =
                            Some(format!("塗り対応の図形{}個にも反映しました。", changed));
                    }
                } else if let Some((_, shape)) = selected_shapes.first().copied()
                    && shape.kind.supports_fill()
                    && self.replace_selected_shape(
                        |selected| selected.with_fill_color(Some(color)),
                        format!("選択中の{}の塗り色を変更しました。", shape.kind.label()),
                    )
                {
                    applied_message =
                        Some(format!("選択中の{}にも反映しました。", shape.kind.label()));
                } else {
                    let selected_fills = self.selected_fill_edit_targets();
                    if !selected_fills.is_empty() {
                        let changed = self.replace_selected_fills(
                            |mut selected| {
                                selected.color = color;
                                selected
                            },
                            format!("選択中の塗り{}個の色を変更しました。", selected_fills.len()),
                        );
                        if changed > 0 {
                            applied_message =
                                Some(format!("選択中の塗り{}個にも反映しました。", changed));
                        }
                    }
                }
            }
        }
        self.push_recent_color(color);
        if let Some(applied_message) = applied_message {
            self.set_info(format!("{announce} {applied_message}"));
        } else {
            self.set_info(announce);
        }
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

    fn set_bucket_fill_tolerance(&mut self, tolerance: FillTolerancePreset) {
        if self.ui_state.bucket_fill_tolerance == tolerance {
            return;
        }

        self.ui_state.bucket_fill_tolerance = tolerance;
        self.ui_state_dirty = true;
        self.set_info(format!(
            "塗りのゆるさを「{}」にしました。",
            tolerance.label()
        ));
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
        ui.small("続きは JSON保存 · 共有は PNG / 透過PNG · 再利用は SVG");
    }

    fn show_tools(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 8.0;
        let shape_context = self.current_shape_style_context();
        let selection_paint_context = self.current_selection_paint_context();
        let arrange_context = SelectionArrangeContext::from_state(
            self.canvas.selection_count(),
            self.canvas.has_active_interaction(),
        );

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
        ui.small("上から順に、選ぶ / 描く / 色 / 図形のまとまりで並んでいます。下へスクロールすると詳細設定も開けます。");
        ui.add_space(8.0);

        ui.group(|ui| {
            ui.label(RichText::new("今の状態").strong());
            if self.document().total_element_count() == 0 {
                ui.horizontal_wrapped(|ui| {
                    summary_chip(ui, "まずは 1 つ描く", true);
                    summary_chip(ui, "選択で動かす", false);
                    summary_chip(ui, "続きは JSON保存", false);
                });
                ui.small("左のペン・えんぴつ・クレヨン・マーカーか図形ツールを選んで、中央でドラッグすると始めやすいです。");
                if !self.tutorial.visible
                    && ui.button("ミニチュートリアルを開く").clicked()
                {
                    self.open_tutorial();
                }
                ui.add_space(6.0);
            }
            ui.small(format!("道具: {}", self.active_tool.label()));
            if matches!(
                self.active_tool,
                CanvasToolKind::Brush
                    | CanvasToolKind::Pencil
                    | CanvasToolKind::Crayon
                    | CanvasToolKind::Marker
            ) {
                ui.small(format!("描き味: {}", brush_kind_summary(self.active_tool)));
            } else if self.active_tool == CanvasToolKind::Bucket {
                ui.small("塗り色で閉じた領域を塗ります。");
                ui.small(format!(
                    "塗りのゆるさ: {}",
                    self.ui_state.bucket_fill_tolerance.label()
                ));
            }
            if let Some(active_layer) = self.document().active_layer() {
                ui.small(format!("レイヤー: {}", active_layer.name));
            }
            if let Some(operation) = self.canvas.current_operation_summary(self.document()) {
                ui.small(format!("操作: {operation}"));
            }
            ui.small(self.canvas.selection_summary(self.document()));
            if let Some(selection_context) = self.selection_layer_context() {
                ui.small(selection_context);
            }
            if arrange_context.show_panel() {
                ui.small(arrange_context.panel_status_label());
            }
            if let Some(shape_context) = shape_context {
                if shape_context.is_selection_target() {
                    ui.small(format!("選択図形: {}", shape_context.kind_summary_label()));
                } else {
                    ui.small(format!("次の図形: {}", shape_context.kind_summary_label()));
                }
                ui.small(format!("見え方: {}", shape_context.paint_mode_label()));
                if let Some(fill_scope_note) = shape_context.fill_scope_note() {
                    ui.small(fill_scope_note);
                }
            } else if let Some(paint_context) = selection_paint_context {
                ui.small(format!("選択描画: {}", paint_context.selection_label()));
            }
            ui.horizontal_wrapped(|ui| {
                let show_fill_swatch = shape_context
                    .map(|context| context.supports_fill())
                    .unwrap_or_else(|| {
                        selection_paint_context
                            .map(|context| context.has_fills())
                            .unwrap_or(self.tool_colors.fill_enabled || self.tool_uses_fill())
                    });
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
                if show_fill_swatch {
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

        let tool_columns = tablet_button_columns(ui.available_width());
        let tool_button_width = if tool_columns == 2 {
            ((ui.available_width() - 8.0) / 2.0).max(104.0)
        } else {
            ui.available_width()
        };
        let render_tool_group =
            |ui: &mut egui::Ui, title: &str, tools: &[CanvasToolKind], app: &mut PaintApp| {
                ui.label(RichText::new(title).strong());
                for row in tools.chunks(tool_columns) {
                    ui.horizontal_wrapped(|ui| {
                        for tool in row {
                            let is_selected = app.active_tool == *tool;
                            let response = ui
                                .add_sized(
                                    [tool_button_width, TOOL_BUTTON_HEIGHT],
                                    egui::Button::new(tool.label()).selected(is_selected),
                                )
                                .on_hover_text(tool_button_tooltip(*tool));
                            let can_activate = !is_selected
                                || (*tool == CanvasToolKind::Select && app.multi_select_mode);
                            if response.clicked() && can_activate {
                                app.apply_tool_button_selection(*tool);
                            }
                        }
                    });
                    ui.add_space(4.0);
                }
                ui.add_space(4.0);
            };
        render_tool_group(
            ui,
            "選ぶ / 動かす",
            &[CanvasToolKind::Select, CanvasToolKind::Pan],
            self,
        );
        render_tool_group(
            ui,
            "描く",
            &[
                CanvasToolKind::Brush,
                CanvasToolKind::Pencil,
                CanvasToolKind::Crayon,
                CanvasToolKind::Marker,
                CanvasToolKind::Eraser,
            ],
            self,
        );
        render_tool_group(
            ui,
            "色 / 塗り",
            &[CanvasToolKind::Eyedropper, CanvasToolKind::Bucket],
            self,
        );
        render_tool_group(
            ui,
            "図形",
            &[
                CanvasToolKind::Rectangle,
                CanvasToolKind::Ellipse,
                CanvasToolKind::Line,
            ],
            self,
        );

        ui.add_space(8.0);
        if matches!(
            self.active_tool,
            CanvasToolKind::Select | CanvasToolKind::Pan
        ) {
            ui.small("選択や手のひらは、上の切り替えと合わせるとタブレットで使いやすくなります。");
        }
        ui.add_space(6.0);

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
        ui.horizontal_wrapped(|ui| {
            layer_status_chip(
                ui,
                if self.multi_select_mode {
                    "複数選択オン"
                } else {
                    "複数選択オフ"
                },
                self.multi_select_mode,
            );
            layer_status_chip(
                ui,
                if self.finger_draw_enabled {
                    "指描きオン"
                } else {
                    "指描きオフ"
                },
                self.finger_draw_enabled,
            );
            layer_status_chip(
                ui,
                if self.active_tool == CanvasToolKind::Pan {
                    "手のひら"
                } else {
                    "通常操作"
                },
                self.active_tool == CanvasToolKind::Pan,
            );
        });
        let tablet_columns = tablet_button_columns(ui.available_width());
        let tablet_button_width = if tablet_columns == 2 {
            ((ui.available_width() - 8.0) / 2.0).max(110.0)
        } else {
            ui.available_width()
        };
        let mut tablet_action = None;
        ui.horizontal_wrapped(|ui| {
            let multi_select_response = ui
                .add_sized(
                    [tablet_button_width, TOOL_BUTTON_HEIGHT],
                    egui::Button::new("複数選択モード").selected(self.multi_select_mode),
                )
                .on_hover_text("タップ / クリックで追加選択や解除をします。");
            if multi_select_response.clicked() {
                tablet_action = Some("toggle-multi");
            }
            let finger_draw_response = ui
                .add_sized(
                    [tablet_button_width, TOOL_BUTTON_HEIGHT],
                    egui::Button::new("指でも描く").selected(self.finger_draw_enabled),
                )
                .on_hover_text("指でもそのまま描画できるようにします。");
            if finger_draw_response.clicked() {
                tablet_action = Some("toggle-finger-draw");
            }
        });
        match tablet_action {
            Some("toggle-multi") => self.set_multi_select_mode(!self.multi_select_mode),
            Some("toggle-finger-draw") => self.set_finger_draw_enabled(!self.finger_draw_enabled),
            _ => {}
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
        ui.add_space(12.0);
        if arrange_context.show_panel() {
            self.show_selection_actions(ui, arrange_context);
            ui.add_space(12.0);
        }

        ui.label(RichText::new("描画ツール設定").strong());
        match self.active_tool {
            CanvasToolKind::Brush
            | CanvasToolKind::Pencil
            | CanvasToolKind::Crayon
            | CanvasToolKind::Marker
            | CanvasToolKind::Eyedropper
            | CanvasToolKind::Bucket
            | CanvasToolKind::Rectangle
            | CanvasToolKind::Ellipse
            | CanvasToolKind::Line => {
                if self.active_tool == CanvasToolKind::Bucket {
                    ui.small(format!(
                        "今のツールは塗り色で閉じた領域を塗ります。塗りのゆるさは「{}」です。",
                        self.ui_state.bucket_fill_tolerance.label()
                    ));
                } else {
                    ui.small(format!(
                        "今のツールは描く太さ {:.1}px を使います。",
                        self.tool_widths.draw_width
                    ));
                }
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

        if self.active_tool == CanvasToolKind::Bucket {
            ui.add_space(6.0);
            ui.group(|ui| {
                ui.label(RichText::new("バケツ塗り設定").strong());
                ui.small("少し色が違う場所まで、どのくらいまとめて塗るかを切り替えます。");
                let columns = if ui.available_width() < 340.0 {
                    2.0
                } else {
                    3.0
                };
                let preset_width =
                    ((ui.available_width() - (columns - 1.0) * 8.0) / columns).max(92.0);
                ui.horizontal_wrapped(|ui| {
                    for preset in FillTolerancePreset::ALL {
                        let response = ui
                            .add_sized(
                                [preset_width, 36.0],
                                egui::Button::new(preset.label())
                                    .selected(self.ui_state.bucket_fill_tolerance == preset),
                            )
                            .on_hover_text(preset.description());
                        if response.clicked() {
                            self.set_bucket_fill_tolerance(preset);
                        }
                    }
                });
                ui.small(self.ui_state.bucket_fill_tolerance.description());
                ui.small("判定は見えている全レイヤーを使い、塗り結果は現在のレイヤーへ入ります。");
                ui.small("迷ったら「ふつう」か「広め」から試すと自然です。");
            });
        }

        if let Some(shape_context) = shape_context {
            ui.add_space(6.0);
            ui.group(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("図形スタイル").strong());
                    if shape_context.is_selection_target() {
                        layer_status_chip(ui, &shape_context.selection_label(), true);
                    } else {
                        layer_status_chip(ui, "次に描く図形", false);
                    }
                    summary_chip(ui, shape_context.kind_summary_label(), true);
                    summary_chip(
                        ui,
                        shape_context.paint_mode_label(),
                        shape_context.has_any_fill(),
                    );
                    if let Some(reflection_label) = shape_context.reflection_chip_label() {
                        layer_status_chip(ui, &reflection_label, true);
                    }
                });
                ui.small(shape_context.edit_summary());
                if let Some(fill_scope_note) = shape_context.fill_scope_note() {
                    ui.small(fill_scope_note);
                } else if shape_context.is_selected_shapes() {
                    ui.small("値が違う図形も、ここでまとめて上書きできます。");
                }
                ui.horizontal_wrapped(|ui| {
                    summary_chip(ui, shape_context.stroke_summary_label(), false);
                    summary_chip(
                        ui,
                        shape_context.fill_summary_label(),
                        shape_context.has_any_fill(),
                    );
                    if !shape_context.supports_fill() {
                        summary_chip(ui, "直線は線のみ", false);
                    }
                });
                if shape_context.supports_fill() {
                    let mode_button_width = ((ui.available_width() - 8.0) / 2.0).max(96.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label("見え方");
                        if ui
                            .add_sized(
                                [mode_button_width, 34.0],
                                egui::Button::new("線だけ").selected(!shape_context.has_any_fill()),
                            )
                            .on_hover_text("塗りなしで、線だけの図形にします。")
                            .clicked()
                        {
                            self.tool_colors.fill_enabled = false;
                            if shape_context.is_selection_target() {
                                let changed = self.replace_selected_shapes(
                                    |shape| shape.with_fill_color(None),
                                    if shape_context.is_selected_shape() {
                                        format!(
                                            "選択中の{}を線だけにしました。",
                                            shape_context.kind.label()
                                        )
                                    } else {
                                        format!(
                                            "選択中の図形{}個を線だけにしました。",
                                            shape_context.shape_count
                                        )
                                    },
                                );
                                if changed == 0 {
                                    self.set_info(if shape_context.is_selected_shape() {
                                        format!("{}を線だけにします。", shape_context.kind.label())
                                    } else {
                                        format!(
                                            "選択中の図形{}個を線だけにします。",
                                            shape_context.shape_count
                                        )
                                    });
                                }
                            } else {
                                self.set_info(format!(
                                    "次に描く{}を線だけにします。",
                                    shape_context.kind.label()
                                ));
                            }
                        }
                        if ui
                            .add_sized(
                                [mode_button_width, 34.0],
                                egui::Button::new("線と塗り")
                                    .selected(shape_context.fill_enabled()),
                            )
                            .on_hover_text("線と塗りの両方を使う図形にします。")
                            .clicked()
                        {
                            let fill_color = shape_context
                                .fill_color
                                .unwrap_or(self.tool_colors.fill_color);
                            self.tool_colors.fill_enabled = true;
                            self.tool_colors.fill_color = fill_color;
                            if shape_context.is_selection_target() {
                                let changed = self.replace_selected_shapes(
                                    |shape| shape.with_fill_color(Some(fill_color)),
                                    if shape_context.is_selected_shape() {
                                        format!(
                                            "選択中の{}を線と塗りにしました。",
                                            shape_context.kind.label()
                                        )
                                    } else {
                                        format!(
                                            "選択中の図形{}個を線と塗りにしました。",
                                            shape_context.fill_supported_count
                                        )
                                    },
                                );
                                if changed == 0 {
                                    self.set_info(if shape_context.is_selected_shape() {
                                        format!(
                                            "{}を線と塗りにします。",
                                            shape_context.kind.label()
                                        )
                                    } else {
                                        format!(
                                            "選択中の図形{}個を線と塗りにします。",
                                            shape_context.fill_supported_count
                                        )
                                    });
                                }
                            } else {
                                self.set_info(format!(
                                    "次に描く{}を線と塗りにします。",
                                    shape_context.kind.label()
                                ));
                            }
                        }
                    });
                } else {
                    ui.small("直線は線だけです。塗り設定は使いません。");
                }

                ui.add_space(4.0);
                ui.label("線幅");
                let mut shape_width = shape_context.width;
                if ui
                    .add(egui::Slider::new(
                        &mut shape_width,
                        MIN_BRUSH_WIDTH..=MAX_BRUSH_WIDTH,
                    ))
                    .on_hover_text("選択中の図形、または次に描く図形の線幅を変えます。")
                    .changed()
                {
                    self.tool_widths.draw_width = shape_width;
                    if shape_context.is_selection_target() {
                        let changed = self.replace_selected_shapes(
                            |shape| ShapeElement {
                                width: shape_width,
                                ..shape
                            },
                            if shape_context.is_selected_shape() {
                                format!(
                                    "{}の線幅を {:.1}px に変更しました。",
                                    shape_context.kind.label(),
                                    shape_width
                                )
                            } else {
                                format!(
                                    "選択中の図形{}個の線幅を {:.1}px に変更しました。",
                                    shape_context.shape_count, shape_width
                                )
                            },
                        );
                        if changed == 0 {
                            self.set_info(if shape_context.is_selected_shape() {
                                format!(
                                    "{}の線幅を {:.1}px にします。",
                                    shape_context.kind.label(),
                                    shape_width
                                )
                            } else {
                                format!(
                                    "選択中の図形{}個の線幅を {:.1}px にします。",
                                    shape_context.shape_count, shape_width
                                )
                            });
                        }
                    } else {
                        self.set_info(format!(
                            "次に描く{}の線幅を {:.1}px に変更しました。",
                            shape_context.kind.label(),
                            shape_width
                        ));
                    }
                }
            });
        }

        if shape_context.is_none()
            && let Some(paint_context) = selection_paint_context
        {
            ui.add_space(6.0);
            ui.group(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("描画スタイル").strong());
                    layer_status_chip(ui, "選択中", true);
                    summary_chip(ui, paint_context.selection_label(), true);
                    layer_status_chip(ui, &paint_context.reflection_chip_label(), true);
                });
                ui.small(paint_context.edit_summary());
                if paint_context.has_strokes() && paint_context.has_fills() {
                    ui.small("線色は線へ、塗り色は塗りへまとまって反映されます。");
                } else if paint_context.has_strokes() {
                    ui.small("色、不透明度、線幅をまとめて直せます。");
                } else if paint_context.has_fills() {
                    ui.small("色と不透明度をまとめて直せます。");
                }

                if let Some(stroke_width) = paint_context.stroke_width {
                    ui.add_space(4.0);
                    ui.label("線幅");
                    let mut next_width = stroke_width;
                    if ui
                        .add(egui::Slider::new(
                            &mut next_width,
                            MIN_BRUSH_WIDTH..=MAX_BRUSH_WIDTH,
                        ))
                        .on_hover_text("選択中の線の太さをまとめて変えます。")
                        .changed()
                    {
                        self.tool_widths.draw_width = next_width;
                        self.replace_selected_strokes(
                            |mut stroke| {
                                stroke.width = next_width;
                                stroke
                            },
                            format!(
                                "選択中の線{}個の太さを {:.1}px に変更しました。",
                                paint_context.stroke_count, next_width
                            ),
                        );
                    }
                }
            });
        }

        ui.label(RichText::new("色と不透明度").strong());
        if let Some(shape_context) = shape_context {
            ui.small(shape_context.edit_summary());
        } else if let Some(paint_context) = selection_paint_context {
            ui.small(paint_context.edit_summary());
        } else if self.active_tool == CanvasToolKind::Bucket {
            ui.small("バケツ塗りは塗り色を使います。スポイトや色パレットで色を変え、塗りのゆるさで塗れ方を調整できます。");
        } else {
            ui.small("線色は描画と図形、塗り色は四角形・楕円・バケツ塗りに使います。");
        }

        let stroke_controls_available = selection_paint_context
            .map(|context| context.has_strokes())
            .unwrap_or(true);
        let fill_controls_available = shape_context
            .map(|context| context.supports_fill())
            .unwrap_or_else(|| {
                selection_paint_context
                    .map(|context| context.has_fills())
                    .unwrap_or(true)
            });
        if !stroke_controls_available && self.tool_colors.quick_color_target == ColorTarget::Stroke
        {
            self.tool_colors.quick_color_target = ColorTarget::Fill;
        }
        if !fill_controls_available && self.tool_colors.quick_color_target == ColorTarget::Fill {
            self.tool_colors.quick_color_target = ColorTarget::Stroke;
        }
        let shape_kind_label = shape_context.map(|context| context.kind.label().to_owned());
        let stroke_source = shape_context
            .map(|context| context.stroke_color)
            .or_else(|| selection_paint_context.and_then(|context| context.stroke_color))
            .unwrap_or(self.tool_colors.stroke_color);
        let fill_enabled_source = shape_context
            .map(|context| context.has_any_fill())
            .or_else(|| selection_paint_context.map(|context| context.has_fills()))
            .unwrap_or(self.tool_colors.fill_enabled);
        let fill_color_source = shape_context
            .and_then(|context| context.fill_color)
            .or_else(|| selection_paint_context.and_then(|context| context.fill_color))
            .unwrap_or(self.tool_colors.fill_color);

        ui.horizontal_wrapped(|ui| {
            ui.label("色の反映先");
            let target_count =
                usize::from(stroke_controls_available) + usize::from(fill_controls_available);
            let target_button_width = if target_count >= 2 {
                ((ui.available_width() - 12.0) / 2.0).max(90.0)
            } else {
                ui.available_width().clamp(90.0, 180.0)
            };
            if stroke_controls_available
                && ui
                    .add_sized(
                        [target_button_width, 30.0],
                        egui::Button::new("線色")
                            .selected(self.tool_colors.quick_color_target == ColorTarget::Stroke),
                    )
                    .on_hover_text("線色を編集対象にします。")
                    .clicked()
            {
                self.tool_colors.quick_color_target = ColorTarget::Stroke;
            }
            if fill_controls_available
                && ui
                    .add_sized(
                        [target_button_width, 30.0],
                        egui::Button::new("塗り色")
                            .selected(self.tool_colors.quick_color_target == ColorTarget::Fill),
                    )
                    .on_hover_text("塗り色を編集対象にします。")
                    .clicked()
            {
                self.tool_colors.quick_color_target = ColorTarget::Fill;
            } else {
                summary_chip(ui, "塗りなし", false);
            }
        });
        ui.horizontal_wrapped(|ui| {
            let stroke_selected = self.tool_colors.quick_color_target == ColorTarget::Stroke;
            let fill_selected = self.tool_colors.quick_color_target == ColorTarget::Fill;
            if stroke_controls_available {
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
            }

            if fill_controls_available {
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
            } else {
                ui.small("直線は線だけです");
            }
        });
        ui.small("スポイト、最近使った色、簡易パレットはここで選んだ色へ入ります。");

        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("線").strong());
                summary_chip(ui, format!("{}%", alpha_percent(stroke_source)), false);
                summary_chip(
                    ui,
                    if self.tool_colors.quick_color_target == ColorTarget::Stroke {
                        "編集中: 線色"
                    } else {
                        "線"
                    },
                    self.tool_colors.quick_color_target == ColorTarget::Stroke,
                );
                if let Some(shape_context) = shape_context
                    && let Some(reflection_label) = shape_context.reflection_chip_label()
                {
                    layer_status_chip(ui, &reflection_label, true);
                } else if let Some(paint_context) = selection_paint_context {
                    layer_status_chip(ui, &paint_context.reflection_chip_label(), true);
                } else if shape_context.is_some() {
                    layer_status_chip(ui, "次に描く図形へ反映", false);
                }
            });
            if stroke_controls_available {
                ui.label("線色");
                let mut stroke_color = color32_from_rgba(stroke_source);
                if ui.color_edit_button_srgba(&mut stroke_color).changed() {
                    let color = rgba_from_color32(stroke_color);
                    self.tool_colors.stroke_color = color;
                    self.push_recent_color(color);
                    if let Some(shape_context) = shape_context
                        && shape_context.is_selection_target()
                    {
                        self.replace_selected_shapes(
                            |shape| ShapeElement { color, ..shape },
                            if shape_context.is_selected_shape() {
                                format!(
                                    "{}の線色を変更しました。",
                                    shape_kind_label
                                        .as_deref()
                                        .unwrap_or(shape_context.kind.label())
                                )
                            } else {
                                format!(
                                    "選択中の図形{}個の線色を変更しました。",
                                    shape_context.shape_count
                                )
                            },
                        );
                    } else if let Some(paint_context) = selection_paint_context {
                        self.replace_selected_strokes(
                            |mut stroke| {
                                stroke.color = color;
                                stroke
                            },
                            format!(
                                "選択中の線{}個の色を変更しました。",
                                paint_context.stroke_count
                            ),
                        );
                    } else {
                        self.set_info("線色を変更しました。");
                    }
                }
                let mut stroke_opacity = alpha_percent(stroke_source);
                if ui
                    .add(egui::Slider::new(&mut stroke_opacity, 0..=100).suffix("%"))
                    .on_hover_text("描画ツールや図形の線の不透明度を変えます。")
                    .changed()
                {
                    let next_stroke_color = set_alpha_percent(stroke_source, stroke_opacity);
                    self.tool_colors.stroke_color = next_stroke_color;
                    self.push_recent_color(next_stroke_color);
                    if let Some(shape_context) = shape_context
                        && shape_context.is_selection_target()
                    {
                        self.replace_selected_shapes(
                            |shape| ShapeElement {
                                color: next_stroke_color,
                                ..shape
                            },
                            if shape_context.is_selected_shape() {
                                format!(
                                    "{}の線の不透明度を {stroke_opacity}% に変更しました。",
                                    shape_kind_label
                                        .as_deref()
                                        .unwrap_or(shape_context.kind.label())
                                )
                            } else {
                                format!(
                                    "選択中の図形{}個の線の不透明度を {stroke_opacity}% に変更しました。",
                                    shape_context.shape_count
                                )
                            },
                        );
                    } else if let Some(paint_context) = selection_paint_context {
                        self.replace_selected_strokes(
                            |mut stroke| {
                                stroke.color = set_alpha_percent(stroke.color, stroke_opacity);
                                stroke
                            },
                            format!(
                                "選択中の線{}個の不透明度を {stroke_opacity}% に変更しました。",
                                paint_context.stroke_count
                            ),
                        );
                    } else {
                        self.set_info(format!(
                            "線の不透明度を {}% に変更しました。",
                            stroke_opacity
                        ));
                    }
                }
            } else {
                ui.small("選択中の要素に線はありません。");
            }
        });

        if fill_controls_available {
            ui.group(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("塗り").strong());
                    summary_chip(
                        ui,
                        shape_context
                            .map(ShapeStyleContext::fill_summary_label)
                            .unwrap_or_else(|| {
                                if fill_enabled_source {
                                    format!("塗り {}%", alpha_percent(fill_color_source))
                                } else {
                                    "塗りなし".to_owned()
                                }
                            }),
                        fill_enabled_source,
                    );
                    summary_chip(
                        ui,
                        if self.tool_colors.quick_color_target == ColorTarget::Fill {
                            "編集中: 塗り色"
                        } else {
                            "塗り"
                        },
                        self.tool_colors.quick_color_target == ColorTarget::Fill,
                    );
                    if let Some(shape_context) = shape_context
                        && let Some(reflection_label) = shape_context.reflection_chip_label()
                    {
                        layer_status_chip(ui, &reflection_label, true);
                    } else if let Some(paint_context) = selection_paint_context {
                        layer_status_chip(ui, &paint_context.reflection_chip_label(), true);
                    } else if shape_context.is_some() {
                        layer_status_chip(ui, "次に描く図形へ反映", false);
                    }
                });
                if let Some(shape_context) = shape_context {
                    if let Some(fill_scope_note) = shape_context.fill_scope_note() {
                        ui.small(fill_scope_note);
                    }
                    let mut fill_enabled = fill_enabled_source;
                    if ui
                        .checkbox(&mut fill_enabled, "塗りを使う")
                        .on_hover_text("四角形や楕円の内側を塗ります。")
                        .changed()
                    {
                        self.tool_colors.fill_enabled = fill_enabled;
                        if fill_enabled {
                            self.tool_colors.fill_color = fill_color_source;
                        }
                        if shape_context.is_selection_target() {
                            self.replace_selected_shapes(
                                |shape| {
                                    shape.with_fill_color(if fill_enabled {
                                        Some(fill_color_source)
                                    } else {
                                        None
                                    })
                                },
                                if shape_context.is_selected_shape() {
                                    if fill_enabled {
                                        format!(
                                            "{}の塗りをオンにしました。",
                                            shape_kind_label
                                                .as_deref()
                                                .unwrap_or(shape_context.kind.label())
                                        )
                                    } else {
                                        format!(
                                            "{}の塗りをオフにしました。",
                                            shape_kind_label
                                                .as_deref()
                                                .unwrap_or(shape_context.kind.label())
                                        )
                                    }
                                } else if fill_enabled {
                                    format!(
                                        "選択中の図形{}個の塗りをオンにしました。",
                                        shape_context.fill_supported_count
                                    )
                                } else {
                                    format!(
                                        "選択中の図形{}個の塗りをオフにしました。",
                                        shape_context.fill_supported_count
                                    )
                                },
                            );
                        } else {
                            self.set_info(if fill_enabled {
                                "図形の塗りをオンにしました。".to_owned()
                            } else {
                                "図形の塗りをオフにしました。".to_owned()
                            });
                        }
                    }
                    ui.add_enabled_ui(fill_enabled, |ui| {
                        ui.label("塗り色");
                        let mut fill_color = color32_from_rgba(fill_color_source);
                        if ui.color_edit_button_srgba(&mut fill_color).changed() {
                            let color = rgba_from_color32(fill_color);
                            self.tool_colors.fill_color = color;
                            self.tool_colors.fill_enabled = true;
                            self.push_recent_color(color);
                            if shape_context.is_selection_target() {
                                self.replace_selected_shapes(
                                    |shape| shape.with_fill_color(Some(color)),
                                    if shape_context.is_selected_shape() {
                                        format!(
                                            "{}の塗り色を変更しました。",
                                            shape_kind_label
                                                .as_deref()
                                                .unwrap_or(shape_context.kind.label())
                                        )
                                    } else {
                                        format!(
                                            "選択中の図形{}個の塗り色を変更しました。",
                                            shape_context.fill_supported_count
                                        )
                                    },
                                );
                            } else {
                                self.set_info("塗り色を変更しました。");
                            }
                        }
                        let mut fill_opacity = alpha_percent(fill_color_source);
                        if ui
                            .add(egui::Slider::new(&mut fill_opacity, 0..=100).suffix("%"))
                            .on_hover_text("四角形や楕円の塗りの不透明度を変えます。")
                            .changed()
                        {
                            let next_fill_color =
                                set_alpha_percent(fill_color_source, fill_opacity);
                            self.tool_colors.fill_color = next_fill_color;
                            self.tool_colors.fill_enabled = true;
                            self.push_recent_color(next_fill_color);
                            if shape_context.is_selection_target() {
                                self.replace_selected_shapes(
                                    |shape| shape.with_fill_color(Some(next_fill_color)),
                                    if shape_context.is_selected_shape() {
                                        format!(
                                            "{}の塗りの不透明度を {fill_opacity}% に変更しました。",
                                            shape_kind_label
                                                .as_deref()
                                                .unwrap_or(shape_context.kind.label())
                                        )
                                    } else {
                                        format!(
                                            "選択中の図形{}個の塗りの不透明度を {fill_opacity}% に変更しました。",
                                            shape_context.fill_supported_count
                                        )
                                    },
                                );
                            } else {
                                self.set_info(format!(
                                    "塗りの不透明度を {}% に変更しました。",
                                    fill_opacity
                                ));
                            }
                        }
                        ui.small(if fill_enabled {
                            "塗りを使うと、透過PNGでも塗りの透明度がそのまま出ます。"
                        } else {
                            "塗りなしです。必要なら「塗りを使う」をオンにします。"
                        });
                    });
                } else if let Some(paint_context) = selection_paint_context {
                    ui.small("選択中の塗りへ反映されます。");
                    ui.label("塗り色");
                    let mut fill_color = color32_from_rgba(fill_color_source);
                    if ui.color_edit_button_srgba(&mut fill_color).changed() {
                        let color = rgba_from_color32(fill_color);
                        self.tool_colors.fill_color = color;
                        self.tool_colors.fill_enabled = true;
                        self.push_recent_color(color);
                        self.replace_selected_fills(
                            |mut fill| {
                                fill.color = color;
                                fill
                            },
                            format!(
                                "選択中の塗り{}個の色を変更しました。",
                                paint_context.fill_count
                            ),
                        );
                    }
                    let mut fill_opacity = alpha_percent(fill_color_source);
                    if ui
                        .add(egui::Slider::new(&mut fill_opacity, 0..=100).suffix("%"))
                        .on_hover_text("選択中の塗りの不透明度を変えます。")
                        .changed()
                    {
                        let next_fill_color = set_alpha_percent(fill_color_source, fill_opacity);
                        self.tool_colors.fill_color = next_fill_color;
                        self.tool_colors.fill_enabled = true;
                        self.push_recent_color(next_fill_color);
                        self.replace_selected_fills(
                            |mut fill| {
                                fill.color = set_alpha_percent(fill.color, fill_opacity);
                                fill
                            },
                            format!(
                                "選択中の塗り{}個の不透明度を {fill_opacity}% に変更しました。",
                                paint_context.fill_count
                            ),
                        );
                    }
                    ui.small("塗り色と不透明度をまとめて直せます。");
                }
            });
        } else {
            ui.small("直線は線だけなので、塗り色や塗り不透明度は使いません。");
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

        if shape_context.is_none() {
            ui.add_space(8.0);
            ui.label("描く太さ");
            let width_source = self.tool_widths.draw_width;
            let mut draw_width = width_source;
            if ui
                .add(egui::Slider::new(
                    &mut draw_width,
                    MIN_BRUSH_WIDTH..=MAX_BRUSH_WIDTH,
                ))
                .on_hover_text(
                    "ペン、えんぴつ、クレヨン、マーカー、四角形、楕円、直線の太さを変えます。",
                )
                .changed()
            {
                self.tool_widths.draw_width = draw_width;
                self.set_info(format!(
                    "描く太さを {:.1}px に変更しました。描画ツールと図形に使います。",
                    draw_width
                ));
            }
            ui.small(format!(
                "{:.1}px · ペン / えんぴつ / クレヨン / マーカー / 四角形 / 楕円 / 直線",
                draw_width
            ));
        }

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
            CanvasToolKind::Brush
                | CanvasToolKind::Pencil
                | CanvasToolKind::Crayon
                | CanvasToolKind::Marker
        ) {
            ui.small(format!("描き味: {}", brush_kind_summary(self.active_tool)));
        } else if self.active_tool == CanvasToolKind::Bucket {
            ui.small(format!(
                "塗りのゆるさ: {}",
                self.ui_state.bucket_fill_tolerance.label()
            ));
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
                "JSON保存は再編集用、PNG書き出しは見たまま共有、透過PNGは素材、SVG書き出しは図形や線の再利用向けです。上部バーから使えます。",
            );
            if response.clicked() {
                self.set_info(
                    "JSON保存は続きから再編集、PNG書き出しは見たまま共有、SVG書き出しは図形や線の再利用に向いています。",
                );
            }
        });
        ui.small("JSONは再編集用、PNGは見たまま共有用、SVGは拡大や再利用向けです。");
        ui.small("PNG書き出しは背景あり、透過PNGは透明背景、SVGは図形と線を中心に書き出します。");
        ui.small(self.storage.storage_strategy_summary());
    }

    fn show_selection_actions(&mut self, ui: &mut egui::Ui, context: SelectionArrangeContext) {
        let selection_summary = self.canvas.selection_summary(self.document());
        let align_button_width = ((ui.available_width() - 16.0) / 3.0).max(54.0);
        let pair_button_width = ((ui.available_width() - 8.0) / 2.0).max(90.0);

        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("整列と順序").strong());
                layer_status_chip(ui, &selection_summary, true);
                if context.can_distribute {
                    summary_chip(ui, "整列・等間隔", false);
                } else {
                    summary_chip(ui, "整列できます", false);
                }
            });
            ui.small("複数選択した要素をそろえたり、前後関係をまとめて整えられます。");

            ui.add_space(6.0);
            ui.label(RichText::new("横位置").strong());
            ui.horizontal_wrapped(|ui| {
                for alignment in [
                    AlignmentKind::Left,
                    AlignmentKind::HorizontalCenter,
                    AlignmentKind::Right,
                ] {
                    let response = ui
                        .add_sized(
                            [align_button_width, LAYER_ACTION_BUTTON_HEIGHT],
                            egui::Button::new(arrangement_button_label(alignment)),
                        )
                        .on_hover_text(alignment.label());
                    if response.clicked() {
                        self.apply_alignment(alignment);
                    }
                }
            });

            ui.add_space(4.0);
            ui.label(RichText::new("縦位置").strong());
            ui.horizontal_wrapped(|ui| {
                for alignment in [
                    AlignmentKind::Top,
                    AlignmentKind::VerticalCenter,
                    AlignmentKind::Bottom,
                ] {
                    let response = ui
                        .add_sized(
                            [align_button_width, LAYER_ACTION_BUTTON_HEIGHT],
                            egui::Button::new(arrangement_button_label(alignment)),
                        )
                        .on_hover_text(alignment.label());
                    if response.clicked() {
                        self.apply_alignment(alignment);
                    }
                }
            });

            if context.can_distribute {
                ui.add_space(6.0);
                ui.label(RichText::new("等間隔").strong());
                ui.horizontal_wrapped(|ui| {
                    for distribution in [DistributionKind::Horizontal, DistributionKind::Vertical] {
                        let response = ui
                            .add_sized(
                                [pair_button_width, LAYER_ACTION_BUTTON_HEIGHT],
                                egui::Button::new(distribution_button_label(distribution)),
                            )
                            .on_hover_text(distribution.label());
                        if response.clicked() {
                            self.apply_distribution(distribution);
                        }
                    }
                });
            }

            if context.can_reorder {
                ui.add_space(6.0);
                ui.label(RichText::new("重なり順").strong());
                ui.horizontal_wrapped(|ui| {
                    for command in [
                        StackOrderCommand::BringToFront,
                        StackOrderCommand::BringForward,
                    ] {
                        let response = ui
                            .add_sized(
                                [pair_button_width, LAYER_ACTION_BUTTON_HEIGHT],
                                egui::Button::new(stack_order_button_label(command)),
                            )
                            .on_hover_text(command.label());
                        if response.clicked() {
                            self.apply_stack_order(command);
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for command in [
                        StackOrderCommand::SendBackward,
                        StackOrderCommand::SendToBack,
                    ] {
                        let response = ui
                            .add_sized(
                                [pair_button_width, LAYER_ACTION_BUTTON_HEIGHT],
                                egui::Button::new(stack_order_button_label(command)),
                            )
                            .on_hover_text(command.label());
                        if response.clicked() {
                            self.apply_stack_order(command);
                        }
                    }
                });
            }
        });
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
                    "グリッドやガイドで位置をそろえ、スマートガイドは移動中だけ目安を出します。",
                );
            }
        });
        ui.small(format!(
            "ルーラー: {} · グリッド: {:.0}px · 吸着: {} · ガイド: {}本",
            if rulers_visible {
                "表示"
            } else {
                "非表示"
            },
            grid.spacing,
            snap_summary_label(grid.snap_enabled, guides_snap),
            guides.len(),
        ));
        if smart_guides_visible {
            ui.small("スマートガイドは移動や整列の時だけ表示されます。");
        }

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
        let selection_layer_id = self.canvas.selection_layer_id();
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
                    layer_status_chip(ui, "作業中", true);
                    if selection_layer_id == Some(active_layer_id) && selection_count > 0 {
                        layer_status_chip(ui, "選択中", true);
                    }
                    if selection_count > 0 {
                        layer_status_chip(ui, &format!("{selection_count}個"), false);
                    }
                    layer_status_chip(ui, if *visible { "表示中" } else { "非表示" }, *visible);
                    layer_status_chip(ui, if *locked { "ロック中" } else { "編集可" }, !*locked);
                    layer_status_chip(
                        ui,
                        if *element_count > 0 {
                            "要素あり"
                        } else {
                            "空"
                        },
                        *element_count > 0,
                    );
                    ui.small(format!("{element_count}個"));
                });
                ui.small(layer_row_summary(*visible, *locked, *element_count));
                if let Some(selection_context) = self.selection_layer_context() {
                    ui.small(selection_context);
                } else if selection_count > 0 {
                    ui.small("選択した要素は別のレイヤーにあります。右側の一覧から作業レイヤーを切り替えられます。");
                }
            });
            ui.add_space(8.0);
        }

        let action_columns = if ui.available_width() < 240.0 {
            2.0
        } else {
            3.0
        };
        let action_width =
            ((ui.available_width() - (action_columns - 1.0) * 8.0) / action_columns).max(72.0);
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
            ui.horizontal_wrapped(|ui| {
                layer_status_chip(ui, &format!("{selection_count}個選択中"), true);
                layer_status_chip(ui, "受け取り先を選べます", false);
            });
            ui.small("移動先のレイヤーで「ここへ移動」または「ここへ複製」を使えます。");
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
                let active_response = ui
                    .add_sized(
                        [ui.available_width(), LAYER_ACTION_BUTTON_HEIGHT],
                        egui::Button::new(
                            RichText::new(name.as_str()).strong().size(if is_active {
                                15.0
                            } else {
                                14.0
                            }),
                        )
                        .selected(is_active),
                    )
                    .on_hover_text("このレイヤーを作業レイヤーにします。");
                if active_response.clicked() {
                    pending_action = Some(LayerAction::SetActive(layer_id));
                }

                ui.horizontal_wrapped(|ui| {
                    layer_status_chip(ui, "作業中", is_active);
                    if selection_layer_id == Some(layer_id) && selection_count > 0 {
                        layer_status_chip(ui, "選択中", true);
                    }
                    if can_receive_selection {
                        let can_drop_here = visible && !locked;
                        layer_status_chip(
                            ui,
                            if can_drop_here {
                                "受け取り可"
                            } else {
                                "受け取り不可"
                            },
                            can_drop_here,
                        );
                    }
                    layer_status_chip(ui, if visible { "表示中" } else { "非表示" }, visible);
                    layer_status_chip(ui, if locked { "ロック中" } else { "編集可" }, !locked);
                    layer_status_chip(
                        ui,
                        if element_count > 0 {
                            "要素あり"
                        } else {
                            "空"
                        },
                        element_count > 0,
                    );
                    ui.small(format!("{element_count}個"));
                    if index + 1 == total_layers {
                        ui.small("最前面");
                    } else if index == 0 {
                        ui.small("最背面");
                    }
                });
                ui.small(layer_row_summary(visible, locked, element_count));

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
                    let transfer_width = ((ui.available_width() - 8.0) / 2.0).max(86.0);
                    ui.horizontal(|ui| {
                        let can_drop_here = visible && !locked;
                        if ui
                            .add_enabled(
                                can_drop_here,
                                egui::Button::new("ここへ移動").min_size(egui::vec2(
                                    transfer_width,
                                    LAYER_ACTION_BUTTON_HEIGHT,
                                )),
                            )
                            .on_hover_text("選択中の要素をこのレイヤーへ移します。")
                            .clicked()
                        {
                            pending_action = Some(LayerAction::MoveSelectionTo(layer_id));
                        }
                        if ui
                            .add_enabled(
                                can_drop_here,
                                egui::Button::new("ここへ複製").min_size(egui::vec2(
                                    transfer_width,
                                    LAYER_ACTION_BUTTON_HEIGHT,
                                )),
                            )
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

                if ui
                    .add_enabled(can_file_io, egui::Button::new("SVG書き出し"))
                    .on_hover_text("図形や線を拡大しやすい SVG として書き出します。バケツ塗りやブラシ質感は簡略化されます。")
                    .clicked()
                {
                    self.export_svg(ctx);
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

            ui.add_space(4.0);
            let compact_summary = ui.available_width() < 760.0;
            let operation_summary = self.canvas.current_operation_summary(self.document());
            let selected_shape_context = self
                .current_shape_style_context()
                .filter(|context| context.is_selection_target());
            let selected_shape_chip = selected_shape_context
                .map(|context| {
                    if context.is_selected_shapes() {
                        if compact_summary {
                            format!("図形{}", context.shape_count)
                        } else {
                            format!("図形: {}個", context.shape_count)
                        }
                    } else if compact_summary {
                        context.kind.label().to_owned()
                    } else {
                        format!("図形: {}", context.kind.label())
                    }
                });
            let selected_shape_mode_chip =
                selected_shape_context.map(ShapeStyleContext::paint_mode_label);
            let multi_shape_style_chip = selected_shape_context
                .filter(|context| context.is_selected_shapes())
                .map(|_| {
                    if compact_summary {
                        "共通".to_owned()
                    } else {
                        "共通スタイル".to_owned()
                    }
                });
            ui.horizontal_wrapped(|ui| {
                summary_chip(
                    ui,
                    if compact_summary {
                        self.active_tool.label().to_owned()
                    } else {
                        format!("道具: {}", self.active_tool.label())
                    },
                    true,
                );
                if let Some(operation) = &operation_summary {
                    summary_chip(ui, operation.clone(), true);
                }
                if matches!(
                    self.active_tool,
                    CanvasToolKind::Brush
                        | CanvasToolKind::Pencil
                        | CanvasToolKind::Crayon
                        | CanvasToolKind::Marker
                ) && !compact_summary
                {
                    summary_chip(ui, brush_kind_summary(self.active_tool), false);
                } else if self.active_tool == CanvasToolKind::Bucket {
                    summary_chip(
                        ui,
                        if compact_summary {
                            format!("塗り {}", self.ui_state.bucket_fill_tolerance.label())
                        } else {
                            format!("ゆるさ: {}", self.ui_state.bucket_fill_tolerance.label())
                        },
                        false,
                    );
                }
                if let Some(layer) = self.document().active_layer() {
                    summary_chip(
                        ui,
                        if compact_summary {
                            format!("L: {}", layer.name)
                        } else {
                            format!("作業: {}", layer.name)
                        },
                        false,
                    );
                }
                if operation_summary.is_none()
                    && let Some(shape_chip) = &selected_shape_chip
                {
                    summary_chip(ui, shape_chip.clone(), false);
                }
                if operation_summary.is_none()
                    && let Some(mode_chip) = selected_shape_mode_chip
                {
                    summary_chip(ui, mode_chip, false);
                }
                if operation_summary.is_none()
                    && let Some(style_chip) = &multi_shape_style_chip
                {
                    summary_chip(ui, style_chip.clone(), false);
                }
                if operation_summary.is_none() {
                    let arrange_context =
                        SelectionArrangeContext::from_state(selection_count, has_canvas_interaction);
                    if let Some(arrange_chip) = arrange_context.summary_chip_label(compact_summary) {
                        summary_chip(ui, arrange_chip, false);
                    }
                }
                if operation_summary.is_none() && selection_count > 0 {
                    summary_chip(
                        ui,
                        if compact_summary {
                            format!("{selection_count}個を選択中")
                        } else {
                            self.canvas.selection_summary(self.document())
                        },
                        false,
                    );
                }
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
                ui.label(RichText::new("最初の流れ").strong());
                ui.small("1. まずは左のペンか図形ツールで 1 つ描きます。");
                ui.small("2. 選択に切り替えると、動かしたり変形したりできます。");
                ui.small("3. 続きを残すなら JSON保存、共有するなら PNG や SVG を使います。");

                ui.add_space(8.0);
                ui.label(RichText::new("短く確認する").strong());
                ui.small("描く: ペン / えんぴつ / クレヨン / マーカーか図形ツールを選んでドラッグします。");
                ui.small("色: スポイト、バケツ塗り、最近使った色、簡易パレットで線色や塗り色をすぐ使い回せます。バケツ塗りは塗りのゆるさも変えられます。");
                ui.small("選ぶ: 選択ツールで移動や変形、複数選択でまとめて整理できます。");
                ui.small("パンとズーム: 手のひら、Space+Drag、2本指ドラッグ、ピンチが使えます。");
                ui.small("保存: JSON保存は再編集用、PNGは共有用、透過PNGは素材用、SVGは再利用向けです。");
                ui.small("レイヤー: 右側で現在のレイヤー、表示、ロックを切り替えます。");
                ui.small("左パネル: 下までスクロールすると配置補助や保存メモが見られます。");
                #[cfg(target_arch = "wasm32")]
                ui.small("Web版: JSON保存 と PNG / 透過PNG / SVG はダウンロード、JSONを開く はファイル選択です。");

                ui.add_space(8.0);
                ui.label(RichText::new("ショートカット").strong());
                ui.small("元に戻す: Ctrl/Cmd+Z · やり直す: Ctrl/Cmd+Shift+Z または Ctrl/Cmd+Y");
                ui.small("JSON保存: Ctrl/Cmd+S · JSONを開く: Ctrl/Cmd+O · PNG書き出し: Ctrl/Cmd+Shift+E");
                ui.small("ツール: V 選択 · H 手のひら · B ペン · N えんぴつ · C クレヨン · M マーカー · I スポイト · F バケツ塗り · R 四角形 · O 楕円 · L 直線 · E 消しゴム");

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
                DocumentEditMode::Fill => {
                    "閉じた領域を塗りました。結果は現在のレイヤーに入ります。"
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
            let cleared_selection = self.canvas.selection_count() > 0;
            self.canvas.discard_active_interaction();
            self.canvas.clear_selection();
            self.sync_layer_name_draft();
            if let Some(layer) = self.document().active_layer() {
                self.set_info(if cleared_selection {
                    format!(
                        "作業レイヤーを {} に切り替えました。選択は新しいレイヤーに合わせて解除しました。",
                        layer.name
                    )
                } else {
                    format!("作業レイヤーを {} に切り替えました。", layer.name)
                });
            }
        }
    }

    fn selection_layer_context(&self) -> Option<String> {
        let layer_id = self.canvas.selection_layer_id()?;
        let selection_count = self.canvas.selection_count();
        if selection_count == 0 {
            return None;
        }

        let layer = self.document().layer(layer_id)?;
        Some(if selection_count == 1 {
            format!("選択レイヤー: {}", layer.name)
        } else {
            format!("選択レイヤー: {}（{}個）", layer.name, selection_count)
        })
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

    fn finish_svg_export(&mut self, exported: ExportedVectorGraphic) {
        self.set_info(format!(
            "SVG を {} として書き出しました。図形や線は再利用向けに出力し、ブラシ質感は簡略化、バケツ塗りは行ごとの矩形へ整えて出力します。",
            exported.file_name
        ));
    }

    fn storage_action_title(action: &'static str) -> &'static str {
        match action {
            "save" => "JSON保存",
            "load" => "JSONを開く",
            "export" => "PNG書き出し",
            "export-transparent" => "透過PNG",
            "export-svg" => "SVG書き出し",
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
            "export-svg" => "ブラウザで SVG ダウンロードを準備しています...",
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

    fn export_svg(&mut self, _ctx: &egui::Context) {
        let suggested_name = self.storage.suggested_svg_file_name(&self.document_name);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let result = self
                .storage
                .export_svg_via_dialog(self.document(), &suggested_name)
                .map(|exported| self.finish_svg_export(exported));
            self.report_storage_result("export-svg", result);
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
                    .export_svg_via_dialog(&document, &suggested_name)
                    .await
                    .map(WebStorageResult::ExportedSvg);
                *task_slot.borrow_mut() = Some(result);
                ctx.request_repaint();
            });

            self.pending_web_task = Some(PendingWebStorageTask {
                label: "export-svg",
                slot,
            });
            self.set_info(Self::storage_pending_message("export-svg"));
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
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::C)) {
            self.set_active_tool(CanvasToolKind::Crayon, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::M)) {
            self.set_active_tool(CanvasToolKind::Marker, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::I)) {
            self.set_active_tool(CanvasToolKind::Eyedropper, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::F)) {
            self.set_active_tool(CanvasToolKind::Bucket, true);
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
                Ok(WebStorageResult::ExportedSvg(exported)) => self.finish_svg_export(exported),
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

        let (tools_panel_width, layers_panel_width) =
            panel_widths_for_window(ctx.available_rect().width());

        egui::SidePanel::left("tools_panel")
            .resizable(false)
            .default_width(tools_panel_width)
            .min_width(tools_panel_width)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.show_tools(ui));
            });

        egui::SidePanel::right("layers_panel")
            .resizable(false)
            .default_width(layers_panel_width)
            .min_width(layers_panel_width)
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

            if let Some(message) = output.message {
                match message.kind {
                    CanvasMessageKind::Info => self.set_info(message.text),
                    CanvasMessageKind::Error => self.set_error(message.text),
                }
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
        CanvasToolKind::Brush => "もっとも素直でくっきり",
        CanvasToolKind::Pencil => "細めで少しラフなえんぴつ",
        CanvasToolKind::Crayon => "やや太くざらっとしたクレヨン",
        CanvasToolKind::Marker => "太めで重ねやすいマーカー",
        _ => "",
    }
}

fn snap_summary_label(grid_snap: bool, guides_snap: bool) -> &'static str {
    match (grid_snap, guides_snap) {
        (true, true) => "グリッド・ガイド",
        (true, false) => "グリッド",
        (false, true) => "ガイド",
        (false, false) => "オフ",
    }
}

fn tablet_button_columns(available_width: f32) -> usize {
    if available_width >= 250.0 { 2 } else { 1 }
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
        RichText::new(format!(" {label} "))
            .small()
            .strong()
            .background_color(ui.visuals().selection.bg_fill.linear_multiply(0.18))
            .color(ui.visuals().selection.stroke.color)
    } else {
        RichText::new(format!(" {label} "))
            .small()
            .background_color(ui.visuals().faint_bg_color.linear_multiply(0.6))
            .color(ui.visuals().weak_text_color())
    };
    ui.label(text);
}

fn layer_row_summary(visible: bool, locked: bool, element_count: usize) -> String {
    let edit_state = if !visible {
        "非表示"
    } else if locked {
        "表示のみ"
    } else {
        "編集できます"
    };
    let content_state = match element_count {
        0 => "空レイヤー".to_owned(),
        1 => "1個の要素".to_owned(),
        count => format!("{count}個の要素"),
    };
    format!("{edit_state} · {content_state}")
}

fn summary_chip(ui: &mut egui::Ui, text: impl Into<String>, accent: bool) {
    let text = text.into();
    let text = if accent {
        RichText::new(format!(" {text} "))
            .small()
            .strong()
            .background_color(ui.visuals().selection.bg_fill.linear_multiply(0.18))
            .color(ui.visuals().selection.stroke.color)
    } else {
        RichText::new(format!(" {text} "))
            .small()
            .background_color(ui.visuals().faint_bg_color.linear_multiply(0.6))
            .color(ui.visuals().strong_text_color())
    };
    ui.label(text);
}

fn arrangement_button_label(alignment: AlignmentKind) -> &'static str {
    match alignment {
        AlignmentKind::Left => "左",
        AlignmentKind::HorizontalCenter => "中央",
        AlignmentKind::Right => "右",
        AlignmentKind::Top => "上",
        AlignmentKind::VerticalCenter => "中央",
        AlignmentKind::Bottom => "下",
    }
}

fn distribution_button_label(distribution: DistributionKind) -> &'static str {
    match distribution {
        DistributionKind::Horizontal => "横等間隔",
        DistributionKind::Vertical => "縦等間隔",
    }
}

fn stack_order_button_label(command: StackOrderCommand) -> &'static str {
    match command {
        StackOrderCommand::BringToFront => "最前面",
        StackOrderCommand::BringForward => "前へ",
        StackOrderCommand::SendBackward => "後ろへ",
        StackOrderCommand::SendToBack => "最背面",
    }
}

fn panel_widths_for_window(window_width: f32) -> (f32, f32) {
    if window_width < 980.0 {
        (198.0, 212.0)
    } else if window_width < 1220.0 {
        (208.0, 224.0)
    } else {
        (220.0, 240.0)
    }
}

fn tool_button_tooltip(tool: CanvasToolKind) -> &'static str {
    match tool {
        CanvasToolKind::Select => "選ぶ・動かす・変形するツールです。",
        CanvasToolKind::Pan => "キャンバスをドラッグして移動します。",
        CanvasToolKind::Brush => "もっとも素直でくっきり描けるペンです。",
        CanvasToolKind::Pencil => "細めで少しラフなえんぴつです。",
        CanvasToolKind::Crayon => "やや太く、少しざらっとしたクレヨンです。",
        CanvasToolKind::Marker => "太めで重ねやすい半透明のマーカーです。",
        CanvasToolKind::Eyedropper => "見えている色を拾って線色や塗り色に使います。",
        CanvasToolKind::Bucket => {
            "塗り色で閉じた領域を塗ります。塗りのゆるさで少し広めにも塗れます。"
        }
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
            body: "ペン、えんぴつ、クレヨン、マーカーか図形ツールを選んで、キャンバスをドラッグします。まずは色を大きくいじらず、1 つ描くだけでも十分です。",
            action: "左の道具から 1 つ選んで、中央でドラッグしてみます。迷ったら「ペン」か「四角形」から始めると自然です。",
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
            body: "JSON保存 は続きから再編集したいとき用です。PNG書き出し は見たままの共有用、透過PNG は素材向け、SVG書き出し は図形や線の再利用向けです。",
            action: "まず覚えるなら「JSON保存」と「PNG書き出し」で十分です。必要になったら透過PNGやSVGも使えます。",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CanvasToolKind, ColorTarget, PaintApp, RECENT_COLOR_LIMIT, SelectionArrangeContext,
        panel_widths_for_window, snap_summary_label, tablet_button_columns,
    };
    use crate::fill::FillTolerancePreset;
    use crate::model::{
        FillElement, FillSpan, PaintDocument, PaintElement, PaintPoint, RgbaColor, ShapeElement,
        ShapeKind, Stroke, ToolKind,
    };

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
    fn freehand_brushes_reuse_draw_width_setting() {
        let mut app = PaintApp::default();
        app.tool_widths.draw_width = 11.0;
        app.tool_widths.eraser_width = 21.0;

        app.set_active_tool(CanvasToolKind::Pencil, false);
        assert_eq!(app.tool_settings().width, 11.0);

        app.set_active_tool(CanvasToolKind::Crayon, false);
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

    #[test]
    fn bucket_tool_settings_include_fill_tolerance() {
        let mut app = PaintApp::default();
        app.ui_state.bucket_fill_tolerance = FillTolerancePreset::Relaxed;
        app.set_active_tool(CanvasToolKind::Bucket, false);

        let settings = app.tool_settings();

        assert_eq!(settings.tool, CanvasToolKind::Bucket);
        assert_eq!(settings.fill_tolerance, FillTolerancePreset::Relaxed);
    }

    #[test]
    fn single_selected_shape_context_prefers_selected_shape_style() {
        let mut app = PaintApp::default();
        let mut document = PaintDocument::default();
        document.push_shape(
            ShapeElement::new(
                ShapeKind::Rectangle,
                RgbaColor::from_rgba(20, 40, 60, 255),
                9.0,
                PaintPoint::new(10.0, 10.0),
                PaintPoint::new(80.0, 60.0),
            )
            .with_fill_color(Some(RgbaColor::from_rgba(200, 180, 90, 140))),
        );
        assert!(app.history.replace_document(document));
        let layer_id = app.document().active_layer_id();
        app.canvas.set_selection_indices(layer_id, vec![0]);
        app.set_active_tool(CanvasToolKind::Select, false);

        let context = app
            .current_shape_style_context()
            .expect("selected rectangle context");

        assert!(context.is_selected_shape());
        assert_eq!(context.kind, ShapeKind::Rectangle);
        assert_eq!(context.paint_mode_label(), "線と塗り");
        assert_eq!(context.stroke_summary_label(), "線 100% / 9.0px");
        assert_eq!(context.fill_summary_label(), "塗り 55%");
        assert_eq!(context.stroke_color, RgbaColor::from_rgba(20, 40, 60, 255));
        assert_eq!(
            context.fill_color,
            Some(RgbaColor::from_rgba(200, 180, 90, 140))
        );
        assert_eq!(context.width, 9.0);
    }

    #[test]
    fn selected_line_shape_context_stays_stroke_only() {
        let mut app = PaintApp::default();
        let mut document = PaintDocument::default();
        document.push_shape(ShapeElement::new(
            ShapeKind::Line,
            RgbaColor::from_rgba(20, 40, 60, 220),
            6.0,
            PaintPoint::new(10.0, 10.0),
            PaintPoint::new(80.0, 60.0),
        ));
        assert!(app.history.replace_document(document));
        let layer_id = app.document().active_layer_id();
        app.canvas.set_selection_indices(layer_id, vec![0]);
        app.set_active_tool(CanvasToolKind::Select, false);

        let context = app
            .current_shape_style_context()
            .expect("selected line context");

        assert!(context.is_selected_shape());
        assert_eq!(context.kind, ShapeKind::Line);
        assert!(!context.supports_fill());
        assert_eq!(context.paint_mode_label(), "線だけ");
        assert_eq!(context.fill_summary_label(), "塗りなし");
        assert_eq!(context.fill_color, None);
    }

    #[test]
    fn quick_color_updates_selected_shape_stroke_when_selecting_shape() {
        let mut app = PaintApp::default();
        let mut document = PaintDocument::default();
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::from_rgba(20, 40, 60, 255),
            9.0,
            PaintPoint::new(10.0, 10.0),
            PaintPoint::new(80.0, 60.0),
        ));
        assert!(app.history.replace_document(document));
        let layer_id = app.document().active_layer_id();
        app.canvas.set_selection_indices(layer_id, vec![0]);
        app.set_active_tool(CanvasToolKind::Select, false);
        app.tool_colors.quick_color_target = ColorTarget::Stroke;

        app.apply_quick_color(RgbaColor::from_rgba(180, 30, 120, 200), "最近使った色");

        let Some(PaintElement::Shape(shape)) = app.document().element(0) else {
            panic!("selected element should stay a shape");
        };
        assert_eq!(shape.color, RgbaColor::from_rgba(180, 30, 120, 200));
    }

    #[test]
    fn multi_selected_shape_context_tracks_shape_count_and_fill_support() {
        let mut app = PaintApp::default();
        let mut document = PaintDocument::default();
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::from_rgba(20, 40, 60, 255),
            9.0,
            PaintPoint::new(10.0, 10.0),
            PaintPoint::new(80.0, 60.0),
        ));
        document.push_shape(
            ShapeElement::new(
                ShapeKind::Line,
                RgbaColor::from_rgba(120, 50, 40, 255),
                6.0,
                PaintPoint::new(100.0, 20.0),
                PaintPoint::new(180.0, 80.0),
            )
            .with_fill_color(Some(RgbaColor::from_rgba(250, 180, 90, 180))),
        );
        assert!(app.history.replace_document(document));
        let layer_id = app.document().active_layer_id();
        app.canvas.set_selection_indices(layer_id, vec![0, 1]);
        app.set_active_tool(CanvasToolKind::Select, false);

        let context = app
            .current_shape_style_context()
            .expect("multi selected shape context");

        assert!(context.is_selected_shapes());
        assert_eq!(context.shape_count, 2);
        assert_eq!(context.total_selection_count, 2);
        assert_eq!(context.fill_supported_count, 1);
        assert_eq!(context.paint_mode_label(), "線だけ");
        assert_eq!(context.fill_summary_label(), "塗りなし");
        assert_eq!(
            context.fill_scope_note(),
            Some("直線には塗りを適用せず、四角形 / 楕円だけに反映します。")
        );
    }

    #[test]
    fn quick_color_updates_multiple_selected_shape_strokes() {
        let mut app = PaintApp::default();
        let mut document = PaintDocument::default();
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::from_rgba(20, 40, 60, 255),
            9.0,
            PaintPoint::new(10.0, 10.0),
            PaintPoint::new(80.0, 60.0),
        ));
        document.push_shape(ShapeElement::new(
            ShapeKind::Ellipse,
            RgbaColor::from_rgba(70, 90, 110, 255),
            7.0,
            PaintPoint::new(90.0, 20.0),
            PaintPoint::new(160.0, 90.0),
        ));
        assert!(app.history.replace_document(document));
        let layer_id = app.document().active_layer_id();
        app.canvas.set_selection_indices(layer_id, vec![0, 1]);
        app.set_active_tool(CanvasToolKind::Select, false);
        app.tool_colors.quick_color_target = ColorTarget::Stroke;

        app.apply_quick_color(RgbaColor::from_rgba(180, 30, 120, 200), "最近使った色");

        let Some(PaintElement::Shape(first)) = app.document().element(0) else {
            panic!("first selected element should stay a shape");
        };
        let Some(PaintElement::Shape(second)) = app.document().element(1) else {
            panic!("second selected element should stay a shape");
        };
        assert_eq!(first.color, RgbaColor::from_rgba(180, 30, 120, 200));
        assert_eq!(second.color, RgbaColor::from_rgba(180, 30, 120, 200));
    }

    #[test]
    fn fill_color_updates_skip_selected_lines() {
        let mut app = PaintApp::default();
        let mut document = PaintDocument::default();
        document.push_shape(
            ShapeElement::new(
                ShapeKind::Rectangle,
                RgbaColor::from_rgba(20, 40, 60, 255),
                9.0,
                PaintPoint::new(10.0, 10.0),
                PaintPoint::new(80.0, 60.0),
            )
            .with_fill_color(Some(RgbaColor::from_rgba(200, 180, 90, 140))),
        );
        document.push_shape(ShapeElement::new(
            ShapeKind::Line,
            RgbaColor::from_rgba(120, 50, 40, 255),
            6.0,
            PaintPoint::new(100.0, 20.0),
            PaintPoint::new(180.0, 80.0),
        ));
        assert!(app.history.replace_document(document));
        let layer_id = app.document().active_layer_id();
        app.canvas.set_selection_indices(layer_id, vec![0, 1]);
        app.set_active_tool(CanvasToolKind::Select, false);
        app.tool_colors.quick_color_target = ColorTarget::Fill;

        app.apply_quick_color(RgbaColor::from_rgba(180, 30, 120, 200), "最近使った色");

        let Some(PaintElement::Shape(rectangle)) = app.document().element(0) else {
            panic!("rectangle should stay a shape");
        };
        let Some(PaintElement::Shape(line)) = app.document().element(1) else {
            panic!("line should stay a shape");
        };
        assert_eq!(
            rectangle.effective_fill_color(),
            Some(RgbaColor::from_rgba(180, 30, 120, 200))
        );
        assert_eq!(line.effective_fill_color(), None);
    }

    #[test]
    fn selection_paint_context_detects_stroke_only_selection() {
        let mut app = PaintApp::default();
        let mut document = PaintDocument::default();
        let mut stroke = Stroke::new(ToolKind::Brush, RgbaColor::from_rgba(20, 40, 60, 255), 9.0);
        stroke.push_point(PaintPoint::new(10.0, 10.0));
        stroke.push_point(PaintPoint::new(30.0, 30.0));
        document.push_stroke(stroke);
        assert!(app.history.replace_document(document));
        let layer_id = app.document().active_layer_id();
        app.canvas.set_selection_indices(layer_id, vec![0]);
        app.set_active_tool(CanvasToolKind::Select, false);

        let context = app
            .current_selection_paint_context()
            .expect("stroke-only selection context");

        assert!(context.has_strokes());
        assert!(!context.has_fills());
        assert_eq!(context.stroke_count, 1);
        assert_eq!(context.stroke_width, Some(9.0));
        assert_eq!(context.selection_label(), "線 1個");
    }

    #[test]
    fn selection_paint_context_ignores_mixed_shape_selection() {
        let mut app = PaintApp::default();
        let mut document = PaintDocument::default();
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::from_rgba(20, 40, 60, 255),
            9.0,
            PaintPoint::new(10.0, 10.0),
            PaintPoint::new(80.0, 60.0),
        ));
        let mut stroke = Stroke::new(ToolKind::Brush, RgbaColor::from_rgba(90, 20, 60, 255), 7.0);
        stroke.push_point(PaintPoint::new(100.0, 20.0));
        stroke.push_point(PaintPoint::new(140.0, 40.0));
        document.push_stroke(stroke);
        assert!(app.history.replace_document(document));
        let layer_id = app.document().active_layer_id();
        app.canvas.set_selection_indices(layer_id, vec![0, 1]);
        app.set_active_tool(CanvasToolKind::Select, false);

        assert!(app.current_selection_paint_context().is_none());
    }

    #[test]
    fn quick_color_updates_selected_strokes_when_no_shapes_are_selected() {
        let mut app = PaintApp::default();
        let mut document = PaintDocument::default();
        let mut first = Stroke::new(ToolKind::Brush, RgbaColor::from_rgba(20, 40, 60, 255), 9.0);
        first.push_point(PaintPoint::new(10.0, 10.0));
        first.push_point(PaintPoint::new(30.0, 30.0));
        let mut second = Stroke::new(
            ToolKind::Pencil,
            RgbaColor::from_rgba(80, 90, 120, 220),
            6.0,
        );
        second.push_point(PaintPoint::new(40.0, 10.0));
        second.push_point(PaintPoint::new(80.0, 30.0));
        document.push_stroke(first);
        document.push_stroke(second);
        assert!(app.history.replace_document(document));
        let layer_id = app.document().active_layer_id();
        app.canvas.set_selection_indices(layer_id, vec![0, 1]);
        app.set_active_tool(CanvasToolKind::Select, false);
        app.tool_colors.quick_color_target = ColorTarget::Stroke;

        app.apply_quick_color(RgbaColor::from_rgba(180, 30, 120, 200), "最近使った色");

        let Some(PaintElement::Stroke(first)) = app.document().element(0) else {
            panic!("first selected element should stay a stroke");
        };
        let Some(PaintElement::Stroke(second)) = app.document().element(1) else {
            panic!("second selected element should stay a stroke");
        };
        assert_eq!(first.color, RgbaColor::from_rgba(180, 30, 120, 200));
        assert_eq!(second.color, RgbaColor::from_rgba(180, 30, 120, 200));
    }

    #[test]
    fn quick_color_updates_selected_fills_when_no_shapes_are_selected() {
        let mut app = PaintApp::default();
        let mut document = PaintDocument::default();
        document.push_fill(FillElement::new(
            RgbaColor::from_rgba(20, 40, 60, 180),
            PaintPoint::new(0.0, 0.0),
            vec![FillSpan {
                y: 0,
                x_start: 0,
                x_end: 5,
            }],
        ));
        document.push_fill(FillElement::new(
            RgbaColor::from_rgba(80, 90, 120, 200),
            PaintPoint::new(10.0, 10.0),
            vec![FillSpan {
                y: 0,
                x_start: 0,
                x_end: 4,
            }],
        ));
        assert!(app.history.replace_document(document));
        let layer_id = app.document().active_layer_id();
        app.canvas.set_selection_indices(layer_id, vec![0, 1]);
        app.set_active_tool(CanvasToolKind::Select, false);
        app.tool_colors.quick_color_target = ColorTarget::Fill;

        app.apply_quick_color(RgbaColor::from_rgba(180, 30, 120, 200), "最近使った色");

        let Some(PaintElement::Fill(first)) = app.document().element(0) else {
            panic!("first selected element should stay a fill");
        };
        let Some(PaintElement::Fill(second)) = app.document().element(1) else {
            panic!("second selected element should stay a fill");
        };
        assert_eq!(first.color, RgbaColor::from_rgba(180, 30, 120, 200));
        assert_eq!(second.color, RgbaColor::from_rgba(180, 30, 120, 200));
    }

    #[test]
    fn selection_layer_context_mentions_active_layer_and_count() {
        let mut app = PaintApp::default();
        let layer_id = app.document().active_layer_id();
        app.canvas.set_selection_indices(layer_id, vec![0, 2, 4]);

        assert_eq!(
            app.selection_layer_context().as_deref(),
            Some("選択レイヤー: レイヤー 1（3個）")
        );
    }

    #[test]
    fn switching_active_layer_reports_selection_clear() {
        let mut app = PaintApp::default();
        app.add_layer();
        let source_layer_id = app.document().active_layer_id();
        app.canvas.set_selection_indices(source_layer_id, vec![0]);
        let target_layer_id = app
            .document()
            .layers()
            .iter()
            .find(|layer| layer.id != source_layer_id)
            .map(|layer| layer.id)
            .expect("second layer");

        app.set_active_layer(target_layer_id);

        assert!(
            app.status_message
                .text
                .contains("選択は新しいレイヤーに合わせて解除")
        );
    }

    #[test]
    fn selection_arrange_context_only_shows_panel_for_multi_selection() {
        let single = SelectionArrangeContext::from_state(1, false);
        assert!(!single.show_panel());
        assert!(single.can_reorder);
        assert!(!single.can_align);

        let multi = SelectionArrangeContext::from_state(2, false);
        assert!(multi.show_panel());
        assert!(multi.can_align);
        assert!(!multi.can_distribute);
    }

    #[test]
    fn selection_arrange_context_summary_prefers_distribution_for_three_items() {
        let two = SelectionArrangeContext::from_state(2, false);
        let three = SelectionArrangeContext::from_state(3, false);

        assert_eq!(
            two.summary_chip_label(false).as_deref(),
            Some("整列・重なり順")
        );
        assert_eq!(
            three.summary_chip_label(false).as_deref(),
            Some("整列・等間隔・順序")
        );
    }

    #[test]
    fn panel_widths_shrink_on_narrow_screens() {
        assert_eq!(panel_widths_for_window(900.0), (198.0, 212.0));
        assert_eq!(panel_widths_for_window(1100.0), (208.0, 224.0));
        assert_eq!(panel_widths_for_window(1400.0), (220.0, 240.0));
    }

    #[test]
    fn snap_summary_label_describes_active_snap_targets() {
        assert_eq!(snap_summary_label(true, true), "グリッド・ガイド");
        assert_eq!(snap_summary_label(true, false), "グリッド");
        assert_eq!(snap_summary_label(false, true), "ガイド");
        assert_eq!(snap_summary_label(false, false), "オフ");
    }

    #[test]
    fn tablet_button_columns_expand_on_roomy_panels() {
        assert_eq!(tablet_button_columns(220.0), 1);
        assert_eq!(tablet_button_columns(250.0), 2);
        assert_eq!(tablet_button_columns(320.0), 2);
    }
}
