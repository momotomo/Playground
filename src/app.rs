use eframe::egui::{self, RichText};

use crate::canvas::{CanvasController, ToolSettings, color32_from_rgba, rgba_from_color32};
use crate::model::{PaintDocument, RgbaColor, ToolKind};
use crate::storage::StorageFacade;

const MIN_BRUSH_WIDTH: f32 = 1.0;
const MAX_BRUSH_WIDTH: f32 = 48.0;

pub struct PaintApp {
    document: PaintDocument,
    canvas: CanvasController,
    storage: StorageFacade,
    active_tool: ToolKind,
    brush_color: RgbaColor,
    brush_width: f32,
    status_message: String,
}

impl Default for PaintApp {
    fn default() -> Self {
        Self {
            document: PaintDocument::default(),
            canvas: CanvasController::default(),
            storage: StorageFacade::new(),
            active_tool: ToolKind::Brush,
            brush_color: RgbaColor::charcoal(),
            brush_width: 6.0,
            status_message: String::from("Save/load are planned for future local-file support."),
        }
    }
}

impl PaintApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::light());

        Self::default()
    }

    fn tool_settings(&self) -> ToolSettings {
        ToolSettings {
            tool: self.active_tool,
            color: self.brush_color,
            width: self.brush_width,
        }
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
            self.status_message = String::from("Brush color updated.");
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
            self.status_message = format!("Brush size set to {:.1}px.", self.brush_width);
        }
        ui.label(format!("{:.1}px", self.brush_width));

        ui.separator();
        ui.label(RichText::new("Canvas").strong());
        ui.label(format!(
            "{:.0} x {:.0}px",
            self.document.canvas_size.width, self.document.canvas_size.height
        ));
        ui.label(format!("Strokes: {}", self.document.stroke_count()));
        ui.add_space(8.0);

        if self.active_tool == ToolKind::Eraser {
            ui.small("Eraser uses the current canvas background color.");
        } else {
            ui.small("Brush color will be stored per stroke for future editable saves.");
        }

        ui.separator();
        ui.label(RichText::new("Storage Plan").strong());
        ui.small(self.storage.roadmap_summary());
        ui.small(self.storage.planned_edit_format());
        ui.small(self.storage.planned_export_format());
    }

    fn show_actions(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.canvas.can_undo(&self.document),
                    egui::Button::new("Undo"),
                )
                .clicked()
                && self.canvas.undo(&mut self.document)
            {
                self.status_message = String::from("Removed the last stroke.");
            }

            if ui.button("Clear").clicked() {
                self.canvas.clear(&mut self.document);
                self.status_message = String::from("Cleared the canvas.");
            }

            let save_response = ui.add_enabled(false, egui::Button::new("Save"));
            let _ = save_response.on_disabled_hover_text(
                "TODO: save editable local files once the format is finalized.",
            );

            let load_response = ui.add_enabled(false, egui::Button::new("Load"));
            let _ = load_response
                .on_disabled_hover_text("TODO: restore editable local files from disk.");

            ui.separator();
            ui.label(RichText::new(&self.status_message).small());
        });
    }
}

impl eframe::App for PaintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("tools_panel")
            .resizable(false)
            .default_width(220.0)
            .min_width(220.0)
            .show(ctx, |ui| self.show_tools(ui));

        egui::TopBottomPanel::top("actions_panel")
            .resizable(false)
            .show(ctx, |ui| self.show_actions(ui));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            let tool_settings = self.tool_settings();
            let output = self.canvas.show(ui, &mut self.document, tool_settings);

            if output.pointer_active {
                ctx.request_repaint();
            }

            if output.stroke_committed {
                self.status_message = String::from("Stroke committed to the document.");
            }
        });
    }
}
