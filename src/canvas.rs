use eframe::egui::{
    self, Align2, Color32, FontId, Painter, PointerButton, Pos2, Rect, Sense, Stroke as EguiStroke,
    Vec2,
};

use crate::model::{
    AlignmentKind, CanvasSize, ElementBounds, PaintDocument, PaintElement, PaintPoint, PaintVector,
    RgbaColor, ShapeElement, ShapeHandle, ShapeKind, StackOrderCommand, Stroke, ToolKind,
};

const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 8.0;
const FIT_MARGIN: f32 = 24.0;
const HIT_TOLERANCE_SCREEN: f32 = 8.0;
const HANDLE_RADIUS: f32 = 6.5;
const HANDLE_HIT_RADIUS: f32 = 12.0;
const ROTATION_HANDLE_OFFSET_SCREEN: f32 = 28.0;

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
    pub committed_edit: Option<CommittedDocumentEdit>,
    pub needs_repaint: bool,
}

#[derive(Debug, Clone)]
pub struct CommittedDocumentEdit {
    pub document: PaintDocument,
    pub selection_indices: Vec<usize>,
    pub mode: DocumentEditMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentEditMode {
    Move,
    Resize,
    Rotate,
    Align(AlignmentKind),
    Reorder(StackOrderCommand),
}

impl DocumentEditMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Move => "moving",
            Self::Resize => "resizing",
            Self::Rotate => "rotating",
            Self::Align(_) => "aligning",
            Self::Reorder(_) => "reordering",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InteractionMode {
    Idle,
    Drawing,
    Panning(PanMode),
    EditingSelection,
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

#[derive(Debug, Default, Clone)]
struct SelectionState {
    indices: Vec<usize>,
}

impl SelectionState {
    fn clear(&mut self) {
        self.indices.clear();
    }

    fn len(&self) -> usize {
        self.indices.len()
    }

    fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    fn contains(&self, index: usize) -> bool {
        self.indices.contains(&index)
    }

    fn single(&self) -> Option<usize> {
        (self.indices.len() == 1).then_some(self.indices[0])
    }

    fn indices(&self) -> &[usize] {
        &self.indices
    }

    fn set_only(&mut self, index: usize) {
        self.indices.clear();
        self.indices.push(index);
    }

    fn toggle(&mut self, index: usize) {
        if let Some(position) = self
            .indices
            .iter()
            .position(|candidate| *candidate == index)
        {
            self.indices.remove(position);
        } else {
            self.indices.push(index);
            self.indices.sort_unstable();
        }
    }

    fn set_indices(&mut self, mut indices: Vec<usize>) {
        normalize_selection_indices(&mut indices);
        self.indices = indices;
    }

    fn retain_valid(&mut self, document: &PaintDocument) {
        self.indices
            .retain(|index| *index < document.element_count());
        normalize_selection_indices(&mut self.indices);
    }
}

#[derive(Debug, Clone)]
enum ActivePreview {
    Stroke(Stroke),
    Shape(ShapeElement),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlTarget {
    Resize(ShapeHandle),
    Rotate,
}

#[derive(Debug, Clone)]
enum SelectionSession {
    Move {
        indices: Vec<usize>,
        base_elements: Vec<(usize, PaintElement)>,
        drag_origin: PaintPoint,
        preview_delta: PaintVector,
    },
    Resize {
        index: usize,
        base_shape: ShapeElement,
        handle: ShapeHandle,
        preview_shape: ShapeElement,
    },
    Rotate {
        index: usize,
        base_shape: ShapeElement,
        start_pointer_angle: f32,
        preview_shape: ShapeElement,
    },
}

impl SelectionSession {
    fn mode(&self) -> DocumentEditMode {
        match self {
            Self::Move { .. } => DocumentEditMode::Move,
            Self::Resize { .. } => DocumentEditMode::Resize,
            Self::Rotate { .. } => DocumentEditMode::Rotate,
        }
    }

    fn control_target(&self) -> Option<ControlTarget> {
        match self {
            Self::Resize { handle, .. } => Some(ControlTarget::Resize(*handle)),
            Self::Rotate { .. } => Some(ControlTarget::Rotate),
            Self::Move { .. } => None,
        }
    }

    fn preview_elements(&self) -> Vec<(usize, PaintElement)> {
        match self {
            Self::Move {
                base_elements,
                preview_delta,
                ..
            } => base_elements
                .iter()
                .map(|(index, element)| (*index, element.translated(*preview_delta)))
                .collect(),
            Self::Resize {
                index,
                preview_shape,
                ..
            }
            | Self::Rotate {
                index,
                preview_shape,
                ..
            } => vec![(*index, PaintElement::Shape(*preview_shape))],
        }
    }

    fn is_valid_for(&self, document: &PaintDocument) -> bool {
        match self {
            Self::Move { indices, .. } => indices
                .iter()
                .all(|index| *index < document.element_count()),
            Self::Resize { index, .. } | Self::Rotate { index, .. } => {
                matches!(document.element(*index), Some(PaintElement::Shape(_)))
            }
        }
    }

    fn finish(self, document: &PaintDocument) -> Option<CommittedDocumentEdit> {
        match self {
            Self::Move {
                indices,
                base_elements,
                preview_delta,
                ..
            } => {
                if preview_delta.is_zero() {
                    return None;
                }

                let mut next = document.clone();
                for (index, base_element) in base_elements {
                    let updated = base_element.translated(preview_delta);
                    if !next.replace_element(index, updated) {
                        return None;
                    }
                }

                Some(CommittedDocumentEdit {
                    document: next,
                    selection_indices: indices,
                    mode: DocumentEditMode::Move,
                })
            }
            Self::Resize {
                index,
                base_shape,
                preview_shape,
                ..
            } => {
                if preview_shape == base_shape {
                    return None;
                }

                let mut next = document.clone();
                if !next.replace_element(index, PaintElement::Shape(preview_shape)) {
                    return None;
                }

                Some(CommittedDocumentEdit {
                    document: next,
                    selection_indices: vec![index],
                    mode: DocumentEditMode::Resize,
                })
            }
            Self::Rotate {
                index,
                base_shape,
                preview_shape,
                ..
            } => {
                if preview_shape == base_shape {
                    return None;
                }

                let mut next = document.clone();
                if !next.replace_element(index, PaintElement::Shape(preview_shape)) {
                    return None;
                }

                Some(CommittedDocumentEdit {
                    document: next,
                    selection_indices: vec![index],
                    mode: DocumentEditMode::Rotate,
                })
            }
        }
    }
}

#[derive(Debug)]
pub struct CanvasController {
    active_preview: Option<ActivePreview>,
    selection_session: Option<SelectionSession>,
    interaction_mode: InteractionMode,
    view: CanvasViewState,
    selection: SelectionState,
}

impl Default for CanvasController {
    fn default() -> Self {
        Self {
            active_preview: None,
            selection_session: None,
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

        let canvas_rect = canvas_rect(
            viewport,
            self.view.pan,
            self.view.zoom,
            document.canvas_size,
        );
        let cursor_icon = self.cursor_icon(ui, document, canvas_rect, tool_settings.tool);
        let response = response.on_hover_cursor(cursor_icon);
        let mut output = self.handle_input(ui, &response, document, canvas_rect, tool_settings);

        let preview_overlays = self.preview_overlay_elements();
        let selected_visual = self.selected_visual_elements(document);

        paint_workspace(&painter, viewport);
        paint_background(&painter, canvas_rect, document.background);
        paint_document(
            &painter,
            canvas_rect,
            self.view.zoom,
            document,
            &preview_overlays,
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

        if !selected_visual.is_empty() {
            let active_control = self
                .selection_session
                .as_ref()
                .and_then(SelectionSession::control_target);
            paint_selection_overlay(
                &painter,
                canvas_rect,
                self.view.zoom,
                &selected_visual,
                active_control,
            );
        }

        if !document.has_elements() && self.active_preview.is_none() {
            painter.text(
                canvas_rect.center(),
                Align2::CENTER_CENTER,
                "Select, Shift+Click to add, then move or arrange elements.",
                FontId::proportional(22.0),
                Color32::from_gray(120),
            );
        }

        if matches!(
            self.interaction_mode,
            InteractionMode::Drawing
                | InteractionMode::Panning(_)
                | InteractionMode::EditingSelection
        ) {
            output.needs_repaint = true;
        }

        output
    }

    pub fn has_active_interaction(&self) -> bool {
        self.active_preview.is_some() || self.selection_session.is_some()
    }

    pub fn discard_active_interaction(&mut self) -> bool {
        let discarded =
            self.active_preview.take().is_some() || self.selection_session.take().is_some();
        self.interaction_mode = InteractionMode::Idle;
        discarded
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
        self.selection_session = None;
        if matches!(self.interaction_mode, InteractionMode::EditingSelection) {
            self.interaction_mode = InteractionMode::Idle;
        }
    }

    pub fn set_selection_indices(&mut self, indices: Vec<usize>) {
        self.selection.set_indices(indices);
    }

    pub fn selection_count(&self) -> usize {
        self.selection.len()
    }

    pub fn sync_with_document(&mut self, document: &PaintDocument) {
        self.selection.retain_valid(document);
        if self
            .selection_session
            .as_ref()
            .is_some_and(|session| !session.is_valid_for(document))
        {
            self.selection_session = None;
            if matches!(self.interaction_mode, InteractionMode::EditingSelection) {
                self.interaction_mode = InteractionMode::Idle;
            }
        }
    }

    pub fn selection_summary(&self, document: &PaintDocument) -> String {
        if self.selection.is_empty() {
            return "Selection: None".to_owned();
        }

        if let Some(session) = &self.selection_session {
            if self.selection.len() == 1
                && let Some(index) = self.selection.single()
                && let Some(element) = document.element(index)
            {
                return format!(
                    "Selection: {} #{} ({})",
                    element.kind_label(),
                    index + 1,
                    session.mode().label()
                );
            }

            return format!(
                "Selection: {} elements ({})",
                self.selection.len(),
                session.mode().label()
            );
        }

        if let Some(index) = self.selection.single()
            && let Some(element) = document.element(index)
        {
            let capability = match element {
                PaintElement::Stroke(_) => "move only",
                PaintElement::Shape(_) => "move / resize / rotate",
            };
            return format!(
                "Selection: {} #{} ({capability})",
                element.kind_label(),
                index + 1
            );
        }

        format!(
            "Selection: {} elements (move / align / order)",
            self.selection.len()
        )
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

    pub fn apply_alignment(
        &mut self,
        document: &PaintDocument,
        alignment: AlignmentKind,
    ) -> Option<CommittedDocumentEdit> {
        if self.has_active_interaction() || self.selection.len() < 2 {
            return None;
        }

        let next = document.aligned_document(self.selection.indices(), alignment)?;
        Some(CommittedDocumentEdit {
            document: next,
            selection_indices: self.selection.indices().to_vec(),
            mode: DocumentEditMode::Align(alignment),
        })
    }

    pub fn apply_stack_order(
        &mut self,
        document: &PaintDocument,
        command: StackOrderCommand,
    ) -> Option<CommittedDocumentEdit> {
        if self.has_active_interaction() || self.selection.is_empty() {
            return None;
        }

        let (next, selection_indices) =
            document.reordered_document(self.selection.indices(), command)?;
        self.selection.set_indices(selection_indices.clone());
        Some(CommittedDocumentEdit {
            document: next,
            selection_indices,
            mode: DocumentEditMode::Reorder(command),
        })
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
        canvas_rect: Rect,
        tool_settings: ToolSettings,
    ) -> CanvasOutput {
        let pointer = ui.input(|input| input.pointer.clone());
        let viewport = response.rect;
        let hover_pos = pointer.hover_pos();
        let hovered = response.contains_pointer();
        let space_pan = ui.input(|input| input.key_down(egui::Key::Space));
        let shift_selection = ui.input(|input| input.modifiers.shift);
        let mut output = CanvasOutput::default();

        if self.active_preview.is_none() && self.selection_session.is_none() && hovered {
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
                            self.begin_selection_interaction(
                                document,
                                canvas_rect,
                                self.view.zoom,
                                position,
                                world,
                                shift_selection,
                            );
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
            InteractionMode::EditingSelection => {
                if pointer.primary_down()
                    && let Some(position) = pointer.interact_pos()
                {
                    self.update_selection_session(
                        viewport,
                        document.canvas_size,
                        canvas_rect,
                        position,
                    );
                    output.needs_repaint = true;
                }

                if pointer.primary_released() {
                    output.committed_edit = self.finish_selection_session(document);
                    self.interaction_mode = InteractionMode::Idle;
                    output.needs_repaint = true;
                }
            }
        }

        output
    }

    fn begin_selection_interaction(
        &mut self,
        document: &PaintDocument,
        canvas_rect: Rect,
        zoom: f32,
        pointer_screen: Pos2,
        pointer_world: PaintPoint,
        extend_selection: bool,
    ) {
        if !extend_selection
            && let Some(control) =
                self.hit_selection_control(document, canvas_rect, zoom, pointer_screen)
            && let Some(index) = self.selection.single()
            && let Some(PaintElement::Shape(shape)) = self.single_selected_element_owned(document)
        {
            self.selection_session = match control {
                ControlTarget::Resize(handle) => Some(SelectionSession::Resize {
                    index,
                    base_shape: shape,
                    handle,
                    preview_shape: shape,
                }),
                ControlTarget::Rotate => Some(SelectionSession::Rotate {
                    index,
                    base_shape: shape,
                    start_pointer_angle: pointer_screen_angle(
                        canvas_rect,
                        zoom,
                        shape.rotation_center(),
                        pointer_screen,
                    ),
                    preview_shape: shape,
                }),
            };
            self.interaction_mode = InteractionMode::EditingSelection;
            return;
        }

        let tolerance = HIT_TOLERANCE_SCREEN / self.view.zoom.max(MIN_ZOOM);
        if let Some(index) = document.hit_test(pointer_world, tolerance) {
            if extend_selection {
                self.selection.toggle(index);
                self.selection_session = None;
                self.interaction_mode = InteractionMode::Idle;
                return;
            }

            if !self.selection.contains(index) {
                self.selection.set_only(index);
            }
            self.begin_move_session(document, pointer_world);
        } else if !extend_selection {
            self.clear_selection();
            self.interaction_mode = InteractionMode::Idle;
        }
    }

    fn begin_move_session(&mut self, document: &PaintDocument, pointer_world: PaintPoint) {
        let indices = self.selection.indices().to_vec();
        let base_elements: Vec<_> = indices
            .iter()
            .filter_map(|index| {
                document
                    .element(*index)
                    .cloned()
                    .map(|element| (*index, element))
            })
            .collect();
        if base_elements.is_empty() {
            return;
        }

        self.selection_session = Some(SelectionSession::Move {
            indices,
            base_elements,
            drag_origin: pointer_world,
            preview_delta: PaintVector::default(),
        });
        self.interaction_mode = InteractionMode::EditingSelection;
    }

    fn begin_stroke_preview(
        &mut self,
        document: &PaintDocument,
        tool_settings: ToolSettings,
        start: PaintPoint,
    ) {
        self.clear_selection();
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
        self.clear_selection();
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

    fn update_selection_session(
        &mut self,
        viewport: Rect,
        canvas_size: CanvasSize,
        canvas_rect: Rect,
        pointer_screen: Pos2,
    ) {
        let pointer_world = screen_to_canvas_unclamped(
            viewport,
            self.view.pan,
            self.view.zoom,
            canvas_size,
            pointer_screen,
        );

        match self.selection_session.as_mut() {
            Some(SelectionSession::Move {
                drag_origin,
                preview_delta,
                ..
            }) => {
                *preview_delta = PaintVector::new(
                    pointer_world.x - drag_origin.x,
                    pointer_world.y - drag_origin.y,
                );
            }
            Some(SelectionSession::Resize {
                base_shape,
                handle,
                preview_shape,
                ..
            }) => {
                if let Some(next) = base_shape.resized_by_handle(*handle, pointer_world) {
                    *preview_shape = next;
                }
            }
            Some(SelectionSession::Rotate {
                base_shape,
                start_pointer_angle,
                preview_shape,
                ..
            }) => {
                let current_angle = pointer_screen_angle(
                    canvas_rect,
                    self.view.zoom,
                    base_shape.rotation_center(),
                    pointer_screen,
                );
                *preview_shape = base_shape.rotated_by(current_angle - *start_pointer_angle);
            }
            None => {}
        }
    }

    fn finish_selection_session(
        &mut self,
        document: &PaintDocument,
    ) -> Option<CommittedDocumentEdit> {
        let session = self.selection_session.take()?;
        let edit = session.finish(document);
        if let Some(committed) = &edit {
            self.selection
                .set_indices(committed.selection_indices.clone());
        }
        edit
    }

    fn preview_overlay_elements(&self) -> Vec<(usize, PaintElement)> {
        self.selection_session
            .as_ref()
            .map(SelectionSession::preview_elements)
            .unwrap_or_default()
    }

    fn selected_visual_elements(&self, document: &PaintDocument) -> Vec<(usize, PaintElement)> {
        if let Some(session) = &self.selection_session {
            return session.preview_elements();
        }

        self.selection
            .indices()
            .iter()
            .filter_map(|index| {
                document
                    .element(*index)
                    .cloned()
                    .map(|element| (*index, element))
            })
            .collect()
    }

    fn single_selected_element_owned(&self, document: &PaintDocument) -> Option<PaintElement> {
        let index = self.selection.single()?;
        if let Some(session) = &self.selection_session {
            return session
                .preview_elements()
                .into_iter()
                .find_map(|(candidate, element)| (candidate == index).then_some(element));
        }

        document.element(index).cloned()
    }

    fn hit_selection_control(
        &self,
        document: &PaintDocument,
        canvas_rect: Rect,
        zoom: f32,
        pointer_screen: Pos2,
    ) -> Option<ControlTarget> {
        let Some(PaintElement::Shape(shape)) = self.single_selected_element_owned(document) else {
            return None;
        };

        for (handle, handle_screen) in shape_control_handles_screen(shape, canvas_rect, zoom) {
            if handle_screen.distance(pointer_screen) <= HANDLE_HIT_RADIUS {
                return Some(ControlTarget::Resize(handle));
            }
        }

        let rotation_handle = shape_rotation_handle_screen(shape, canvas_rect, zoom)?;
        (rotation_handle.distance(pointer_screen) <= HANDLE_HIT_RADIUS)
            .then_some(ControlTarget::Rotate)
    }

    fn cursor_icon(
        &self,
        ui: &egui::Ui,
        document: &PaintDocument,
        canvas_rect: Rect,
        tool: CanvasToolKind,
    ) -> egui::CursorIcon {
        if matches!(self.interaction_mode, InteractionMode::Panning(_)) {
            egui::CursorIcon::Grabbing
        } else if matches!(self.interaction_mode, InteractionMode::EditingSelection) {
            match self.selection_session.as_ref().map(SelectionSession::mode) {
                Some(DocumentEditMode::Resize) => egui::CursorIcon::ResizeNwSe,
                Some(DocumentEditMode::Rotate) => egui::CursorIcon::Crosshair,
                Some(DocumentEditMode::Move)
                | Some(DocumentEditMode::Align(_))
                | Some(DocumentEditMode::Reorder(_))
                | None => egui::CursorIcon::Grabbing,
            }
        } else if ui.input(|input| input.key_down(egui::Key::Space)) {
            egui::CursorIcon::Grab
        } else if tool == CanvasToolKind::Select {
            if let Some(pointer) = ui.input(|input| input.pointer.hover_pos())
                && self
                    .hit_selection_control(document, canvas_rect, self.view.zoom, pointer)
                    .is_some()
            {
                egui::CursorIcon::PointingHand
            } else {
                egui::CursorIcon::PointingHand
            }
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
    overlays: &[(usize, PaintElement)],
) {
    for (index, element) in document.elements.iter().enumerate() {
        if overlays
            .iter()
            .any(|(overlay_index, _)| *overlay_index == index)
        {
            continue;
        }
        paint_element(painter, rect, zoom, element, document.background);
    }

    for (_, element) in overlays {
        paint_element(painter, rect, zoom, element, document.background);
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

    match shape.kind {
        ShapeKind::Line => {
            painter.line_segment(
                [
                    canvas_to_screen(rect, zoom, shape.start),
                    canvas_to_screen(rect, zoom, shape.end),
                ],
                stroke,
            );
        }
        ShapeKind::Rectangle => {
            let points: Vec<Pos2> = shape
                .rotated_box_corners()
                .into_iter()
                .map(|point| canvas_to_screen(rect, zoom, point))
                .collect();
            painter.add(egui::Shape::closed_line(points, stroke));
        }
        ShapeKind::Ellipse => {
            let ellipse_points = ellipse_outline_points(shape, rect, zoom);
            if ellipse_points.len() >= 2 {
                painter.add(egui::Shape::closed_line(ellipse_points, stroke));
            }
        }
    }
}

fn paint_selection_overlay(
    painter: &Painter,
    rect: Rect,
    zoom: f32,
    selected_elements: &[(usize, PaintElement)],
    active_control: Option<ControlTarget>,
) {
    if selected_elements.len() == 1 {
        paint_single_selection_overlay(
            painter,
            rect,
            zoom,
            selected_elements[0].1.clone(),
            active_control,
        );
    } else {
        paint_multi_selection_overlay(painter, rect, zoom, selected_elements);
    }
}

fn paint_single_selection_overlay(
    painter: &Painter,
    rect: Rect,
    zoom: f32,
    element: PaintElement,
    active_control: Option<ControlTarget>,
) {
    let accent = Color32::from_rgb(26, 115, 232);
    let inactive_fill = Color32::WHITE;
    let active_fill = Color32::from_rgb(26, 115, 232);

    match element {
        PaintElement::Stroke(stroke) => {
            if let Some(bounds) = stroke.bounds() {
                paint_axis_aligned_bounds(painter, rect, zoom, bounds, accent, 6.0, 2.0);
            }
        }
        PaintElement::Shape(shape) => match shape.kind {
            ShapeKind::Line => {
                paint_axis_aligned_bounds(painter, rect, zoom, shape.bounds(), accent, 6.0, 2.0);
                painter.line_segment(
                    [
                        canvas_to_screen(rect, zoom, shape.start),
                        canvas_to_screen(rect, zoom, shape.end),
                    ],
                    EguiStroke::new(1.5, accent),
                );
                for (handle, position) in shape_control_handles_screen(shape, rect, zoom) {
                    paint_handle(
                        painter,
                        position,
                        HANDLE_RADIUS,
                        match active_control {
                            Some(ControlTarget::Resize(active)) if active == handle => active_fill,
                            _ => inactive_fill,
                        },
                        accent,
                    );
                }
                if let Some(rotation_handle) = shape_rotation_handle_screen(shape, rect, zoom) {
                    paint_rotation_link(
                        painter,
                        canvas_to_screen(rect, zoom, shape.rotation_center()),
                        rotation_handle,
                        accent,
                    );
                    paint_handle(
                        painter,
                        rotation_handle,
                        HANDLE_RADIUS,
                        match active_control {
                            Some(ControlTarget::Rotate) => active_fill,
                            _ => inactive_fill,
                        },
                        accent,
                    );
                }
            }
            ShapeKind::Rectangle | ShapeKind::Ellipse => {
                let outline: Vec<Pos2> = shape
                    .selection_outline()
                    .into_iter()
                    .map(|point| canvas_to_screen(rect, zoom, point))
                    .collect();
                painter.add(egui::Shape::closed_line(
                    outline.clone(),
                    EguiStroke::new(2.0, accent),
                ));

                for (handle, position) in shape_control_handles_screen(shape, rect, zoom) {
                    paint_handle(
                        painter,
                        position,
                        HANDLE_RADIUS,
                        match active_control {
                            Some(ControlTarget::Resize(active)) if active == handle => active_fill,
                            _ => inactive_fill,
                        },
                        accent,
                    );
                }

                if let Some(rotation_handle) = shape_rotation_handle_screen(shape, rect, zoom) {
                    let top_mid = outline[0].lerp(outline[1], 0.5);
                    paint_rotation_link(painter, top_mid, rotation_handle, accent);
                    paint_handle(
                        painter,
                        rotation_handle,
                        HANDLE_RADIUS,
                        match active_control {
                            Some(ControlTarget::Rotate) => active_fill,
                            _ => inactive_fill,
                        },
                        accent,
                    );
                }
            }
        },
    }
}

fn paint_multi_selection_overlay(
    painter: &Painter,
    rect: Rect,
    zoom: f32,
    selected_elements: &[(usize, PaintElement)],
) {
    let item_accent = Color32::from_rgba_unmultiplied(26, 115, 232, 140);
    let group_accent = Color32::from_rgb(26, 115, 232);

    for (_, element) in selected_elements {
        if let Some(bounds) = element.bounds() {
            paint_axis_aligned_bounds(painter, rect, zoom, bounds, item_accent, 4.0, 1.5);
        }
    }

    if let Some(group_bounds) = selection_bounds_from_elements(selected_elements) {
        paint_axis_aligned_bounds(painter, rect, zoom, group_bounds, group_accent, 10.0, 2.5);
    }
}

fn paint_axis_aligned_bounds(
    painter: &Painter,
    rect: Rect,
    zoom: f32,
    bounds: ElementBounds,
    accent: Color32,
    expand: f32,
    stroke_width: f32,
) {
    let screen_rect = Rect::from_two_pos(
        canvas_to_screen(rect, zoom, bounds.min),
        canvas_to_screen(rect, zoom, bounds.max),
    )
    .expand(expand);
    painter.rect_stroke(
        screen_rect,
        6.0,
        EguiStroke::new(stroke_width, accent),
        egui::StrokeKind::Outside,
    );
}

fn paint_handle(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    fill: Color32,
    stroke_color: Color32,
) {
    let handle_rect = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
    painter.rect_filled(handle_rect, 3.0, fill);
    painter.rect_stroke(
        handle_rect,
        3.0,
        EguiStroke::new(1.0, stroke_color),
        egui::StrokeKind::Outside,
    );
}

fn paint_rotation_link(painter: &Painter, from: Pos2, to: Pos2, accent: Color32) {
    painter.line_segment([from, to], EguiStroke::new(1.0, accent));
}

fn selection_bounds_from_elements(
    selected_elements: &[(usize, PaintElement)],
) -> Option<ElementBounds> {
    let mut bounds = selected_elements
        .iter()
        .filter_map(|(_, element)| element.bounds());
    let first = bounds.next()?;
    Some(bounds.fold(first, ElementBounds::union))
}

fn ellipse_outline_points(shape: &ShapeElement, rect: Rect, zoom: f32) -> Vec<Pos2> {
    let center = shape.center();
    let half = shape.half_extents();
    if half.dx <= f32::EPSILON || half.dy <= f32::EPSILON {
        return Vec::new();
    }

    (0..64)
        .map(|step| {
            let t = step as f32 / 64.0 * std::f32::consts::TAU;
            let local = PaintVector::new(half.dx * t.cos(), half.dy * t.sin());
            let world = center.offset(rotate_vector(local, shape.rotation_radians));
            canvas_to_screen(rect, zoom, world)
        })
        .collect()
}

fn shape_control_handles_screen(
    shape: ShapeElement,
    rect: Rect,
    zoom: f32,
) -> Vec<(ShapeHandle, Pos2)> {
    shape
        .control_handles()
        .into_iter()
        .map(|(handle, point)| (handle, canvas_to_screen(rect, zoom, point)))
        .collect()
}

fn shape_rotation_handle_screen(shape: ShapeElement, rect: Rect, zoom: f32) -> Option<Pos2> {
    let world_offset = ROTATION_HANDLE_OFFSET_SCREEN / zoom.max(MIN_ZOOM);
    shape
        .rotation_handle_position(world_offset)
        .map(|point| canvas_to_screen(rect, zoom, point))
}

fn pointer_screen_angle(rect: Rect, zoom: f32, center: PaintPoint, pointer: Pos2) -> f32 {
    let center_screen = canvas_to_screen(rect, zoom, center);
    (pointer.y - center_screen.y).atan2(pointer.x - center_screen.x)
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

fn rotate_vector(vector: PaintVector, angle_radians: f32) -> PaintVector {
    let cos = angle_radians.cos();
    let sin = angle_radians.sin();
    PaintVector::new(
        vector.dx * cos - vector.dy * sin,
        vector.dx * sin + vector.dy * cos,
    )
}

fn normalize_selection_indices(indices: &mut Vec<usize>) {
    indices.sort_unstable();
    indices.dedup();
}

#[cfg(test)]
mod tests {
    use super::{
        CanvasViewState, canvas_rect, normalize_selection_indices, screen_to_canvas,
        shape_rotation_handle_screen,
    };
    use crate::model::{CanvasSize, PaintPoint, RgbaColor, ShapeElement, ShapeKind};
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

    #[test]
    fn rotation_handle_stays_off_the_shape() {
        let shape = ShapeElement::with_rotation(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            4.0,
            PaintPoint::new(40.0, 40.0),
            PaintPoint::new(100.0, 80.0),
            0.5,
        );
        let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(300.0, 200.0));
        let handle = shape_rotation_handle_screen(shape, rect, 1.0).expect("rotation handle");
        let center = Pos2::new(70.0, 60.0);

        assert!(handle.distance(center) > 20.0);
    }

    #[test]
    fn normalize_selection_indices_sorts_and_deduplicates() {
        let mut indices = vec![3, 1, 2, 3, 1];
        normalize_selection_indices(&mut indices);
        assert_eq!(indices, vec![1, 2, 3]);
    }
}
