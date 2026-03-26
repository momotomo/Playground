use eframe::egui::{
    self, Align2, Color32, FontId, Painter, PointerButton, Pos2, Rect, Sense, Stroke as EguiStroke,
    Vec2,
};

use crate::model::{
    CanvasSize, ElementBounds, PaintDocument, PaintElement, PaintPoint, PaintVector, RgbaColor,
    ShapeElement, ShapeKind, Stroke, ToolKind,
};

const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 8.0;
const FIT_MARGIN: f32 = 24.0;
const HIT_TOLERANCE_SCREEN: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasToolKind {
    Select,
    Brush,
    Eraser,
    Rectangle,
    Ellipse,
    Line,
}

impl CanvasToolKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Brush => "Brush",
            Self::Eraser => "Eraser",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Line => "Line",
        }
    }

    fn shape_kind(self) -> Option<ShapeKind> {
        match self {
            Self::Rectangle => Some(ShapeKind::Rectangle),
            Self::Ellipse => Some(ShapeKind::Ellipse),
            Self::Line => Some(ShapeKind::Line),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ToolSettings {
    pub tool: CanvasToolKind,
    pub color: RgbaColor,
    pub width: f32,
}

#[derive(Debug, Default, Clone)]
pub struct CanvasOutput {
    pub committed_element: Option<PaintElement>,
    pub move_commit: Option<MoveCommit>,
    pub needs_repaint: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MoveCommit {
    pub index: usize,
    pub delta: PaintVector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InteractionMode {
    Idle,
    Drawing,
    Panning(PanMode),
    MovingSelection,
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

#[derive(Debug, Default, Clone, Copy)]
struct SelectionState {
    selected_index: Option<usize>,
    drag_origin: Option<PaintPoint>,
    preview_delta: PaintVector,
}

impl SelectionState {
    fn clear(&mut self) {
        self.selected_index = None;
        self.drag_origin = None;
        self.preview_delta = PaintVector::default();
    }

    fn is_valid_for(&self, document: &PaintDocument) -> bool {
        self.selected_index
            .and_then(|index| document.element(index))
            .is_some()
    }
}

#[derive(Debug, Clone)]
enum ActivePreview {
    Stroke(Stroke),
    Shape(ShapeElement),
}

#[derive(Debug)]
pub struct CanvasController {
    active_preview: Option<ActivePreview>,
    interaction_mode: InteractionMode,
    view: CanvasViewState,
    selection: SelectionState,
}

impl Default for CanvasController {
    fn default() -> Self {
        Self {
            active_preview: None,
            interaction_mode: InteractionMode::Idle,
            view: CanvasViewState::default(),
            selection: SelectionState::default(),
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
        self.sync_with_document(document);

        let available = ui.available_size_before_wrap();
        let desired_size = Vec2::new(available.x.max(320.0), available.y.max(240.0));
        let (response, painter) = ui.allocate_painter(desired_size, Sense::click_and_drag());
        let viewport = response.rect;
        self.view.remember_viewport(viewport);
        self.view.ensure_visible_defaults(document);

        let cursor_icon = self.cursor_icon(ui, tool_settings.tool);
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
        paint_document(
            &painter,
            canvas_rect,
            self.view.zoom,
            document,
            self.selection_overlay_element(document),
        );

        if let Some(preview) = &self.active_preview {
            paint_preview(
                &painter,
                canvas_rect,
                self.view.zoom,
                preview,
                document.background,
            );
        }

        if let Some(bounds) = self.selected_bounds(document) {
            paint_selection_overlay(&painter, canvas_rect, self.view.zoom, bounds);
        }

        if !document.has_elements() && self.active_preview.is_none() {
            painter.text(
                canvas_rect.center(),
                Align2::CENTER_CENTER,
                "Select, draw, or drag shapes. Space + Drag pans the view.",
                FontId::proportional(22.0),
                Color32::from_gray(120),
            );
        }

        if matches!(
            self.interaction_mode,
            InteractionMode::Drawing
                | InteractionMode::Panning(_)
                | InteractionMode::MovingSelection
        ) {
            output.needs_repaint = true;
        }

        output
    }

    pub fn has_active_stroke(&self) -> bool {
        self.active_preview.is_some()
    }

    pub fn discard_active_stroke(&mut self) -> bool {
        self.interaction_mode = InteractionMode::Idle;
        self.active_preview.take().is_some()
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    pub fn sync_with_document(&mut self, document: &PaintDocument) {
        if !self.selection.is_valid_for(document) {
            self.selection.clear();
        }
    }

    pub fn selection_summary(&self, document: &PaintDocument) -> String {
        if let Some(index) = self.selection.selected_index
            && let Some(element) = document.element(index)
        {
            return format!("Selection: {} #{}", element.kind_label(), index + 1);
        }

        "Selection: None".to_owned()
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

        if self.active_preview.is_none()
            && !matches!(self.interaction_mode, InteractionMode::MovingSelection)
            && hovered
        {
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

                if hovered
                    && pointer.primary_pressed()
                    && let Some(position) = pointer.interact_pos()
                {
                    let world = screen_to_canvas(
                        viewport,
                        self.view.pan,
                        self.view.zoom,
                        document.canvas_size,
                        position,
                    );

                    match tool_settings.tool {
                        CanvasToolKind::Select => {
                            self.begin_selection_drag(document, world);
                            output.needs_repaint = true;
                        }
                        CanvasToolKind::Brush | CanvasToolKind::Eraser => {
                            self.begin_stroke_preview(document, tool_settings, world);
                            output.needs_repaint = true;
                        }
                        CanvasToolKind::Rectangle
                        | CanvasToolKind::Ellipse
                        | CanvasToolKind::Line => {
                            self.begin_shape_preview(tool_settings, world);
                            output.needs_repaint = true;
                        }
                    }
                }
            }
            InteractionMode::Drawing => {
                if pointer.primary_down()
                    && let Some(position) = pointer.interact_pos()
                {
                    let world = screen_to_canvas(
                        viewport,
                        self.view.pan,
                        self.view.zoom,
                        document.canvas_size,
                        position,
                    );
                    self.update_active_preview(world);
                    output.needs_repaint = true;
                }

                if pointer.primary_released() {
                    output.committed_element = self.commit_active_preview();
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
            InteractionMode::MovingSelection => {
                if pointer.primary_down()
                    && let Some(position) = pointer.interact_pos()
                {
                    let world = screen_to_canvas_unclamped(
                        viewport,
                        self.view.pan,
                        self.view.zoom,
                        document.canvas_size,
                        position,
                    );
                    if let Some(origin) = self.selection.drag_origin {
                        self.selection.preview_delta =
                            PaintVector::new(world.x - origin.x, world.y - origin.y);
                    }
                    output.needs_repaint = true;
                }

                if pointer.primary_released() {
                    output.move_commit = self.finish_move_commit();
                    self.interaction_mode = InteractionMode::Idle;
                    output.needs_repaint = true;
                }
            }
        }

        output
    }

    fn begin_selection_drag(&mut self, document: &PaintDocument, world: PaintPoint) {
        let tolerance = HIT_TOLERANCE_SCREEN / self.view.zoom.max(MIN_ZOOM);
        if let Some(index) = document.hit_test(world, tolerance) {
            self.selection.selected_index = Some(index);
            self.selection.drag_origin = Some(world);
            self.selection.preview_delta = PaintVector::default();
            self.interaction_mode = InteractionMode::MovingSelection;
        } else {
            self.selection.clear();
            self.interaction_mode = InteractionMode::Idle;
        }
    }

    fn begin_stroke_preview(
        &mut self,
        document: &PaintDocument,
        tool_settings: ToolSettings,
        start: PaintPoint,
    ) {
        self.selection.clear();
        let color = match tool_settings.tool {
            CanvasToolKind::Brush => tool_settings.color,
            CanvasToolKind::Eraser => document.background,
            _ => tool_settings.color,
        };
        let tool = match tool_settings.tool {
            CanvasToolKind::Eraser => ToolKind::Eraser,
            _ => ToolKind::Brush,
        };

        let mut stroke = Stroke::new(tool, color, tool_settings.width);
        stroke.push_point(start);
        self.active_preview = Some(ActivePreview::Stroke(stroke));
        self.interaction_mode = InteractionMode::Drawing;
    }

    fn begin_shape_preview(&mut self, tool_settings: ToolSettings, start: PaintPoint) {
        self.selection.clear();
        let Some(kind) = tool_settings.tool.shape_kind() else {
            return;
        };

        self.active_preview = Some(ActivePreview::Shape(ShapeElement::new(
            kind,
            tool_settings.color,
            tool_settings.width,
            start,
            start,
        )));
        self.interaction_mode = InteractionMode::Drawing;
    }

    fn update_active_preview(&mut self, world: PaintPoint) {
        match self.active_preview.as_mut() {
            Some(ActivePreview::Stroke(stroke)) => stroke.push_point(world),
            Some(ActivePreview::Shape(shape)) => shape.end = world,
            None => {}
        }
    }

    fn commit_active_preview(&mut self) -> Option<PaintElement> {
        let preview = self.active_preview.take()?;
        match preview {
            ActivePreview::Stroke(stroke) => {
                stroke_is_committable(&stroke).then_some(PaintElement::Stroke(stroke))
            }
            ActivePreview::Shape(shape) => shape_is_committable(shape).map(PaintElement::Shape),
        }
    }

    fn finish_move_commit(&mut self) -> Option<MoveCommit> {
        let index = self.selection.selected_index?;
        let delta = self.selection.preview_delta;
        self.selection.drag_origin = None;
        self.selection.preview_delta = PaintVector::default();

        (!delta.is_zero()).then_some(MoveCommit { index, delta })
    }

    fn selection_overlay_element(&self, document: &PaintDocument) -> Option<(usize, PaintElement)> {
        let index = self.selection.selected_index?;
        let element = document.element(index)?.clone();

        if self.selection.preview_delta.is_zero() {
            None
        } else {
            Some((index, element.translated(self.selection.preview_delta)))
        }
    }

    fn selected_bounds(&self, document: &PaintDocument) -> Option<ElementBounds> {
        let index = self.selection.selected_index?;
        let element = document.element(index)?;
        let element = if self.selection.preview_delta.is_zero() {
            element.clone()
        } else {
            element.translated(self.selection.preview_delta)
        };
        element.bounds()
    }

    fn cursor_icon(&self, ui: &egui::Ui, tool: CanvasToolKind) -> egui::CursorIcon {
        if matches!(
            self.interaction_mode,
            InteractionMode::Panning(_) | InteractionMode::MovingSelection
        ) {
            egui::CursorIcon::Grabbing
        } else if ui.input(|input| input.key_down(egui::Key::Space)) {
            egui::CursorIcon::Grab
        } else if tool == CanvasToolKind::Select {
            egui::CursorIcon::PointingHand
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

fn paint_document(
    painter: &Painter,
    rect: Rect,
    zoom: f32,
    document: &PaintDocument,
    overlay: Option<(usize, PaintElement)>,
) {
    for (index, element) in document.elements.iter().enumerate() {
        if overlay
            .as_ref()
            .is_some_and(|(overlay_index, _)| *overlay_index == index)
        {
            continue;
        }
        paint_element(painter, rect, zoom, element, document.background);
    }

    if let Some((_, element)) = overlay {
        paint_element(painter, rect, zoom, &element, document.background);
    }
}

fn paint_preview(
    painter: &Painter,
    rect: Rect,
    zoom: f32,
    preview: &ActivePreview,
    background: RgbaColor,
) {
    match preview {
        ActivePreview::Stroke(stroke) => paint_element(
            painter,
            rect,
            zoom,
            &PaintElement::Stroke(stroke.clone()),
            background,
        ),
        ActivePreview::Shape(shape) => paint_element(
            painter,
            rect,
            zoom,
            &PaintElement::Shape(*shape),
            background,
        ),
    }
}

fn paint_element(
    painter: &Painter,
    rect: Rect,
    zoom: f32,
    element: &PaintElement,
    background: RgbaColor,
) {
    match element {
        PaintElement::Stroke(stroke) => paint_stroke(painter, rect, zoom, stroke, background),
        PaintElement::Shape(shape) => paint_shape(painter, rect, zoom, shape),
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

fn paint_shape(painter: &Painter, rect: Rect, zoom: f32, shape: &ShapeElement) {
    let stroke = EguiStroke::new(shape.width * zoom, color32_from_rgba(shape.color));
    let start = canvas_to_screen(rect, zoom, shape.start);
    let end = canvas_to_screen(rect, zoom, shape.end);

    match shape.kind {
        ShapeKind::Line => {
            painter.line_segment([start, end], stroke);
        }
        ShapeKind::Rectangle => {
            painter.rect_stroke(
                Rect::from_two_pos(start, end),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        ShapeKind::Ellipse => {
            let ellipse_points = ellipse_outline_points(Rect::from_two_pos(start, end));
            if ellipse_points.len() >= 2 {
                painter.add(egui::Shape::closed_line(ellipse_points, stroke));
            }
        }
    }
}

fn paint_selection_overlay(painter: &Painter, rect: Rect, zoom: f32, bounds: ElementBounds) {
    let screen_rect = Rect::from_two_pos(
        canvas_to_screen(rect, zoom, bounds.min),
        canvas_to_screen(rect, zoom, bounds.max),
    )
    .expand(6.0);

    let accent = Color32::from_rgb(26, 115, 232);
    painter.rect_stroke(
        screen_rect,
        6.0,
        EguiStroke::new(2.0, accent),
        egui::StrokeKind::Outside,
    );

    for handle in [
        screen_rect.left_top(),
        screen_rect.right_top(),
        screen_rect.left_bottom(),
        screen_rect.right_bottom(),
    ] {
        painter.rect_filled(
            Rect::from_center_size(handle, Vec2::splat(8.0)),
            2.0,
            Color32::WHITE,
        );
        painter.rect_stroke(
            Rect::from_center_size(handle, Vec2::splat(8.0)),
            2.0,
            EguiStroke::new(1.0, accent),
            egui::StrokeKind::Outside,
        );
    }
}

fn ellipse_outline_points(rect: Rect) -> Vec<Pos2> {
    let center = rect.center();
    let rx = rect.width().abs() * 0.5;
    let ry = rect.height().abs() * 0.5;
    if rx <= f32::EPSILON || ry <= f32::EPSILON {
        return Vec::new();
    }

    (0..48)
        .map(|step| {
            let t = step as f32 / 48.0 * std::f32::consts::TAU;
            Pos2::new(center.x + rx * t.cos(), center.y + ry * t.sin())
        })
        .collect()
}

fn stroke_is_committable(stroke: &Stroke) -> bool {
    !stroke.points.is_empty()
}

fn shape_is_committable(shape: ShapeElement) -> Option<ShapeElement> {
    let dx = (shape.end.x - shape.start.x).abs();
    let dy = (shape.end.y - shape.start.y).abs();
    match shape.kind {
        ShapeKind::Line if dx.max(dy) > 0.5 => Some(shape),
        ShapeKind::Rectangle | ShapeKind::Ellipse if dx > 0.5 && dy > 0.5 => Some(shape),
        _ => None,
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

        assert!((before.x - after.x).abs() < 0.01);
        assert!((before.y - after.y).abs() < 0.01);
    }
}
