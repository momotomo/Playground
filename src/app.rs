use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText};
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

use crate::canvas::{CanvasController, ToolSettings, color32_from_rgba, rgba_from_color32};
use crate::model::{DocumentHistory, PaintDocument, RgbaColor, Stroke, ToolKind};
use crate::storage::{ExportedImage, LoadedDocument, SavedDocument, StorageError, StorageFacade};

const MIN_BRUSH_WIDTH: f32 = 1.0;
const MAX_BRUSH_WIDTH: f32 = 48.0;

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
    Exported(ExportedImage),
}

pub struct PaintApp {
    history: DocumentHistory,
    canvas: CanvasController,
    storage: StorageFacade,
    active_tool: ToolKind,
    brush_color: RgbaColor,
    brush_width: f32,
    status_message: StatusMessage,
    document_name: String,
    saved_snapshot: PaintDocument,
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
            active_tool: ToolKind::Brush,
            brush_color: RgbaColor::charcoal(),
            brush_width: 6.0,
            status_message: StatusMessage::info(
                "Ready. Save JSON for editing, export PNG for sharing.",
            ),
            document_name: storage.suggested_file_name().to_owned(),
            saved_snapshot: document,
            #[cfg(target_arch = "wasm32")]
            pending_web_task: None,
        }
    }
}

impl PaintApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::light());

        Self::default()
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

    fn tool_settings(&self) -> ToolSettings {
        ToolSettings {
            tool: self.active_tool,
            color: self.brush_color,
            width: self.brush_width,
        }
    }

    fn show_file_summary(&self, ui: &mut egui::Ui) {
        let dirty_suffix = if self.is_dirty() { " *" } else { "" };
        ui.label(RichText::new("File").strong());
        ui.label(format!("{}{}", self.document_name, dirty_suffix));
    }

    fn show_tools(&mut self, ui: &mut egui::Ui) {
        ui.heading("Tools");
        ui.label("Pick a drawing mode, then drag on the canvas.");
        ui.add_space(8.0);

        ui.selectable_value(
            &mut self.active_tool,
            ToolKind::Brush,
            ToolKind::Brush.label(),
        );
        ui.selectable_value(
            &mut self.active_tool,
            ToolKind::Eraser,
            ToolKind::Eraser.label(),
        );

        ui.add_space(12.0);
        ui.label("Color");
        let mut color = color32_from_rgba(self.brush_color);
        if ui.color_edit_button_srgba(&mut color).changed() {
            self.brush_color = rgba_from_color32(color);
            self.active_tool = ToolKind::Brush;
            self.set_info("Brush color updated.");
        }

        ui.add_space(12.0);
        ui.label("Brush Size");
        if ui
            .add(egui::Slider::new(
                &mut self.brush_width,
                MIN_BRUSH_WIDTH..=MAX_BRUSH_WIDTH,
            ))
            .changed()
        {
            self.set_info(format!("Brush size set to {:.1}px.", self.brush_width));
        }
        ui.label(format!("{:.1}px", self.brush_width));

        ui.separator();
        self.show_file_summary(ui);
        ui.add_space(8.0);
        ui.label(RichText::new("Canvas").strong());
        ui.label(format!(
            "{:.0} x {:.0}px",
            self.document().canvas_size.width,
            self.document().canvas_size.height
        ));
        ui.label(format!("Strokes: {}", self.document().stroke_count()));
        ui.label(format!("Zoom: {}", self.canvas.zoom_label()));
        ui.small("Zoom: Ctrl/Cmd + Wheel or toolbar buttons");
        ui.small("Pan: Space + Drag or Middle Drag");
        ui.small("Reset view: Ctrl/Cmd + 0");
        ui.add_space(8.0);

        if self.active_tool == ToolKind::Eraser {
            ui.small("Eraser uses the current canvas background color.");
        } else {
            ui.small("Brush color is stored per stroke for editable saves.");
        }

        ui.separator();
        ui.label(RichText::new("Storage").strong());
        ui.small(self.storage.storage_strategy_summary());
        ui.small(self.storage.editable_format_label());
        ui.small(self.storage.planned_export_format());
        ui.small("Shortcuts: Undo, Redo, Save, Load, Export PNG");
    }

    fn show_actions(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let has_active_stroke = self.canvas.has_active_stroke();
        let can_undo = has_active_stroke || self.history.can_undo();
        let can_redo = !has_active_stroke && self.history.can_redo();
        let can_clear = has_active_stroke || self.document().has_strokes();
        let can_file_io = !has_active_stroke && !self.has_pending_storage_task();
        let can_adjust_view = !has_active_stroke;

        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(can_undo, egui::Button::new("Undo"))
                .clicked()
            {
                self.perform_undo();
            }

            if ui
                .add_enabled(can_redo, egui::Button::new("Redo"))
                .clicked()
            {
                self.perform_redo();
            }

            if ui
                .add_enabled(can_clear, egui::Button::new("Clear"))
                .clicked()
            {
                self.perform_clear();
            }

            ui.separator();

            if ui
                .add_enabled(can_file_io, egui::Button::new("Save"))
                .clicked()
            {
                self.save_document(ctx);
            }

            if ui
                .add_enabled(can_file_io, egui::Button::new("Load"))
                .clicked()
            {
                self.load_document(ctx);
            }

            if ui
                .add_enabled(can_file_io, egui::Button::new("Export PNG"))
                .clicked()
            {
                self.export_png(ctx);
            }

            ui.separator();

            if ui
                .add_enabled(can_adjust_view, egui::Button::new("-"))
                .clicked()
            {
                self.zoom_out();
            }

            ui.label(RichText::new(self.canvas.zoom_label()).monospace());

            if ui
                .add_enabled(can_adjust_view, egui::Button::new("+"))
                .clicked()
            {
                self.zoom_in();
            }

            if ui
                .add_enabled(can_adjust_view, egui::Button::new("Reset View"))
                .clicked()
            {
                self.reset_view();
            }

            ui.separator();
            ui.label(self.status_message.rich_text());
        });
    }

    fn perform_undo(&mut self) {
        if self.canvas.discard_active_stroke() {
            self.set_info("Discarded the in-progress stroke.");
        } else if self.history.undo() {
            self.set_info("Undid the last change.");
        }
    }

    fn perform_redo(&mut self) {
        if self.history.redo() {
            self.set_info("Redid the last undone change.");
        }
    }

    fn perform_clear(&mut self) {
        let discarded = self.canvas.discard_active_stroke();
        if self.history.clear() {
            self.set_info("Cleared the canvas.");
        } else if discarded {
            self.set_info("Discarded the in-progress stroke.");
        }
    }

    fn zoom_in(&mut self) {
        let canvas_size = self.document().canvas_size;
        if self.canvas.zoom_in(canvas_size) {
            self.set_info(format!("Zoomed in to {}.", self.canvas.zoom_label()));
        }
    }

    fn zoom_out(&mut self) {
        let canvas_size = self.document().canvas_size;
        if self.canvas.zoom_out(canvas_size) {
            self.set_info(format!("Zoomed out to {}.", self.canvas.zoom_label()));
        }
    }

    fn reset_view(&mut self) {
        let canvas_size = self.document().canvas_size;
        self.canvas.request_view_reset();
        let _ = self.canvas.reset_view(canvas_size);
        self.set_info("Reset the view to fit the canvas.");
    }

    fn commit_stroke(&mut self, stroke: Stroke) {
        if self.history.commit_stroke(stroke) {
            self.set_info("Stroke committed to the document.");
        }
    }

    fn finish_save(&mut self, saved: SavedDocument) {
        self.document_name = saved.file_name;
        self.saved_snapshot = self.document().clone();
        self.set_info(format!("Saved {}.", self.document_name));
    }

    fn finish_load(&mut self, loaded: LoadedDocument) {
        self.canvas.discard_active_stroke();
        self.canvas.request_view_reset();
        if self.history.replace_document(loaded.document.clone()) {
            self.document_name = loaded.file_name;
            self.saved_snapshot = loaded.document;
            self.set_info(format!("Loaded {}.", self.document_name));
        } else {
            self.document_name = loaded.file_name;
            self.saved_snapshot = loaded.document;
            self.set_info("Loaded the document without changes.");
        }
    }

    fn finish_export(&mut self, exported: ExportedImage) {
        self.set_info(format!("Exported {}.", exported.file_name));
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn report_storage_result(&mut self, action: &'static str, result: Result<(), StorageError>) {
        if let Err(error) = result {
            match error {
                StorageError::Cancelled => {
                    self.set_info(format!("{} cancelled.", capitalize(action)));
                }
                other => {
                    self.set_error(format!("{} failed: {other}", capitalize(action)));
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
            self.set_info("Waiting for the browser save flow...");
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
            self.set_info("Waiting for the browser file picker...");
        }
    }

    fn export_png(&mut self, _ctx: &egui::Context) {
        let suggested_name = self.storage.suggested_png_file_name(&self.document_name);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let result = self
                .storage
                .export_png_via_dialog(self.document(), &suggested_name)
                .map(|exported| self.finish_export(exported));
            self.report_storage_result("export", result);
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
                    .export_png_via_dialog(&document, &suggested_name)
                    .await
                    .map(WebStorageResult::Exported);
                *task_slot.borrow_mut() = Some(result);
                ctx.request_repaint();
            });

            self.pending_web_task = Some(PendingWebStorageTask {
                label: "export",
                slot,
            });
            self.set_info("Preparing the PNG download...");
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let redo_pressed = ctx.input_mut(|input| input.consume_shortcut(&shortcut_redo()))
            || ctx.input_mut(|input| input.consume_shortcut(&shortcut_redo_alt()));
        if redo_pressed && !self.canvas.has_active_stroke() && self.history.can_redo() {
            self.perform_redo();
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_undo()))
            && (self.canvas.has_active_stroke() || self.history.can_undo())
        {
            self.perform_undo();
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_save()))
            && !self.canvas.has_active_stroke()
            && !self.has_pending_storage_task()
        {
            self.save_document(ctx);
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_load()))
            && !self.canvas.has_active_stroke()
            && !self.has_pending_storage_task()
        {
            self.load_document(ctx);
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_export_png()))
            && !self.canvas.has_active_stroke()
            && !self.has_pending_storage_task()
        {
            self.export_png(ctx);
        }

        let zoom_in_pressed = ctx.input_mut(|input| input.consume_shortcut(&shortcut_zoom_in()))
            || ctx.input_mut(|input| input.consume_shortcut(&shortcut_zoom_in_alt()));
        if zoom_in_pressed && !self.canvas.has_active_stroke() {
            self.zoom_in();
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_zoom_out()))
            && !self.canvas.has_active_stroke()
        {
            self.zoom_out();
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_reset_view()))
            && !self.canvas.has_active_stroke()
        {
            self.reset_view();
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
                Ok(WebStorageResult::Exported(exported)) => self.finish_export(exported),
                Err(StorageError::Cancelled) => {
                    self.set_info(format!("{} cancelled.", capitalize(task.label)));
                }
                Err(error) => {
                    self.set_error(format!("{} failed: {error}", capitalize(task.label)));
                }
            }
        } else {
            self.pending_web_task = Some(task);
        }
    }
}

impl eframe::App for PaintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_arch = "wasm32")]
        self.poll_web_storage_task();

        self.handle_shortcuts(ctx);

        egui::SidePanel::left("tools_panel")
            .resizable(false)
            .default_width(220.0)
            .min_width(220.0)
            .show(ctx, |ui| self.show_tools(ui));

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

            if let Some(stroke) = output.committed_stroke {
                self.commit_stroke(stroke);
            }
        });
    }
}

fn capitalize(action: &str) -> String {
    let mut chars = action.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };

    first.to_uppercase().chain(chars).collect()
}
