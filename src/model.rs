use serde::{Deserialize, Serialize};

const HIT_TOLERANCE_MIN: f32 = 2.5;
const MIN_SHAPE_HALF_EXTENT: f32 = 0.5;
const DEFAULT_LAYER_ID: u64 = 1;
const DEFAULT_GRID_SPACING: f32 = 48.0;
const MIN_GRID_SPACING: f32 = 8.0;

pub type LayerId = u64;

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_active_layer_id() -> LayerId {
    DEFAULT_LAYER_ID
}

fn default_next_layer_id() -> LayerId {
    DEFAULT_LAYER_ID + 1
}

fn default_grid_spacing() -> f32 {
    DEFAULT_GRID_SPACING
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuideAxis {
    Horizontal,
    Vertical,
}

impl GuideAxis {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Horizontal => "横",
            Self::Vertical => "縦",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GuideLine {
    pub axis: GuideAxis,
    pub position: f32,
}

impl GuideLine {
    pub fn new(axis: GuideAxis, position: f32) -> Self {
        Self { axis, position }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GridSettings {
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_false")]
    pub snap_enabled: bool,
    #[serde(default = "default_grid_spacing")]
    pub spacing: f32,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            visible: true,
            snap_enabled: false,
            spacing: DEFAULT_GRID_SPACING,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuideSettings {
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_true")]
    pub snap_enabled: bool,
    #[serde(default)]
    pub lines: Vec<GuideLine>,
}

impl Default for GuideSettings {
    fn default() -> Self {
        Self {
            visible: true,
            snap_enabled: true,
            lines: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SmartGuideSettings {
    #[serde(default = "default_true")]
    pub visible: bool,
}

impl Default for SmartGuideSettings {
    fn default() -> Self {
        Self { visible: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RulerSettings {
    #[serde(default = "default_true")]
    pub visible: bool,
}

impl Default for RulerSettings {
    fn default() -> Self {
        Self { visible: true }
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

    pub fn with_alpha_scaled(self, factor: f32) -> Self {
        let alpha = ((self.a as f32) * factor).round().clamp(0.0, 255.0) as u8;
        Self::from_rgba(self.r, self.g, self.b, alpha)
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

    pub fn scaled_from(self, anchor: Self, scale_x: f32, scale_y: f32) -> Self {
        Self::new(
            anchor.x + (self.x - anchor.x) * scale_x,
            anchor.y + (self.y - anchor.y) * scale_y,
        )
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

    pub fn intersects(self, other: Self) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Brush,
    Pencil,
    Marker,
    Eraser,
}

impl ToolKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Brush => "ペン",
            Self::Pencil => "えんぴつ",
            Self::Marker => "マーカー",
            Self::Eraser => "消しゴム",
        }
    }

    pub const fn width_scale(self) -> f32 {
        match self {
            Self::Brush => 1.0,
            Self::Pencil => 0.85,
            Self::Marker => 1.35,
            Self::Eraser => 1.0,
        }
    }

    pub const fn alpha_scale(self) -> f32 {
        match self {
            Self::Brush => 1.0,
            Self::Pencil => 0.72,
            Self::Marker => 0.48,
            Self::Eraser => 1.0,
        }
    }

    pub fn styled_color(self, color: RgbaColor) -> RgbaColor {
        match self {
            Self::Brush | Self::Pencil | Self::Marker => {
                color.with_alpha_scaled(self.alpha_scale())
            }
            Self::Eraser => color,
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
            Self::Rectangle => "四角形",
            Self::Ellipse => "楕円",
            Self::Line => "直線",
        }
    }

    pub const fn supports_fill(self) -> bool {
        matches!(self, Self::Rectangle | Self::Ellipse)
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
            Self::Left => "左揃え",
            Self::HorizontalCenter => "横中央揃え",
            Self::Right => "右揃え",
            Self::Top => "上揃え",
            Self::VerticalCenter => "縦中央揃え",
            Self::Bottom => "下揃え",
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
            Self::BringToFront => "最前面へ",
            Self::SendToBack => "最背面へ",
            Self::BringForward => "一つ前面へ",
            Self::SendBackward => "一つ背面へ",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistributionKind {
    Horizontal,
    Vertical,
}

impl DistributionKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Horizontal => "横方向に等間隔",
            Self::Vertical => "縦方向に等間隔",
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeRenderPass {
    pub color: RgbaColor,
    pub width: f32,
    pub offset: PaintVector,
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

    pub fn effective_width(&self) -> f32 {
        (self.width * self.tool.width_scale()).max(1.0)
    }

    pub fn render_passes(&self) -> Vec<StrokeRenderPass> {
        let base_color = self.tool.styled_color(self.color);
        let base_width = self.effective_width();

        match self.tool {
            ToolKind::Brush | ToolKind::Eraser => vec![StrokeRenderPass {
                color: base_color,
                width: base_width,
                offset: PaintVector::default(),
            }],
            ToolKind::Pencil => {
                let offset = stroke_texture_offset(&self.points, base_width * 0.12);
                vec![
                    StrokeRenderPass {
                        color: base_color,
                        width: base_width.max(1.0),
                        offset: PaintVector::default(),
                    },
                    StrokeRenderPass {
                        color: base_color.with_alpha_scaled(0.42),
                        width: (base_width * 0.52).max(0.9),
                        offset,
                    },
                    StrokeRenderPass {
                        color: base_color.with_alpha_scaled(0.28),
                        width: (base_width * 0.34).max(0.8),
                        offset: PaintVector::new(-offset.dx, -offset.dy),
                    },
                ]
            }
            ToolKind::Marker => vec![
                StrokeRenderPass {
                    color: base_color.with_alpha_scaled(0.78),
                    width: (base_width * 1.18).max(1.0),
                    offset: PaintVector::default(),
                },
                StrokeRenderPass {
                    color: base_color,
                    width: (base_width * 0.82).max(0.9),
                    offset: PaintVector::default(),
                },
            ],
        }
    }

    pub fn translated(&self, delta: PaintVector) -> Self {
        let mut translated = self.clone();
        for point in &mut translated.points {
            *point = point.offset(delta);
        }
        translated
    }

    pub fn scaled_from(&self, anchor: PaintPoint, scale_x: f32, scale_y: f32) -> Self {
        let mut scaled = self.clone();
        for point in &mut scaled.points {
            *point = point.scaled_from(anchor, scale_x, scale_y);
        }
        scaled
    }

    pub fn rotated_around(&self, center: PaintPoint, angle_radians: f32) -> Self {
        let mut rotated = self.clone();
        for point in &mut rotated.points {
            *point = point.rotated_around(center, angle_radians);
        }
        rotated
    }

    pub fn bounds(&self) -> Option<ElementBounds> {
        ElementBounds::from_points(&self.points, self.effective_width() * 0.5)
    }

    pub fn hit_test(&self, point: PaintPoint, tolerance: f32) -> bool {
        let tolerance = tolerance.max(HIT_TOLERANCE_MIN);

        match self.points.as_slice() {
            [] => false,
            [only] => only.distance_to(point) <= (self.effective_width() * 0.5 + tolerance),
            [first, rest @ ..] => {
                let radius = self.effective_width() * 0.5 + tolerance;
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
    #[serde(default)]
    pub fill_color: Option<RgbaColor>,
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
            fill_color: None,
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
            fill_color: None,
            width,
            start,
            end,
            rotation_radians,
        }
    }

    pub fn with_fill_color(mut self, fill_color: Option<RgbaColor>) -> Self {
        self.fill_color = if self.kind.supports_fill() {
            fill_color
        } else {
            None
        };
        self
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
            start: self.start.offset(delta),
            end: self.end.offset(delta),
            ..*self
        }
    }

    pub fn scaled_from(&self, anchor: PaintPoint, scale_x: f32, scale_y: f32) -> Self {
        match self.kind {
            ShapeKind::Line => Self {
                start: self.start.scaled_from(anchor, scale_x, scale_y),
                end: self.end.scaled_from(anchor, scale_x, scale_y),
                rotation_radians: 0.0,
                ..*self
            },
            ShapeKind::Rectangle | ShapeKind::Ellipse => {
                let center = self.center().scaled_from(anchor, scale_x, scale_y);
                let half = self.half_extents();
                let scaled_half =
                    PaintVector::new((half.dx * scale_x).abs(), (half.dy * scale_y).abs());

                Self {
                    start: PaintPoint::new(center.x - scaled_half.dx, center.y - scaled_half.dy),
                    end: PaintPoint::new(center.x + scaled_half.dx, center.y + scaled_half.dy),
                    ..*self
                }
            }
        }
    }

    pub fn rotated_around(&self, pivot: PaintPoint, delta_radians: f32) -> Self {
        match self.kind {
            ShapeKind::Line => Self {
                start: self.start.rotated_around(pivot, delta_radians),
                end: self.end.rotated_around(pivot, delta_radians),
                rotation_radians: 0.0,
                ..*self
            },
            ShapeKind::Rectangle | ShapeKind::Ellipse => {
                let center = self.center().rotated_around(pivot, delta_radians);
                let half = self.half_extents();
                Self {
                    start: PaintPoint::new(center.x - half.dx, center.y - half.dy),
                    end: PaintPoint::new(center.x + half.dx, center.y + half.dy),
                    rotation_radians: self.rotation_radians + delta_radians,
                    ..*self
                }
            }
        }
    }

    pub fn rotated_by(&self, delta_radians: f32) -> Self {
        match self.kind {
            ShapeKind::Line => {
                let center = self.rotation_center();
                Self {
                    start: self.start.rotated_around(center, delta_radians),
                    end: self.end.rotated_around(center, delta_radians),
                    rotation_radians: 0.0,
                    ..*self
                }
            }
            ShapeKind::Rectangle | ShapeKind::Ellipse => Self {
                rotation_radians: self.rotation_radians + delta_radians,
                ..*self
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
            start: PaintPoint::new(new_center.x - new_half.dx, new_center.y - new_half.dy),
            end: PaintPoint::new(new_center.x + new_half.dx, new_center.y + new_half.dy),
            ..*self
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

fn stroke_texture_offset(points: &[PaintPoint], distance: f32) -> PaintVector {
    if distance.abs() <= f32::EPSILON {
        return PaintVector::default();
    }

    let direction = match points {
        [first, .., last] => PaintVector::new(last.x - first.x, last.y - first.y),
        _ => PaintVector::new(1.0, -0.6),
    };
    let length = (direction.dx * direction.dx + direction.dy * direction.dy).sqrt();
    if length <= f32::EPSILON {
        return PaintVector::new(distance, -distance * 0.6);
    }

    let normal = PaintVector::new(-direction.dy / length, direction.dx / length);
    PaintVector::new(normal.dx * distance, normal.dy * distance)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupElement {
    #[serde(default)]
    pub elements: Vec<PaintElement>,
}

impl GroupElement {
    pub fn bounds(&self) -> Option<ElementBounds> {
        let mut bounds = self.elements.iter().filter_map(PaintElement::bounds);
        let first = bounds.next()?;
        Some(bounds.fold(first, ElementBounds::union))
    }

    pub fn hit_test(&self, point: PaintPoint, tolerance: f32) -> bool {
        self.elements
            .iter()
            .rev()
            .any(|element| element.hit_test(point, tolerance))
    }

    pub fn translated(&self, delta: PaintVector) -> Self {
        Self {
            elements: self
                .elements
                .iter()
                .map(|element| element.translated(delta))
                .collect(),
        }
    }

    pub fn scaled_from(&self, anchor: PaintPoint, scale_x: f32, scale_y: f32) -> Self {
        Self {
            elements: self
                .elements
                .iter()
                .map(|element| element.scaled_from(anchor, scale_x, scale_y))
                .collect(),
        }
    }

    pub fn rotated_around(&self, pivot: PaintPoint, angle_radians: f32) -> Self {
        Self {
            elements: self
                .elements
                .iter()
                .map(|element| element.rotated_around(pivot, angle_radians))
                .collect(),
        }
    }

    pub fn is_transform_editable(&self) -> bool {
        self.elements
            .iter()
            .any(PaintElement::is_transform_editable)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "element_type", rename_all = "snake_case")]
pub enum PaintElement {
    Stroke(Stroke),
    Shape(ShapeElement),
    Group(GroupElement),
}

impl PaintElement {
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Stroke(stroke) => stroke.tool.label(),
            Self::Shape(shape) => shape.kind.label(),
            Self::Group(_) => "グループ",
        }
    }

    pub fn bounds(&self) -> Option<ElementBounds> {
        match self {
            Self::Stroke(stroke) => stroke.bounds(),
            Self::Shape(shape) => Some(shape.bounds()),
            Self::Group(group) => group.bounds(),
        }
    }

    pub fn hit_test(&self, point: PaintPoint, tolerance: f32) -> bool {
        match self {
            Self::Stroke(stroke) => stroke.hit_test(point, tolerance),
            Self::Shape(shape) => shape.hit_test(point, tolerance),
            Self::Group(group) => group.hit_test(point, tolerance),
        }
    }

    pub fn translated(&self, delta: PaintVector) -> Self {
        match self {
            Self::Stroke(stroke) => Self::Stroke(stroke.translated(delta)),
            Self::Shape(shape) => Self::Shape(shape.translated(delta)),
            Self::Group(group) => Self::Group(group.translated(delta)),
        }
    }

    pub fn scaled_from(&self, anchor: PaintPoint, scale_x: f32, scale_y: f32) -> Self {
        match self {
            Self::Stroke(stroke) => Self::Stroke(stroke.scaled_from(anchor, scale_x, scale_y)),
            Self::Shape(shape) => Self::Shape(shape.scaled_from(anchor, scale_x, scale_y)),
            Self::Group(group) => Self::Group(group.scaled_from(anchor, scale_x, scale_y)),
        }
    }

    pub fn rotated_around(&self, pivot: PaintPoint, angle_radians: f32) -> Self {
        match self {
            Self::Stroke(stroke) => Self::Stroke(stroke.rotated_around(pivot, angle_radians)),
            Self::Shape(shape) => Self::Shape(shape.rotated_around(pivot, angle_radians)),
            Self::Group(group) => Self::Group(group.rotated_around(pivot, angle_radians)),
        }
    }

    pub fn is_transform_editable(&self) -> bool {
        match self {
            Self::Stroke(stroke) => !stroke.points.is_empty(),
            Self::Shape(shape) => shape.is_transform_editable(),
            Self::Group(group) => group.is_transform_editable(),
        }
    }

    pub fn is_group(&self) -> bool {
        matches!(self, Self::Group(_))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaintLayer {
    pub id: LayerId,
    pub name: String,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub elements: Vec<PaintElement>,
}

impl PaintLayer {
    pub fn new(id: LayerId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            visible: true,
            locked: false,
            elements: Vec::new(),
        }
    }

    pub fn is_editable(&self) -> bool {
        self.visible && !self.locked
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaintDocument {
    #[serde(default)]
    pub canvas_size: CanvasSize,
    #[serde(default = "RgbaColor::white")]
    pub background: RgbaColor,
    #[serde(default)]
    pub grid: GridSettings,
    #[serde(default)]
    pub guides: GuideSettings,
    #[serde(default)]
    pub smart_guides: SmartGuideSettings,
    #[serde(default)]
    pub rulers: RulerSettings,
    #[serde(default = "PaintDocument::default_layers")]
    pub layers: Vec<PaintLayer>,
    #[serde(default = "default_active_layer_id")]
    pub active_layer_id: LayerId,
    #[serde(default = "default_next_layer_id")]
    pub next_layer_id: LayerId,
}

impl Default for PaintDocument {
    fn default() -> Self {
        Self {
            canvas_size: CanvasSize::default(),
            background: RgbaColor::white(),
            grid: GridSettings::default(),
            guides: GuideSettings::default(),
            smart_guides: SmartGuideSettings::default(),
            rulers: RulerSettings::default(),
            layers: Self::default_layers(),
            active_layer_id: default_active_layer_id(),
            next_layer_id: default_next_layer_id(),
        }
    }
}

impl PaintDocument {
    fn default_layers() -> Vec<PaintLayer> {
        vec![PaintLayer::new(DEFAULT_LAYER_ID, "レイヤー 1")]
    }

    pub fn from_flat_elements(
        canvas_size: CanvasSize,
        background: RgbaColor,
        elements: Vec<PaintElement>,
    ) -> Self {
        Self {
            canvas_size,
            background,
            grid: GridSettings::default(),
            guides: GuideSettings::default(),
            smart_guides: SmartGuideSettings::default(),
            rulers: RulerSettings::default(),
            layers: vec![PaintLayer {
                id: DEFAULT_LAYER_ID,
                name: "レイヤー 1".to_owned(),
                visible: true,
                locked: false,
                elements,
            }],
            active_layer_id: DEFAULT_LAYER_ID,
            next_layer_id: DEFAULT_LAYER_ID + 1,
        }
    }

    pub fn sanitized(mut self) -> Self {
        self.sanitize_in_place();
        self
    }

    pub fn sanitize_in_place(&mut self) {
        if self.layers.is_empty() {
            self.layers = Self::default_layers();
        }

        self.grid.spacing = self.grid.spacing.max(MIN_GRID_SPACING);
        for guide in &mut self.guides.lines {
            guide.position = clamp_guide_position(self.canvas_size, *guide);
        }
        self.guides.lines.sort_by(|left, right| {
            guide_sort_key(*left)
                .cmp(&guide_sort_key(*right))
                .then_with(|| {
                    left.position
                        .partial_cmp(&right.position)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        let max_id = self
            .layers
            .iter()
            .map(|layer| layer.id)
            .max()
            .unwrap_or(DEFAULT_LAYER_ID);

        if !self
            .layers
            .iter()
            .any(|layer| layer.id == self.active_layer_id)
        {
            self.active_layer_id = self
                .layers
                .last()
                .map(|layer| layer.id)
                .unwrap_or(DEFAULT_LAYER_ID);
        }

        if self.next_layer_id <= max_id {
            self.next_layer_id = max_id + 1;
        }
    }

    pub fn has_elements(&self) -> bool {
        self.layers.iter().any(|layer| !layer.elements.is_empty())
    }

    pub fn has_strokes(&self) -> bool {
        self.has_elements()
    }

    pub fn grid(&self) -> GridSettings {
        self.grid
    }

    pub fn guides(&self) -> &GuideSettings {
        &self.guides
    }

    pub fn smart_guides(&self) -> SmartGuideSettings {
        self.smart_guides
    }

    pub fn rulers(&self) -> RulerSettings {
        self.rulers
    }

    pub fn element_count(&self) -> usize {
        self.active_layer().map_or(0, |layer| layer.elements.len())
    }

    pub fn stroke_count(&self) -> usize {
        self.element_count()
    }

    pub fn total_element_count(&self) -> usize {
        self.layers.iter().map(|layer| layer.elements.len()).sum()
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    pub fn layers(&self) -> &[PaintLayer] {
        &self.layers
    }

    pub fn visible_layers(&self) -> impl Iterator<Item = &PaintLayer> {
        self.layers.iter().filter(|layer| layer.visible)
    }

    pub fn active_layer_id(&self) -> LayerId {
        self.active_layer_id
    }

    pub fn active_layer_index(&self) -> Option<usize> {
        self.layer_index(self.active_layer_id)
    }

    pub fn active_layer(&self) -> Option<&PaintLayer> {
        self.layers
            .iter()
            .find(|layer| layer.id == self.active_layer_id)
    }

    pub fn layer(&self, layer_id: LayerId) -> Option<&PaintLayer> {
        self.layers.iter().find(|layer| layer.id == layer_id)
    }

    fn layer_index(&self, layer_id: LayerId) -> Option<usize> {
        self.layers.iter().position(|layer| layer.id == layer_id)
    }

    fn active_layer_mut(&mut self) -> Option<&mut PaintLayer> {
        let active_id = self.active_layer_id;
        self.layers.iter_mut().find(|layer| layer.id == active_id)
    }

    fn layer_mut(&mut self, layer_id: LayerId) -> Option<&mut PaintLayer> {
        self.layers.iter_mut().find(|layer| layer.id == layer_id)
    }

    pub fn active_layer_is_editable(&self) -> bool {
        self.active_layer().is_some_and(PaintLayer::is_editable)
    }

    pub fn layer_is_editable(&self, layer_id: LayerId) -> bool {
        self.layer(layer_id).is_some_and(PaintLayer::is_editable)
    }

    pub fn set_active_layer(&mut self, layer_id: LayerId) -> bool {
        if self.layers.iter().any(|layer| layer.id == layer_id) {
            self.active_layer_id = layer_id;
            true
        } else {
            false
        }
    }

    pub fn add_layer_document(&self) -> (Self, LayerId) {
        let mut next = self.clone();
        let new_id = next.next_layer_id.max(
            next.layers
                .iter()
                .map(|layer| layer.id)
                .max()
                .unwrap_or(DEFAULT_LAYER_ID)
                + 1,
        );
        let layer_name = format!("レイヤー {}", next.layers.len() + 1);
        next.layers.push(PaintLayer::new(new_id, layer_name));
        next.active_layer_id = new_id;
        next.next_layer_id = new_id + 1;
        (next, new_id)
    }

    pub fn duplicate_active_layer_document(&self) -> Option<(Self, LayerId)> {
        let mut next = self.clone();
        let active_index = next.active_layer_index()?;
        let source_layer = next.layers.get(active_index)?.clone();
        let new_id = next.next_layer_id.max(
            next.layers
                .iter()
                .map(|layer| layer.id)
                .max()
                .unwrap_or(DEFAULT_LAYER_ID)
                + 1,
        );

        let mut duplicated = source_layer;
        duplicated.id = new_id;
        duplicated.name = format!("{} のコピー", duplicated.name);
        duplicated.visible = true;
        duplicated.locked = false;
        next.layers.insert(active_index + 1, duplicated);
        next.active_layer_id = new_id;
        next.next_layer_id = new_id + 1;
        Some((next, new_id))
    }

    pub fn delete_active_layer_document(&self) -> Option<(Self, LayerId)> {
        if self.layers.len() <= 1 {
            return None;
        }

        let mut next = self.clone();
        let active_index = next.active_layer_index()?;
        next.layers.remove(active_index);
        let fallback_index = active_index.saturating_sub(1).min(next.layers.len() - 1);
        let next_active = next.layers.get(fallback_index)?.id;
        next.active_layer_id = next_active;
        Some((next, next_active))
    }

    pub fn renamed_layer_document(&self, layer_id: LayerId, name: &str) -> Option<Self> {
        let next_name = name.trim();
        if next_name.is_empty() {
            return None;
        }

        let mut next = self.clone();
        let layer = next.layer_mut(layer_id)?;
        if layer.name == next_name {
            return None;
        }
        layer.name = next_name.to_owned();
        Some(next)
    }

    pub fn toggled_layer_visibility_document(&self, layer_id: LayerId) -> Option<Self> {
        let mut next = self.clone();
        let layer = next.layer_mut(layer_id)?;
        layer.visible = !layer.visible;
        Some(next)
    }

    pub fn toggled_layer_locked_document(&self, layer_id: LayerId) -> Option<Self> {
        let mut next = self.clone();
        let layer = next.layer_mut(layer_id)?;
        layer.locked = !layer.locked;
        Some(next)
    }

    pub fn moved_layer_up_document(&self, layer_id: LayerId) -> Option<Self> {
        let mut next = self.clone();
        let index = next.layers.iter().position(|layer| layer.id == layer_id)?;
        if index + 1 >= next.layers.len() {
            return None;
        }
        next.layers.swap(index, index + 1);
        Some(next)
    }

    pub fn moved_layer_down_document(&self, layer_id: LayerId) -> Option<Self> {
        let mut next = self.clone();
        let index = next.layers.iter().position(|layer| layer.id == layer_id)?;
        if index == 0 {
            return None;
        }
        next.layers.swap(index - 1, index);
        Some(next)
    }

    pub fn toggled_grid_visibility_document(&self) -> Option<Self> {
        let mut next = self.clone();
        next.grid.visible = !next.grid.visible;
        (next.grid.visible != self.grid.visible).then_some(next)
    }

    pub fn toggled_grid_snap_document(&self) -> Option<Self> {
        let mut next = self.clone();
        next.grid.snap_enabled = !next.grid.snap_enabled;
        (next.grid.snap_enabled != self.grid.snap_enabled).then_some(next)
    }

    pub fn set_grid_spacing_document(&self, spacing: f32) -> Option<Self> {
        if !spacing.is_finite() {
            return None;
        }

        let mut next = self.clone();
        next.grid.spacing = spacing.max(MIN_GRID_SPACING);
        next.sanitize_in_place();
        ((next.grid.spacing - self.grid.spacing).abs() >= 0.1).then_some(next)
    }

    pub fn toggled_guides_visibility_document(&self) -> Option<Self> {
        let mut next = self.clone();
        next.guides.visible = !next.guides.visible;
        (next.guides.visible != self.guides.visible).then_some(next)
    }

    pub fn toggled_guides_snap_document(&self) -> Option<Self> {
        let mut next = self.clone();
        next.guides.snap_enabled = !next.guides.snap_enabled;
        (next.guides.snap_enabled != self.guides.snap_enabled).then_some(next)
    }

    pub fn toggled_smart_guides_visibility_document(&self) -> Option<Self> {
        let mut next = self.clone();
        next.smart_guides.visible = !next.smart_guides.visible;
        (next.smart_guides.visible != self.smart_guides.visible).then_some(next)
    }

    pub fn toggled_rulers_visibility_document(&self) -> Option<Self> {
        let mut next = self.clone();
        next.rulers.visible = !next.rulers.visible;
        (next.rulers.visible != self.rulers.visible).then_some(next)
    }

    pub fn add_guide_document(&self, axis: GuideAxis, position: f32) -> Option<Self> {
        let position = clamp_guide_position(self.canvas_size, GuideLine::new(axis, position));
        if self
            .guides
            .lines
            .iter()
            .any(|guide| guide.axis == axis && (guide.position - position).abs() < 0.1)
        {
            return None;
        }

        let mut next = self.clone();
        next.guides.lines.push(GuideLine::new(axis, position));
        next.sanitize_in_place();
        Some(next)
    }

    pub fn remove_guide_document(&self, index: usize) -> Option<Self> {
        if index >= self.guides.lines.len() {
            return None;
        }

        let mut next = self.clone();
        next.guides.lines.remove(index);
        Some(next)
    }

    pub fn moved_guide_document(&self, index: usize, position: f32) -> Option<Self> {
        let guide = self.guides.lines.get(index).copied()?;
        let position = clamp_guide_position(self.canvas_size, GuideLine::new(guide.axis, position));

        if (guide.position - position).abs() < 0.1 {
            return None;
        }

        if self
            .guides
            .lines
            .iter()
            .enumerate()
            .any(|(candidate_index, candidate)| {
                candidate_index != index
                    && candidate.axis == guide.axis
                    && (candidate.position - position).abs() < 0.1
            })
        {
            return None;
        }

        let mut next = self.clone();
        next.guides.lines[index].position = position;
        next.sanitize_in_place();
        Some(next)
    }

    pub fn moved_selection_to_layer_document(
        &self,
        indices: &[usize],
        destination_layer_id: LayerId,
    ) -> Option<(Self, Vec<usize>)> {
        self.transfer_selection_to_layer_document(indices, destination_layer_id, false)
    }

    pub fn duplicated_selection_to_layer_document(
        &self,
        indices: &[usize],
        destination_layer_id: LayerId,
    ) -> Option<(Self, Vec<usize>)> {
        self.transfer_selection_to_layer_document(indices, destination_layer_id, true)
    }

    pub fn push_stroke(&mut self, stroke: Stroke) {
        self.push_element(PaintElement::Stroke(stroke));
    }

    pub fn push_shape(&mut self, shape: ShapeElement) {
        self.push_element(PaintElement::Shape(shape));
    }

    pub fn push_element(&mut self, element: PaintElement) {
        if let Some(layer) = self.active_layer_mut() {
            layer.elements.push(element);
        }
    }

    pub fn replace_element(&mut self, index: usize, element: PaintElement) -> bool {
        let Some(layer) = self.active_layer_mut() else {
            return false;
        };

        if let Some(slot) = layer.elements.get_mut(index) {
            *slot = element;
            true
        } else {
            false
        }
    }

    pub fn element(&self, index: usize) -> Option<&PaintElement> {
        self.active_layer()?.elements.get(index)
    }

    pub fn translate_element(&mut self, index: usize, delta: PaintVector) -> bool {
        if delta.is_zero() {
            return false;
        }

        let Some(element) = self.element(index).cloned() else {
            return false;
        };

        let Some(layer) = self.active_layer_mut() else {
            return false;
        };
        layer.elements[index] = element.translated(delta);
        true
    }

    pub fn translate_elements(&mut self, indices: &[usize], delta: PaintVector) -> bool {
        if delta.is_zero() {
            return false;
        }

        let element_len = self.active_layer().map_or(0, |layer| layer.elements.len());
        let indices = normalize_indices(indices, element_len);
        if indices.is_empty() {
            return false;
        }

        let Some(layer) = self.active_layer_mut() else {
            return false;
        };
        for index in indices {
            let element = layer
                .elements
                .get(index)
                .cloned()
                .expect("normalized indices should stay in bounds");
            layer.elements[index] = element.translated(delta);
        }

        true
    }

    pub fn selection_bounds(&self, indices: &[usize]) -> Option<ElementBounds> {
        let layer = self.active_layer()?;
        let mut bounds = normalize_indices(indices, layer.elements.len())
            .into_iter()
            .filter_map(|index| layer.elements[index].bounds());
        let first = bounds.next()?;
        Some(bounds.fold(first, ElementBounds::union))
    }

    pub fn hit_test_rect(&self, selection_rect: ElementBounds) -> Vec<usize> {
        if !self.active_layer_is_editable() {
            return Vec::new();
        }

        let Some(layer) = self.active_layer() else {
            return Vec::new();
        };

        layer
            .elements
            .iter()
            .enumerate()
            .filter_map(|(index, element)| {
                element
                    .bounds()
                    .is_some_and(|bounds| bounds.intersects(selection_rect))
                    .then_some(index)
            })
            .collect()
    }

    pub fn replace_elements(&mut self, replacements: &[(usize, PaintElement)]) -> bool {
        let element_len = self.active_layer().map_or(0, |layer| layer.elements.len());
        let replacements = normalize_replacements(replacements, element_len);
        if replacements.is_empty() {
            return false;
        }

        let Some(layer) = self.active_layer_mut() else {
            return false;
        };
        for (index, element) in replacements {
            layer.elements[index] = element;
        }

        true
    }

    pub fn selection_contains_group(&self, indices: &[usize]) -> bool {
        let Some(layer) = self.active_layer() else {
            return false;
        };

        normalize_indices(indices, layer.elements.len())
            .into_iter()
            .any(|index| layer.elements[index].is_group())
    }

    pub fn grouped_document(&self, indices: &[usize]) -> Option<(Self, Vec<usize>)> {
        let layer = self.active_layer()?;
        let indices = normalize_indices(indices, layer.elements.len());
        if indices.len() < 2 {
            return None;
        }

        let insert_index = indices[0];
        let selected_flags = selection_flags(layer.elements.len(), &indices);
        let grouped_elements: Vec<PaintElement> = indices
            .iter()
            .map(|index| layer.elements[*index].clone())
            .collect();

        let mut next = self.clone();
        let active_index = next.active_layer_index()?;
        let mut next_elements = Vec::with_capacity(layer.elements.len() - indices.len() + 1);
        for (index, element) in layer.elements.iter().enumerate() {
            if index == insert_index {
                next_elements.push(PaintElement::Group(GroupElement {
                    elements: grouped_elements.clone(),
                }));
            }

            if !selected_flags[index] {
                next_elements.push(element.clone());
            }
        }
        next.layers[active_index].elements = next_elements;

        Some((next, vec![insert_index]))
    }

    pub fn ungrouped_document(&self, indices: &[usize]) -> Option<(Self, Vec<usize>)> {
        let layer = self.active_layer()?;
        let selected_indices = normalize_indices(indices, layer.elements.len());
        if selected_indices.is_empty() {
            return None;
        }

        let selected_flags = selection_flags(layer.elements.len(), &selected_indices);
        let mut next_elements = Vec::new();
        let mut next_selection = Vec::new();
        let mut changed = false;

        for (index, element) in layer.elements.iter().enumerate() {
            match (selected_flags[index], element) {
                (true, PaintElement::Group(group)) => {
                    changed = true;
                    for child in &group.elements {
                        next_selection.push(next_elements.len());
                        next_elements.push(child.clone());
                    }
                }
                (true, _) => {
                    next_selection.push(next_elements.len());
                    next_elements.push(element.clone());
                }
                (false, _) => next_elements.push(element.clone()),
            }
        }

        if !changed {
            return None;
        }

        let mut next = self.clone();
        let active_index = next.active_layer_index()?;
        next.layers[active_index].elements = next_elements;
        Some((next, next_selection))
    }

    pub fn aligned_document(&self, indices: &[usize], alignment: AlignmentKind) -> Option<Self> {
        if !self.active_layer_is_editable() {
            return None;
        }

        let layer = self.active_layer()?;
        let indices = normalize_indices(indices, layer.elements.len());
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
        if !self.active_layer_is_editable() {
            return None;
        }

        let layer = self.active_layer()?;
        let selected_indices = normalize_indices(indices, layer.elements.len());
        if selected_indices.is_empty() {
            return None;
        }

        let mut next_elements = layer.elements.clone();
        let mut changed = false;
        match command {
            StackOrderCommand::BringToFront => {
                let selected_flags = selection_flags(layer.elements.len(), &selected_indices);
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
                if !changed {
                    return None;
                }
                let mut next = self.clone();
                let active_index = next.active_layer_index()?;
                next.layers[active_index].elements = next_elements;
                return Some((next, next_selected));
            }
            StackOrderCommand::SendToBack => {
                let selected_flags = selection_flags(layer.elements.len(), &selected_indices);
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
                if !changed {
                    return None;
                }
                let mut next = self.clone();
                let active_index = next.active_layer_index()?;
                next.layers[active_index].elements = next_elements;
                return Some((next, next_selected));
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
        if !changed {
            return None;
        }

        let mut next = self.clone();
        let active_index = next.active_layer_index()?;
        next.layers[active_index].elements = next_elements;
        Some((next, next_selected))
    }

    pub fn hit_test(&self, point: PaintPoint, tolerance: f32) -> Option<usize> {
        if !self.active_layer_is_editable() {
            return None;
        }

        self.active_layer()?
            .elements
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, element)| element.hit_test(point, tolerance).then_some(index))
    }

    pub fn resized_selection_document(
        &self,
        indices: &[usize],
        anchor: PaintPoint,
        scale_x: f32,
        scale_y: f32,
    ) -> Option<Self> {
        let layer = self.active_layer()?;
        let indices = normalize_indices(indices, layer.elements.len());
        if indices.is_empty() {
            return None;
        }

        let mut replacements = Vec::new();
        for index in indices {
            let element = self.element(index)?.clone();
            replacements.push((index, element.scaled_from(anchor, scale_x, scale_y)));
        }

        let mut next = self.clone();
        next.replace_elements(&replacements).then_some(next)
    }

    pub fn rotated_selection_document(
        &self,
        indices: &[usize],
        pivot: PaintPoint,
        angle_radians: f32,
    ) -> Option<Self> {
        let layer = self.active_layer()?;
        let indices = normalize_indices(indices, layer.elements.len());
        if indices.is_empty() || angle_radians.abs() < f32::EPSILON {
            return None;
        }

        let mut replacements = Vec::new();
        for index in indices {
            let element = self.element(index)?.clone();
            replacements.push((index, element.rotated_around(pivot, angle_radians)));
        }

        let mut next = self.clone();
        next.replace_elements(&replacements).then_some(next)
    }

    pub fn distributed_document(
        &self,
        indices: &[usize],
        distribution: DistributionKind,
    ) -> Option<Self> {
        if !self.active_layer_is_editable() {
            return None;
        }

        let layer = self.active_layer()?;
        let indices = normalize_indices(indices, layer.elements.len());
        if indices.len() < 3 {
            return None;
        }

        let mut ordered: Vec<_> = indices
            .iter()
            .filter_map(|index| {
                self.element(*index)
                    .and_then(PaintElement::bounds)
                    .map(|bounds| (*index, bounds))
            })
            .collect();
        if ordered.len() < 3 {
            return None;
        }

        match distribution {
            DistributionKind::Horizontal => {
                ordered.sort_by(|left, right| left.1.min.x.total_cmp(&right.1.min.x))
            }
            DistributionKind::Vertical => {
                ordered.sort_by(|left, right| left.1.min.y.total_cmp(&right.1.min.y))
            }
        }

        let total_size: f32 = ordered
            .iter()
            .map(|(_, bounds)| match distribution {
                DistributionKind::Horizontal => bounds.width(),
                DistributionKind::Vertical => bounds.height(),
            })
            .sum();

        let span = match distribution {
            DistributionKind::Horizontal => ordered.last()?.1.max.x - ordered.first()?.1.min.x,
            DistributionKind::Vertical => ordered.last()?.1.max.y - ordered.first()?.1.min.y,
        };
        let gap = (span - total_size) / (ordered.len().saturating_sub(1) as f32);
        let mut next = self.clone();
        let mut changed = false;

        let mut cursor = match distribution {
            DistributionKind::Horizontal => ordered.first()?.1.max.x + gap,
            DistributionKind::Vertical => ordered.first()?.1.max.y + gap,
        };

        for (index, bounds) in ordered.iter().skip(1).take(ordered.len().saturating_sub(2)) {
            let delta = match distribution {
                DistributionKind::Horizontal => PaintVector::new(cursor - bounds.min.x, 0.0),
                DistributionKind::Vertical => PaintVector::new(0.0, cursor - bounds.min.y),
            };

            if !delta.is_zero() && next.translate_element(*index, delta) {
                changed = true;
            }

            cursor += match distribution {
                DistributionKind::Horizontal => bounds.width() + gap,
                DistributionKind::Vertical => bounds.height() + gap,
            };
        }

        changed.then_some(next)
    }

    fn transfer_selection_to_layer_document(
        &self,
        indices: &[usize],
        destination_layer_id: LayerId,
        duplicate: bool,
    ) -> Option<(Self, Vec<usize>)> {
        if !self.active_layer_is_editable() || self.active_layer_id == destination_layer_id {
            return None;
        }

        let source_layer = self.active_layer()?;
        let selected_indices = normalize_indices(indices, source_layer.elements.len());
        if selected_indices.is_empty() {
            return None;
        }

        let destination_index = self.layer_index(destination_layer_id)?;
        if !self.layers.get(destination_index)?.is_editable() {
            return None;
        }

        let transferred_elements: Vec<_> = selected_indices
            .iter()
            .map(|index| source_layer.elements[*index].clone())
            .collect();
        let source_index = self.active_layer_index()?;
        let mut next = self.clone();
        let insertion_start = next.layers[destination_index].elements.len();
        next.layers[destination_index]
            .elements
            .extend(transferred_elements.iter().cloned());

        if !duplicate {
            let selected_flags =
                selection_flags(next.layers[source_index].elements.len(), &selected_indices);
            next.layers[source_index].elements = next.layers[source_index]
                .elements
                .iter()
                .enumerate()
                .filter_map(|(index, element)| (!selected_flags[index]).then_some(element.clone()))
                .collect();
        }

        next.active_layer_id = destination_layer_id;
        let next_selection =
            (insertion_start..insertion_start + transferred_elements.len()).collect();
        Some((next, next_selection))
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
            current: document.sanitized(),
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
        for layer in &mut self.current.layers {
            layer.elements.clear();
        }
        self.redo_stack.clear();
        true
    }

    pub fn replace_document(&mut self, document: PaintDocument) -> bool {
        let document = document.sanitized();
        if self.current == document {
            return false;
        }

        self.push_undo_snapshot();
        self.current = document;
        self.redo_stack.clear();
        true
    }

    pub fn set_active_layer(&mut self, layer_id: LayerId) -> bool {
        self.current.set_active_layer(layer_id)
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

fn guide_sort_key(guide: GuideLine) -> (u8, i32) {
    let axis = match guide.axis {
        GuideAxis::Horizontal => 0,
        GuideAxis::Vertical => 1,
    };
    (axis, (guide.position * 100.0).round() as i32)
}

fn clamp_guide_position(canvas_size: CanvasSize, guide: GuideLine) -> f32 {
    match guide.axis {
        GuideAxis::Horizontal => guide.position.clamp(0.0, canvas_size.height),
        GuideAxis::Vertical => guide.position.clamp(0.0, canvas_size.width),
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

fn normalize_replacements(
    replacements: &[(usize, PaintElement)],
    len: usize,
) -> Vec<(usize, PaintElement)> {
    let mut replacements: Vec<_> = replacements
        .iter()
        .filter(|(index, _)| *index < len)
        .map(|(index, element)| (*index, element.clone()))
        .collect();
    replacements.sort_unstable_by_key(|(index, _)| *index);
    replacements.dedup_by(|left, right| left.0 == right.0);
    replacements
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
        AlignmentKind, CanvasSize, DistributionKind, DocumentHistory, ElementBounds, GroupElement,
        GuideAxis, LayerId, PaintDocument, PaintElement, PaintPoint, PaintVector, RgbaColor,
        ShapeElement, ShapeHandle, ShapeKind, StackOrderCommand, Stroke, ToolKind,
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

    fn layered_transfer_document() -> (PaintDocument, LayerId, LayerId) {
        let mut document = PaintDocument::default();
        let source_layer_id = document.active_layer_id();
        document.push_shape(sample_shape());
        document.push_stroke(sample_stroke());

        let (mut next, destination_layer_id) = document.add_layer_document();
        next.push_shape(ShapeElement::new(
            ShapeKind::Ellipse,
            RgbaColor::new(32, 96, 220, 255),
            3.0,
            PaintPoint::new(24.0, 24.0),
            PaintPoint::new(60.0, 52.0),
        ));
        assert!(next.set_active_layer(source_layer_id));
        (next, source_layer_id, destination_layer_id)
    }

    #[test]
    fn stroke_hit_test_accepts_nearby_points() {
        let stroke = sample_stroke();
        assert!(stroke.hit_test(PaintPoint::new(20.0, 12.5), 2.0));
        assert!(!stroke.hit_test(PaintPoint::new(20.0, 30.0), 2.0));
    }

    #[test]
    fn tool_kinds_adjust_stroke_width_and_alpha() {
        let color = RgbaColor::from_rgba(120, 80, 40, 200);
        let pen = Stroke::new(ToolKind::Brush, color, 10.0);
        let pencil = Stroke::new(ToolKind::Pencil, color, 10.0);
        let marker = Stroke::new(ToolKind::Marker, color, 10.0);

        assert_eq!(pen.effective_width(), 10.0);
        assert!(pencil.effective_width() < pen.effective_width());
        assert!(marker.effective_width() > pen.effective_width());
        assert!(ToolKind::Pencil.styled_color(color).a < color.a);
        assert!(ToolKind::Marker.styled_color(color).a < ToolKind::Pencil.styled_color(color).a);
        assert_eq!(pen.render_passes().len(), 1);
        assert_eq!(pencil.render_passes().len(), 3);
        assert_eq!(marker.render_passes().len(), 2);
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
            ..PaintDocument::default()
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
    fn hit_test_rect_selects_multiple_elements() {
        let mut document = PaintDocument::default();
        document.push_shape(sample_shape());
        document.push_stroke(sample_stroke());

        let indices = document.hit_test_rect(ElementBounds {
            min: PaintPoint::new(0.0, 0.0),
            max: PaintPoint::new(100.0, 60.0),
        });

        assert_eq!(indices, vec![0, 1]);
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
            .active_layer()
            .expect("active layer")
            .elements
            .iter()
            .map(|element| match element {
                PaintElement::Shape(shape) => shape.color,
                PaintElement::Stroke(_) => RgbaColor::white(),
                PaintElement::Group(_) => RgbaColor::new(1, 1, 1, 255),
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

    #[test]
    fn resized_selection_document_scales_multiple_elements() {
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
            PaintPoint::new(60.0, 20.0),
            PaintPoint::new(80.0, 40.0),
        ));

        let resized = document
            .resized_selection_document(&[0, 1], PaintPoint::new(20.0, 20.0), 2.0, 2.0)
            .expect("resize should succeed");

        let first_bounds = resized
            .element(0)
            .and_then(PaintElement::bounds)
            .expect("first bounds");
        let second_bounds = resized
            .element(1)
            .and_then(PaintElement::bounds)
            .expect("second bounds");

        assert!(first_bounds.width() > 20.0);
        assert!(second_bounds.min.x > 80.0);
    }

    #[test]
    fn rotated_selection_document_rotates_shapes_and_strokes() {
        let mut document = PaintDocument::default();
        document.push_shape(sample_shape());
        document.push_stroke(sample_stroke());

        let pivot = PaintPoint::new(100.0, 50.0);
        let rotated = document
            .rotated_selection_document(&[0, 1], pivot, std::f32::consts::FRAC_PI_2)
            .expect("rotate should succeed");

        let Some(PaintElement::Shape(shape)) = rotated.element(0) else {
            panic!("first element should stay a shape");
        };
        let Some(PaintElement::Stroke(stroke)) = rotated.element(1) else {
            panic!("second element should stay a stroke");
        };

        assert!(shape.rotation_radians.abs() > 0.1);
        assert_ne!(stroke.points[0], PaintPoint::new(10.0, 10.0));
    }

    #[test]
    fn history_replace_document_tracks_multi_resize_undo_redo() {
        let mut history = DocumentHistory::new(PaintDocument::default());
        history.commit_shape(sample_shape());
        history.commit_stroke(sample_stroke());

        let resized = history
            .current()
            .resized_selection_document(&[0, 1], PaintPoint::new(0.0, 0.0), 1.5, 1.25)
            .expect("resize should succeed");
        assert!(history.replace_document(resized.clone()));
        assert!(history.undo());
        assert!(history.redo());
        assert_eq!(history.current(), &resized);
    }

    #[test]
    fn grouped_and_ungrouped_documents_preserve_internal_order() {
        let mut document = PaintDocument::default();
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::new(255, 0, 0, 255),
            2.0,
            PaintPoint::new(10.0, 10.0),
            PaintPoint::new(30.0, 30.0),
        ));
        document.push_stroke(sample_stroke());
        document.push_shape(ShapeElement::new(
            ShapeKind::Ellipse,
            RgbaColor::new(0, 0, 255, 255),
            2.0,
            PaintPoint::new(40.0, 12.0),
            PaintPoint::new(70.0, 32.0),
        ));

        let (grouped, selection) = document
            .grouped_document(&[0, 2])
            .expect("grouping should succeed");
        assert_eq!(selection, vec![0]);

        let Some(PaintElement::Group(group)) = grouped.element(0) else {
            panic!("first element should become a group");
        };
        assert_eq!(group.elements.len(), 2);
        assert!(matches!(group.elements[0], PaintElement::Shape(_)));
        assert!(matches!(group.elements[1], PaintElement::Shape(_)));

        let (ungrouped, selection) = grouped
            .ungrouped_document(&[0])
            .expect("ungrouping should succeed");
        assert_eq!(selection, vec![0, 1]);
        assert_eq!(ungrouped.element_count(), 3);
        assert_eq!(ungrouped.element(0), document.element(0));
        assert_eq!(ungrouped.element(1), document.element(2));
        assert_eq!(ungrouped.element(2), document.element(1));
    }

    #[test]
    fn selection_contains_group_detects_group_elements() {
        let document = PaintDocument::from_flat_elements(
            CanvasSize::default(),
            RgbaColor::white(),
            vec![PaintElement::Group(GroupElement {
                elements: vec![PaintElement::Stroke(sample_stroke())],
            })],
        );

        assert!(document.selection_contains_group(&[0]));
        assert!(!document.selection_contains_group(&[1]));
    }

    #[test]
    fn distribute_horizontal_evens_out_middle_positions() {
        let mut document = PaintDocument::default();
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            2.0,
            PaintPoint::new(0.0, 0.0),
            PaintPoint::new(10.0, 10.0),
        ));
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            2.0,
            PaintPoint::new(30.0, 0.0),
            PaintPoint::new(40.0, 10.0),
        ));
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            2.0,
            PaintPoint::new(90.0, 0.0),
            PaintPoint::new(100.0, 10.0),
        ));

        let distributed = document
            .distributed_document(&[0, 1, 2], DistributionKind::Horizontal)
            .expect("distribution should succeed");

        let first = distributed
            .element(0)
            .and_then(PaintElement::bounds)
            .unwrap();
        let second = distributed
            .element(1)
            .and_then(PaintElement::bounds)
            .unwrap();
        let third = distributed
            .element(2)
            .and_then(PaintElement::bounds)
            .unwrap();

        let gap_a = second.min.x - first.max.x;
        let gap_b = third.min.x - second.max.x;
        assert!((gap_a - gap_b).abs() < 0.001);
    }

    #[test]
    fn history_replace_document_tracks_group_undo_redo() {
        let mut history = DocumentHistory::new(PaintDocument::default());
        history.commit_shape(sample_shape());
        history.commit_stroke(sample_stroke());

        let grouped = history
            .current()
            .grouped_document(&[0, 1])
            .expect("group should succeed")
            .0;
        assert!(history.replace_document(grouped.clone()));
        assert!(history.undo());
        assert!(history.redo());
        assert_eq!(history.current(), &grouped);
    }

    #[test]
    fn layer_add_delete_tracks_undo_redo() {
        let mut history = DocumentHistory::new(PaintDocument::default());
        let (with_layer, new_layer_id) = history.current().add_layer_document();
        assert!(history.replace_document(with_layer.clone()));
        assert_eq!(history.current().layer_count(), 2);
        assert_eq!(history.current().active_layer_id(), new_layer_id);
        assert!(history.undo());
        assert_eq!(history.current().layer_count(), 1);
        assert!(history.redo());
        assert_eq!(history.current().layer_count(), 2);

        let deleted = history
            .current()
            .delete_active_layer_document()
            .expect("delete layer should succeed")
            .0;
        assert!(history.replace_document(deleted.clone()));
        assert_eq!(history.current().layer_count(), 1);
    }

    #[test]
    fn duplicate_active_layer_tracks_undo_redo() {
        let mut document = PaintDocument::default();
        document.push_shape(sample_shape());
        let mut history = DocumentHistory::new(document);

        let (duplicated, duplicate_id) = history
            .current()
            .duplicate_active_layer_document()
            .expect("duplicate layer should succeed");
        assert!(history.replace_document(duplicated.clone()));
        assert_eq!(history.current().layer_count(), 2);
        assert_eq!(history.current().active_layer_id(), duplicate_id);
        assert_eq!(
            history
                .current()
                .layer(duplicate_id)
                .unwrap()
                .elements
                .len(),
            1
        );

        assert!(history.undo());
        assert_eq!(history.current().layer_count(), 1);
        assert!(history.redo());
        assert_eq!(history.current().layer_count(), 2);
    }

    #[test]
    fn hidden_or_locked_active_layer_disables_selection_hits() {
        let mut document = PaintDocument::default();
        document.push_shape(sample_shape());

        let hidden = document
            .toggled_layer_visibility_document(document.active_layer_id())
            .expect("hide active layer");
        assert_eq!(hidden.hit_test(PaintPoint::new(82.0, 23.0), 3.0), None);
        assert!(
            hidden
                .hit_test_rect(hidden.selection_bounds(&[0]).unwrap())
                .is_empty()
        );

        let locked = document
            .toggled_layer_locked_document(document.active_layer_id())
            .expect("lock active layer");
        assert_eq!(locked.hit_test(PaintPoint::new(82.0, 23.0), 3.0), None);
        assert!(
            locked
                .hit_test_rect(locked.selection_bounds(&[0]).unwrap())
                .is_empty()
        );
    }

    #[test]
    fn move_selection_to_another_layer_tracks_undo_redo() {
        let (document, source_layer_id, destination_layer_id) = layered_transfer_document();
        let (moved, selection) = document
            .moved_selection_to_layer_document(&[0, 1], destination_layer_id)
            .expect("move to other layer should succeed");

        assert_eq!(moved.active_layer_id(), destination_layer_id);
        assert_eq!(selection, vec![1, 2]);
        assert_eq!(moved.layer(source_layer_id).unwrap().elements.len(), 0);
        assert_eq!(moved.layer(destination_layer_id).unwrap().elements.len(), 3);

        let mut history = DocumentHistory::new(document.clone());
        assert!(history.replace_document(moved.clone()));
        assert_eq!(history.current(), &moved);
        assert!(history.undo());
        assert_eq!(history.current(), &document);
        assert!(history.redo());
        assert_eq!(history.current(), &moved);
    }

    #[test]
    fn duplicate_selection_to_another_layer_tracks_undo_redo() {
        let (document, source_layer_id, destination_layer_id) = layered_transfer_document();
        let (duplicated, selection) = document
            .duplicated_selection_to_layer_document(&[0, 1], destination_layer_id)
            .expect("duplicate to other layer should succeed");

        assert_eq!(duplicated.active_layer_id(), destination_layer_id);
        assert_eq!(selection, vec![1, 2]);
        assert_eq!(duplicated.layer(source_layer_id).unwrap().elements.len(), 2);
        assert_eq!(
            duplicated
                .layer(destination_layer_id)
                .unwrap()
                .elements
                .len(),
            3
        );

        let mut history = DocumentHistory::new(document.clone());
        assert!(history.replace_document(duplicated.clone()));
        assert_eq!(history.current(), &duplicated);
        assert!(history.undo());
        assert_eq!(history.current(), &document);
        assert!(history.redo());
        assert_eq!(history.current(), &duplicated);
    }

    #[test]
    fn hidden_or_locked_destination_rejects_layer_transfer() {
        let (document, _, destination_layer_id) = layered_transfer_document();
        let hidden = document
            .toggled_layer_visibility_document(destination_layer_id)
            .expect("hide destination layer");
        assert!(
            hidden
                .moved_selection_to_layer_document(&[0], destination_layer_id)
                .is_none()
        );
        assert!(
            hidden
                .duplicated_selection_to_layer_document(&[0], destination_layer_id)
                .is_none()
        );

        let locked = document
            .toggled_layer_locked_document(destination_layer_id)
            .expect("lock destination layer");
        assert!(
            locked
                .moved_selection_to_layer_document(&[0], destination_layer_id)
                .is_none()
        );
        assert!(
            locked
                .duplicated_selection_to_layer_document(&[0], destination_layer_id)
                .is_none()
        );
    }

    #[test]
    fn guide_and_grid_settings_track_undo_redo() {
        let mut history = DocumentHistory::new(PaintDocument::default());

        let with_grid_hidden = history
            .current()
            .toggled_grid_visibility_document()
            .expect("toggle grid visibility");
        assert!(history.replace_document(with_grid_hidden.clone()));
        assert!(!history.current().grid.visible);
        assert!(history.undo());
        assert!(history.current().grid.visible);
        assert!(history.redo());
        assert!(!history.current().grid.visible);

        let with_guide = history
            .current()
            .add_guide_document(GuideAxis::Vertical, 128.0)
            .expect("add guide");
        assert!(history.replace_document(with_guide.clone()));
        assert_eq!(history.current().guides.lines.len(), 1);
        assert!(history.undo());
        assert!(history.current().guides.lines.is_empty());
        assert!(history.redo());
        assert_eq!(history.current().guides.lines.len(), 1);

        let spaced = history
            .current()
            .set_grid_spacing_document(32.0)
            .expect("update grid spacing");
        assert!(history.replace_document(spaced.clone()));
        assert_eq!(history.current().grid.spacing, 32.0);
        assert!(history.undo());
        assert_eq!(history.current().grid.spacing, super::DEFAULT_GRID_SPACING);
        assert!(history.redo());
        assert_eq!(history.current().grid.spacing, 32.0);

        let moved = history
            .current()
            .moved_guide_document(0, 196.0)
            .expect("move guide");
        assert!(history.replace_document(moved.clone()));
        assert_eq!(history.current().guides.lines[0].position, 196.0);
        assert!(history.undo());
        assert_eq!(history.current().guides.lines[0].position, 128.0);
        assert!(history.redo());
        assert_eq!(history.current().guides.lines[0].position, 196.0);

        let rulers_hidden = history
            .current()
            .toggled_rulers_visibility_document()
            .expect("toggle rulers");
        assert!(history.replace_document(rulers_hidden.clone()));
        assert!(!history.current().rulers.visible);
        assert!(history.undo());
        assert!(history.current().rulers.visible);

        let smart_guides_hidden = history
            .current()
            .toggled_smart_guides_visibility_document()
            .expect("toggle smart guides");
        assert!(history.replace_document(smart_guides_hidden.clone()));
        assert!(!history.current().smart_guides.visible);
        assert!(history.undo());
        assert!(history.current().smart_guides.visible);
    }
}
