use serde::{Deserialize, Serialize};

const HIT_TOLERANCE_MIN: f32 = 2.5;
const MIN_SHAPE_HALF_EXTENT: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CanvasSize {
    pub width: f32,
    pub height: f32,
}

impl CanvasSize {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

impl Default for CanvasSize {
    fn default() -> Self {
        Self::new(1600.0, 900.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RgbaColor {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::new(r, g, b, a)
    }

    pub const fn white() -> Self {
        Self::new(255, 255, 255, 255)
    }

    pub const fn charcoal() -> Self {
        Self::new(37, 37, 41, 255)
    }
}

impl Default for RgbaColor {
    fn default() -> Self {
        Self::charcoal()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PaintPoint {
    pub x: f32,
    pub y: f32,
}

impl PaintPoint {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn midpoint(self, other: Self) -> Self {
        Self::new((self.x + other.x) * 0.5, (self.y + other.y) * 0.5)
    }

    pub fn offset(self, delta: PaintVector) -> Self {
        Self::new(self.x + delta.dx, self.y + delta.dy)
    }

    pub fn distance_to(self, other: Self) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }

    pub fn angle_from(self, center: Self) -> f32 {
        (self.y - center.y).atan2(self.x - center.x)
    }

    pub fn rotated_around(self, center: Self, angle_radians: f32) -> Self {
        center.offset(rotate_vector(
            PaintVector::new(self.x - center.x, self.y - center.y),
            angle_radians,
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PaintVector {
    pub dx: f32,
    pub dy: f32,
}

impl PaintVector {
    pub const fn new(dx: f32, dy: f32) -> Self {
        Self { dx, dy }
    }

    pub fn is_zero(self) -> bool {
        self.dx.abs() < f32::EPSILON && self.dy.abs() < f32::EPSILON
    }

    pub fn midpoint(self, other: Self) -> Self {
        Self::new((self.dx + other.dx) * 0.5, (self.dy + other.dy) * 0.5)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElementBounds {
    pub min: PaintPoint,
    pub max: PaintPoint,
}

impl ElementBounds {
    pub fn from_points(points: &[PaintPoint], padding: f32) -> Option<Self> {
        let first = *points.first()?;
        let mut min_x = first.x;
        let mut min_y = first.y;
        let mut max_x = first.x;
        let mut max_y = first.y;

        for point in points.iter().skip(1) {
            min_x = min_x.min(point.x);
            min_y = min_y.min(point.y);
            max_x = max_x.max(point.x);
            max_y = max_y.max(point.y);
        }

        Some(Self {
            min: PaintPoint::new(min_x - padding, min_y - padding),
            max: PaintPoint::new(max_x + padding, max_y + padding),
        })
    }

    pub fn from_line(start: PaintPoint, end: PaintPoint, padding: f32) -> Self {
        Self {
            min: PaintPoint::new(start.x.min(end.x) - padding, start.y.min(end.y) - padding),
            max: PaintPoint::new(start.x.max(end.x) + padding, start.y.max(end.y) + padding),
        }
    }

    pub fn translate(self, delta: PaintVector) -> Self {
        Self {
            min: self.min.offset(delta),
            max: self.max.offset(delta),
        }
    }

    pub fn contains(self, point: PaintPoint) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }

    pub fn width(self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn center(self) -> PaintPoint {
        PaintPoint::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
        )
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            min: PaintPoint::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: PaintPoint::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Brush,
    Eraser,
}

impl ToolKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Brush => "Brush",
            Self::Eraser => "Eraser",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Line,
}

impl ShapeKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Line => "Line",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignmentKind {
    Left,
    HorizontalCenter,
    Right,
    Top,
    VerticalCenter,
    Bottom,
}

impl AlignmentKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Left => "Align Left",
            Self::HorizontalCenter => "Align Center Horizontally",
            Self::Right => "Align Right",
            Self::Top => "Align Top",
            Self::VerticalCenter => "Align Center Vertically",
            Self::Bottom => "Align Bottom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackOrderCommand {
    BringToFront,
    SendToBack,
    BringForward,
    SendBackward,
}

impl StackOrderCommand {
    pub const fn label(self) -> &'static str {
        match self {
            Self::BringToFront => "Bring to Front",
            Self::SendToBack => "Send to Back",
            Self::BringForward => "Bring Forward",
            Self::SendBackward => "Send Backward",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeHandle {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
    Start,
    End,
}

impl ShapeHandle {
    pub fn corner_signs(self) -> Option<(f32, f32)> {
        match self {
            Self::TopLeft => Some((-1.0, -1.0)),
            Self::TopRight => Some((1.0, -1.0)),
            Self::BottomRight => Some((1.0, 1.0)),
            Self::BottomLeft => Some((-1.0, 1.0)),
            Self::Start | Self::End => None,
        }
    }

    pub fn opposite_corner(self) -> Option<Self> {
        match self {
            Self::TopLeft => Some(Self::BottomRight),
            Self::TopRight => Some(Self::BottomLeft),
            Self::BottomRight => Some(Self::TopLeft),
            Self::BottomLeft => Some(Self::TopRight),
            Self::Start | Self::End => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub tool: ToolKind,
    pub color: RgbaColor,
    pub width: f32,
    pub points: Vec<PaintPoint>,
}

impl Stroke {
    pub fn new(tool: ToolKind, color: RgbaColor, width: f32) -> Self {
        Self {
            tool,
            color,
            width,
            points: Vec::new(),
        }
    }

    pub fn push_point(&mut self, point: PaintPoint) {
        self.points.push(point);
    }

    pub fn translated(&self, delta: PaintVector) -> Self {
        let mut translated = self.clone();
        for point in &mut translated.points {
            *point = point.offset(delta);
        }
        translated
    }

    pub fn bounds(&self) -> Option<ElementBounds> {
        ElementBounds::from_points(&self.points, self.width.max(1.0) * 0.5)
    }

    pub fn hit_test(&self, point: PaintPoint, tolerance: f32) -> bool {
        let tolerance = tolerance.max(HIT_TOLERANCE_MIN);

        match self.points.as_slice() {
            [] => false,
            [only] => only.distance_to(point) <= (self.width * 0.5 + tolerance),
            [first, rest @ ..] => {
                let radius = self.width * 0.5 + tolerance;
                let radius_sq = radius * radius;
                let mut previous = *first;

                for current in rest {
                    if distance_to_segment_sq(point, previous, *current) <= radius_sq {
                        return true;
                    }
                    previous = *current;
                }

                false
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShapeElement {
    pub kind: ShapeKind,
    pub color: RgbaColor,
    pub width: f32,
    pub start: PaintPoint,
    pub end: PaintPoint,
    #[serde(default)]
    pub rotation_radians: f32,
}

impl ShapeElement {
    pub fn new(
        kind: ShapeKind,
        color: RgbaColor,
        width: f32,
        start: PaintPoint,
        end: PaintPoint,
    ) -> Self {
        Self {
            kind,
            color,
            width,
            start,
            end,
            rotation_radians: 0.0,
        }
    }

    pub fn with_rotation(
        kind: ShapeKind,
        color: RgbaColor,
        width: f32,
        start: PaintPoint,
        end: PaintPoint,
        rotation_radians: f32,
    ) -> Self {
        Self {
            kind,
            color,
            width,
            start,
            end,
            rotation_radians,
        }
    }

    pub fn center(&self) -> PaintPoint {
        self.start.midpoint(self.end)
    }

    pub fn half_extents(&self) -> PaintVector {
        PaintVector::new(
            (self.end.x - self.start.x).abs() * 0.5,
            (self.end.y - self.start.y).abs() * 0.5,
        )
    }

    pub fn rotation_center(&self) -> PaintPoint {
        match self.kind {
            ShapeKind::Line => self.start.midpoint(self.end),
            ShapeKind::Rectangle | ShapeKind::Ellipse => self.center(),
        }
    }

    pub fn line_angle(&self) -> f32 {
        (self.end.y - self.start.y).atan2(self.end.x - self.start.x)
    }

    pub fn control_handles(&self) -> Vec<(ShapeHandle, PaintPoint)> {
        match self.kind {
            ShapeKind::Line => vec![
                (ShapeHandle::Start, self.start),
                (ShapeHandle::End, self.end),
            ],
            ShapeKind::Rectangle | ShapeKind::Ellipse => {
                let corners = self.rotated_box_corners();
                vec![
                    (ShapeHandle::TopLeft, corners[0]),
                    (ShapeHandle::TopRight, corners[1]),
                    (ShapeHandle::BottomRight, corners[2]),
                    (ShapeHandle::BottomLeft, corners[3]),
                ]
            }
        }
    }

    pub fn rotation_handle_position(&self, distance: f32) -> Option<PaintPoint> {
        match self.kind {
            ShapeKind::Line => {
                let center = self.rotation_center();
                let direction =
                    PaintVector::new(self.end.x - self.start.x, self.end.y - self.start.y);
                let length = (direction.dx * direction.dx + direction.dy * direction.dy).sqrt();
                if length <= f32::EPSILON {
                    return None;
                }

                let normal = PaintVector::new(-direction.dy / length, direction.dx / length);
                Some(center.offset(PaintVector::new(
                    normal.dx * (length * 0.5 + distance),
                    normal.dy * (length * 0.5 + distance),
                )))
            }
            ShapeKind::Rectangle | ShapeKind::Ellipse => {
                let center = self.center();
                let half = self.half_extents();
                Some(center.offset(rotate_vector(
                    PaintVector::new(0.0, -(half.dy + distance)),
                    self.rotation_radians,
                )))
            }
        }
    }

    pub fn translated(&self, delta: PaintVector) -> Self {
        Self {
            kind: self.kind,
            color: self.color,
            width: self.width,
            start: self.start.offset(delta),
            end: self.end.offset(delta),
            rotation_radians: self.rotation_radians,
        }
    }

    pub fn rotated_by(&self, delta_radians: f32) -> Self {
        match self.kind {
            ShapeKind::Line => {
                let center = self.rotation_center();
                Self {
                    kind: self.kind,
                    color: self.color,
                    width: self.width,
                    start: self.start.rotated_around(center, delta_radians),
                    end: self.end.rotated_around(center, delta_radians),
                    rotation_radians: 0.0,
                }
            }
            ShapeKind::Rectangle | ShapeKind::Ellipse => Self {
                kind: self.kind,
                color: self.color,
                width: self.width,
                start: self.start,
                end: self.end,
                rotation_radians: self.rotation_radians + delta_radians,
            },
        }
    }

    pub fn resized_by_handle(&self, handle: ShapeHandle, world_point: PaintPoint) -> Option<Self> {
        match self.kind {
            ShapeKind::Line => self.resized_line(handle, world_point),
            ShapeKind::Rectangle | ShapeKind::Ellipse => self.resized_box(handle, world_point),
        }
    }

    pub fn bounds(&self) -> ElementBounds {
        match self.kind {
            ShapeKind::Line => {
                ElementBounds::from_line(self.start, self.end, self.width.max(1.0) * 0.5)
            }
            ShapeKind::Rectangle | ShapeKind::Ellipse => {
                ElementBounds::from_points(&self.rotated_box_corners(), self.width.max(1.0) * 0.5)
                    .expect("rotated box corners should not be empty")
            }
        }
    }

    pub fn hit_test(&self, point: PaintPoint, tolerance: f32) -> bool {
        let tolerance = tolerance.max(HIT_TOLERANCE_MIN);
        match self.kind {
            ShapeKind::Line => {
                let radius = self.width * 0.5 + tolerance;
                distance_to_segment_sq(point, self.start, self.end) <= radius * radius
            }
            ShapeKind::Rectangle => {
                let local_point = self.local_box_point(point);
                hit_test_box_outline(self.start, self.end, self.width, local_point, tolerance)
            }
            ShapeKind::Ellipse => {
                let local_point = self.local_box_point(point);
                hit_test_ellipse_outline(self.start, self.end, self.width, local_point, tolerance)
            }
        }
    }

    pub fn selection_outline(&self) -> Vec<PaintPoint> {
        match self.kind {
            ShapeKind::Line => vec![self.start, self.end],
            ShapeKind::Rectangle | ShapeKind::Ellipse => self.rotated_box_corners().to_vec(),
        }
    }

    pub fn is_transform_editable(&self) -> bool {
        true
    }

    pub fn rotated_box_corners(&self) -> [PaintPoint; 4] {
        let center = self.center();
        let half = self.half_extents();
        let local = [
            PaintVector::new(-half.dx, -half.dy),
            PaintVector::new(half.dx, -half.dy),
            PaintVector::new(half.dx, half.dy),
            PaintVector::new(-half.dx, half.dy),
        ];

        local.map(|offset| center.offset(rotate_vector(offset, self.rotation_radians)))
    }

    fn resized_line(&self, handle: ShapeHandle, world_point: PaintPoint) -> Option<Self> {
        let mut next = *self;
        match handle {
            ShapeHandle::Start => next.start = world_point,
            ShapeHandle::End => next.end = world_point,
            _ => return None,
        }
        Some(next)
    }

    fn resized_box(&self, handle: ShapeHandle, world_point: PaintPoint) -> Option<Self> {
        let (sign_x, sign_y) = handle.corner_signs()?;
        let anchor = self.opposite_corner_local(handle)?;
        let center = self.center();
        let local_pointer = self.world_to_local_offset(world_point);
        let clamped_pointer = PaintVector::new(
            clamp_resized_axis(local_pointer.dx, anchor.dx, sign_x),
            clamp_resized_axis(local_pointer.dy, anchor.dy, sign_y),
        );
        let new_center_local = anchor.midpoint(clamped_pointer);
        let new_half = PaintVector::new(
            (anchor.dx - clamped_pointer.dx).abs() * 0.5,
            (anchor.dy - clamped_pointer.dy).abs() * 0.5,
        );
        let new_center = center.offset(rotate_vector(new_center_local, self.rotation_radians));

        Some(Self {
            kind: self.kind,
            color: self.color,
            width: self.width,
            start: PaintPoint::new(new_center.x - new_half.dx, new_center.y - new_half.dy),
            end: PaintPoint::new(new_center.x + new_half.dx, new_center.y + new_half.dy),
            rotation_radians: self.rotation_radians,
        })
    }

    fn world_to_local_offset(&self, point: PaintPoint) -> PaintVector {
        let center = self.center();
        rotate_vector(
            PaintVector::new(point.x - center.x, point.y - center.y),
            -self.rotation_radians,
        )
    }

    fn local_box_point(&self, point: PaintPoint) -> PaintPoint {
        let local = self.world_to_local_offset(point);
        let center = self.center();
        PaintPoint::new(center.x + local.dx, center.y + local.dy)
    }

    fn opposite_corner_local(&self, handle: ShapeHandle) -> Option<PaintVector> {
        let opposite = handle.opposite_corner()?;
        let half = self.half_extents();
        let (sign_x, sign_y) = opposite.corner_signs()?;
        Some(PaintVector::new(half.dx * sign_x, half.dy * sign_y))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "element_type", rename_all = "snake_case")]
pub enum PaintElement {
    Stroke(Stroke),
    Shape(ShapeElement),
}

impl PaintElement {
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Stroke(stroke) => stroke.tool.label(),
            Self::Shape(shape) => shape.kind.label(),
        }
    }

    pub fn bounds(&self) -> Option<ElementBounds> {
        match self {
            Self::Stroke(stroke) => stroke.bounds(),
            Self::Shape(shape) => Some(shape.bounds()),
        }
    }

    pub fn hit_test(&self, point: PaintPoint, tolerance: f32) -> bool {
        match self {
            Self::Stroke(stroke) => stroke.hit_test(point, tolerance),
            Self::Shape(shape) => shape.hit_test(point, tolerance),
        }
    }

    pub fn translated(&self, delta: PaintVector) -> Self {
        match self {
            Self::Stroke(stroke) => Self::Stroke(stroke.translated(delta)),
            Self::Shape(shape) => Self::Shape(shape.translated(delta)),
        }
    }

    pub fn is_transform_editable(&self) -> bool {
        matches!(self, Self::Shape(shape) if shape.is_transform_editable())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaintDocument {
    #[serde(default)]
    pub canvas_size: CanvasSize,
    #[serde(default = "RgbaColor::white")]
    pub background: RgbaColor,
    #[serde(default)]
    pub elements: Vec<PaintElement>,
}

impl Default for PaintDocument {
    fn default() -> Self {
        Self {
            canvas_size: CanvasSize::default(),
            background: RgbaColor::white(),
            elements: Vec::new(),
        }
    }
}

impl PaintDocument {
    pub fn has_elements(&self) -> bool {
        !self.elements.is_empty()
    }

    pub fn has_strokes(&self) -> bool {
        self.has_elements()
    }

    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    pub fn stroke_count(&self) -> usize {
        self.element_count()
    }

    pub fn push_stroke(&mut self, stroke: Stroke) {
        self.elements.push(PaintElement::Stroke(stroke));
    }

    pub fn push_shape(&mut self, shape: ShapeElement) {
        self.elements.push(PaintElement::Shape(shape));
    }

    pub fn push_element(&mut self, element: PaintElement) {
        self.elements.push(element);
    }

    pub fn replace_element(&mut self, index: usize, element: PaintElement) -> bool {
        if let Some(slot) = self.elements.get_mut(index) {
            *slot = element;
            true
        } else {
            false
        }
    }

    pub fn element(&self, index: usize) -> Option<&PaintElement> {
        self.elements.get(index)
    }

    pub fn translate_element(&mut self, index: usize, delta: PaintVector) -> bool {
        if delta.is_zero() {
            return false;
        }

        let Some(element) = self.elements.get(index).cloned() else {
            return false;
        };

        self.elements[index] = element.translated(delta);
        true
    }

    pub fn translate_elements(&mut self, indices: &[usize], delta: PaintVector) -> bool {
        if delta.is_zero() {
            return false;
        }

        let indices = normalize_indices(indices, self.elements.len());
        if indices.is_empty() {
            return false;
        }

        for index in indices {
            let element = self
                .elements
                .get(index)
                .cloned()
                .expect("normalized indices should stay in bounds");
            self.elements[index] = element.translated(delta);
        }

        true
    }

    pub fn selection_bounds(&self, indices: &[usize]) -> Option<ElementBounds> {
        let mut bounds = normalize_indices(indices, self.elements.len())
            .into_iter()
            .filter_map(|index| self.elements[index].bounds());
        let first = bounds.next()?;
        Some(bounds.fold(first, ElementBounds::union))
    }

    pub fn aligned_document(&self, indices: &[usize], alignment: AlignmentKind) -> Option<Self> {
        let indices = normalize_indices(indices, self.elements.len());
        if indices.len() < 2 {
            return None;
        }

        let selection_bounds = self.selection_bounds(&indices)?;
        let mut next = self.clone();
        let mut changed = false;

        for index in indices {
            let Some(bounds) = self.element(index).and_then(PaintElement::bounds) else {
                continue;
            };

            let delta = alignment_delta(bounds, selection_bounds, alignment);
            if delta.is_zero() {
                continue;
            }

            if next.translate_element(index, delta) {
                changed = true;
            }
        }

        changed.then_some(next)
    }

    pub fn reordered_document(
        &self,
        indices: &[usize],
        command: StackOrderCommand,
    ) -> Option<(Self, Vec<usize>)> {
        let selected_indices = normalize_indices(indices, self.elements.len());
        if selected_indices.is_empty() {
            return None;
        }

        let mut next_elements = self.elements.clone();
        let mut changed = false;
        match command {
            StackOrderCommand::BringToFront => {
                let selected_flags = selection_flags(self.elements.len(), &selected_indices);
                let selected: Vec<_> = next_elements
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| selected_flags[*index])
                    .map(|(_, element)| element.clone())
                    .collect();
                let unselected: Vec<_> = next_elements
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| !selected_flags[*index])
                    .map(|(_, element)| element.clone())
                    .collect();
                let split = unselected.len();
                next_elements = unselected;
                next_elements.extend(selected);
                let next_selected: Vec<usize> = (split..split + selected_indices.len()).collect();
                changed = next_selected != selected_indices;
                return changed.then_some((
                    Self {
                        canvas_size: self.canvas_size,
                        background: self.background,
                        elements: next_elements,
                    },
                    next_selected,
                ));
            }
            StackOrderCommand::SendToBack => {
                let selected_flags = selection_flags(self.elements.len(), &selected_indices);
                let selected: Vec<_> = next_elements
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| selected_flags[*index])
                    .map(|(_, element)| element.clone())
                    .collect();
                let unselected: Vec<_> = next_elements
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| !selected_flags[*index])
                    .map(|(_, element)| element.clone())
                    .collect();
                next_elements = selected;
                next_elements.extend(unselected);
                let next_selected: Vec<usize> = (0..selected_indices.len()).collect();
                changed = next_selected != selected_indices;
                return changed.then_some((
                    Self {
                        canvas_size: self.canvas_size,
                        background: self.background,
                        elements: next_elements,
                    },
                    next_selected,
                ));
            }
            StackOrderCommand::BringForward | StackOrderCommand::SendBackward => {}
        }

        let mut selected_flags = selection_flags(next_elements.len(), &selected_indices);
        match command {
            StackOrderCommand::BringForward => {
                for index in (0..next_elements.len().saturating_sub(1)).rev() {
                    if selected_flags[index] && !selected_flags[index + 1] {
                        next_elements.swap(index, index + 1);
                        selected_flags.swap(index, index + 1);
                        changed = true;
                    }
                }
            }
            StackOrderCommand::SendBackward => {
                for index in 1..next_elements.len() {
                    if selected_flags[index] && !selected_flags[index - 1] {
                        next_elements.swap(index - 1, index);
                        selected_flags.swap(index - 1, index);
                        changed = true;
                    }
                }
            }
            StackOrderCommand::BringToFront | StackOrderCommand::SendToBack => unreachable!(),
        }

        let next_selected = selected_flags_to_indices(&selected_flags);
        changed.then_some((
            Self {
                canvas_size: self.canvas_size,
                background: self.background,
                elements: next_elements,
            },
            next_selected,
        ))
    }

    pub fn hit_test(&self, point: PaintPoint, tolerance: f32) -> Option<usize> {
        self.elements
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, element)| element.hit_test(point, tolerance).then_some(index))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentHistory {
    current: PaintDocument,
    undo_stack: Vec<PaintDocument>,
    redo_stack: Vec<PaintDocument>,
}

impl DocumentHistory {
    pub fn new(document: PaintDocument) -> Self {
        Self {
            current: document,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn current(&self) -> &PaintDocument {
        &self.current
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn commit_stroke(&mut self, stroke: Stroke) -> bool {
        self.commit_element(PaintElement::Stroke(stroke))
    }

    pub fn commit_shape(&mut self, shape: ShapeElement) -> bool {
        self.commit_element(PaintElement::Shape(shape))
    }

    pub fn commit_element(&mut self, element: PaintElement) -> bool {
        self.push_undo_snapshot();
        self.current.push_element(element);
        self.redo_stack.clear();
        true
    }

    pub fn replace_element(&mut self, index: usize, element: PaintElement) -> bool {
        let Some(existing) = self.current.element(index) else {
            return false;
        };
        if existing == &element {
            return false;
        }

        let mut next = self.current.clone();
        if !next.replace_element(index, element) {
            return false;
        }

        self.push_undo_snapshot();
        self.current = next;
        self.redo_stack.clear();
        true
    }

    pub fn translate_element(&mut self, index: usize, delta: PaintVector) -> bool {
        if delta.is_zero() {
            return false;
        }

        let mut next = self.current.clone();
        if !next.translate_element(index, delta) {
            return false;
        }

        self.push_undo_snapshot();
        self.current = next;
        self.redo_stack.clear();
        true
    }

    pub fn clear(&mut self) -> bool {
        if !self.current.has_elements() {
            return false;
        }

        self.push_undo_snapshot();
        self.current.elements.clear();
        self.redo_stack.clear();
        true
    }

    pub fn replace_document(&mut self, document: PaintDocument) -> bool {
        if self.current == document {
            return false;
        }

        self.push_undo_snapshot();
        self.current = document;
        self.redo_stack.clear();
        true
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo_stack.pop() else {
            return false;
        };

        self.redo_stack.push(self.current.clone());
        self.current = previous;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo_stack.pop() else {
            return false;
        };

        self.undo_stack.push(self.current.clone());
        self.current = next;
        true
    }

    fn push_undo_snapshot(&mut self) {
        self.undo_stack.push(self.current.clone());
    }
}

fn hit_test_box_outline(
    start: PaintPoint,
    end: PaintPoint,
    stroke_width: f32,
    point: PaintPoint,
    tolerance: f32,
) -> bool {
    let outer = ElementBounds::from_line(start, end, stroke_width.max(1.0) * 0.5);
    if !outer.contains(point) {
        return false;
    }

    let inset = stroke_width * 0.5 + tolerance;
    let inner_min_x = outer.min.x + inset;
    let inner_min_y = outer.min.y + inset;
    let inner_max_x = outer.max.x - inset;
    let inner_max_y = outer.max.y - inset;

    if inner_min_x >= inner_max_x || inner_min_y >= inner_max_y {
        return true;
    }

    !(point.x >= inner_min_x
        && point.x <= inner_max_x
        && point.y >= inner_min_y
        && point.y <= inner_max_y)
}

fn hit_test_ellipse_outline(
    start: PaintPoint,
    end: PaintPoint,
    stroke_width: f32,
    point: PaintPoint,
    tolerance: f32,
) -> bool {
    let bounds = ElementBounds::from_line(start, end, stroke_width.max(1.0) * 0.5);
    let center = bounds.center();
    let outer_rx = (bounds.width() * 0.5).max(1.0);
    let outer_ry = (bounds.height() * 0.5).max(1.0);

    let outer = ellipse_distance(point, center, outer_rx, outer_ry);
    if outer > 1.0 {
        return false;
    }

    let inset = stroke_width * 0.5 + tolerance;
    let inner_rx = outer_rx - inset;
    let inner_ry = outer_ry - inset;
    if inner_rx <= 0.0 || inner_ry <= 0.0 {
        return true;
    }

    ellipse_distance(point, center, inner_rx, inner_ry) >= 1.0
}

fn ellipse_distance(point: PaintPoint, center: PaintPoint, rx: f32, ry: f32) -> f32 {
    let nx = (point.x - center.x) / rx.max(1.0);
    let ny = (point.y - center.y) / ry.max(1.0);
    nx * nx + ny * ny
}

fn distance_to_segment_sq(point: PaintPoint, start: PaintPoint, end: PaintPoint) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;

    if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
        return point.distance_to(start).powi(2);
    }

    let projection =
        (((point.x - start.x) * dx) + ((point.y - start.y) * dy)) / ((dx * dx) + (dy * dy));
    let t = projection.clamp(0.0, 1.0);
    let nearest = PaintPoint::new(start.x + dx * t, start.y + dy * t);
    point.distance_to(nearest).powi(2)
}

fn rotate_vector(vector: PaintVector, angle_radians: f32) -> PaintVector {
    let cos = angle_radians.cos();
    let sin = angle_radians.sin();
    PaintVector::new(
        vector.dx * cos - vector.dy * sin,
        vector.dx * sin + vector.dy * cos,
    )
}

fn clamp_resized_axis(value: f32, anchor: f32, sign: f32) -> f32 {
    if sign < 0.0 {
        value.min(anchor - MIN_SHAPE_HALF_EXTENT * 2.0)
    } else {
        value.max(anchor + MIN_SHAPE_HALF_EXTENT * 2.0)
    }
}

fn normalize_indices(indices: &[usize], len: usize) -> Vec<usize> {
    let mut normalized: Vec<_> = indices
        .iter()
        .copied()
        .filter(|index| *index < len)
        .collect();
    normalized.sort_unstable();
    normalized.dedup();
    normalized
}

fn selection_flags(len: usize, indices: &[usize]) -> Vec<bool> {
    let mut flags = vec![false; len];
    for index in indices {
        if let Some(slot) = flags.get_mut(*index) {
            *slot = true;
        }
    }
    flags
}

fn selected_flags_to_indices(flags: &[bool]) -> Vec<usize> {
    flags
        .iter()
        .enumerate()
        .filter_map(|(index, selected)| selected.then_some(index))
        .collect()
}

fn alignment_delta(
    element_bounds: ElementBounds,
    selection_bounds: ElementBounds,
    alignment: AlignmentKind,
) -> PaintVector {
    match alignment {
        AlignmentKind::Left => PaintVector::new(selection_bounds.min.x - element_bounds.min.x, 0.0),
        AlignmentKind::HorizontalCenter => {
            PaintVector::new(selection_bounds.center().x - element_bounds.center().x, 0.0)
        }
        AlignmentKind::Right => {
            PaintVector::new(selection_bounds.max.x - element_bounds.max.x, 0.0)
        }
        AlignmentKind::Top => PaintVector::new(0.0, selection_bounds.min.y - element_bounds.min.y),
        AlignmentKind::VerticalCenter => {
            PaintVector::new(0.0, selection_bounds.center().y - element_bounds.center().y)
        }
        AlignmentKind::Bottom => {
            PaintVector::new(0.0, selection_bounds.max.y - element_bounds.max.y)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AlignmentKind, CanvasSize, DocumentHistory, PaintDocument, PaintElement, PaintPoint,
        PaintVector, RgbaColor, ShapeElement, ShapeHandle, ShapeKind, StackOrderCommand, Stroke,
        ToolKind,
    };

    fn sample_stroke() -> Stroke {
        let mut stroke = Stroke::new(ToolKind::Brush, RgbaColor::charcoal(), 8.0);
        stroke.push_point(PaintPoint::new(10.0, 10.0));
        stroke.push_point(PaintPoint::new(60.0, 10.0));
        stroke
    }

    fn sample_shape() -> ShapeElement {
        ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            4.0,
            PaintPoint::new(80.0, 20.0),
            PaintPoint::new(140.0, 70.0),
        )
    }

    #[test]
    fn stroke_hit_test_accepts_nearby_points() {
        let stroke = sample_stroke();
        assert!(stroke.hit_test(PaintPoint::new(20.0, 12.5), 2.0));
        assert!(!stroke.hit_test(PaintPoint::new(20.0, 30.0), 2.0));
    }

    #[test]
    fn rectangle_hit_test_targets_outline() {
        let shape = sample_shape();
        assert!(shape.hit_test(PaintPoint::new(82.0, 25.0), 2.0));
        assert!(!shape.hit_test(PaintPoint::new(110.0, 45.0), 2.0));
    }

    #[test]
    fn rotated_rectangle_hit_test_works() {
        let shape = ShapeElement::with_rotation(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            4.0,
            PaintPoint::new(80.0, 20.0),
            PaintPoint::new(140.0, 70.0),
            std::f32::consts::FRAC_PI_4,
        );

        assert!(shape.hit_test(PaintPoint::new(121.0, 20.0), 4.0));
        assert!(!shape.hit_test(PaintPoint::new(110.0, 45.0), 2.0));
    }

    #[test]
    fn ellipse_hit_test_targets_outline() {
        let shape = ShapeElement::new(
            ShapeKind::Ellipse,
            RgbaColor::charcoal(),
            4.0,
            PaintPoint::new(20.0, 20.0),
            PaintPoint::new(120.0, 80.0),
        );

        assert!(shape.hit_test(PaintPoint::new(70.0, 20.0), 3.0));
        assert!(!shape.hit_test(PaintPoint::new(70.0, 50.0), 3.0));
    }

    #[test]
    fn document_hit_test_prefers_topmost_element() {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(200.0, 120.0),
            background: RgbaColor::white(),
            elements: Vec::new(),
        };
        document.push_shape(sample_shape());
        document.push_stroke(sample_stroke());

        assert_eq!(document.hit_test(PaintPoint::new(82.0, 23.0), 3.0), Some(0));
        assert_eq!(document.hit_test(PaintPoint::new(20.0, 10.0), 3.0), Some(1));
    }

    #[test]
    fn translate_element_moves_points() {
        let mut document = PaintDocument::default();
        document.push_shape(sample_shape());

        assert!(document.translate_element(0, PaintVector::new(10.0, -5.0)));

        let Some(PaintElement::Shape(shape)) = document.element(0) else {
            panic!("shape should stay a shape");
        };

        assert_eq!(shape.start, PaintPoint::new(90.0, 15.0));
        assert_eq!(shape.end, PaintPoint::new(150.0, 65.0));
    }

    #[test]
    fn resize_rectangle_preserves_rotation() {
        let shape = ShapeElement::with_rotation(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            4.0,
            PaintPoint::new(80.0, 20.0),
            PaintPoint::new(140.0, 70.0),
            0.5,
        );

        let resized = shape
            .resized_by_handle(ShapeHandle::TopLeft, PaintPoint::new(60.0, 10.0))
            .expect("rectangle resize should succeed");

        assert_eq!(resized.rotation_radians, 0.5);
        assert!(resized.bounds().width() >= shape.bounds().width());
    }

    #[test]
    fn rotate_line_moves_endpoints() {
        let line = ShapeElement::new(
            ShapeKind::Line,
            RgbaColor::charcoal(),
            3.0,
            PaintPoint::new(20.0, 20.0),
            PaintPoint::new(60.0, 20.0),
        );

        let rotated = line.rotated_by(std::f32::consts::FRAC_PI_2);
        assert!((rotated.start.x - 40.0).abs() < 0.01);
        assert!((rotated.end.x - 40.0).abs() < 0.01);
    }

    #[test]
    fn history_tracks_replace_for_shape_editing() {
        let mut history = DocumentHistory::new(PaintDocument::default());
        history.commit_shape(sample_shape());

        let replacement = PaintElement::Shape(sample_shape().rotated_by(0.5));
        assert!(history.replace_element(0, replacement));
        assert!(history.undo());
        assert!(history.redo());
    }

    #[test]
    fn history_replace_clears_redo() {
        let mut history = DocumentHistory::new(PaintDocument::default());
        history.commit_stroke(sample_stroke());
        assert!(history.undo());
        assert!(history.can_redo());

        let mut replacement = PaintDocument::default();
        replacement.push_shape(sample_shape());
        assert!(history.replace_document(replacement));

        assert!(!history.can_redo());
    }

    #[test]
    fn selection_bounds_union_multiple_elements() {
        let mut document = PaintDocument::default();
        document.push_shape(sample_shape());
        document.push_stroke(sample_stroke());

        let bounds = document
            .selection_bounds(&[0, 1])
            .expect("selection bounds should exist");

        assert!(bounds.min.x <= 10.0);
        assert!(bounds.max.x >= 140.0);
    }

    #[test]
    fn align_left_moves_selection_to_group_edge() {
        let mut document = PaintDocument::default();
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            2.0,
            PaintPoint::new(20.0, 20.0),
            PaintPoint::new(40.0, 40.0),
        ));
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            2.0,
            PaintPoint::new(80.0, 20.0),
            PaintPoint::new(120.0, 40.0),
        ));

        let aligned = document
            .aligned_document(&[0, 1], AlignmentKind::Left)
            .expect("alignment should change the document");

        let left_a = aligned
            .element(0)
            .and_then(PaintElement::bounds)
            .expect("element bounds");
        let left_b = aligned
            .element(1)
            .and_then(PaintElement::bounds)
            .expect("element bounds");
        assert!((left_a.min.x - left_b.min.x).abs() < 0.001);
    }

    #[test]
    fn bring_to_front_preserves_relative_order() {
        let mut document = PaintDocument::default();
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::new(200, 0, 0, 255),
            2.0,
            PaintPoint::new(10.0, 10.0),
            PaintPoint::new(30.0, 30.0),
        ));
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::new(0, 200, 0, 255),
            2.0,
            PaintPoint::new(15.0, 15.0),
            PaintPoint::new(35.0, 35.0),
        ));
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::new(0, 0, 200, 255),
            2.0,
            PaintPoint::new(20.0, 20.0),
            PaintPoint::new(40.0, 40.0),
        ));

        let (reordered, indices) = document
            .reordered_document(&[0, 1], StackOrderCommand::BringToFront)
            .expect("reorder should succeed");

        let colors: Vec<_> = reordered
            .elements
            .iter()
            .map(|element| match element {
                PaintElement::Shape(shape) => shape.color,
                PaintElement::Stroke(_) => RgbaColor::white(),
            })
            .collect();
        assert_eq!(
            colors,
            vec![
                RgbaColor::new(0, 0, 200, 255),
                RgbaColor::new(200, 0, 0, 255),
                RgbaColor::new(0, 200, 0, 255),
            ]
        );
        assert_eq!(indices, vec![1, 2]);
    }

    #[test]
    fn history_replace_document_supports_multi_move_undo_redo() {
        let mut history = DocumentHistory::new(PaintDocument::default());
        history.commit_shape(sample_shape());
        history.commit_stroke(sample_stroke());

        let mut moved = history.current().clone();
        assert!(moved.translate_elements(&[0, 1], PaintVector::new(12.0, -8.0)));
        assert!(history.replace_document(moved.clone()));
        assert_eq!(history.current(), &moved);
        assert!(history.undo());
        assert!(history.redo());
        assert_eq!(history.current(), &moved);
    }

    #[test]
    fn replace_document_skips_identical_state() {
        let document = PaintDocument::default();
        let mut history = DocumentHistory::new(document.clone());
        assert!(!history.replace_document(document));
    }
}
