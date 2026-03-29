use std::collections::VecDeque;
use std::ops::RangeInclusive;

use tiny_skia::Pixmap;

use crate::model::{FillElement, FillSpan, PaintDocument, PaintPoint, RgbaColor};
use crate::render::{RasterBackground, render_document_pixmap_with_background};

const FILL_TOLERANCE: u8 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloodFillFailure {
    ActiveLayerNotEditable,
    OutsideCanvas,
    SameColor,
    RegionNotFound,
    RenderFailed,
}

impl FloodFillFailure {
    pub const fn message(self) -> &'static str {
        match self {
            Self::ActiveLayerNotEditable => {
                "現在のレイヤーは編集できません。表示とロックを確認してください。"
            }
            Self::OutsideCanvas => "キャンバスの外は塗れません。",
            Self::SameColor => "同じ色なので見た目は変わりませんでした。",
            Self::RegionNotFound => "閉じた領域が見つかりませんでした。",
            Self::RenderFailed => "塗りの準備中に失敗しました。",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FloodFillResult {
    pub element: FillElement,
    pub pixel_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AbsoluteFillSpan {
    y: u32,
    x_start: u32,
    x_end: u32,
}

#[derive(Debug, Clone, Copy)]
struct FillMatchRule {
    target_color: RgbaColor,
    tolerance: u8,
}

pub fn flood_fill_document(
    document: &PaintDocument,
    seed: PaintPoint,
    fill_color: RgbaColor,
) -> Result<FloodFillResult, FloodFillFailure> {
    // 第一版のバケツ塗りは「見えている作品結果」を境界判定に使い、
    // 作業レイヤーへ scanline span として保存することで JSON / PNG / wasm を壊さず扱います。
    if !document.active_layer_is_editable() {
        return Err(FloodFillFailure::ActiveLayerNotEditable);
    }

    let pixmap = render_document_pixmap_with_background(document, RasterBackground::Opaque)
        .map_err(|_| FloodFillFailure::RenderFailed)?;
    let width = pixmap.width();
    let height = pixmap.height();

    if seed.x < 0.0 || seed.y < 0.0 || seed.x >= width as f32 || seed.y >= height as f32 {
        return Err(FloodFillFailure::OutsideCanvas);
    }

    let seed_x = seed.x.floor() as u32;
    let seed_y = seed.y.floor() as u32;
    let target_color =
        pixel_color(&pixmap, seed_x, seed_y).ok_or(FloodFillFailure::OutsideCanvas)?;
    if rgba_within_tolerance(target_color, fill_color, FILL_TOLERANCE) {
        return Err(FloodFillFailure::SameColor);
    }

    let spans = extract_fill_region(&pixmap, seed_x, seed_y, target_color, FILL_TOLERANCE);
    if spans.is_empty() {
        return Err(FloodFillFailure::RegionNotFound);
    }

    let min_x = spans
        .iter()
        .map(|span| span.x_start)
        .min()
        .unwrap_or(seed_x);
    let min_y = spans.iter().map(|span| span.y).min().unwrap_or(seed_y);
    let pixel_count = spans
        .iter()
        .map(|span| (span.x_end - span.x_start) as usize)
        .sum();

    let relative_spans = spans
        .into_iter()
        .map(|span| FillSpan {
            y: span.y - min_y,
            x_start: span.x_start - min_x,
            x_end: span.x_end - min_x,
        })
        .collect();

    Ok(FloodFillResult {
        element: FillElement::new(
            fill_color,
            PaintPoint::new(min_x as f32, min_y as f32),
            relative_spans,
        ),
        pixel_count,
    })
}

fn extract_fill_region(
    pixmap: &Pixmap,
    seed_x: u32,
    seed_y: u32,
    target_color: RgbaColor,
    tolerance: u8,
) -> Vec<AbsoluteFillSpan> {
    let width = pixmap.width();
    let height = pixmap.height();
    let mut visited = vec![false; (width * height) as usize];
    let mut queue = VecDeque::from([(seed_x, seed_y)]);
    let mut spans = Vec::new();

    while let Some((x, y)) = queue.pop_front() {
        let seed_index = pixel_index(width, x, y);
        if visited[seed_index] {
            continue;
        }
        let Some(color) = pixel_color(pixmap, x, y) else {
            continue;
        };
        if !rgba_within_tolerance(color, target_color, tolerance) {
            continue;
        }

        let mut left = x;
        while left > 0 {
            let next_x = left - 1;
            let index = pixel_index(width, next_x, y);
            if visited[index] {
                break;
            }
            let Some(color) = pixel_color(pixmap, next_x, y) else {
                break;
            };
            if !rgba_within_tolerance(color, target_color, tolerance) {
                break;
            }
            left = next_x;
        }

        let mut right = x;
        while right + 1 < width {
            let next_x = right + 1;
            let index = pixel_index(width, next_x, y);
            if visited[index] {
                break;
            }
            let Some(color) = pixel_color(pixmap, next_x, y) else {
                break;
            };
            if !rgba_within_tolerance(color, target_color, tolerance) {
                break;
            }
            right = next_x;
        }

        for fill_x in left..=right {
            let index = pixel_index(width, fill_x, y);
            visited[index] = true;
        }
        spans.push(AbsoluteFillSpan {
            y,
            x_start: left,
            x_end: right + 1,
        });

        if y > 0 {
            enqueue_adjacent_matches(
                pixmap,
                &mut visited,
                &mut queue,
                left..=right,
                y - 1,
                FillMatchRule {
                    target_color,
                    tolerance,
                },
            );
        }
        if y + 1 < height {
            enqueue_adjacent_matches(
                pixmap,
                &mut visited,
                &mut queue,
                left..=right,
                y + 1,
                FillMatchRule {
                    target_color,
                    tolerance,
                },
            );
        }
    }

    spans
}

fn enqueue_adjacent_matches(
    pixmap: &Pixmap,
    visited: &mut [bool],
    queue: &mut VecDeque<(u32, u32)>,
    x_range: RangeInclusive<u32>,
    y: u32,
    rule: FillMatchRule,
) {
    let width = pixmap.width();
    let right = *x_range.end();
    let mut x = *x_range.start();
    while x <= right {
        let index = pixel_index(width, x, y);
        let Some(color) = pixel_color(pixmap, x, y) else {
            x += 1;
            continue;
        };

        if visited[index] || !rgba_within_tolerance(color, rule.target_color, rule.tolerance) {
            x += 1;
            continue;
        }

        queue.push_back((x, y));
        x += 1;
        while x <= right {
            let index = pixel_index(width, x, y);
            let Some(color) = pixel_color(pixmap, x, y) else {
                break;
            };
            if visited[index] || !rgba_within_tolerance(color, rule.target_color, rule.tolerance) {
                break;
            }
            x += 1;
        }
    }
}

fn pixel_index(width: u32, x: u32, y: u32) -> usize {
    (y * width + x) as usize
}

fn pixel_color(pixmap: &Pixmap, x: u32, y: u32) -> Option<RgbaColor> {
    let pixel = pixmap.pixel(x, y)?.demultiply();
    Some(RgbaColor::from_rgba(
        pixel.red(),
        pixel.green(),
        pixel.blue(),
        pixel.alpha(),
    ))
}

fn rgba_within_tolerance(left: RgbaColor, right: RgbaColor, tolerance: u8) -> bool {
    channel_distance(left.r, right.r) <= tolerance
        && channel_distance(left.g, right.g) <= tolerance
        && channel_distance(left.b, right.b) <= tolerance
        && channel_distance(left.a, right.a) <= tolerance
}

fn channel_distance(left: u8, right: u8) -> u8 {
    left.abs_diff(right)
}

#[cfg(test)]
mod tests {
    use super::{FloodFillFailure, flood_fill_document};
    use crate::model::{
        CanvasSize, PaintDocument, PaintElement, PaintPoint, RgbaColor, ShapeElement, ShapeKind,
        Stroke, ToolKind,
    };
    use crate::render::{RasterBackground, render_document_pixmap_with_background};

    fn closed_shape_document() -> PaintDocument {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(64.0, 64.0),
            background: RgbaColor::white(),
            ..PaintDocument::default()
        };
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::charcoal(),
            4.0,
            PaintPoint::new(12.0, 12.0),
            PaintPoint::new(52.0, 52.0),
        ));
        document
    }

    #[test]
    fn flood_fill_fills_inside_closed_rectangle() {
        let document = closed_shape_document();

        let result = flood_fill_document(
            &document,
            PaintPoint::new(32.0, 32.0),
            RgbaColor::from_rgba(255, 64, 64, 200),
        )
        .expect("fill should succeed");

        assert!(result.pixel_count > 200);
        assert_eq!(result.element.color, RgbaColor::from_rgba(255, 64, 64, 200));
        assert!(!result.element.spans.is_empty());
    }

    #[test]
    fn flood_fill_uses_visible_layers_as_boundaries() {
        let document = closed_shape_document();
        let (mut layered, fill_layer_id) = document.add_layer_document();
        layered = layered
            .moved_layer_down_document(fill_layer_id)
            .expect("move layer down");
        layered.set_active_layer(fill_layer_id);

        let result = flood_fill_document(
            &layered,
            PaintPoint::new(32.0, 32.0),
            RgbaColor::from_rgba(64, 120, 220, 180),
        )
        .expect("fill should respect visible upper layer");

        let bounds = result.element.bounds().expect("fill bounds");
        assert!(
            bounds.min.x >= 14.0,
            "fill should stay inside the upper outline"
        );
        assert!(
            bounds.max.x <= 50.0,
            "fill should stay inside the upper outline"
        );
        assert!(
            bounds.min.y >= 14.0,
            "fill should stay inside the upper outline"
        );
        assert!(
            bounds.max.y <= 50.0,
            "fill should stay inside the upper outline"
        );

        let mut rendered = layered.clone();
        rendered.push_element(PaintElement::Fill(result.element));
        let pixmap =
            render_document_pixmap_with_background(&rendered, RasterBackground::Opaque).unwrap();
        let pixel = pixmap.pixel(32, 32).expect("filled pixel").demultiply();
        assert_eq!((pixel.red(), pixel.green(), pixel.blue()), (120, 160, 230));
    }

    #[test]
    fn flood_fill_rejects_same_color_target() {
        let document = PaintDocument::default();
        let result = flood_fill_document(&document, PaintPoint::new(4.0, 4.0), RgbaColor::white());

        assert_eq!(result, Err(FloodFillFailure::SameColor));
    }

    #[test]
    fn flood_fill_stops_at_visible_strokes() {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(64.0, 64.0),
            background: RgbaColor::white(),
            ..PaintDocument::default()
        };
        let mut stroke = Stroke::new(ToolKind::Brush, RgbaColor::charcoal(), 6.0);
        stroke.push_point(PaintPoint::new(20.0, 0.0));
        stroke.push_point(PaintPoint::new(20.0, 63.0));
        document.push_stroke(stroke);

        let result = flood_fill_document(
            &document,
            PaintPoint::new(8.0, 32.0),
            RgbaColor::from_rgba(255, 180, 60, 220),
        )
        .expect("fill should succeed");

        let bounds = result.element.bounds().expect("fill bounds");
        assert!(bounds.max.x < 20.0, "fill should stay on the left side");
    }
}
