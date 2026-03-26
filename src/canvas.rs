use eframe::egui::{
    self, Align2, Color32, FontId, Painter, PointerButton, Pos2, Rect, Sense, Stroke as EguiStroke,
    Vec2,
};

use crate::model::{CanvasSize, PaintDocument, PaintPoint, RgbaColor, Stroke, ToolKind};

const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 8.0;
const FIT_MARGIN: f32 = 24.0;

#[derive(Debug, Clone, Copy)]
pub struct ToolSettings {
    pub tool: ToolKind,
    pub color: RgbaColor,
    pub width: f32,
}

#[derive(Debug, Default, Clone)]
pub struct CanvasOutput {
    pub committed_stroke: Option<Stroke>,
    pub needs_repaint: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InteractionMode {
    Idle,
    Drawing,
    Panning(PanMode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanMode {
    SpaceDrag,
    MiddleDrag,
}

#[derive(Debug, Clone, Copy)]
struct CanvasViewState {
    zoom: f32,
    pan: Vec2,
    viewport: Option<Rect>,
    needs_reset: bool,
}

impl Default for CanvasViewState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
            viewport: None,
            needs_reset: true,
        }
    }
}

impl CanvasViewState {
    fn zoom_percent(&self) -> f32 {
        self.zoom * 100.0
    }

    fn remember_viewport(&mut self, viewport: Rect) {
        self.viewport = Some(viewport);
    }

    fn request_reset(&mut self) {
        self.needs_reset = true;
    }

    fn ensure_visible_defaults(&mut self, document: &PaintDocument) {
        if self.needs_reset
            && let Some(viewport) = self.viewport
        {
            self.reset_to_fit(viewport, document.canvas_size);
        }
    }

    fn reset_to_fit(&mut self, viewport: Rect, canvas_size: CanvasSize) {
        let available = Vec2::new(
            (viewport.width() - FIT_MARGIN * 2.0).max(1.0),
            (viewport.height() - FIT_MARGIN * 2.0).max(1.0),
        );
        let fit_scale = (available.x / canvas_size.width)
            .min(available.y / canvas_size.height)
            .clamp(MIN_ZOOM, MAX_ZOOM);

        self.zoom = fit_scale;
        self.pan = Vec2::ZERO;
        self.needs_reset = false;
    }

    fn zoom_around(&mut self, factor: f32, focus: Pos2, canvas_size: CanvasSize) -> bool {
        let Some(viewport) = self.viewport else {
            return false;
        };

        let old_zoom = self.zoom;
        let new_zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        if (new_zoom - old_zoom).abs() < f32::EPSILON {
            return false;
        }

        let world_focus =
            screen_to_canvas_unclamped(viewport, self.pan, old_zoom, canvas_size, focus);
        self.zoom = new_zoom;

        let new_canvas_rect = canvas_rect(viewport, self.pan, new_zoom, canvas_size);
        let projected_focus = canvas_to_screen(new_canvas_rect, new_zoom, world_focus);
        self.pan += focus - projected_focus;
        self.needs_reset = false;
        true
    }

    fn pan_by(&mut self, delta: Vec2) -> bool {
        if delta == Vec2::ZERO {
            return false;
        }

        self.pan += delta;
        self.needs_reset = false;
        true
    }
}

#[derive(Debug)]
pub struct CanvasController {
    active_stroke: Option<Stroke>,
    interaction_mode: InteractionMode,
    view: CanvasViewState,
}

impl Default for CanvasController {
    fn default() -> Self {
        Self {
            active_stroke: None,
            interaction_mode: InteractionMode::Idle,
            view: CanvasViewState::default(),
        }
    }
}

impl CanvasController {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        document: &PaintDocument,
        tool_settings: ToolSettings,
    ) -> CanvasOutput {
        let available = ui.available_size_before_wrap();
        let desired_size = Vec2::new(available.x.max(320.0), available.y.max(240.0));
        let (response, painter) = ui.allocate_painter(desired_size, Sense::click_and_drag());
        let viewport = response.rect;
        self.view.remember_viewport(viewport);
        self.view.ensure_visible_defaults(document);

        let cursor_icon = self.cursor_icon(ui);
        let response = response.on_hover_cursor(cursor_icon);
        let mut output = self.handle_input(ui, &response, document, tool_settings);

        paint_workspace(&painter, viewport);
        let canvas_rect = canvas_rect(
            viewport,
            self.view.pan,
            self.view.zoom,
            document.canvas_size,
        );

        paint_background(&painter, canvas_rect, document.background);
        paint_document(&painter, canvas_rect, self.view.zoom, document);

        if let Some(active_stroke) = &self.active_stroke {
            paint_stroke(
                &painter,
                canvas_rect,
                self.view.zoom,
                active_stroke,
                document.background,
            );
        }

        if !document.has_strokes() && self.active_stroke.is_none() {
            painter.text(
                canvas_rect.center(),
                Align2::CENTER_CENTER,
                "Drag to draw. Space + Drag or Middle Drag to pan.",
                FontId::proportional(22.0),
                Color32::from_gray(120),
            );
        }

        if matches!(
            self.interaction_mode,
            InteractionMode::Drawing | InteractionMode::Panning(_)
        ) {
            output.needs_repaint = true;
        }

        output
    }

    pub fn has_active_stroke(&self) -> bool {
        self.active_stroke.is_some()
    }

    pub fn discard_active_stroke(&mut self) -> bool {
        self.interaction_mode = InteractionMode::Idle;
        self.active_stroke.take().is_some()
    }

    pub fn zoom_label(&self) -> String {
        format!("{:.0}%", self.view.zoom_percent())
    }

    pub fn zoom_in(&mut self, canvas_size: CanvasSize) -> bool {
        self.zoom_by(1.2, canvas_size)
    }

    pub fn zoom_out(&mut self, canvas_size: CanvasSize) -> bool {
        self.zoom_by(1.0 / 1.2, canvas_size)
    }

    pub fn reset_view(&mut self, canvas_size: CanvasSize) -> bool {
        let Some(viewport) = self.view.viewport else {
            self.view.request_reset();
            return false;
        };

        self.view.reset_to_fit(viewport, canvas_size);
        true
    }

    pub fn request_view_reset(&mut self) {
        self.view.request_reset();
    }

    fn zoom_by(&mut self, factor: f32, canvas_size: CanvasSize) -> bool {
        let Some(viewport) = self.view.viewport else {
            return false;
        };

        self.view
            .zoom_around(factor, viewport.center(), canvas_size)
    }

    fn handle_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        document: &PaintDocument,
        tool_settings: ToolSettings,
    ) -> CanvasOutput {
        let pointer = ui.input(|input| input.pointer.clone());
        let viewport = response.rect;
        let hover_pos = pointer.hover_pos();
        let hovered = response.contains_pointer();
        let space_pan = ui.input(|input| input.key_down(egui::Key::Space));
        let mut output = CanvasOutput::default();

        if self.active_stroke.is_none() && hovered {
            let zoom_delta = ui.ctx().input(|input| input.zoom_delta());
            if zoom_delta != 1.0
                && let Some(pointer_pos) = hover_pos
                && self
                    .view
                    .zoom_around(zoom_delta, pointer_pos, document.canvas_size)
            {
                output.needs_repaint = true;
            }
        }

        match self.interaction_mode {
            InteractionMode::Idle => {
                if hovered && pointer.button_pressed(PointerButton::Middle) {
                    self.interaction_mode = InteractionMode::Panning(PanMode::MiddleDrag);
                    output.needs_repaint = true;
                    return output;
                }

                if hovered && space_pan && pointer.primary_pressed() {
                    self.interaction_mode = InteractionMode::Panning(PanMode::SpaceDrag);
                    output.needs_repaint = true;
                    return output;
                }

                if hovered && pointer.primary_pressed() {
                    let mut stroke = Stroke::new(
                        tool_settings.tool,
                        match tool_settings.tool {
                            ToolKind::Brush => tool_settings.color,
                            ToolKind::Eraser => document.background,
                        },
                        tool_settings.width,
                    );
                    if let Some(position) = pointer.interact_pos() {
                        stroke.push_point(screen_to_canvas(
                            viewport,
                            self.view.pan,
                            self.view.zoom,
                            document.canvas_size,
                            position,
                        ));
                        self.active_stroke = Some(stroke);
                        self.interaction_mode = InteractionMode::Drawing;
                        output.needs_repaint = true;
                    }
                }
            }
            InteractionMode::Drawing => {
                if pointer.primary_down()
                    && let Some(position) = pointer.interact_pos()
                    && let Some(stroke) = &mut self.active_stroke
                {
                    stroke.push_point(screen_to_canvas(
                        viewport,
                        self.view.pan,
                        self.view.zoom,
                        document.canvas_size,
                        position,
                    ));
                    output.needs_repaint = true;
                }

                if pointer.primary_released() {
                    output.committed_stroke = self.commit_active_stroke();
                    self.interaction_mode = InteractionMode::Idle;
                    output.needs_repaint = true;
                }
            }
            InteractionMode::Panning(mode) => {
                output.needs_repaint |= self.view.pan_by(pointer.delta());
                let still_active = match mode {
                    PanMode::SpaceDrag => pointer.primary_down() && space_pan,
                    PanMode::MiddleDrag => pointer.middle_down(),
                };

                if !still_active {
                    self.interaction_mode = InteractionMode::Idle;
                }
            }
        }
        output
    }

    fn commit_active_stroke(&mut self) -> Option<Stroke> {
        let stroke = self.active_stroke.take()?;

        if stroke.is_committable() {
            Some(stroke)
        } else {
            None
        }
    }

    fn cursor_icon(&self, ui: &egui::Ui) -> egui::CursorIcon {
        if matches!(self.interaction_mode, InteractionMode::Panning(_)) {
            egui::CursorIcon::Grabbing
        } else if ui.input(|input| input.key_down(egui::Key::Space)) {
            egui::CursorIcon::Grab
        } else {
            egui::CursorIcon::Crosshair
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

fn paint_workspace(painter: &Painter, viewport: Rect) {
    painter.rect_filled(viewport, 0.0, Color32::from_gray(235));
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

fn paint_document(painter: &Painter, rect: Rect, zoom: f32, document: &PaintDocument) {
    for stroke in &document.strokes {
        paint_stroke(painter, rect, zoom, stroke, document.background);
    }
}

fn paint_stroke(painter: &Painter, rect: Rect, zoom: f32, stroke: &Stroke, background: RgbaColor) {
    let color = match stroke.tool {
        ToolKind::Brush => color32_from_rgba(stroke.color),
        ToolKind::Eraser => color32_from_rgba(background),
    };

    match stroke.points.as_slice() {
        [] => {}
        [point] => {
            painter.circle_filled(
                canvas_to_screen(rect, zoom, *point),
                stroke.width * zoom * 0.5,
                color,
            );
        }
        points => {
            let line_points = points
                .iter()
                .copied()
                .map(|point| canvas_to_screen(rect, zoom, point))
                .collect();
            painter.add(egui::Shape::line(
                line_points,
                EguiStroke::new(stroke.width * zoom, color),
            ));
        }
    }
}

fn canvas_rect(viewport: Rect, pan: Vec2, zoom: f32, canvas_size: CanvasSize) -> Rect {
    let size = Vec2::new(canvas_size.width * zoom, canvas_size.height * zoom);
    Rect::from_center_size(viewport.center() + pan, size)
}

fn screen_to_canvas(
    viewport: Rect,
    pan: Vec2,
    zoom: f32,
    canvas_size: CanvasSize,
    position: Pos2,
) -> PaintPoint {
    let unclamped = screen_to_canvas_unclamped(viewport, pan, zoom, canvas_size, position);
    PaintPoint::new(
        unclamped.x.clamp(0.0, canvas_size.width),
        unclamped.y.clamp(0.0, canvas_size.height),
    )
}

fn screen_to_canvas_unclamped(
    viewport: Rect,
    pan: Vec2,
    zoom: f32,
    canvas_size: CanvasSize,
    position: Pos2,
) -> PaintPoint {
    let rect = canvas_rect(viewport, pan, zoom, canvas_size);
    PaintPoint::new(
        (position.x - rect.min.x) / zoom,
        (position.y - rect.min.y) / zoom,
    )
}

fn canvas_to_screen(rect: Rect, zoom: f32, point: PaintPoint) -> Pos2 {
    Pos2::new(rect.min.x + point.x * zoom, rect.min.y + point.y * zoom)
}

#[cfg(test)]
mod tests {
    use super::{CanvasViewState, canvas_rect, screen_to_canvas};
    use crate::model::{CanvasSize, PaintPoint};
    use eframe::egui::{Pos2, Rect, Vec2};

    #[test]
    fn reset_view_fits_canvas_inside_viewport() {
        let mut view = CanvasViewState::default();
        let viewport = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 600.0));
        let canvas_size = CanvasSize::new(1600.0, 900.0);
        view.remember_viewport(viewport);
        view.reset_to_fit(viewport, canvas_size);

        let rect = canvas_rect(viewport, view.pan, view.zoom, canvas_size);
        assert!(rect.width() <= viewport.width());
        assert!(rect.height() <= viewport.height());
        assert!(view.zoom < 1.0);
    }

    #[test]
    fn zoom_around_keeps_focus_world_position_stable() {
        let mut view = CanvasViewState::default();
        let viewport = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(1000.0, 800.0));
        let canvas_size = CanvasSize::new(1000.0, 800.0);
        let focus = Pos2::new(400.0, 300.0);
        view.remember_viewport(viewport);
        view.reset_to_fit(viewport, canvas_size);

        let before = screen_to_canvas(viewport, view.pan, view.zoom, canvas_size, focus);
        assert!(view.zoom_around(1.5, focus, canvas_size));
        let after = screen_to_canvas(viewport, view.pan, view.zoom, canvas_size, focus);

        assert!((before.x - after.x).abs() < 0.01);
        assert!((before.y - after.y).abs() < 0.01);
    }

    #[test]
    fn pan_changes_canvas_position_without_changing_world_coordinates() {
        let mut view = CanvasViewState::default();
        let viewport = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(400.0, 300.0));
        let canvas_size = CanvasSize::new(200.0, 100.0);
        view.remember_viewport(viewport);
        view.reset_to_fit(viewport, canvas_size);

        let before = screen_to_canvas(
            viewport,
            view.pan,
            view.zoom,
            canvas_size,
            Pos2::new(200.0, 150.0),
        );
        assert_eq!(before, PaintPoint::new(100.0, 50.0));

        assert!(view.pan_by(Vec2::new(40.0, -20.0)));
        let after = screen_to_canvas(
            viewport,
            view.pan,
            view.zoom,
            canvas_size,
            Pos2::new(240.0, 130.0),
        );

        assert!((after.x - 100.0).abs() < 0.01);
        assert!((after.y - 50.0).abs() < 0.01);
    }
}
