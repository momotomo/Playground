use eframe::egui::{
    self, Align2, Color32, FontId, Painter, PointerButton, Pos2, Rect, Sense, Stroke as EguiStroke,
    Vec2,
};

use crate::model::{
    AlignmentKind, CanvasSize, DistributionKind, ElementBounds, GuideAxis, GuideLine, LayerId,
    PaintDocument, PaintElement, PaintPoint, PaintVector, RgbaColor, ShapeElement, ShapeHandle,
    ShapeKind, StackOrderCommand, Stroke, ToolKind,
};

const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 8.0;
const FIT_MARGIN: f32 = 24.0;
const HIT_TOLERANCE_SCREEN: f32 = 8.0;
const TOUCH_HIT_TOLERANCE_SCREEN: f32 = 18.0;
const HANDLE_RADIUS: f32 = 6.5;
const HANDLE_HIT_RADIUS: f32 = 12.0;
const TOUCH_HANDLE_HIT_RADIUS: f32 = 22.0;
const ROTATION_HANDLE_OFFSET_SCREEN: f32 = 28.0;
const MIN_SELECTION_TRANSFORM_EXTENT: f32 = 4.0;
const MARQUEE_VISIBLE_MIN_SCREEN: f32 = 3.0;
const SNAP_TOLERANCE_SCREEN: f32 = 10.0;
const MIN_GRID_VISIBLE_SPACING_SCREEN: f32 = 12.0;
const GUIDE_HIT_TOLERANCE_SCREEN: f32 = 8.0;
const TOUCH_GUIDE_HIT_TOLERANCE_SCREEN: f32 = 16.0;
const SMART_GUIDE_TOLERANCE_SCREEN: f32 = 10.0;
const RULER_THICKNESS: f32 = 20.0;
const RULER_LABEL_MIN_SPACING_SCREEN: f32 = 56.0;
const TOUCH_LONG_PRESS_DURATION_SECONDS: f64 = 0.30;
const TOUCH_LONG_PRESS_MOVE_TOLERANCE_SCREEN: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct SmartGuideOverlay {
    vertical: Option<f32>,
    horizontal: Option<f32>,
}

impl SmartGuideOverlay {
    const fn empty() -> Self {
        Self {
            vertical: None,
            horizontal: None,
        }
    }

    fn is_empty(self) -> bool {
        self.vertical.is_none() && self.horizontal.is_none()
    }
}

#[derive(Debug, Clone, Copy)]
struct RulerPaintStyle {
    tick_color: Color32,
    label_color: Color32,
    step: f32,
    label_step: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasToolKind {
    Select,
    Pan,
    Brush,
    Eraser,
    Rectangle,
    Ellipse,
    Line,
}

impl CanvasToolKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Select => "選択",
            Self::Pan => "手のひら",
            Self::Brush => "ブラシ",
            Self::Eraser => "消しゴム",
            Self::Rectangle => "四角形",
            Self::Ellipse => "楕円",
            Self::Line => "直線",
        }
    }

    const fn is_drawing_tool(self) -> bool {
        matches!(
            self,
            Self::Brush | Self::Eraser | Self::Rectangle | Self::Ellipse | Self::Line
        )
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
    pub multi_select_mode: bool,
    pub finger_draw_enabled: bool,
}

#[derive(Debug, Default, Clone)]
pub struct CanvasOutput {
    pub committed_element: Option<PaintElement>,
    pub committed_edit: Option<CommittedDocumentEdit>,
    pub requested_tool: Option<CanvasToolKind>,
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
    Guide,
    Group,
    Ungroup,
    Align(AlignmentKind),
    Distribute(DistributionKind),
    Reorder(StackOrderCommand),
}

impl DocumentEditMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Move => "移動中",
            Self::Resize => "サイズ変更中",
            Self::Rotate => "回転中",
            Self::Guide => "ガイド移動中",
            Self::Group => "グループ化中",
            Self::Ungroup => "グループ解除中",
            Self::Align(_) => "整列中",
            Self::Distribute(_) => "等間隔配置中",
            Self::Reorder(_) => "重なり順変更中",
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
    Space,
    Middle,
    Tool,
    Touch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TouchContactKind {
    Finger,
    PenLike,
}

#[derive(Debug, Clone, Copy)]
struct PendingLongPress {
    start_screen: Pos2,
    start_world: PaintPoint,
    start_time: f64,
    extend_selection: bool,
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
    layer_id: Option<LayerId>,
    indices: Vec<usize>,
}

impl SelectionState {
    fn clear(&mut self) {
        self.layer_id = None;
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
        match self.indices.as_slice() {
            [index] => Some(*index),
            _ => None,
        }
    }

    fn indices(&self) -> &[usize] {
        &self.indices
    }

    fn set_only(&mut self, layer_id: LayerId, index: usize) {
        self.layer_id = Some(layer_id);
        self.indices.clear();
        self.indices.push(index);
    }

    fn toggle(&mut self, layer_id: LayerId, index: usize) {
        if self.layer_id != Some(layer_id) {
            self.indices.clear();
            self.layer_id = Some(layer_id);
        }

        if let Some(position) = self
            .indices
            .iter()
            .position(|candidate| *candidate == index)
        {
            self.indices.remove(position);
        } else {
            self.indices.push(index);
            normalize_selection_indices(&mut self.indices);
        }
    }

    fn set_indices(&mut self, layer_id: LayerId, mut indices: Vec<usize>) {
        normalize_selection_indices(&mut indices);
        self.layer_id = Some(layer_id);
        self.indices = indices;
        if self.indices.is_empty() {
            self.layer_id = None;
        }
    }

    fn retain_valid(&mut self, document: &PaintDocument) {
        if self.layer_id != Some(document.active_layer_id()) || !document.active_layer_is_editable()
        {
            self.clear();
            return;
        }

        self.indices
            .retain(|index| *index < document.element_count());
        normalize_selection_indices(&mut self.indices);
        if self.indices.is_empty() {
            self.layer_id = None;
        }
    }
}

#[derive(Debug, Clone)]
enum ActivePreview {
    Stroke(Stroke),
    Shape(ShapeElement),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlTarget {
    SingleResize(ShapeHandle),
    SingleRotate,
    GroupResize(ShapeHandle),
    GroupRotate,
}

#[derive(Debug, Clone)]
enum SelectionSession {
    Move {
        indices: Vec<usize>,
        base_elements: Vec<(usize, PaintElement)>,
        drag_origin: PaintPoint,
        preview_delta: PaintVector,
        smart_guides: SmartGuideOverlay,
    },
    SingleResize {
        index: usize,
        base_shape: ShapeElement,
        handle: ShapeHandle,
        preview_shape: ShapeElement,
    },
    SingleRotate {
        index: usize,
        base_shape: ShapeElement,
        start_pointer_angle: f32,
        preview_shape: ShapeElement,
    },
    MultiResize {
        indices: Vec<usize>,
        base_elements: Vec<(usize, PaintElement)>,
        base_bounds: ElementBounds,
        handle: ShapeHandle,
        preview_elements: Vec<(usize, PaintElement)>,
    },
    MultiRotate {
        indices: Vec<usize>,
        base_elements: Vec<(usize, PaintElement)>,
        group_center: PaintPoint,
        start_pointer_angle: f32,
        preview_elements: Vec<(usize, PaintElement)>,
    },
    GuideMove {
        index: usize,
        axis: GuideAxis,
        start_position: f32,
        preview_position: f32,
    },
    Marquee {
        start_world: PaintPoint,
        current_world: PaintPoint,
        additive: bool,
        base_selection: Vec<usize>,
    },
}

impl SelectionSession {
    fn mode_label(&self) -> &'static str {
        match self {
            Self::Move { .. } => DocumentEditMode::Move.label(),
            Self::SingleResize { .. } | Self::MultiResize { .. } => {
                DocumentEditMode::Resize.label()
            }
            Self::SingleRotate { .. } | Self::MultiRotate { .. } => {
                DocumentEditMode::Rotate.label()
            }
            Self::GuideMove { .. } => DocumentEditMode::Guide.label(),
            Self::Marquee { .. } => "選択中",
        }
    }

    fn control_target(&self) -> Option<ControlTarget> {
        match self {
            Self::SingleResize { handle, .. } => Some(ControlTarget::SingleResize(*handle)),
            Self::SingleRotate { .. } => Some(ControlTarget::SingleRotate),
            Self::MultiResize { handle, .. } => Some(ControlTarget::GroupResize(*handle)),
            Self::MultiRotate { .. } => Some(ControlTarget::GroupRotate),
            Self::Move { .. } | Self::GuideMove { .. } | Self::Marquee { .. } => None,
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
            Self::SingleResize {
                index,
                preview_shape,
                ..
            }
            | Self::SingleRotate {
                index,
                preview_shape,
                ..
            } => vec![(*index, PaintElement::Shape(*preview_shape))],
            Self::MultiResize {
                preview_elements, ..
            }
            | Self::MultiRotate {
                preview_elements, ..
            } => preview_elements.clone(),
            Self::GuideMove { .. } => Vec::new(),
            Self::Marquee { .. } => Vec::new(),
        }
    }

    fn is_valid_for(&self, document: &PaintDocument) -> bool {
        match self {
            Self::Move { indices, .. }
            | Self::MultiResize { indices, .. }
            | Self::MultiRotate { indices, .. } => indices
                .iter()
                .all(|index| *index < document.element_count()),
            Self::SingleResize { index, .. } | Self::SingleRotate { index, .. } => {
                matches!(document.element(*index), Some(PaintElement::Shape(_)))
            }
            Self::GuideMove { index, .. } => *index < document.guides().lines.len(),
            Self::Marquee { .. } => true,
        }
    }

    fn marquee_bounds(&self) -> Option<ElementBounds> {
        match self {
            Self::Marquee {
                start_world,
                current_world,
                ..
            } => Some(bounds_from_points(*start_world, *current_world)),
            _ => None,
        }
    }

    fn preview_guide(&self) -> Option<(usize, GuideLine)> {
        match self {
            Self::GuideMove {
                index,
                axis,
                preview_position,
                ..
            } => Some((*index, GuideLine::new(*axis, *preview_position))),
            _ => None,
        }
    }

    fn smart_guides(&self) -> SmartGuideOverlay {
        match self {
            Self::Move { smart_guides, .. } => *smart_guides,
            Self::SingleResize { .. }
            | Self::SingleRotate { .. }
            | Self::MultiResize { .. }
            | Self::MultiRotate { .. }
            | Self::GuideMove { .. }
            | Self::Marquee { .. } => SmartGuideOverlay::empty(),
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
    touch_contact_kind: Option<TouchContactKind>,
    pending_long_press: Option<PendingLongPress>,
}

impl Default for CanvasController {
    fn default() -> Self {
        Self {
            active_preview: None,
            selection_session: None,
            interaction_mode: InteractionMode::Idle,
            view: CanvasViewState::default(),
            selection: SelectionState::default(),
            touch_contact_kind: None,
            pending_long_press: None,
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
        let hover_pos = ui.input(|input| input.pointer.hover_pos());
        let touch_active = ui.input(|input| input.any_touches());
        let hovered_guide =
            if tool_settings.tool == CanvasToolKind::Select && self.selection_session.is_none() {
                hover_pos.and_then(|pointer| {
                    self.hit_guide(document, canvas_rect, self.view.zoom, pointer, touch_active)
                        .map(|(index, _)| index)
                })
            } else {
                None
            };
        let preview_guide = self
            .selection_session
            .as_ref()
            .and_then(SelectionSession::preview_guide);
        let smart_guide_overlay = self
            .selection_session
            .as_ref()
            .map(SelectionSession::smart_guides)
            .unwrap_or_else(SmartGuideOverlay::empty);
        let active_control = self
            .selection_session
            .as_ref()
            .and_then(SelectionSession::control_target);
        let show_handles = !matches!(
            self.selection_session,
            Some(SelectionSession::Marquee { .. } | SelectionSession::GuideMove { .. })
        );

        paint_workspace(&painter, viewport);
        paint_background(&painter, canvas_rect, document.background);
        paint_grid(&painter, canvas_rect, self.view.zoom, document);
        paint_document(
            &painter,
            canvas_rect,
            self.view.zoom,
            document,
            &preview_overlays,
        );
        paint_guides(
            &painter,
            canvas_rect,
            self.view.zoom,
            document,
            hovered_guide,
            preview_guide,
        );
        paint_smart_guides(&painter, canvas_rect, self.view.zoom, smart_guide_overlay);

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
            paint_selection_overlay(
                &painter,
                canvas_rect,
                self.view.zoom,
                &selected_visual,
                active_control,
                show_handles,
            );
        }

        if let Some(marquee_bounds) = self
            .selection_session
            .as_ref()
            .and_then(SelectionSession::marquee_bounds)
        {
            paint_marquee_overlay(&painter, canvas_rect, self.view.zoom, marquee_bounds);
        }

        if !document.has_elements() && self.active_preview.is_none() {
            paint_empty_state(&painter, canvas_rect);
        }

        paint_rulers(&painter, viewport, canvas_rect, self.view.zoom, document);

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
        let discarded = self.active_preview.take().is_some()
            || self.selection_session.take().is_some()
            || self.pending_long_press.take().is_some();
        self.interaction_mode = InteractionMode::Idle;
        discarded
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
        self.selection_session = None;
        self.pending_long_press = None;
        if matches!(self.interaction_mode, InteractionMode::EditingSelection) {
            self.interaction_mode = InteractionMode::Idle;
        }
    }

    pub fn set_selection_indices(&mut self, layer_id: LayerId, indices: Vec<usize>) {
        self.selection.set_indices(layer_id, indices);
    }

    pub fn selection_count(&self) -> usize {
        self.selection.len()
    }

    pub fn selection_indices(&self) -> &[usize] {
        self.selection.indices()
    }

    pub fn selection_layer_id(&self) -> Option<LayerId> {
        self.selection.layer_id
    }

    pub fn selection_contains_group(&self, document: &PaintDocument) -> bool {
        document.selection_contains_group(self.selection.indices())
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
        if !document.active_layer_is_editable() {
            self.pending_long_press = None;
        }
    }

    pub fn selection_summary(&self, document: &PaintDocument) -> String {
        let Some(active_layer) = document.active_layer() else {
            return "選択: なし".to_owned();
        };

        if !active_layer.visible {
            return format!("選択: なし（{} は非表示）", active_layer.name);
        }

        if active_layer.locked {
            return format!("選択: なし（{} はロック中）", active_layer.name);
        }

        if self.selection.is_empty() {
            return format!("選択: なし（{}）", active_layer.name);
        }

        if let Some(session) = &self.selection_session {
            if self.selection.len() == 1
                && let Some(index) = self.selection.single()
                && let Some(element) = document.element(index)
            {
                return format!(
                    "選択: {} #{}（{}）",
                    element.kind_label(),
                    index + 1,
                    session.mode_label()
                );
            }

            return format!(
                "選択: {}個（{}）",
                self.selection.len(),
                session.mode_label()
            );
        }

        if let Some(index) = self.selection.single()
            && let Some(element) = document.element(index)
        {
            let capability = match element {
                PaintElement::Stroke(_) => "移動 / 拡大縮小 / 回転",
                PaintElement::Shape(_) => "移動 / サイズ変更 / 回転",
                PaintElement::Group(_) => "移動 / サイズ変更 / 回転 / グループ解除",
            };
            return format!(
                "選択: {} #{}（{capability}）",
                element.kind_label(),
                index + 1
            );
        }

        format!(
            "選択: {}個（移動 / サイズ変更 / 回転 / グループ化 / 整列 / 等間隔 / 重なり順）",
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

    pub fn apply_group(&mut self, document: &PaintDocument) -> Option<CommittedDocumentEdit> {
        if self.has_active_interaction() || self.selection.len() < 2 {
            return None;
        }

        let (next, selection_indices) = document.grouped_document(self.selection.indices())?;
        self.selection
            .set_indices(document.active_layer_id(), selection_indices.clone());
        Some(CommittedDocumentEdit {
            document: next,
            selection_indices,
            mode: DocumentEditMode::Group,
        })
    }

    pub fn apply_ungroup(&mut self, document: &PaintDocument) -> Option<CommittedDocumentEdit> {
        if self.has_active_interaction() || self.selection.is_empty() {
            return None;
        }

        let (next, selection_indices) = document.ungrouped_document(self.selection.indices())?;
        self.selection
            .set_indices(document.active_layer_id(), selection_indices.clone());
        Some(CommittedDocumentEdit {
            document: next,
            selection_indices,
            mode: DocumentEditMode::Ungroup,
        })
    }

    pub fn apply_distribution(
        &mut self,
        document: &PaintDocument,
        distribution: DistributionKind,
    ) -> Option<CommittedDocumentEdit> {
        if self.has_active_interaction() || self.selection.len() < 3 {
            return None;
        }

        let next = document.distributed_document(self.selection.indices(), distribution)?;
        Some(CommittedDocumentEdit {
            document: next,
            selection_indices: self.selection.indices().to_vec(),
            mode: DocumentEditMode::Distribute(distribution),
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
        self.selection
            .set_indices(document.active_layer_id(), selection_indices.clone());
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

    fn sync_touch_contact_kind(
        &mut self,
        ui: &egui::Ui,
        touch_active: bool,
        multi_touch_active: bool,
        primary_pressed: bool,
    ) -> bool {
        if !touch_active {
            self.touch_contact_kind = None;
            self.pending_long_press = None;
            return false;
        }

        if multi_touch_active {
            self.touch_contact_kind = Some(TouchContactKind::Finger);
            self.pending_long_press = None;
            return true;
        }

        if primary_pressed
            && let Some(kind) = ui.input(|input| touch_contact_kind_from_events(&input.events))
        {
            self.touch_contact_kind = Some(kind);
        }

        matches!(self.touch_contact_kind, Some(TouchContactKind::Finger))
    }

    fn handle_pending_long_press(
        &mut self,
        ui: &egui::Ui,
        pointer: &egui::PointerState,
        document: &PaintDocument,
        viewport: Rect,
        canvas_rect: Rect,
    ) -> Option<CanvasOutput> {
        let pending = self.pending_long_press?;
        let touch_active = ui.input(|input| input.any_touches());
        if !touch_active || !pointer.primary_down() {
            self.pending_long_press = None;
            return None;
        }

        let pointer_screen = pointer.interact_pos()?;
        if pending.start_screen.distance(pointer_screen) > TOUCH_LONG_PRESS_MOVE_TOLERANCE_SCREEN {
            self.pending_long_press = None;
            self.interaction_mode = InteractionMode::Panning(PanMode::Touch);
            return Some(CanvasOutput {
                needs_repaint: true,
                ..Default::default()
            });
        }

        let elapsed = ui.input(|input| input.time) - pending.start_time;
        if elapsed < TOUCH_LONG_PRESS_DURATION_SECONDS {
            return Some(CanvasOutput {
                needs_repaint: true,
                ..Default::default()
            });
        }

        self.pending_long_press = None;
        let pointer_world = screen_to_canvas(
            viewport,
            self.view.pan,
            self.view.zoom,
            document.canvas_size,
            pointer_screen,
        );
        let tolerance = hit_tolerance_world(
            self.view.zoom,
            true,
            HIT_TOLERANCE_SCREEN,
            TOUCH_HIT_TOLERANCE_SCREEN,
        );

        if let Some(index) = document.hit_test(pointer_world, tolerance) {
            if pending.extend_selection {
                self.selection.toggle(document.active_layer_id(), index);
                return Some(CanvasOutput {
                    requested_tool: Some(CanvasToolKind::Select),
                    needs_repaint: true,
                    ..Default::default()
                });
            }

            if !self.selection.contains(index) {
                self.selection.set_only(document.active_layer_id(), index);
            }
            self.begin_move_session(document, pointer_world);
            return Some(CanvasOutput {
                requested_tool: Some(CanvasToolKind::Select),
                needs_repaint: true,
                ..Default::default()
            });
        }

        self.begin_marquee_selection(pending.start_world, pending.extend_selection);
        self.update_selection_session(viewport, document, canvas_rect, pointer_screen);
        Some(CanvasOutput {
            requested_tool: Some(CanvasToolKind::Select),
            needs_repaint: true,
            ..Default::default()
        })
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
        let multi_touch = ui.input(|input| input.multi_touch());
        let touch_active = ui.input(|input| input.any_touches());
        let multi_touch_active = multi_touch.is_some_and(|gesture| gesture.num_touches >= 2);
        let finger_touch_active = self.sync_touch_contact_kind(
            ui,
            touch_active,
            multi_touch_active,
            pointer.primary_pressed(),
        );
        let viewport = response.rect;
        let hover_pos = pointer.hover_pos();
        let hovered = response.contains_pointer()
            || multi_touch.is_some_and(|gesture| viewport.contains(gesture.center_pos));
        let space_pan = ui.input(|input| input.key_down(egui::Key::Space));
        let extend_selection =
            ui.input(|input| input.modifiers.shift) || tool_settings.multi_select_mode;
        let mut output = CanvasOutput::default();

        if self.active_preview.is_none()
            && self.selection_session.is_none()
            && let Some(gesture) = multi_touch
            && gesture.num_touches >= 2
            && viewport.contains(gesture.center_pos)
        {
            if matches!(self.interaction_mode, InteractionMode::Panning(_)) {
                self.interaction_mode = InteractionMode::Idle;
            }

            let pan_changed = self.view.pan_by(gesture.translation_delta);
            let zoom_changed = if gesture.zoom_delta != 1.0 {
                self.view
                    .zoom_around(gesture.zoom_delta, gesture.center_pos, document.canvas_size)
            } else {
                false
            };
            output.needs_repaint = pan_changed || zoom_changed;
            return output;
        }

        if self.active_preview.is_none()
            && self.selection_session.is_none()
            && tool_settings.tool.is_drawing_tool()
            && finger_touch_active
            && !tool_settings.finger_draw_enabled
            && let Some(long_press_output) =
                self.handle_pending_long_press(ui, &pointer, document, viewport, canvas_rect)
        {
            return long_press_output;
        }

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
                    self.interaction_mode = InteractionMode::Panning(PanMode::Middle);
                    output.needs_repaint = true;
                    return output;
                }

                if hovered && space_pan && pointer.primary_pressed() {
                    self.interaction_mode = InteractionMode::Panning(PanMode::Space);
                    output.needs_repaint = true;
                    return output;
                }

                if hovered && tool_settings.tool == CanvasToolKind::Pan && pointer.primary_pressed()
                {
                    self.interaction_mode = InteractionMode::Panning(PanMode::Tool);
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

                    if tool_settings.tool.is_drawing_tool()
                        && finger_touch_active
                        && !tool_settings.finger_draw_enabled
                    {
                        self.pending_long_press = Some(PendingLongPress {
                            start_screen: position,
                            start_world: world,
                            start_time: ui.input(|input| input.time),
                            extend_selection,
                        });
                        output.needs_repaint = true;
                        return output;
                    }

                    match tool_settings.tool {
                        CanvasToolKind::Select => {
                            self.begin_selection_interaction(
                                document,
                                canvas_rect,
                                position,
                                world,
                                extend_selection,
                                touch_active,
                            );
                            output.needs_repaint = true;
                        }
                        CanvasToolKind::Pan => {
                            self.interaction_mode = InteractionMode::Panning(PanMode::Tool);
                            output.needs_repaint = true;
                        }
                        CanvasToolKind::Brush | CanvasToolKind::Eraser => {
                            self.begin_stroke_preview(document, tool_settings, world);
                            output.needs_repaint = true;
                        }
                        CanvasToolKind::Rectangle
                        | CanvasToolKind::Ellipse
                        | CanvasToolKind::Line => {
                            self.begin_shape_preview(document, tool_settings, world);
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
                    self.update_active_preview(document, world);
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
                    PanMode::Space => pointer.primary_down() && space_pan,
                    PanMode::Middle => pointer.middle_down(),
                    PanMode::Tool => pointer.primary_down(),
                    PanMode::Touch => pointer.primary_down() && touch_active,
                };

                if !still_active {
                    self.interaction_mode = InteractionMode::Idle;
                }
            }
            InteractionMode::EditingSelection => {
                if pointer.primary_down()
                    && let Some(position) = pointer.interact_pos()
                {
                    self.update_selection_session(viewport, document, canvas_rect, position);
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
        pointer_screen: Pos2,
        pointer_world: PaintPoint,
        extend_selection: bool,
        touch_active: bool,
    ) {
        if !extend_selection
            && let Some(control) = self.hit_selection_control(
                document,
                canvas_rect,
                self.view.zoom,
                pointer_screen,
                touch_active,
            )
        {
            match control {
                ControlTarget::SingleResize(handle) => {
                    if let Some((index, shape)) = self.single_selected_shape_owned(document) {
                        self.selection_session = Some(SelectionSession::SingleResize {
                            index,
                            base_shape: shape,
                            handle,
                            preview_shape: shape,
                        });
                        self.interaction_mode = InteractionMode::EditingSelection;
                        return;
                    }
                }
                ControlTarget::SingleRotate => {
                    if let Some((index, shape)) = self.single_selected_shape_owned(document) {
                        self.selection_session = Some(SelectionSession::SingleRotate {
                            index,
                            base_shape: shape,
                            start_pointer_angle: pointer_screen_angle(
                                canvas_rect,
                                self.view.zoom,
                                shape.rotation_center(),
                                pointer_screen,
                            ),
                            preview_shape: shape,
                        });
                        self.interaction_mode = InteractionMode::EditingSelection;
                        return;
                    }
                }
                ControlTarget::GroupResize(handle) => {
                    if let Some(base_bounds) = self.selection_control_bounds(document) {
                        let base_elements = self.selected_visual_elements(document);
                        self.selection_session = Some(SelectionSession::MultiResize {
                            indices: self.selection.indices().to_vec(),
                            base_elements: base_elements.clone(),
                            base_bounds,
                            handle,
                            preview_elements: base_elements,
                        });
                        self.interaction_mode = InteractionMode::EditingSelection;
                        return;
                    }
                }
                ControlTarget::GroupRotate => {
                    if let Some(base_bounds) = self.selection_control_bounds(document) {
                        let base_elements = self.selected_visual_elements(document);
                        self.selection_session = Some(SelectionSession::MultiRotate {
                            indices: self.selection.indices().to_vec(),
                            base_elements: base_elements.clone(),
                            group_center: base_bounds.center(),
                            start_pointer_angle: pointer_screen_angle(
                                canvas_rect,
                                self.view.zoom,
                                base_bounds.center(),
                                pointer_screen,
                            ),
                            preview_elements: base_elements,
                        });
                        self.interaction_mode = InteractionMode::EditingSelection;
                        return;
                    }
                }
            }
        }

        if !extend_selection
            && self.hit_selection_move_bounds(document, pointer_world, touch_active)
        {
            self.begin_move_session(document, pointer_world);
            return;
        }

        let tolerance = hit_tolerance_world(
            self.view.zoom,
            touch_active,
            HIT_TOLERANCE_SCREEN,
            TOUCH_HIT_TOLERANCE_SCREEN,
        );
        if let Some(index) = document.hit_test(pointer_world, tolerance) {
            if extend_selection {
                self.selection.toggle(document.active_layer_id(), index);
                self.selection_session = None;
                self.interaction_mode = InteractionMode::Idle;
                return;
            }

            if !self.selection.contains(index) {
                self.selection.set_only(document.active_layer_id(), index);
            }
            self.begin_move_session(document, pointer_world);
        } else if !extend_selection
            && let Some((guide_index, guide)) = self.hit_guide(
                document,
                canvas_rect,
                self.view.zoom,
                pointer_screen,
                touch_active,
            )
        {
            self.begin_guide_move(guide_index, guide);
        } else {
            self.begin_marquee_selection(pointer_world, extend_selection);
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
            smart_guides: SmartGuideOverlay::empty(),
        });
        self.interaction_mode = InteractionMode::EditingSelection;
    }

    fn begin_marquee_selection(&mut self, start_world: PaintPoint, additive: bool) {
        self.selection_session = Some(SelectionSession::Marquee {
            start_world,
            current_world: start_world,
            additive,
            base_selection: self.selection.indices().to_vec(),
        });
        self.interaction_mode = InteractionMode::EditingSelection;
    }

    fn begin_guide_move(&mut self, index: usize, guide: GuideLine) {
        self.selection_session = Some(SelectionSession::GuideMove {
            index,
            axis: guide.axis,
            start_position: guide.position,
            preview_position: guide.position,
        });
        self.interaction_mode = InteractionMode::EditingSelection;
    }

    fn begin_stroke_preview(
        &mut self,
        document: &PaintDocument,
        tool_settings: ToolSettings,
        start: PaintPoint,
    ) {
        if !document.active_layer_is_editable() {
            return;
        }

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

    fn begin_shape_preview(
        &mut self,
        document: &PaintDocument,
        tool_settings: ToolSettings,
        start: PaintPoint,
    ) {
        if !document.active_layer_is_editable() {
            return;
        }

        self.clear_selection();
        let Some(kind) = tool_settings.tool.shape_kind() else {
            return;
        };
        let start = snap_point_for_document(document, self.view.zoom, start);

        self.active_preview = Some(ActivePreview::Shape(ShapeElement::new(
            kind,
            tool_settings.color,
            tool_settings.width,
            start,
            start,
        )));
        self.interaction_mode = InteractionMode::Drawing;
    }

    fn update_active_preview(&mut self, document: &PaintDocument, world: PaintPoint) {
        let snapped_world = snap_point_for_document(document, self.view.zoom, world);
        match self.active_preview.as_mut() {
            Some(ActivePreview::Stroke(stroke)) => stroke.push_point(world),
            Some(ActivePreview::Shape(shape)) => shape.end = snapped_world,
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
        document: &PaintDocument,
        canvas_rect: Rect,
        pointer_screen: Pos2,
    ) {
        let zoom = self.view.zoom;
        let pointer_world = screen_to_canvas_unclamped(
            viewport,
            self.view.pan,
            zoom,
            document.canvas_size,
            pointer_screen,
        );

        match self.selection_session.as_mut() {
            Some(SelectionSession::Move {
                indices,
                base_elements,
                drag_origin,
                preview_delta,
                smart_guides,
                ..
            }) => {
                let raw_delta = PaintVector::new(
                    pointer_world.x - drag_origin.x,
                    pointer_world.y - drag_origin.y,
                );
                let preview = snap_move_preview_for_document(
                    document,
                    zoom,
                    document.active_layer_id(),
                    indices,
                    base_elements,
                    raw_delta,
                );
                *preview_delta = preview.delta;
                *smart_guides = preview.smart_guides;
            }
            Some(SelectionSession::SingleResize {
                base_shape,
                handle,
                preview_shape,
                ..
            }) => {
                let snapped_world = snap_point_for_document(document, zoom, pointer_world);
                if let Some(next) = base_shape.resized_by_handle(*handle, snapped_world) {
                    *preview_shape = next;
                }
            }
            Some(SelectionSession::SingleRotate {
                base_shape,
                start_pointer_angle,
                preview_shape,
                ..
            }) => {
                let current_angle = pointer_screen_angle(
                    canvas_rect,
                    zoom,
                    base_shape.rotation_center(),
                    pointer_screen,
                );
                *preview_shape = base_shape.rotated_by(current_angle - *start_pointer_angle);
            }
            Some(SelectionSession::MultiResize {
                base_elements,
                base_bounds,
                handle,
                preview_elements,
                ..
            }) => {
                let snapped_world = snap_point_for_document(document, zoom, pointer_world);
                if let Some((anchor, scale_x, scale_y)) =
                    group_resize_transform(*base_bounds, *handle, snapped_world)
                {
                    *preview_elements = transformed_preview_elements(base_elements, |element| {
                        element.scaled_from(anchor, scale_x, scale_y)
                    });
                }
            }
            Some(SelectionSession::MultiRotate {
                base_elements,
                group_center,
                start_pointer_angle,
                preview_elements,
                ..
            }) => {
                let current_angle =
                    pointer_screen_angle(canvas_rect, zoom, *group_center, pointer_screen);
                let delta = current_angle - *start_pointer_angle;
                *preview_elements = transformed_preview_elements(base_elements, |element| {
                    element.rotated_around(*group_center, delta)
                });
            }
            Some(SelectionSession::GuideMove {
                axis,
                preview_position,
                ..
            }) => {
                let axis_value = match axis {
                    GuideAxis::Horizontal => pointer_world.y,
                    GuideAxis::Vertical => pointer_world.x,
                };
                *preview_position = clamp_guide_position_for_document(document, *axis, axis_value);
            }
            Some(SelectionSession::Marquee { current_world, .. }) => {
                *current_world = pointer_world;
            }
            None => {}
        }
    }

    fn finish_selection_session(
        &mut self,
        document: &PaintDocument,
    ) -> Option<CommittedDocumentEdit> {
        let session = self.selection_session.take()?;
        match session {
            SelectionSession::Marquee {
                start_world,
                current_world,
                additive,
                base_selection,
            } => {
                let marquee_bounds = bounds_from_points(start_world, current_world);
                let mut next_selection = if marquee_rect_is_visible(marquee_bounds, self.view.zoom)
                {
                    document.hit_test_rect(marquee_bounds)
                } else {
                    Vec::new()
                };

                if additive {
                    next_selection.extend(base_selection);
                    normalize_selection_indices(&mut next_selection);
                }

                self.selection
                    .set_indices(document.active_layer_id(), next_selection);
                None
            }
            SelectionSession::Move {
                indices,
                base_elements,
                preview_delta,
                ..
            } => {
                if preview_delta.is_zero() {
                    return None;
                }

                let replacements = transformed_preview_elements(&base_elements, |element| {
                    element.translated(preview_delta)
                });
                let edit = commit_preview_replacements(
                    document,
                    &indices,
                    replacements,
                    DocumentEditMode::Move,
                );
                if let Some(committed) = &edit {
                    self.selection.set_indices(
                        document.active_layer_id(),
                        committed.selection_indices.clone(),
                    );
                }
                edit
            }
            SelectionSession::SingleResize {
                index,
                base_shape,
                preview_shape,
                ..
            } => {
                if preview_shape == base_shape {
                    return None;
                }

                let edit = commit_preview_replacements(
                    document,
                    &[index],
                    vec![(index, PaintElement::Shape(preview_shape))],
                    DocumentEditMode::Resize,
                );
                if let Some(committed) = &edit {
                    self.selection.set_indices(
                        document.active_layer_id(),
                        committed.selection_indices.clone(),
                    );
                }
                edit
            }
            SelectionSession::SingleRotate {
                index,
                base_shape,
                preview_shape,
                ..
            } => {
                if preview_shape == base_shape {
                    return None;
                }

                let edit = commit_preview_replacements(
                    document,
                    &[index],
                    vec![(index, PaintElement::Shape(preview_shape))],
                    DocumentEditMode::Rotate,
                );
                if let Some(committed) = &edit {
                    self.selection.set_indices(
                        document.active_layer_id(),
                        committed.selection_indices.clone(),
                    );
                }
                edit
            }
            SelectionSession::MultiResize {
                indices,
                base_elements,
                preview_elements,
                ..
            } => {
                if preview_elements == base_elements {
                    return None;
                }

                let edit = commit_preview_replacements(
                    document,
                    &indices,
                    preview_elements,
                    DocumentEditMode::Resize,
                );
                if let Some(committed) = &edit {
                    self.selection.set_indices(
                        document.active_layer_id(),
                        committed.selection_indices.clone(),
                    );
                }
                edit
            }
            SelectionSession::MultiRotate {
                indices,
                base_elements,
                preview_elements,
                ..
            } => {
                if preview_elements == base_elements {
                    return None;
                }

                let edit = commit_preview_replacements(
                    document,
                    &indices,
                    preview_elements,
                    DocumentEditMode::Rotate,
                );
                if let Some(committed) = &edit {
                    self.selection.set_indices(
                        document.active_layer_id(),
                        committed.selection_indices.clone(),
                    );
                }
                edit
            }
            SelectionSession::GuideMove {
                index,
                axis: _,
                start_position,
                preview_position,
            } => {
                if (preview_position - start_position).abs() < 0.1 {
                    return None;
                }

                document
                    .moved_guide_document(index, preview_position)
                    .map(|next| CommittedDocumentEdit {
                        document: next,
                        selection_indices: self.selection.indices().to_vec(),
                        mode: DocumentEditMode::Guide,
                    })
            }
        }
    }

    fn preview_overlay_elements(&self) -> Vec<(usize, PaintElement)> {
        self.selection_session
            .as_ref()
            .map(SelectionSession::preview_elements)
            .unwrap_or_default()
    }

    fn selected_visual_elements(&self, document: &PaintDocument) -> Vec<(usize, PaintElement)> {
        if let Some(session) = &self.selection_session {
            let preview = session.preview_elements();
            if !preview.is_empty() {
                return preview;
            }
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
            let preview = session.preview_elements();
            if let Some(element) = preview
                .into_iter()
                .find_map(|(candidate, element)| (candidate == index).then_some(element))
            {
                return Some(element);
            }
        }

        document.element(index).cloned()
    }

    fn single_selected_shape_owned(
        &self,
        document: &PaintDocument,
    ) -> Option<(usize, ShapeElement)> {
        let index = self.selection.single()?;
        match self.single_selected_element_owned(document)? {
            PaintElement::Shape(shape) => Some((index, shape)),
            PaintElement::Stroke(_) | PaintElement::Group(_) => None,
        }
    }

    fn selection_uses_group_controls(&self, document: &PaintDocument) -> bool {
        if self.selection.len() > 1 {
            return true;
        }

        matches!(
            self.single_selected_element_owned(document),
            Some(PaintElement::Stroke(_) | PaintElement::Group(_))
        )
    }

    fn selection_control_bounds(&self, document: &PaintDocument) -> Option<ElementBounds> {
        if self.selection.is_empty() {
            None
        } else {
            selection_bounds_from_elements(&self.selected_visual_elements(document))
        }
    }

    fn hit_selection_move_bounds(
        &self,
        document: &PaintDocument,
        pointer_world: PaintPoint,
        touch_active: bool,
    ) -> bool {
        let Some(bounds) = self.selection_control_bounds(document) else {
            return false;
        };

        let padding = hit_tolerance_world(
            self.view.zoom,
            touch_active,
            HIT_TOLERANCE_SCREEN,
            TOUCH_HIT_TOLERANCE_SCREEN,
        );
        expand_bounds(bounds, padding).contains(pointer_world)
    }

    fn hit_selection_control(
        &self,
        document: &PaintDocument,
        canvas_rect: Rect,
        zoom: f32,
        pointer_screen: Pos2,
        touch_active: bool,
    ) -> Option<ControlTarget> {
        let handle_hit_radius = effective_screen_hit_tolerance(
            touch_active,
            HANDLE_HIT_RADIUS,
            TOUCH_HANDLE_HIT_RADIUS,
        );
        if self.selection_uses_group_controls(document) {
            let bounds = self.selection_control_bounds(document)?;
            for (handle, handle_screen) in bounds_control_handles_screen(bounds, canvas_rect, zoom)
            {
                if handle_screen.distance(pointer_screen) <= handle_hit_radius {
                    return Some(ControlTarget::GroupResize(handle));
                }
            }

            let rotation_handle = bounds_rotation_handle_screen(bounds, canvas_rect, zoom)?;
            if rotation_handle.distance(pointer_screen) <= handle_hit_radius {
                return Some(ControlTarget::GroupRotate);
            }
            return None;
        }

        let (_, shape) = self.single_selected_shape_owned(document)?;

        for (handle, handle_screen) in shape_control_handles_screen(shape, canvas_rect, zoom) {
            if handle_screen.distance(pointer_screen) <= handle_hit_radius {
                return Some(ControlTarget::SingleResize(handle));
            }
        }

        let rotation_handle = shape_rotation_handle_screen(shape, canvas_rect, zoom)?;
        (rotation_handle.distance(pointer_screen) <= handle_hit_radius)
            .then_some(ControlTarget::SingleRotate)
    }

    fn hit_guide(
        &self,
        document: &PaintDocument,
        canvas_rect: Rect,
        zoom: f32,
        pointer_screen: Pos2,
        touch_active: bool,
    ) -> Option<(usize, GuideLine)> {
        if !document.guides().visible {
            return None;
        }

        let guide_tolerance = effective_screen_hit_tolerance(
            touch_active,
            GUIDE_HIT_TOLERANCE_SCREEN,
            TOUCH_GUIDE_HIT_TOLERANCE_SCREEN,
        );
        document
            .guides()
            .lines
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, guide)| {
                let distance = guide_screen_distance(canvas_rect, zoom, guide, pointer_screen);
                (distance <= guide_tolerance).then_some((index, guide, distance))
            })
            .min_by(|left, right| left.2.total_cmp(&right.2))
            .map(|(index, guide, _)| (index, guide))
    }

    fn cursor_icon(
        &self,
        ui: &egui::Ui,
        document: &PaintDocument,
        canvas_rect: Rect,
        tool: CanvasToolKind,
    ) -> egui::CursorIcon {
        let touch_active = ui.input(|input| input.any_touches());
        if matches!(self.interaction_mode, InteractionMode::Panning(_)) {
            egui::CursorIcon::Grabbing
        } else if let Some(session) = &self.selection_session {
            match session {
                SelectionSession::SingleResize { .. } | SelectionSession::MultiResize { .. } => {
                    egui::CursorIcon::ResizeNwSe
                }
                SelectionSession::SingleRotate { .. }
                | SelectionSession::MultiRotate { .. }
                | SelectionSession::Marquee { .. } => egui::CursorIcon::Crosshair,
                SelectionSession::GuideMove { axis, .. } => match axis {
                    GuideAxis::Horizontal => egui::CursorIcon::ResizeVertical,
                    GuideAxis::Vertical => egui::CursorIcon::ResizeHorizontal,
                },
                SelectionSession::Move { .. } => egui::CursorIcon::Grabbing,
            }
        } else if tool == CanvasToolKind::Pan || ui.input(|input| input.key_down(egui::Key::Space))
        {
            egui::CursorIcon::Grab
        } else if tool == CanvasToolKind::Select {
            if let Some(pointer) = ui.input(|input| input.pointer.hover_pos()) {
                match self.hit_selection_control(
                    document,
                    canvas_rect,
                    self.view.zoom,
                    pointer,
                    touch_active,
                ) {
                    Some(ControlTarget::SingleResize(_)) | Some(ControlTarget::GroupResize(_)) => {
                        egui::CursorIcon::ResizeNwSe
                    }
                    Some(ControlTarget::SingleRotate) | Some(ControlTarget::GroupRotate) => {
                        egui::CursorIcon::Crosshair
                    }
                    None => match self.hit_guide(
                        document,
                        canvas_rect,
                        self.view.zoom,
                        pointer,
                        touch_active,
                    ) {
                        Some((_, guide)) => match guide.axis {
                            GuideAxis::Horizontal => egui::CursorIcon::ResizeVertical,
                            GuideAxis::Vertical => egui::CursorIcon::ResizeHorizontal,
                        },
                        None => egui::CursorIcon::PointingHand,
                    },
                }
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

fn paint_grid(painter: &Painter, rect: Rect, zoom: f32, document: &PaintDocument) {
    let grid = document.grid();
    if !grid.visible || grid.spacing * zoom < MIN_GRID_VISIBLE_SPACING_SCREEN {
        return;
    }

    let spacing = grid.spacing.max(8.0) * zoom;
    let stroke = EguiStroke::new(1.0, Color32::from_rgba_unmultiplied(80, 92, 118, 48));

    let mut x = rect.left();
    while x <= rect.right() {
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            stroke,
        );
        x += spacing;
    }

    let mut y = rect.top();
    while y <= rect.bottom() {
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            stroke,
        );
        y += spacing;
    }
}

fn paint_guides(
    painter: &Painter,
    rect: Rect,
    zoom: f32,
    document: &PaintDocument,
    hovered_guide: Option<usize>,
    preview_guide: Option<(usize, GuideLine)>,
) {
    if !document.guides().visible {
        return;
    }

    for (index, base_guide) in document.guides().lines.iter().copied().enumerate() {
        let guide = preview_guide
            .filter(|(preview_index, _)| *preview_index == index)
            .map(|(_, guide)| guide)
            .unwrap_or(base_guide);
        let is_preview = preview_guide.is_some_and(|(preview_index, _)| preview_index == index);
        let is_hovered = hovered_guide == Some(index);
        let stroke = if is_preview {
            EguiStroke::new(2.5, Color32::from_rgba_unmultiplied(200, 96, 32, 240))
        } else if is_hovered {
            EguiStroke::new(2.0, Color32::from_rgba_unmultiplied(200, 96, 32, 220))
        } else {
            EguiStroke::new(1.5, Color32::from_rgba_unmultiplied(200, 96, 32, 180))
        };

        match guide.axis {
            GuideAxis::Horizontal => {
                let y = guide_screen_position(rect, zoom, guide);
                painter.line_segment(
                    [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                    stroke,
                );
            }
            GuideAxis::Vertical => {
                let x = guide_screen_position(rect, zoom, guide);
                painter.line_segment(
                    [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                    stroke,
                );
            }
        }
    }
}

fn paint_smart_guides(painter: &Painter, rect: Rect, zoom: f32, smart_guides: SmartGuideOverlay) {
    if smart_guides.is_empty() {
        return;
    }

    let stroke = EguiStroke::new(1.5, Color32::from_rgba_unmultiplied(26, 115, 232, 220));
    if let Some(x) = smart_guides.vertical {
        let screen_x = rect.left() + x * zoom;
        painter.line_segment(
            [
                Pos2::new(screen_x, rect.top()),
                Pos2::new(screen_x, rect.bottom()),
            ],
            stroke,
        );
    }

    if let Some(y) = smart_guides.horizontal {
        let screen_y = rect.top() + y * zoom;
        painter.line_segment(
            [
                Pos2::new(rect.left(), screen_y),
                Pos2::new(rect.right(), screen_y),
            ],
            stroke,
        );
    }
}

fn paint_rulers(
    painter: &Painter,
    viewport: Rect,
    canvas_rect: Rect,
    zoom: f32,
    document: &PaintDocument,
) {
    if !document.rulers().visible {
        return;
    }

    let top_ruler = Rect::from_min_max(
        viewport.min,
        Pos2::new(
            viewport.max.x,
            (viewport.min.y + RULER_THICKNESS).min(viewport.max.y),
        ),
    );
    let left_ruler = Rect::from_min_max(
        viewport.min,
        Pos2::new(
            (viewport.min.x + RULER_THICKNESS).min(viewport.max.x),
            viewport.max.y,
        ),
    );
    let corner = Rect::from_min_max(top_ruler.min, Pos2::new(left_ruler.max.x, top_ruler.max.y));
    let background = Color32::from_rgba_unmultiplied(244, 246, 250, 245);
    let border = EguiStroke::new(1.0, Color32::from_gray(170));
    let style = RulerPaintStyle {
        tick_color: Color32::from_gray(115),
        label_color: Color32::from_gray(78),
        step: ruler_step_world(zoom, 20.0),
        label_step: ruler_step_world(zoom, RULER_LABEL_MIN_SPACING_SCREEN),
    };

    painter.rect_filled(top_ruler, 0.0, background);
    painter.rect_filled(left_ruler, 0.0, background);
    painter.rect_filled(
        corner,
        0.0,
        Color32::from_rgba_unmultiplied(234, 238, 244, 250),
    );
    painter.line_segment(
        [
            Pos2::new(top_ruler.left(), top_ruler.bottom()),
            Pos2::new(top_ruler.right(), top_ruler.bottom()),
        ],
        border,
    );
    painter.line_segment(
        [
            Pos2::new(left_ruler.right(), left_ruler.top()),
            Pos2::new(left_ruler.right(), left_ruler.bottom()),
        ],
        border,
    );

    let visible_x = visible_canvas_range(
        canvas_rect.left(),
        canvas_rect.right(),
        viewport.left(),
        viewport.right(),
        zoom,
        document.canvas_size.width,
    );
    let visible_y = visible_canvas_range(
        canvas_rect.top(),
        canvas_rect.bottom(),
        viewport.top(),
        viewport.bottom(),
        zoom,
        document.canvas_size.height,
    );

    paint_horizontal_ruler_ticks(
        painter,
        top_ruler,
        canvas_rect.left(),
        zoom,
        visible_x,
        style,
    );
    paint_vertical_ruler_ticks(
        painter,
        left_ruler,
        canvas_rect.top(),
        zoom,
        visible_y,
        style,
    );
}

fn paint_horizontal_ruler_ticks(
    painter: &Painter,
    ruler_rect: Rect,
    canvas_screen_start: f32,
    zoom: f32,
    visible_range: (f32, f32),
    style: RulerPaintStyle,
) {
    let mut value = (visible_range.0 / style.step).floor() * style.step;
    while value <= visible_range.1 {
        let x = canvas_screen_start + value * zoom;
        let is_labeled = is_ruler_label_tick(value, style.label_step);
        let tick_height = if is_labeled { 12.0 } else { 7.0 };
        painter.line_segment(
            [
                Pos2::new(x, ruler_rect.bottom()),
                Pos2::new(x, ruler_rect.bottom() - tick_height),
            ],
            EguiStroke::new(1.0, style.tick_color),
        );
        if is_labeled {
            painter.text(
                Pos2::new(x + 2.0, ruler_rect.bottom() - 2.0),
                Align2::LEFT_BOTTOM,
                format_ruler_value(value),
                FontId::monospace(9.0),
                style.label_color,
            );
        }
        value += style.step;
    }
}

fn paint_vertical_ruler_ticks(
    painter: &Painter,
    ruler_rect: Rect,
    canvas_screen_start: f32,
    zoom: f32,
    visible_range: (f32, f32),
    style: RulerPaintStyle,
) {
    let mut value = (visible_range.0 / style.step).floor() * style.step;
    while value <= visible_range.1 {
        let y = canvas_screen_start + value * zoom;
        let is_labeled = is_ruler_label_tick(value, style.label_step);
        let tick_width = if is_labeled { 12.0 } else { 7.0 };
        painter.line_segment(
            [
                Pos2::new(ruler_rect.right(), y),
                Pos2::new(ruler_rect.right() - tick_width, y),
            ],
            EguiStroke::new(1.0, style.tick_color),
        );
        if is_labeled {
            painter.text(
                Pos2::new(ruler_rect.right() - 2.0, y - 1.0),
                Align2::RIGHT_BOTTOM,
                format_ruler_value(value),
                FontId::monospace(9.0),
                style.label_color,
            );
        }
        value += style.step;
    }
}

fn paint_document(
    painter: &Painter,
    rect: Rect,
    zoom: f32,
    document: &PaintDocument,
    overlays: &[(usize, PaintElement)],
) {
    let active_layer_id = document.active_layer_id();
    for layer in document.visible_layers() {
        let is_active_layer = layer.id == active_layer_id;
        for (index, element) in layer.elements.iter().enumerate() {
            if is_active_layer
                && overlays
                    .iter()
                    .any(|(overlay_index, _)| *overlay_index == index)
            {
                continue;
            }
            paint_element(painter, rect, zoom, element, document.background);
        }

        if is_active_layer {
            for (_, element) in overlays {
                paint_element(painter, rect, zoom, element, document.background);
            }
        }
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
        PaintElement::Group(group) => {
            for child in &group.elements {
                paint_element(painter, rect, zoom, child, background);
            }
        }
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
    show_handles: bool,
) {
    if selected_elements.len() == 1 && matches!(&selected_elements[0].1, PaintElement::Shape(_)) {
        paint_single_selection_overlay(
            painter,
            rect,
            zoom,
            selected_elements[0].1.clone(),
            active_control,
            show_handles,
        );
    } else {
        paint_multi_selection_overlay(
            painter,
            rect,
            zoom,
            selected_elements,
            active_control,
            show_handles,
        );
    }
}

fn paint_single_selection_overlay(
    painter: &Painter,
    rect: Rect,
    zoom: f32,
    element: PaintElement,
    active_control: Option<ControlTarget>,
    show_handles: bool,
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
                if show_handles {
                    for (handle, position) in shape_control_handles_screen(shape, rect, zoom) {
                        paint_handle(
                            painter,
                            position,
                            HANDLE_RADIUS,
                            match active_control {
                                Some(ControlTarget::SingleResize(active)) if active == handle => {
                                    active_fill
                                }
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
                                Some(ControlTarget::SingleRotate) => active_fill,
                                _ => inactive_fill,
                            },
                            accent,
                        );
                    }
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

                if show_handles {
                    for (handle, position) in shape_control_handles_screen(shape, rect, zoom) {
                        paint_handle(
                            painter,
                            position,
                            HANDLE_RADIUS,
                            match active_control {
                                Some(ControlTarget::SingleResize(active)) if active == handle => {
                                    active_fill
                                }
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
                                Some(ControlTarget::SingleRotate) => active_fill,
                                _ => inactive_fill,
                            },
                            accent,
                        );
                    }
                }
            }
        },
        PaintElement::Group(_) => {}
    }
}

fn paint_multi_selection_overlay(
    painter: &Painter,
    rect: Rect,
    zoom: f32,
    selected_elements: &[(usize, PaintElement)],
    active_control: Option<ControlTarget>,
    show_handles: bool,
) {
    let item_accent = Color32::from_rgba_unmultiplied(26, 115, 232, 140);
    let group_accent = Color32::from_rgb(26, 115, 232);
    let inactive_fill = Color32::WHITE;
    let active_fill = Color32::from_rgb(26, 115, 232);

    for (_, element) in selected_elements {
        if let Some(bounds) = element.bounds() {
            paint_axis_aligned_bounds(painter, rect, zoom, bounds, item_accent, 4.0, 1.5);
        }
    }

    if let Some(group_bounds) = selection_bounds_from_elements(selected_elements) {
        paint_axis_aligned_bounds(painter, rect, zoom, group_bounds, group_accent, 10.0, 2.5);

        if show_handles {
            for (handle, position) in bounds_control_handles_screen(group_bounds, rect, zoom) {
                paint_handle(
                    painter,
                    position,
                    HANDLE_RADIUS,
                    match active_control {
                        Some(ControlTarget::GroupResize(active)) if active == handle => active_fill,
                        _ => inactive_fill,
                    },
                    group_accent,
                );
            }

            if let Some(rotation_handle) = bounds_rotation_handle_screen(group_bounds, rect, zoom) {
                let top_mid = group_bounds_screen_top_mid(group_bounds, rect, zoom);
                paint_rotation_link(painter, top_mid, rotation_handle, group_accent);
                paint_handle(
                    painter,
                    rotation_handle,
                    HANDLE_RADIUS,
                    match active_control {
                        Some(ControlTarget::GroupRotate) => active_fill,
                        _ => inactive_fill,
                    },
                    group_accent,
                );
            }
        }
    }
}

fn paint_marquee_overlay(painter: &Painter, rect: Rect, zoom: f32, bounds: ElementBounds) {
    let screen_rect = Rect::from_two_pos(
        canvas_to_screen(rect, zoom, bounds.min),
        canvas_to_screen(rect, zoom, bounds.max),
    );
    let fill = Color32::from_rgba_unmultiplied(26, 115, 232, 24);
    let stroke = Color32::from_rgba_unmultiplied(26, 115, 232, 180);
    painter.rect_filled(screen_rect, 4.0, fill);
    painter.rect_stroke(
        screen_rect,
        4.0,
        EguiStroke::new(1.5, stroke),
        egui::StrokeKind::Outside,
    );
}

fn paint_empty_state(painter: &Painter, rect: Rect) {
    let panel = Rect::from_center_size(rect.center(), Vec2::new(500.0, 116.0));
    painter.rect_filled(
        panel,
        12.0,
        Color32::from_rgba_unmultiplied(255, 255, 255, 235),
    );
    painter.rect_stroke(
        panel,
        12.0,
        EguiStroke::new(1.0, Color32::from_rgba_unmultiplied(120, 132, 150, 80)),
        egui::StrokeKind::Outside,
    );
    painter.text(
        Pos2::new(panel.center().x, panel.top() + 30.0),
        Align2::CENTER_CENTER,
        "ブラシか図形ツールを選んで、まず 1 つ描いてみましょう。",
        FontId::proportional(22.0),
        Color32::from_gray(72),
    );
    painter.text(
        Pos2::new(panel.center().x, panel.top() + 60.0),
        Align2::CENTER_CENTER,
        "選択ツールで動かせます。詳しい流れはヘルプやミニチュートリアルで確認できます。",
        FontId::proportional(16.0),
        Color32::from_gray(96),
    );
    painter.text(
        Pos2::new(panel.center().x, panel.top() + 86.0),
        Align2::CENTER_CENTER,
        "JSON保存は再編集用、PNG書き出しは共有用です。",
        FontId::proportional(15.0),
        Color32::from_gray(110),
    );
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

fn expand_bounds(bounds: ElementBounds, padding: f32) -> ElementBounds {
    ElementBounds {
        min: PaintPoint::new(bounds.min.x - padding, bounds.min.y - padding),
        max: PaintPoint::new(bounds.max.x + padding, bounds.max.y + padding),
    }
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

fn bounds_control_handles_screen(
    bounds: ElementBounds,
    rect: Rect,
    zoom: f32,
) -> Vec<(ShapeHandle, Pos2)> {
    vec![
        (
            ShapeHandle::TopLeft,
            canvas_to_screen(rect, zoom, bounds.min),
        ),
        (
            ShapeHandle::TopRight,
            canvas_to_screen(rect, zoom, PaintPoint::new(bounds.max.x, bounds.min.y)),
        ),
        (
            ShapeHandle::BottomRight,
            canvas_to_screen(rect, zoom, bounds.max),
        ),
        (
            ShapeHandle::BottomLeft,
            canvas_to_screen(rect, zoom, PaintPoint::new(bounds.min.x, bounds.max.y)),
        ),
    ]
}

fn bounds_rotation_handle_screen(bounds: ElementBounds, rect: Rect, zoom: f32) -> Option<Pos2> {
    let world_offset = ROTATION_HANDLE_OFFSET_SCREEN / zoom.max(MIN_ZOOM);
    let top_center = PaintPoint::new(bounds.center().x, bounds.min.y - world_offset);
    Some(canvas_to_screen(rect, zoom, top_center))
}

fn group_bounds_screen_top_mid(bounds: ElementBounds, rect: Rect, zoom: f32) -> Pos2 {
    canvas_to_screen(rect, zoom, PaintPoint::new(bounds.center().x, bounds.min.y))
}

fn pointer_screen_angle(rect: Rect, zoom: f32, center: PaintPoint, pointer: Pos2) -> f32 {
    let center_screen = canvas_to_screen(rect, zoom, center);
    (pointer.y - center_screen.y).atan2(pointer.x - center_screen.x)
}

fn transformed_preview_elements(
    base_elements: &[(usize, PaintElement)],
    mut transform: impl FnMut(&PaintElement) -> PaintElement,
) -> Vec<(usize, PaintElement)> {
    base_elements
        .iter()
        .map(|(index, element)| (*index, transform(element)))
        .collect()
}

fn commit_preview_replacements(
    document: &PaintDocument,
    selection_indices: &[usize],
    replacements: Vec<(usize, PaintElement)>,
    mode: DocumentEditMode,
) -> Option<CommittedDocumentEdit> {
    let mut next = document.clone();
    if !next.replace_elements(&replacements) || next == *document {
        return None;
    }

    Some(CommittedDocumentEdit {
        document: next,
        selection_indices: selection_indices.to_vec(),
        mode,
    })
}

fn group_resize_transform(
    bounds: ElementBounds,
    handle: ShapeHandle,
    pointer_world: PaintPoint,
) -> Option<(PaintPoint, f32, f32)> {
    let (sign_x, sign_y) = handle.corner_signs()?;
    let anchor = match handle {
        ShapeHandle::TopLeft => bounds.max,
        ShapeHandle::TopRight => PaintPoint::new(bounds.min.x, bounds.max.y),
        ShapeHandle::BottomRight => bounds.min,
        ShapeHandle::BottomLeft => PaintPoint::new(bounds.max.x, bounds.min.y),
        ShapeHandle::Start | ShapeHandle::End => return None,
    };

    let clamped_x = clamp_group_axis(pointer_world.x, anchor.x, sign_x);
    let clamped_y = clamp_group_axis(pointer_world.y, anchor.y, sign_y);
    let scale_x = (clamped_x - anchor.x).abs() / bounds.width().max(1.0);
    let scale_y = (clamped_y - anchor.y).abs() / bounds.height().max(1.0);
    Some((anchor, scale_x, scale_y))
}

fn clamp_group_axis(value: f32, anchor: f32, sign: f32) -> f32 {
    if sign < 0.0 {
        value.min(anchor - MIN_SELECTION_TRANSFORM_EXTENT)
    } else {
        value.max(anchor + MIN_SELECTION_TRANSFORM_EXTENT)
    }
}

fn bounds_from_points(a: PaintPoint, b: PaintPoint) -> ElementBounds {
    ElementBounds {
        min: PaintPoint::new(a.x.min(b.x), a.y.min(b.y)),
        max: PaintPoint::new(a.x.max(b.x), a.y.max(b.y)),
    }
}

fn marquee_rect_is_visible(bounds: ElementBounds, zoom: f32) -> bool {
    bounds.width() * zoom >= MARQUEE_VISIBLE_MIN_SCREEN
        || bounds.height() * zoom >= MARQUEE_VISIBLE_MIN_SCREEN
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

fn visible_canvas_range(
    canvas_screen_min: f32,
    canvas_screen_max: f32,
    viewport_min: f32,
    viewport_max: f32,
    zoom: f32,
    canvas_limit: f32,
) -> (f32, f32) {
    let intersect_min = canvas_screen_min.max(viewport_min);
    let intersect_max = canvas_screen_max.min(viewport_max);
    if intersect_min >= intersect_max {
        return (0.0, -1.0);
    }

    (
        ((intersect_min - canvas_screen_min) / zoom)
            .floor()
            .clamp(0.0, canvas_limit),
        ((intersect_max - canvas_screen_min) / zoom)
            .ceil()
            .clamp(0.0, canvas_limit),
    )
}

fn ruler_step_world(zoom: f32, min_screen_spacing: f32) -> f32 {
    let target_world = (min_screen_spacing / zoom.max(MIN_ZOOM)).max(0.1);
    let base = 10_f32.powf(target_world.log10().floor());
    for factor in [1.0, 2.0, 5.0, 10.0] {
        let step = base * factor;
        if step >= target_world {
            return step;
        }
    }
    base * 10.0
}

fn is_ruler_label_tick(value: f32, label_step: f32) -> bool {
    let snapped = (value / label_step).round() * label_step;
    (snapped - value).abs() <= 0.01
}

fn format_ruler_value(value: f32) -> String {
    if (value.round() - value).abs() < 0.01 {
        format!("{:.0}", value)
    } else {
        format!("{value:.1}")
    }
}

fn guide_screen_position(rect: Rect, zoom: f32, guide: GuideLine) -> f32 {
    match guide.axis {
        GuideAxis::Horizontal => rect.top() + guide.position * zoom,
        GuideAxis::Vertical => rect.left() + guide.position * zoom,
    }
}

fn guide_screen_distance(rect: Rect, zoom: f32, guide: GuideLine, pointer: Pos2) -> f32 {
    match guide.axis {
        GuideAxis::Horizontal => (pointer.y - guide_screen_position(rect, zoom, guide)).abs(),
        GuideAxis::Vertical => (pointer.x - guide_screen_position(rect, zoom, guide)).abs(),
    }
}

fn clamp_guide_position_for_document(
    document: &PaintDocument,
    axis: GuideAxis,
    position: f32,
) -> f32 {
    match axis {
        GuideAxis::Horizontal => position.clamp(0.0, document.canvas_size.height),
        GuideAxis::Vertical => position.clamp(0.0, document.canvas_size.width),
    }
}

fn snap_tolerance_world(zoom: f32) -> f32 {
    SNAP_TOLERANCE_SCREEN / zoom.max(MIN_ZOOM)
}

fn snap_point_for_document(document: &PaintDocument, zoom: f32, point: PaintPoint) -> PaintPoint {
    PaintPoint::new(
        snap_axis_value_for_document(document, zoom, GuideAxis::Vertical, point.x),
        snap_axis_value_for_document(document, zoom, GuideAxis::Horizontal, point.y),
    )
}

#[derive(Debug, Clone, Copy)]
struct MoveSnapPreview {
    delta: PaintVector,
    smart_guides: SmartGuideOverlay,
}

#[derive(Debug, Clone, Copy)]
struct SmartGuideAxisMatch {
    target: f32,
    delta: f32,
}

fn snap_move_preview_for_document(
    document: &PaintDocument,
    zoom: f32,
    selection_layer_id: LayerId,
    selection_indices: &[usize],
    base_elements: &[(usize, PaintElement)],
    raw_delta: PaintVector,
) -> MoveSnapPreview {
    if raw_delta.is_zero() {
        return MoveSnapPreview {
            delta: raw_delta,
            smart_guides: SmartGuideOverlay::empty(),
        };
    }

    let transformed =
        transformed_preview_elements(base_elements, |element| element.translated(raw_delta));
    let Some(bounds) = selection_bounds_from_elements(&transformed) else {
        return MoveSnapPreview {
            delta: raw_delta,
            smart_guides: SmartGuideOverlay::empty(),
        };
    };

    let snap_delta = snap_delta_for_bounds(document, zoom, bounds);
    let mut delta = PaintVector::new(raw_delta.dx + snap_delta.dx, raw_delta.dy + snap_delta.dy);
    let mut smart_guides = SmartGuideOverlay::empty();

    if document.smart_guides().visible {
        let transformed =
            transformed_preview_elements(base_elements, |element| element.translated(delta));
        if let Some(bounds) = selection_bounds_from_elements(&transformed) {
            let mut smart_bounds = bounds;
            if let Some(matched) = smart_guide_axis_match(
                document,
                selection_layer_id,
                selection_indices,
                zoom,
                GuideAxis::Vertical,
                smart_bounds,
            ) {
                delta.dx += matched.delta;
                smart_guides.vertical = Some(matched.target);
                smart_bounds = smart_bounds.translate(PaintVector::new(matched.delta, 0.0));
            }

            if let Some(matched) = smart_guide_axis_match(
                document,
                selection_layer_id,
                selection_indices,
                zoom,
                GuideAxis::Horizontal,
                smart_bounds,
            ) {
                delta.dy += matched.delta;
                smart_guides.horizontal = Some(matched.target);
            }
        }
    }

    MoveSnapPreview {
        delta,
        smart_guides,
    }
}

fn smart_guide_axis_match(
    document: &PaintDocument,
    selection_layer_id: LayerId,
    selection_indices: &[usize],
    zoom: f32,
    axis: GuideAxis,
    moving_bounds: ElementBounds,
) -> Option<SmartGuideAxisMatch> {
    let moving_positions = match axis {
        GuideAxis::Horizontal => [
            moving_bounds.min.y,
            moving_bounds.center().y,
            moving_bounds.max.y,
        ],
        GuideAxis::Vertical => [
            moving_bounds.min.x,
            moving_bounds.center().x,
            moving_bounds.max.x,
        ],
    };
    let tolerance = SMART_GUIDE_TOLERANCE_SCREEN / zoom.max(MIN_ZOOM);
    let mut best_match: Option<SmartGuideAxisMatch> = None;

    for layer in document.visible_layers() {
        if layer.locked {
            continue;
        }

        for (index, element) in layer.elements.iter().enumerate() {
            if layer.id == selection_layer_id && selection_indices.contains(&index) {
                continue;
            }

            let Some(bounds) = element.bounds() else {
                continue;
            };

            let target_positions = match axis {
                GuideAxis::Horizontal => [bounds.min.y, bounds.center().y, bounds.max.y],
                GuideAxis::Vertical => [bounds.min.x, bounds.center().x, bounds.max.x],
            };

            for moving_position in moving_positions {
                for target in target_positions {
                    let delta = target - moving_position;
                    if delta.abs() <= tolerance
                        && best_match.is_none_or(|best| delta.abs() < best.delta.abs())
                    {
                        best_match = Some(SmartGuideAxisMatch { target, delta });
                    }
                }
            }
        }
    }

    best_match
}

fn snap_delta_for_bounds(
    document: &PaintDocument,
    zoom: f32,
    bounds: ElementBounds,
) -> PaintVector {
    let x = snap_axis_delta_for_document(
        document,
        zoom,
        GuideAxis::Vertical,
        &[bounds.min.x, bounds.center().x, bounds.max.x],
    )
    .unwrap_or(0.0);
    let y = snap_axis_delta_for_document(
        document,
        zoom,
        GuideAxis::Horizontal,
        &[bounds.min.y, bounds.center().y, bounds.max.y],
    )
    .unwrap_or(0.0);
    PaintVector::new(x, y)
}

fn snap_axis_value_for_document(
    document: &PaintDocument,
    zoom: f32,
    axis: GuideAxis,
    value: f32,
) -> f32 {
    value + snap_axis_delta_for_document(document, zoom, axis, &[value]).unwrap_or(0.0)
}

fn snap_axis_delta_for_document(
    document: &PaintDocument,
    zoom: f32,
    axis: GuideAxis,
    candidates: &[f32],
) -> Option<f32> {
    let tolerance = snap_tolerance_world(zoom);
    let limit = match axis {
        GuideAxis::Horizontal => document.canvas_size.height,
        GuideAxis::Vertical => document.canvas_size.width,
    };
    let mut best_delta = None;

    if document.grid().snap_enabled {
        let spacing = document.grid().spacing.max(8.0);
        for candidate in candidates {
            let target = (*candidate / spacing).round() * spacing;
            let target = target.clamp(0.0, limit);
            let delta = target - *candidate;
            if delta.abs() <= tolerance
                && best_delta.is_none_or(|best: f32| delta.abs() < best.abs())
            {
                best_delta = Some(delta);
            }
        }
    }

    if document.guides().snap_enabled {
        for guide in &document.guides().lines {
            if guide.axis != axis {
                continue;
            }

            for candidate in candidates {
                let delta = guide.position - *candidate;
                if delta.abs() <= tolerance
                    && best_delta.is_none_or(|best: f32| delta.abs() < best.abs())
                {
                    best_delta = Some(delta);
                }
            }
        }
    }

    best_delta
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

fn effective_screen_hit_tolerance(touch_active: bool, base: f32, touch_minimum: f32) -> f32 {
    if touch_active {
        base.max(touch_minimum)
    } else {
        base
    }
}

fn hit_tolerance_world(zoom: f32, touch_active: bool, base: f32, touch_minimum: f32) -> f32 {
    effective_screen_hit_tolerance(touch_active, base, touch_minimum) / zoom.max(MIN_ZOOM)
}

fn touch_contact_kind_from_events(events: &[egui::Event]) -> Option<TouchContactKind> {
    let mut saw_touch = false;
    let mut saw_force = false;
    for event in events {
        if let egui::Event::Touch { force, .. } = event {
            saw_touch = true;
            saw_force |= force.is_some();
        }
    }

    if saw_force {
        Some(TouchContactKind::PenLike)
    } else if saw_touch {
        Some(TouchContactKind::Finger)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CanvasController, CanvasViewState, SelectionSession, SelectionState, TouchContactKind,
        bounds_from_points, canvas_rect, canvas_to_screen, effective_screen_hit_tolerance,
        group_resize_transform, hit_tolerance_world, marquee_rect_is_visible,
        normalize_selection_indices, ruler_step_world, screen_to_canvas,
        shape_rotation_handle_screen, smart_guide_axis_match, snap_axis_value_for_document,
        snap_point_for_document, touch_contact_kind_from_events,
    };
    use crate::model::{
        CanvasSize, ElementBounds, GuideAxis, LayerId, PaintDocument, PaintPoint, PaintVector,
        RgbaColor, ShapeElement, ShapeHandle, ShapeKind,
    };
    use eframe::egui::{Event, Pos2, Rect, TouchDeviceId, TouchId, TouchPhase, Vec2};

    fn test_document() -> PaintDocument {
        PaintDocument {
            canvas_size: CanvasSize::new(100.0, 100.0),
            ..PaintDocument::default()
        }
    }

    #[test]
    fn selection_single_returns_none_for_empty_selection() {
        let selection = SelectionState::default();
        assert_eq!(selection.single(), None);
    }

    #[test]
    fn selection_single_returns_index_for_single_selection() {
        let mut selection = SelectionState::default();
        selection.set_only(1 as LayerId, 4);
        assert_eq!(selection.single(), Some(4));
    }

    #[test]
    fn selection_single_returns_none_for_multi_selection() {
        let layer_id = 1 as LayerId;
        let mut selection = SelectionState::default();
        selection.set_indices(layer_id, vec![2, 5]);
        assert_eq!(selection.single(), None);
    }

    #[test]
    fn single_selection_can_start_move_from_inside_selection_bounds() {
        let mut document = test_document();
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            2.0,
            PaintPoint::new(20.0, 20.0),
            PaintPoint::new(80.0, 80.0),
        ));

        let mut controller = CanvasController::default();
        controller.selection.set_only(document.active_layer_id(), 0);

        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0));
        let canvas_rect = canvas_rect(
            viewport,
            controller.view.pan,
            controller.view.zoom,
            document.canvas_size,
        );
        let pointer_world = PaintPoint::new(50.0, 50.0);
        let pointer_screen = canvas_to_screen(canvas_rect, controller.view.zoom, pointer_world);

        controller.begin_selection_interaction(
            &document,
            canvas_rect,
            pointer_screen,
            pointer_world,
            false,
            false,
        );

        assert!(matches!(
            controller.selection_session,
            Some(SelectionSession::Move { .. })
        ));
    }

    #[test]
    fn multi_selection_can_start_move_from_group_bounds_gap() {
        let mut document = test_document();
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            2.0,
            PaintPoint::new(20.0, 20.0),
            PaintPoint::new(35.0, 40.0),
        ));
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            2.0,
            PaintPoint::new(65.0, 20.0),
            PaintPoint::new(80.0, 40.0),
        ));

        let mut controller = CanvasController::default();
        controller
            .selection
            .set_indices(document.active_layer_id(), vec![0, 1]);

        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0));
        let canvas_rect = canvas_rect(
            viewport,
            controller.view.pan,
            controller.view.zoom,
            document.canvas_size,
        );
        let pointer_world = PaintPoint::new(50.0, 30.0);
        let pointer_screen = canvas_to_screen(canvas_rect, controller.view.zoom, pointer_world);

        controller.begin_selection_interaction(
            &document,
            canvas_rect,
            pointer_screen,
            pointer_world,
            false,
            false,
        );

        assert!(matches!(
            controller.selection_session,
            Some(SelectionSession::Move { .. })
        ));
    }

    #[test]
    fn selection_handle_hit_still_wins_over_move_from_bounds() {
        let mut document = test_document();
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            2.0,
            PaintPoint::new(20.0, 20.0),
            PaintPoint::new(80.0, 80.0),
        ));

        let mut controller = CanvasController::default();
        controller.selection.set_only(document.active_layer_id(), 0);

        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0));
        let canvas_rect = canvas_rect(
            viewport,
            controller.view.pan,
            controller.view.zoom,
            document.canvas_size,
        );
        let pointer_world = PaintPoint::new(20.0, 20.0);
        let pointer_screen = canvas_to_screen(canvas_rect, controller.view.zoom, pointer_world);

        controller.begin_selection_interaction(
            &document,
            canvas_rect,
            pointer_screen,
            pointer_world,
            false,
            false,
        );

        assert!(matches!(
            controller.selection_session,
            Some(SelectionSession::SingleResize { .. })
        ));
    }

    #[test]
    fn background_drag_outside_selection_bounds_still_starts_marquee() {
        let mut document = test_document();
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            2.0,
            PaintPoint::new(20.0, 20.0),
            PaintPoint::new(40.0, 40.0),
        ));

        let mut controller = CanvasController::default();
        controller.selection.set_only(document.active_layer_id(), 0);

        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0));
        let canvas_rect = canvas_rect(
            viewport,
            controller.view.pan,
            controller.view.zoom,
            document.canvas_size,
        );
        let pointer_world = PaintPoint::new(90.0, 90.0);
        let pointer_screen = canvas_to_screen(canvas_rect, controller.view.zoom, pointer_world);

        controller.begin_selection_interaction(
            &document,
            canvas_rect,
            pointer_screen,
            pointer_world,
            false,
            false,
        );

        assert!(matches!(
            controller.selection_session,
            Some(SelectionSession::Marquee { .. })
        ));
    }

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

    #[test]
    fn marquee_bounds_cover_drag_area() {
        let bounds = bounds_from_points(PaintPoint::new(40.0, 80.0), PaintPoint::new(10.0, 30.0));
        assert_eq!(bounds.min, PaintPoint::new(10.0, 30.0));
        assert_eq!(bounds.max, PaintPoint::new(40.0, 80.0));
    }

    #[test]
    fn marquee_visibility_uses_screen_size() {
        let bounds = ElementBounds {
            min: PaintPoint::new(10.0, 10.0),
            max: PaintPoint::new(12.0, 12.0),
        };
        assert!(!marquee_rect_is_visible(bounds, 1.0));
        assert!(marquee_rect_is_visible(bounds, 2.0));
    }

    #[test]
    fn group_resize_transform_clamps_past_anchor() {
        let bounds = ElementBounds {
            min: PaintPoint::new(20.0, 20.0),
            max: PaintPoint::new(80.0, 80.0),
        };
        let (anchor, scale_x, scale_y) =
            group_resize_transform(bounds, ShapeHandle::TopLeft, PaintPoint::new(79.0, 79.0))
                .expect("group resize should work");

        assert_eq!(anchor, PaintPoint::new(80.0, 80.0));
        assert!(scale_x > 0.0);
        assert!(scale_y > 0.0);
    }

    #[test]
    fn snap_axis_value_uses_grid_when_close() {
        let document = PaintDocument::default()
            .toggled_grid_snap_document()
            .expect("enable grid snap");

        let snapped = snap_axis_value_for_document(&document, 1.0, GuideAxis::Vertical, 51.0);
        assert_eq!(snapped, 48.0);
    }

    #[test]
    fn snap_axis_value_respects_updated_grid_spacing() {
        let document = PaintDocument::default()
            .set_grid_spacing_document(32.0)
            .expect("set grid spacing")
            .toggled_grid_snap_document()
            .expect("enable grid snap");

        let snapped = snap_axis_value_for_document(&document, 1.0, GuideAxis::Vertical, 33.5);
        assert_eq!(snapped, 32.0);
    }

    #[test]
    fn snap_point_uses_guides_when_close() {
        let mut document = PaintDocument::default()
            .add_guide_document(GuideAxis::Vertical, 120.0)
            .expect("add guide");
        if !document.guides().snap_enabled {
            document = document
                .toggled_guides_snap_document()
                .expect("enable guide snap");
        }

        let snapped = snap_point_for_document(&document, 1.0, PaintPoint::new(126.0, 32.0));
        assert_eq!(snapped.x, 120.0);
    }

    #[test]
    fn smart_guide_match_aligns_to_other_element_center() {
        let mut document = PaintDocument::default();
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            2.0,
            PaintPoint::new(100.0, 20.0),
            PaintPoint::new(140.0, 60.0),
        ));
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            2.0,
            PaintPoint::new(20.0, 20.0),
            PaintPoint::new(30.0, 60.0),
        ));

        let moving_bounds = document
            .element(1)
            .and_then(|element| element.bounds())
            .expect("bounds");
        let moved_bounds = moving_bounds.translate(PaintVector::new(94.0, 0.0));
        let matched = smart_guide_axis_match(
            &document,
            document.active_layer_id(),
            &[1],
            1.0,
            GuideAxis::Vertical,
            moved_bounds,
        )
        .expect("smart guide should match");

        assert!((matched.target - 120.0).abs() < 0.01);
        assert!((matched.delta - 1.0).abs() < 0.01);
    }

    #[test]
    fn touch_hit_tolerance_expands_for_touch_input() {
        assert_eq!(effective_screen_hit_tolerance(false, 8.0, 18.0), 8.0);
        assert_eq!(effective_screen_hit_tolerance(true, 8.0, 18.0), 18.0);
    }

    #[test]
    fn touch_world_tolerance_scales_with_zoom() {
        let tolerance = hit_tolerance_world(2.0, true, 8.0, 18.0);
        assert!((tolerance - 9.0).abs() < f32::EPSILON);
    }

    #[test]
    fn touch_events_with_force_are_treated_as_pen_like() {
        let kind = touch_contact_kind_from_events(&[Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(1),
            phase: TouchPhase::Start,
            pos: Pos2::new(10.0, 20.0),
            force: Some(0.5),
        }]);
        assert_eq!(kind, Some(TouchContactKind::PenLike));
    }

    #[test]
    fn touch_events_without_force_are_treated_as_finger() {
        let kind = touch_contact_kind_from_events(&[Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(2),
            phase: TouchPhase::Start,
            pos: Pos2::new(10.0, 20.0),
            force: None,
        }]);
        assert_eq!(kind, Some(TouchContactKind::Finger));
    }

    #[test]
    fn ruler_step_world_grows_when_zoomed_out() {
        let close = ruler_step_world(2.0, 20.0);
        let far = ruler_step_world(0.25, 20.0);

        assert!(far > close);
    }
}
