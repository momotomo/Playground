use eframe::egui::{
    self, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Stroke as EguiStroke, Vec2,
};

use crate::model::{PaintDocument, PaintPoint, RgbaColor, Stroke, ToolKind};

#[derive(Debug, Clone, Copy)]
pub struct ToolSettings {
    pub tool: ToolKind,
    pub color: RgbaColor,
    pub width: f32,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CanvasOutput {
    pub pointer_active: bool,
    pub stroke_committed: bool,
}

#[derive(Debug, Default)]
pub struct CanvasController {
    active_stroke: Option<Stroke>,
}

impl CanvasController {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut PaintDocument,
        tool_settings: ToolSettings,
    ) -> CanvasOutput {
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let desired_size =
                    Vec2::new(document.canvas_size.width, document.canvas_size.height);
                let (response, painter) = ui.allocate_painter(desired_size, Sense::drag());
                let response = response.on_hover_cursor(egui::CursorIcon::Crosshair);
                let rect = response.rect;

                let output = self.handle_input(ui, &response, rect, document, tool_settings);

                paint_background(&painter, rect, document.background);
                paint_document(&painter, rect, document);

                if let Some(active_stroke) = &self.active_stroke {
                    paint_stroke(&painter, rect, active_stroke, document.background);
                }

                if !document.has_strokes() && self.active_stroke.is_none() {
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "Drag on the canvas to start sketching",
                        FontId::proportional(24.0),
                        Color32::from_gray(120),
                    );
                }

                output
            })
            .inner
    }

    pub fn can_undo(&self, document: &PaintDocument) -> bool {
        self.active_stroke.is_some() || document.has_strokes()
    }

    pub fn undo(&mut self, document: &mut PaintDocument) -> bool {
        if self.active_stroke.take().is_some() {
            true
        } else {
            document.undo()
        }
    }

    pub fn clear(&mut self, document: &mut PaintDocument) {
        self.active_stroke = None;
        document.clear();
    }

    fn handle_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: Rect,
        document: &mut PaintDocument,
        tool_settings: ToolSettings,
    ) -> CanvasOutput {
        let pointer = ui.input(|input| input.pointer.clone());
        let mut output = CanvasOutput::default();

        if response.hovered()
            && pointer.primary_pressed()
            && let Some(position) = pointer.interact_pos()
        {
            let mut stroke = Stroke::new(
                tool_settings.tool,
                match tool_settings.tool {
                    ToolKind::Brush => tool_settings.color,
                    ToolKind::Eraser => document.background,
                },
                tool_settings.width,
            );
            stroke.push_point(screen_to_canvas(rect, position, document));
            self.active_stroke = Some(stroke);
            output.pointer_active = true;
        }

        if pointer.primary_down()
            && let Some(position) = pointer.interact_pos()
            && let Some(stroke) = &mut self.active_stroke
        {
            stroke.push_point(screen_to_canvas(rect, position, document));
            output.pointer_active = true;
        }

        if pointer.primary_released() {
            output.stroke_committed = self.commit_active_stroke(document);
        }

        output
    }

    fn commit_active_stroke(&mut self, document: &mut PaintDocument) -> bool {
        let Some(stroke) = self.active_stroke.take() else {
            return false;
        };

        if stroke.is_committable() {
            document.push_stroke(stroke);
            true
        } else {
            false
        }
    }
}

pub fn color32_from_rgba(color: RgbaColor) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

pub fn rgba_from_color32(color: Color32) -> RgbaColor {
    let [r, g, b, a] = color.to_array();
    RgbaColor::from_rgba(r, g, b, a)
}

fn paint_background(painter: &Painter, rect: Rect, background: RgbaColor) {
    painter.rect_filled(rect, 12.0, color32_from_rgba(background));
    painter.rect_stroke(
        rect,
        12.0,
        EguiStroke::new(1.0, Color32::from_gray(150)),
        egui::StrokeKind::Outside,
    );
}

fn paint_document(painter: &Painter, rect: Rect, document: &PaintDocument) {
    for stroke in &document.strokes {
        paint_stroke(painter, rect, stroke, document.background);
    }
}

fn paint_stroke(painter: &Painter, rect: Rect, stroke: &Stroke, background: RgbaColor) {
    let color = match stroke.tool {
        ToolKind::Brush => color32_from_rgba(stroke.color),
        ToolKind::Eraser => color32_from_rgba(background),
    };

    match stroke.points.as_slice() {
        [] => {}
        [point] => {
            painter.circle_filled(canvas_to_screen(rect, *point), stroke.width * 0.5, color);
        }
        points => {
            let line_points = points
                .iter()
                .copied()
                .map(|point| canvas_to_screen(rect, point))
                .collect();
            painter.add(egui::Shape::line(
                line_points,
                EguiStroke::new(stroke.width, color),
            ));
        }
    }
}

fn screen_to_canvas(rect: Rect, position: Pos2, document: &PaintDocument) -> PaintPoint {
    PaintPoint::new(
        (position.x - rect.min.x).clamp(0.0, document.canvas_size.width),
        (position.y - rect.min.y).clamp(0.0, document.canvas_size.height),
    )
}

fn canvas_to_screen(rect: Rect, point: PaintPoint) -> Pos2 {
    Pos2::new(rect.min.x + point.x, rect.min.y + point.y)
}
