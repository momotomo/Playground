use serde::{Deserialize, Serialize};

pub const DEFAULT_CANVAS_WIDTH: f32 = 1600.0;
pub const DEFAULT_CANVAS_HEIGHT: f32 = 900.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PaintPoint {
    pub x: f32,
    pub y: f32,
}

impl PaintPoint {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn distance_to(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;

        (dx * dx + dy * dy).sqrt()
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
    pub const fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn white() -> Self {
        Self::from_rgba(255, 255, 255, 255)
    }

    pub const fn charcoal() -> Self {
        Self::from_rgba(30, 41, 59, 255)
    }
}

impl Default for RgbaColor {
    fn default() -> Self {
        Self::charcoal()
    }
}

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
        Self::new(DEFAULT_CANVAS_WIDTH, DEFAULT_CANVAS_HEIGHT)
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
            width: width.max(1.0),
            points: Vec::new(),
        }
    }

    pub fn push_point(&mut self, point: PaintPoint) {
        let should_push = self
            .points
            .last()
            .is_none_or(|last| last.distance_to(point) >= 0.75);

        if should_push {
            self.points.push(point);
        }
    }

    pub fn is_committable(&self) -> bool {
        !self.points.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaintDocument {
    pub canvas_size: CanvasSize,
    pub background: RgbaColor,
    pub strokes: Vec<Stroke>,
}

impl Default for PaintDocument {
    fn default() -> Self {
        Self {
            canvas_size: CanvasSize::default(),
            background: RgbaColor::white(),
            strokes: Vec::new(),
        }
    }
}

impl PaintDocument {
    pub fn push_stroke(&mut self, stroke: Stroke) {
        if stroke.is_committable() {
            self.strokes.push(stroke);
        }
    }

    pub fn undo(&mut self) -> bool {
        self.strokes.pop().is_some()
    }

    pub fn clear(&mut self) {
        self.strokes.clear();
    }

    pub fn has_strokes(&self) -> bool {
        !self.strokes.is_empty()
    }

    pub fn stroke_count(&self) -> usize {
        self.strokes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{PaintDocument, PaintPoint, RgbaColor, Stroke, ToolKind};

    #[test]
    fn undo_removes_last_stroke() {
        let mut document = PaintDocument::default();
        let mut stroke = Stroke::new(ToolKind::Brush, RgbaColor::default(), 4.0);
        stroke.push_point(PaintPoint::new(10.0, 10.0));
        stroke.push_point(PaintPoint::new(20.0, 20.0));
        document.push_stroke(stroke);

        assert!(document.undo());
        assert_eq!(document.stroke_count(), 0);
    }

    #[test]
    fn clear_removes_all_strokes() {
        let mut document = PaintDocument::default();
        let mut first = Stroke::new(ToolKind::Brush, RgbaColor::default(), 4.0);
        first.push_point(PaintPoint::new(1.0, 1.0));
        let mut second = Stroke::new(ToolKind::Eraser, RgbaColor::white(), 12.0);
        second.push_point(PaintPoint::new(2.0, 2.0));

        document.push_stroke(first);
        document.push_stroke(second);
        document.clear();

        assert!(!document.has_strokes());
    }
}
