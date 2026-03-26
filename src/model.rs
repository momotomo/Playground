use serde::{Deserialize, Serialize};

const HIT_TOLERANCE_MIN: f32 = 2.5;

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

    pub fn offset(self, delta: PaintVector) -> Self {
        Self::new(self.x + delta.dx, self.y + delta.dy)
    }

    pub fn distance_to(self, other: Self) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
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
        }
    }

    pub fn translated(&self, delta: PaintVector) -> Self {
        Self {
            kind: self.kind,
            color: self.color,
            width: self.width,
            start: self.start.offset(delta),
            end: self.end.offset(delta),
        }
    }

    pub fn bounds(&self) -> ElementBounds {
        ElementBounds::from_line(self.start, self.end, self.width.max(1.0) * 0.5)
    }

    pub fn hit_test(&self, point: PaintPoint, tolerance: f32) -> bool {
        let tolerance = tolerance.max(HIT_TOLERANCE_MIN);
        match self.kind {
            ShapeKind::Line => {
                let radius = self.width * 0.5 + tolerance;
                distance_to_segment_sq(point, self.start, self.end) <= radius * radius
            }
            ShapeKind::Rectangle => hit_test_rectangle_outline(*self, point, tolerance),
            ShapeKind::Ellipse => hit_test_ellipse_outline(*self, point, tolerance),
        }
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

fn hit_test_rectangle_outline(shape: ShapeElement, point: PaintPoint, tolerance: f32) -> bool {
    let outer = shape.bounds();
    if !outer.contains(point) {
        return false;
    }

    let inset = shape.width * 0.5 + tolerance;
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

fn hit_test_ellipse_outline(shape: ShapeElement, point: PaintPoint, tolerance: f32) -> bool {
    let bounds = shape.bounds();
    let center = bounds.center();
    let outer_rx = (bounds.width() * 0.5).max(1.0);
    let outer_ry = (bounds.height() * 0.5).max(1.0);

    let outer = ellipse_distance(point, center, outer_rx, outer_ry);
    if outer > 1.0 {
        return false;
    }

    let inset = shape.width * 0.5 + tolerance;
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

#[cfg(test)]
mod tests {
    use super::{
        CanvasSize, DocumentHistory, PaintDocument, PaintElement, PaintPoint, PaintVector,
        RgbaColor, ShapeElement, ShapeKind, Stroke, ToolKind,
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

        assert_eq!(document.hit_test(PaintPoint::new(82.0, 23.0), 3.0), Some(0),);
        assert_eq!(document.hit_test(PaintPoint::new(20.0, 10.0), 3.0), Some(1),);
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
    fn history_tracks_move_without_recording_zero_delta() {
        let mut history = DocumentHistory::new(PaintDocument::default());
        history.commit_stroke(sample_stroke());

        assert!(!history.translate_element(0, PaintVector::default()));
        assert!(history.translate_element(0, PaintVector::new(5.0, 4.0)));
        assert!(history.can_undo());
        assert!(history.undo());
        assert!(history.redo());
    }

    #[test]
    fn history_replace_clears_redo() {
        let mut history = DocumentHistory::new(PaintDocument::default());
        history.commit_stroke(sample_stroke());
        assert!(history.undo());
        assert!(history.can_redo());

        history.replace_document(PaintDocument::default());

        assert!(!history.can_redo());
    }
}
