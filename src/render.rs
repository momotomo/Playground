use std::error::Error;
use std::fmt::{self, Display, Formatter, Write};

use tiny_skia::{
    Color, FillRule, Paint, PathBuilder, Pixmap, Rect as TinyRect, Stroke as TinyStroke, Transform,
};

use crate::model::{
    FillElement, GroupElement, PaintDocument, PaintElement, PaintLayer, PaintPoint, PaintVector,
    RgbaColor, ShapeElement, ShapeKind, Stroke, ToolKind,
};

// Raster export options stay separate from document traversal so future SVG export can
// reuse the same element walk without coupling everything to PNG-specific choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasterBackground {
    Opaque,
    Transparent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    InvalidCanvasSize,
    PngEncode(String),
}

impl Display for RenderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCanvasSize => {
                write!(f, "Canvas size must be finite and greater than zero")
            }
            Self::PngEncode(error) => write!(f, "PNG encoding failed: {error}"),
        }
    }
}

impl Error for RenderError {}

pub fn render_document_pixmap_with_background(
    document: &PaintDocument,
    background: RasterBackground,
) -> Result<Pixmap, RenderError> {
    let (width, height) = raster_dimensions(document)?;
    let mut pixmap = Pixmap::new(width, height).ok_or(RenderError::InvalidCanvasSize)?;
    if matches!(background, RasterBackground::Opaque) {
        pixmap.fill(color_from_rgba(document.background));
    }

    for layer in document.visible_layers() {
        for element in &layer.elements {
            render_element(&mut pixmap, element, document.background, background);
        }
    }

    Ok(pixmap)
}

pub fn render_document_pixmap(document: &PaintDocument) -> Result<Pixmap, RenderError> {
    render_document_pixmap_with_background(document, RasterBackground::Opaque)
}

pub fn render_document_png_with_background(
    document: &PaintDocument,
    background: RasterBackground,
) -> Result<Vec<u8>, RenderError> {
    let pixmap = render_document_pixmap_with_background(document, background)?;
    pixmap
        .encode_png()
        .map_err(|error| RenderError::PngEncode(error.to_string()))
}

pub fn render_document_png(document: &PaintDocument) -> Result<Vec<u8>, RenderError> {
    render_document_png_with_background(document, RasterBackground::Opaque)
}

pub fn render_document_svg(document: &PaintDocument) -> Result<Vec<u8>, RenderError> {
    let (width, height) = raster_dimensions(document)?;
    let mut svg = String::new();
    writeln!(
        svg,
        r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>"#
    )
    .expect("write into String should succeed");
    writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}">"#
    )
    .expect("write into String should succeed");

    let mut skipped_eraser = false;
    for layer in document.visible_layers() {
        append_svg_layer(&mut svg, layer, &mut skipped_eraser);
    }

    if skipped_eraser {
        writeln!(
            svg,
            "<!-- 消しゴムストロークは SVG では簡略化せず省略しています。PNG は見たまま出力向けです。 -->"
        )
        .expect("write into String should succeed");
    }

    svg.push_str("</svg>\n");
    Ok(svg.into_bytes())
}

pub fn sample_document_color(document: &PaintDocument, point: PaintPoint) -> Option<RgbaColor> {
    let (width, height) = raster_dimensions(document).ok()?;
    if point.x < 0.0 || point.y < 0.0 || point.x >= width as f32 || point.y >= height as f32 {
        return None;
    }

    let pixmap = render_document_pixmap_with_background(document, RasterBackground::Opaque).ok()?;
    let x = point.x.floor().clamp(0.0, width.saturating_sub(1) as f32) as u32;
    let y = point.y.floor().clamp(0.0, height.saturating_sub(1) as f32) as u32;
    let pixel = pixmap.pixel(x, y)?.demultiply();
    Some(RgbaColor::from_rgba(
        pixel.red(),
        pixel.green(),
        pixel.blue(),
        pixel.alpha(),
    ))
}

fn raster_dimensions(document: &PaintDocument) -> Result<(u32, u32), RenderError> {
    let width = document.canvas_size.width;
    let height = document.canvas_size.height;

    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(RenderError::InvalidCanvasSize);
    }

    Ok((width.round() as u32, height.round() as u32))
}

fn render_element(
    pixmap: &mut Pixmap,
    element: &PaintElement,
    background: RgbaColor,
    raster_background: RasterBackground,
) {
    match element {
        PaintElement::Stroke(stroke) => {
            render_stroke(pixmap, stroke, background, raster_background)
        }
        PaintElement::Shape(shape) => render_shape(pixmap, shape),
        PaintElement::Fill(fill) => render_fill(pixmap, fill),
        PaintElement::Group(group) => render_group(pixmap, group, background, raster_background),
    }
}

fn render_group(
    pixmap: &mut Pixmap,
    group: &GroupElement,
    background: RgbaColor,
    raster_background: RasterBackground,
) {
    for element in &group.elements {
        render_element(pixmap, element, background, raster_background);
    }
}

fn append_svg_layer(svg: &mut String, layer: &PaintLayer, skipped_eraser: &mut bool) {
    writeln!(svg, r#"<g data-layer="{}">"#, escape_svg_attr(&layer.name))
        .expect("write into String should succeed");
    for element in &layer.elements {
        append_svg_element(svg, element, skipped_eraser);
    }
    svg.push_str("</g>\n");
}

fn append_svg_element(svg: &mut String, element: &PaintElement, skipped_eraser: &mut bool) {
    match element {
        PaintElement::Stroke(stroke) => append_svg_stroke(svg, stroke, skipped_eraser),
        PaintElement::Shape(shape) => append_svg_shape(svg, shape),
        PaintElement::Fill(fill) => append_svg_fill(svg, fill),
        PaintElement::Group(group) => append_svg_group(svg, group, skipped_eraser),
    }
}

fn append_svg_group(svg: &mut String, group: &GroupElement, skipped_eraser: &mut bool) {
    svg.push_str("<g>\n");
    for element in &group.elements {
        append_svg_element(svg, element, skipped_eraser);
    }
    svg.push_str("</g>\n");
}

fn append_svg_stroke(svg: &mut String, stroke: &Stroke, skipped_eraser: &mut bool) {
    if matches!(stroke.tool, ToolKind::Eraser) {
        *skipped_eraser = true;
        return;
    }

    let passes = svg_stroke_export_passes(stroke);
    let points = simplify_stroke_points_for_svg(&stroke.points, stroke.effective_width());
    match points.as_slice() {
        [] => {}
        [point] => {
            for pass in passes {
                writeln!(
                    svg,
                    r#"<circle cx="{}" cy="{}" r="{}" fill="{}" fill-opacity="{}" />"#,
                    svg_scalar(point.x),
                    svg_scalar(point.y),
                    svg_scalar((pass.width * 0.5).max(0.5)),
                    svg_rgb(pass.color),
                    svg_opacity(pass.color),
                )
                .expect("write into String should succeed");
            }
        }
        [..] => {
            let data = stroke_points_to_svg_path_data(&points);
            for pass in passes {
                writeln!(
                    svg,
                    r#"<path d="{}" fill="none" stroke="{}" stroke-opacity="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round" />"#,
                    data,
                    svg_rgb(pass.color),
                    svg_opacity(pass.color),
                    svg_scalar(pass.width),
                )
                .expect("write into String should succeed");
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SvgStrokePass {
    color: RgbaColor,
    width: f32,
}

fn svg_stroke_export_passes(stroke: &Stroke) -> Vec<SvgStrokePass> {
    let base_color = stroke.tool.styled_color(stroke.color);
    let base_width = stroke.effective_width();

    match stroke.tool {
        ToolKind::Brush => vec![SvgStrokePass {
            color: base_color,
            width: base_width,
        }],
        ToolKind::Pencil => vec![
            SvgStrokePass {
                color: base_color.with_alpha_scaled(0.88),
                width: (base_width * 0.88).max(0.8),
            },
            SvgStrokePass {
                color: base_color.with_alpha_scaled(0.28),
                width: (base_width * 0.46).max(0.7),
            },
            SvgStrokePass {
                color: base_color.with_alpha_scaled(0.16),
                width: (base_width * 0.24).max(0.6),
            },
        ],
        ToolKind::Crayon => vec![
            SvgStrokePass {
                color: base_color.with_alpha_scaled(0.86),
                width: (base_width * 1.12).max(0.95),
            },
            SvgStrokePass {
                color: base_color.with_alpha_scaled(0.34),
                width: (base_width * 0.82).max(0.85),
            },
            SvgStrokePass {
                color: base_color.with_alpha_scaled(0.2),
                width: (base_width * 0.58).max(0.75),
            },
        ],
        ToolKind::Marker => vec![
            SvgStrokePass {
                color: base_color.with_alpha_scaled(0.58),
                width: (base_width * 1.26).max(1.0),
            },
            SvgStrokePass {
                color: base_color.with_alpha_scaled(0.46),
                width: (base_width * 1.08).max(0.9),
            },
            SvgStrokePass {
                color: base_color.with_alpha_scaled(0.82),
                width: (base_width * 0.9).max(0.85),
            },
        ],
        ToolKind::Eraser => Vec::new(),
    }
}

fn simplify_stroke_points_for_svg(points: &[PaintPoint], width: f32) -> Vec<PaintPoint> {
    let Some((&first, rest)) = points.split_first() else {
        return Vec::new();
    };
    if points.len() <= 2 {
        return points.to_vec();
    }

    let min_step = (width * 0.18).clamp(0.75, 3.25);
    let mut sampled = Vec::with_capacity(points.len());
    sampled.push(first);

    for &point in rest.iter().take(rest.len().saturating_sub(1)) {
        if point.distance_to(*sampled.last().expect("first point exists")) >= min_step {
            sampled.push(point);
        }
    }

    let last = *points.last().expect("split_first guarantees one point");
    if sampled.last().copied() != Some(last) {
        sampled.push(last);
    }

    if sampled.len() <= 2 {
        return sampled;
    }

    let deviation = (width * 0.08).clamp(0.18, 1.2);
    let mut simplified = Vec::with_capacity(sampled.len());
    simplified.push(sampled[0]);

    for index in 1..sampled.len() - 1 {
        let previous = *simplified
            .last()
            .expect("simplified contains the first sampled point");
        let current = sampled[index];
        let next = sampled[index + 1];
        if point_line_distance(current, previous, next) > deviation {
            simplified.push(current);
        }
    }

    if simplified.last().copied() != Some(last) {
        simplified.push(last);
    }
    simplified
}

fn stroke_points_to_svg_path_data(points: &[PaintPoint]) -> String {
    let Some((first, _)) = points.split_first() else {
        return String::new();
    };
    if points.len() == 2 {
        return points_to_path_data(points, false);
    }

    let mut data = format!("M {} {}", svg_scalar(first.x), svg_scalar(first.y));
    for index in 0..points.len() - 1 {
        let previous = if index == 0 {
            points[index]
        } else {
            points[index - 1]
        };
        let start = points[index];
        let end = points[index + 1];
        let next = points.get(index + 2).copied().unwrap_or(end);
        let control_a = cubic_control_point(previous, start, end);
        let control_b = cubic_control_point(start, end, next).mirrored_around(end);
        write!(
            data,
            " C {} {} {} {} {} {}",
            svg_scalar(control_a.x),
            svg_scalar(control_a.y),
            svg_scalar(control_b.x),
            svg_scalar(control_b.y),
            svg_scalar(end.x),
            svg_scalar(end.y),
        )
        .expect("write into String should succeed");
    }
    data
}

fn cubic_control_point(previous: PaintPoint, current: PaintPoint, next: PaintPoint) -> PaintPoint {
    let smoothing = 1.0 / 6.0;
    PaintPoint::new(
        current.x + (next.x - previous.x) * smoothing,
        current.y + (next.y - previous.y) * smoothing,
    )
}

fn point_line_distance(point: PaintPoint, line_start: PaintPoint, line_end: PaintPoint) -> f32 {
    let dx = line_end.x - line_start.x;
    let dy = line_end.y - line_start.y;
    let length_sq = dx * dx + dy * dy;
    if length_sq <= f32::EPSILON {
        return point.distance_to(line_start);
    }

    let projection = ((point.x - line_start.x) * dx + (point.y - line_start.y) * dy) / length_sq;
    let clamped = projection.clamp(0.0, 1.0);
    let projected = PaintPoint::new(line_start.x + dx * clamped, line_start.y + dy * clamped);
    point.distance_to(projected)
}

trait MirroredAround {
    fn mirrored_around(self, center: PaintPoint) -> PaintPoint;
}

impl MirroredAround for PaintPoint {
    fn mirrored_around(self, center: PaintPoint) -> PaintPoint {
        PaintPoint::new(center.x * 2.0 - self.x, center.y * 2.0 - self.y)
    }
}

fn append_svg_shape(svg: &mut String, shape: &ShapeElement) {
    let style = shape_render_style(shape);
    match shape.kind {
        ShapeKind::Line => {
            writeln!(
                svg,
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" fill="none" stroke="{}" stroke-opacity="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round" />"#,
                svg_scalar(shape.start.x),
                svg_scalar(shape.start.y),
                svg_scalar(shape.end.x),
                svg_scalar(shape.end.y),
                svg_rgb(style.stroke_color),
                svg_opacity(style.stroke_color),
                svg_scalar(style.stroke_width),
            )
            .expect("write into String should succeed");
        }
        ShapeKind::Rectangle => {
            append_svg_closed_shape(
                svg,
                &points_to_path_data(&shape.rotated_box_corners(), true),
                style,
            );
        }
        ShapeKind::Ellipse => {
            let points = ellipse_outline_points(shape);
            if !points.is_empty() {
                append_svg_closed_shape(svg, &points_to_path_data(&points, true), style);
            }
        }
    }
}

fn append_svg_closed_shape(svg: &mut String, d: &str, style: ShapeRenderStyle) {
    let fill = style.fill_color.unwrap_or(RgbaColor::from_rgba(0, 0, 0, 0));
    writeln!(
        svg,
        r#"<path d="{}" fill="{}" fill-opacity="{}" stroke="{}" stroke-opacity="{}" stroke-width="{}" stroke-linejoin="round" />"#,
        d,
        if style.fill_color.is_some() {
            svg_rgb(fill)
        } else {
            "none".to_owned()
        },
        if style.fill_color.is_some() {
            svg_opacity(fill)
        } else {
            "0".to_owned()
        },
        svg_rgb(style.stroke_color),
        svg_opacity(style.stroke_color),
        svg_scalar(style.stroke_width),
    )
    .expect("write into String should succeed");
}

fn append_svg_fill(svg: &mut String, fill: &FillElement) {
    if fill.spans.is_empty() {
        return;
    }

    // バケツ塗り結果は PNG 向けの見た目を優先して内部では scanline spans を持つため、
    // SVG では同じ見た目を保ちやすい矩形 path へまとめて安全側で持ち出します。
    let rects = svg_fill_rects(fill);
    if rects.is_empty() {
        return;
    }
    let data = svg_fill_rects_to_path_data(&rects);
    let fill_opacity = svg_opacity(fill.color);
    let fill_rgb = svg_rgb(fill.color);
    writeln!(
        svg,
        r#"<path data-fill="bucket" d="{}" fill="{}" fill-opacity="{}" stroke="none" shape-rendering="crispEdges" />"#,
        data,
        fill_rgb,
        fill_opacity,
    )
    .expect("write into String should succeed");
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SvgFillRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

fn svg_fill_rects(fill: &FillElement) -> Vec<SvgFillRect> {
    let mut spans = fill.spans.to_vec();
    spans.sort_by_key(|span| (span.y, span.x_start, span.x_end));

    let mut rects: Vec<SvgFillRect> = Vec::with_capacity(spans.len());
    for span in spans {
        let width = span.width() as f32;
        if width <= 0.0 {
            continue;
        }

        let x = fill.origin.x + span.x_start as f32;
        let y = fill.origin.y + span.y as f32;
        if let Some(last) = rects.last_mut()
            && (last.x - x).abs() <= f32::EPSILON
            && (last.width - width).abs() <= f32::EPSILON
            && (last.y + last.height - y).abs() <= f32::EPSILON
        {
            last.height += 1.0;
            continue;
        }

        rects.push(SvgFillRect {
            x,
            y,
            width,
            height: 1.0,
        });
    }

    rects
}

fn svg_fill_rects_to_path_data(rects: &[SvgFillRect]) -> String {
    let mut data = String::new();
    for rect in rects {
        write!(
            data,
            "M {} {} h {} v {} h -{} Z ",
            svg_scalar(rect.x),
            svg_scalar(rect.y),
            svg_scalar(rect.width),
            svg_scalar(rect.height),
            svg_scalar(rect.width),
        )
        .expect("write into String should succeed");
    }
    data.trim_end().to_owned()
}

fn points_to_path_data(points: &[PaintPoint], close: bool) -> String {
    let Some((first, rest)) = points.split_first() else {
        return String::new();
    };

    let mut data = format!("M {} {}", svg_scalar(first.x), svg_scalar(first.y));
    for point in rest {
        write!(data, " L {} {}", svg_scalar(point.x), svg_scalar(point.y))
            .expect("write into String should succeed");
    }
    if close {
        data.push_str(" Z");
    }
    data
}

fn svg_rgb(color: RgbaColor) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

fn svg_opacity(color: RgbaColor) -> String {
    format!("{:.4}", (color.a as f32 / 255.0).clamp(0.0, 1.0))
}

fn svg_scalar(value: f32) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    if rounded.fract().abs() < 0.005 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.2}")
    }
}

fn escape_svg_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn render_stroke(
    pixmap: &mut Pixmap,
    stroke: &Stroke,
    background: RgbaColor,
    raster_background: RasterBackground,
) {
    match stroke.tool {
        ToolKind::Brush | ToolKind::Pencil | ToolKind::Crayon | ToolKind::Marker => {
            for pass in stroke.render_passes() {
                render_stroke_pass(pixmap, &stroke.points, pass.width, pass.color, pass.offset);
            }
        }
        ToolKind::Eraser => {
            let mut paint = Paint {
                anti_alias: true,
                ..Paint::default()
            };
            match raster_background {
                RasterBackground::Opaque => {
                    paint.set_color_rgba8(background.r, background.g, background.b, background.a);
                }
                RasterBackground::Transparent => {
                    paint.blend_mode = tiny_skia::BlendMode::Clear;
                }
            }
            render_stroke_path(
                pixmap,
                &stroke.points,
                stroke.effective_width(),
                PaintVector::default(),
                &paint,
            );
        }
    }
}

fn render_stroke_pass(
    pixmap: &mut Pixmap,
    points: &[PaintPoint],
    width: f32,
    color: RgbaColor,
    offset: PaintVector,
) {
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    paint.set_color_rgba8(color.r, color.g, color.b, color.a);
    render_stroke_path(pixmap, points, width, offset, &paint);
}

fn render_stroke_path(
    pixmap: &mut Pixmap,
    points: &[PaintPoint],
    width: f32,
    offset: PaintVector,
    paint: &Paint,
) {
    match points {
        [] => {}
        [point] => {
            let point = point.offset(offset);
            if let Some(path) = PathBuilder::from_circle(point.x, point.y, width * 0.5) {
                pixmap.fill_path(&path, paint, FillRule::Winding, Transform::identity(), None);
            }
        }
        [first, rest @ ..] => {
            let first = first.offset(offset);
            let mut builder = PathBuilder::new();
            builder.move_to(first.x, first.y);
            for point in rest {
                let point = point.offset(offset);
                builder.line_to(point.x, point.y);
            }

            if let Some(path) = builder.finish() {
                pixmap.stroke_path(
                    &path,
                    paint,
                    &stroke_style(width),
                    Transform::identity(),
                    None,
                );
            }
        }
    }
}

fn render_shape(pixmap: &mut Pixmap, shape: &ShapeElement) {
    let Some(path) = shape_vector_path(shape) else {
        return;
    };
    let style = shape_render_style(shape);

    let mut stroke_paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    stroke_paint.set_color_rgba8(
        style.stroke_color.r,
        style.stroke_color.g,
        style.stroke_color.b,
        style.stroke_color.a,
    );

    if let Some(fill_color) = style.fill_color {
        let mut fill_paint = Paint {
            anti_alias: true,
            ..Paint::default()
        };
        fill_paint.set_color_rgba8(fill_color.r, fill_color.g, fill_color.b, fill_color.a);
        pixmap.fill_path(
            &path,
            &fill_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    pixmap.stroke_path(
        &path,
        &stroke_paint,
        &stroke_style(style.stroke_width),
        Transform::identity(),
        None,
    );
}

fn render_fill(pixmap: &mut Pixmap, fill: &FillElement) {
    if fill.spans.is_empty() {
        return;
    }

    let mut paint = Paint {
        anti_alias: false,
        ..Paint::default()
    };
    paint.set_color_rgba8(fill.color.r, fill.color.g, fill.color.b, fill.color.a);

    for span in &fill.spans {
        let width = span.width() as f32;
        if width <= 0.0 {
            continue;
        }
        let x = fill.origin.x + span.x_start as f32;
        let y = fill.origin.y + span.y as f32;
        let Some(rect) = TinyRect::from_xywh(x, y, width, 1.0) else {
            continue;
        };
        let path = PathBuilder::from_rect(rect);
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

#[derive(Debug, Clone, Copy)]
struct ShapeRenderStyle {
    stroke_color: RgbaColor,
    fill_color: Option<RgbaColor>,
    stroke_width: f32,
}

fn shape_render_style(shape: &ShapeElement) -> ShapeRenderStyle {
    ShapeRenderStyle {
        stroke_color: shape.color,
        fill_color: shape.effective_fill_color(),
        stroke_width: shape.width,
    }
}

// Keep vector-friendly geometry separate from raster-only export details so future SVG
// export and fill tools can reuse the same path construction without coupling to PNG.
fn shape_vector_path(shape: &ShapeElement) -> Option<tiny_skia::Path> {
    match shape.kind {
        ShapeKind::Line => line_path(shape.start, shape.end),
        ShapeKind::Rectangle => polygon_path(&shape.rotated_box_corners()),
        ShapeKind::Ellipse => ellipse_path(shape),
    }
}

fn line_path(start: PaintPoint, end: PaintPoint) -> Option<tiny_skia::Path> {
    let mut builder = PathBuilder::new();
    builder.move_to(start.x, start.y);
    builder.line_to(end.x, end.y);
    builder.finish()
}

fn polygon_path(points: &[PaintPoint]) -> Option<tiny_skia::Path> {
    let (first, rest) = points.split_first()?;
    let mut builder = PathBuilder::new();
    builder.move_to(first.x, first.y);
    for point in rest {
        builder.line_to(point.x, point.y);
    }
    builder.close();
    builder.finish()
}

fn ellipse_path(shape: &ShapeElement) -> Option<tiny_skia::Path> {
    let points = ellipse_outline_points(shape);
    polygon_path(&points)
}

fn ellipse_outline_points(shape: &ShapeElement) -> Vec<PaintPoint> {
    let center = shape.center();
    let half = shape.half_extents();
    if half.dx <= f32::EPSILON || half.dy <= f32::EPSILON {
        return Vec::new();
    }

    (0..96)
        .map(|step| {
            let t = step as f32 / 96.0 * std::f32::consts::TAU;
            let local = PaintVector::new(half.dx * t.cos(), half.dy * t.sin());
            center.offset(rotate_vector(local, shape.rotation_radians))
        })
        .collect()
}

fn stroke_style(width: f32) -> TinyStroke {
    TinyStroke {
        width: width.max(1.0),
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..TinyStroke::default()
    }
}

fn color_from_rgba(color: RgbaColor) -> Color {
    Color::from_rgba8(color.r, color.g, color.b, color.a)
}

fn rotate_vector(vector: PaintVector, angle_radians: f32) -> PaintVector {
    let cos = angle_radians.cos();
    let sin = angle_radians.sin();
    PaintVector::new(
        vector.dx * cos - vector.dy * sin,
        vector.dx * sin + vector.dy * cos,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        RasterBackground, render_document_pixmap, render_document_pixmap_with_background,
        render_document_png, render_document_svg, sample_document_color,
        simplify_stroke_points_for_svg, svg_fill_rects, svg_fill_rects_to_path_data,
        svg_stroke_export_passes,
    };
    use crate::model::{
        CanvasSize, FillElement, FillSpan, GroupElement, GuideAxis, PaintDocument, PaintElement,
        PaintPoint, RgbaColor, ShapeElement, ShapeKind, Stroke, ToolKind,
    };
    use tiny_skia::Pixmap;

    fn sample_document() -> PaintDocument {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(64.0, 64.0),
            background: RgbaColor::white(),
            ..PaintDocument::default()
        };

        let mut stroke = Stroke::new(ToolKind::Brush, RgbaColor::charcoal(), 6.0);
        stroke.push_point(PaintPoint::new(8.0, 8.0));
        stroke.push_point(PaintPoint::new(28.0, 8.0));
        document.push_stroke(stroke);
        document.push_shape(
            ShapeElement::with_rotation(
                ShapeKind::Rectangle,
                RgbaColor::new(220, 64, 64, 255),
                4.0,
                PaintPoint::new(24.0, 24.0),
                PaintPoint::new(44.0, 40.0),
                std::f32::consts::FRAC_PI_4,
            )
            .with_fill_color(Some(RgbaColor::new(255, 196, 64, 180))),
        );
        document
    }

    #[test]
    fn render_pixmap_contains_background_and_elements() {
        let pixmap = render_document_pixmap(&sample_document()).expect("document should render");

        let background = pixmap
            .pixel(2, 60)
            .expect("background pixel should exist")
            .demultiply();
        let stroke = pixmap
            .pixel(16, 8)
            .expect("stroke pixel should exist")
            .demultiply();
        let shape = pixmap
            .pixel(34, 24)
            .expect("shape pixel should exist")
            .demultiply();

        assert_eq!(
            (background.red(), background.green(), background.blue()),
            (255, 255, 255)
        );
        assert_ne!(
            (stroke.red(), stroke.green(), stroke.blue()),
            (255, 255, 255)
        );
        assert_ne!((shape.red(), shape.green(), shape.blue()), (255, 255, 255));
    }

    #[test]
    fn render_png_can_be_decoded() {
        let png = render_document_png(&sample_document()).expect("png export should succeed");
        let decoded = Pixmap::decode_png(&png).expect("exported png should decode");

        assert_eq!(decoded.width(), 64);
        assert_eq!(decoded.height(), 64);
    }

    #[test]
    fn transparent_render_keeps_background_alpha_zero() {
        let pixmap = render_document_pixmap_with_background(
            &sample_document(),
            RasterBackground::Transparent,
        )
        .expect("transparent render should succeed");
        let background = pixmap.pixel(0, 0).expect("background pixel");

        assert_eq!(background.alpha(), 0);
    }

    #[test]
    fn render_respects_element_stack_order() {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(48.0, 48.0),
            background: RgbaColor::white(),
            ..PaintDocument::default()
        };

        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::new(220, 64, 64, 255),
            10.0,
            PaintPoint::new(8.0, 8.0),
            PaintPoint::new(40.0, 40.0),
        ));
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::new(64, 96, 220, 255),
            10.0,
            PaintPoint::new(8.0, 8.0),
            PaintPoint::new(40.0, 40.0),
        ));

        let pixmap = render_document_pixmap(&document).expect("document should render");
        let pixel = pixmap
            .pixel(8, 24)
            .expect("overlapping pixel should exist")
            .demultiply();

        assert_eq!((pixel.red(), pixel.green(), pixel.blue()), (64, 96, 220));
    }

    #[test]
    fn render_group_elements_into_png() {
        let document = PaintDocument::from_flat_elements(
            CanvasSize::new(64.0, 64.0),
            RgbaColor::white(),
            vec![PaintElement::Group(GroupElement {
                elements: vec![
                    PaintElement::Shape(ShapeElement::new(
                        ShapeKind::Rectangle,
                        RgbaColor::new(220, 64, 64, 255),
                        5.0,
                        PaintPoint::new(10.0, 10.0),
                        PaintPoint::new(28.0, 28.0),
                    )),
                    PaintElement::Shape(ShapeElement::new(
                        ShapeKind::Line,
                        RgbaColor::new(32, 80, 220, 255),
                        4.0,
                        PaintPoint::new(8.0, 40.0),
                        PaintPoint::new(40.0, 52.0),
                    )),
                ],
            })],
        );

        let png = render_document_png(&document).expect("grouped document should render");
        let decoded = Pixmap::decode_png(&png).expect("group png should decode");
        let colored_pixels = (0..decoded.width()).any(|x| {
            (0..decoded.height()).any(|y| {
                let pixel = decoded
                    .pixel(x, y)
                    .expect("pixel should exist")
                    .demultiply();
                (pixel.red(), pixel.green(), pixel.blue()) != (255, 255, 255)
            })
        });

        assert!(colored_pixels, "group rendering should contribute pixels");
    }

    #[test]
    fn hidden_layers_do_not_render() {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(32.0, 32.0),
            background: RgbaColor::white(),
            ..PaintDocument::default()
        };
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::new(220, 64, 64, 255),
            6.0,
            PaintPoint::new(4.0, 4.0),
            PaintPoint::new(28.0, 28.0),
        ));
        let (mut layered, hidden_id) = document.add_layer_document();
        layered.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::new(64, 96, 220, 255),
            6.0,
            PaintPoint::new(4.0, 4.0),
            PaintPoint::new(28.0, 28.0),
        ));
        layered = layered
            .toggled_layer_visibility_document(hidden_id)
            .expect("hide layer");

        let pixmap = render_document_pixmap(&layered).expect("document should render");
        let pixel = pixmap
            .pixel(4, 16)
            .expect("pixel should exist")
            .demultiply();

        assert_eq!((pixel.red(), pixel.green(), pixel.blue()), (220, 64, 64));
    }

    #[test]
    fn grid_and_guides_do_not_render_into_png() {
        let document = PaintDocument::default()
            .add_guide_document(GuideAxis::Horizontal, 24.0)
            .expect("add guide")
            .toggled_rulers_visibility_document()
            .expect("toggle rulers")
            .toggled_smart_guides_visibility_document()
            .expect("toggle smart guides");
        let pixmap = render_document_pixmap(&document).expect("document should render");

        let has_non_background = (0..pixmap.width()).any(|x| {
            (0..pixmap.height()).any(|y| {
                let pixel = pixmap.pixel(x, y).expect("pixel should exist").demultiply();
                (pixel.red(), pixel.green(), pixel.blue()) != (255, 255, 255)
            })
        });

        assert!(
            !has_non_background,
            "grid and guides should stay out of PNG output"
        );
    }

    #[test]
    fn filled_shape_renders_its_fill_color_inside_bounds() {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(64.0, 64.0),
            background: RgbaColor::white(),
            ..PaintDocument::default()
        };
        document.push_shape(
            ShapeElement::new(
                ShapeKind::Rectangle,
                RgbaColor::new(220, 64, 64, 255),
                3.0,
                PaintPoint::new(16.0, 16.0),
                PaintPoint::new(48.0, 48.0),
            )
            .with_fill_color(Some(RgbaColor::new(32, 160, 220, 255))),
        );

        let pixmap = render_document_pixmap(&document).expect("document should render");
        let pixel = pixmap.pixel(32, 32).expect("filled pixel").demultiply();
        assert_eq!((pixel.red(), pixel.green(), pixel.blue()), (32, 160, 220));
    }

    #[test]
    fn sampled_document_color_reads_composited_fill() {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(64.0, 64.0),
            background: RgbaColor::white(),
            ..PaintDocument::default()
        };
        document.push_shape(
            ShapeElement::new(
                ShapeKind::Rectangle,
                RgbaColor::new(220, 64, 64, 255),
                2.0,
                PaintPoint::new(8.0, 8.0),
                PaintPoint::new(56.0, 56.0),
            )
            .with_fill_color(Some(RgbaColor::new(90, 180, 120, 255))),
        );

        let sampled =
            sample_document_color(&document, PaintPoint::new(24.0, 24.0)).expect("sampled color");
        assert_eq!(sampled, RgbaColor::new(90, 180, 120, 255));
    }

    #[test]
    fn marker_stroke_keeps_partial_alpha_on_transparent_export() {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(32.0, 32.0),
            background: RgbaColor::white(),
            ..PaintDocument::default()
        };
        let mut stroke = Stroke::new(ToolKind::Marker, RgbaColor::new(24, 80, 200, 255), 8.0);
        stroke.push_point(PaintPoint::new(4.0, 16.0));
        stroke.push_point(PaintPoint::new(28.0, 16.0));
        document.push_stroke(stroke);

        let pixmap =
            render_document_pixmap_with_background(&document, RasterBackground::Transparent)
                .expect("transparent render should succeed");
        let pixel = pixmap.pixel(16, 16).expect("marker pixel");

        assert!(pixel.alpha() > 0);
        assert!(pixel.alpha() < 255);
    }

    #[test]
    fn svg_export_contains_shape_fill_and_line_geometry() {
        let mut document = sample_document();
        document.push_shape(ShapeElement::new(
            ShapeKind::Line,
            RgbaColor::new(32, 80, 220, 255),
            4.0,
            PaintPoint::new(6.0, 48.0),
            PaintPoint::new(58.0, 58.0),
        ));

        let svg = String::from_utf8(render_document_svg(&document).expect("svg export"))
            .expect("svg should be utf-8");

        assert!(svg.contains(r#"<line x1="6" y1="48" x2="58" y2="58""#));
        assert!(svg.contains("fill=\"#ffc440\""));
        assert!(svg.contains("stroke=\"#dc4040\""));
    }

    #[test]
    fn svg_export_serializes_bucket_fill_as_compact_path() {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(32.0, 32.0),
            background: RgbaColor::white(),
            ..PaintDocument::default()
        };
        document.push_fill(FillElement::new(
            RgbaColor::from_rgba(48, 140, 220, 180),
            PaintPoint::new(6.0, 7.0),
            vec![
                FillSpan {
                    y: 0,
                    x_start: 0,
                    x_end: 4,
                },
                FillSpan {
                    y: 1,
                    x_start: 1,
                    x_end: 5,
                },
            ],
        ));

        let svg = String::from_utf8(render_document_svg(&document).expect("svg export"))
            .expect("svg should be utf-8");

        assert!(svg.contains(r#"data-fill="bucket""#));
        assert!(svg.contains(r#"<path data-fill="bucket" d="M 6 7 h 4 v 1 h -4 Z"#));
        assert!(svg.contains(r#"fill-opacity="0.7059""#));
    }

    #[test]
    fn svg_export_merges_bucket_fill_runs_with_same_width() {
        let fill = FillElement::new(
            RgbaColor::from_rgba(48, 140, 220, 180),
            PaintPoint::new(6.0, 7.0),
            vec![
                FillSpan {
                    y: 0,
                    x_start: 0,
                    x_end: 4,
                },
                FillSpan {
                    y: 1,
                    x_start: 0,
                    x_end: 4,
                },
                FillSpan {
                    y: 2,
                    x_start: 2,
                    x_end: 5,
                },
            ],
        );

        let rects = svg_fill_rects(&fill);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].height, 2.0);

        let data = svg_fill_rects_to_path_data(&rects);
        assert!(data.contains("M 6 7 h 4 v 2 h -4 Z"));
        assert!(data.contains("M 8 9 h 3 v 1 h -3 Z"));
    }

    #[test]
    fn svg_export_skips_hidden_layers() {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(32.0, 32.0),
            background: RgbaColor::white(),
            ..PaintDocument::default()
        };
        document.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::new(220, 64, 64, 255),
            4.0,
            PaintPoint::new(4.0, 4.0),
            PaintPoint::new(28.0, 28.0),
        ));
        let (mut layered, hidden_id) = document.add_layer_document();
        layered.push_shape(ShapeElement::new(
            ShapeKind::Rectangle,
            RgbaColor::new(64, 96, 220, 255),
            4.0,
            PaintPoint::new(6.0, 6.0),
            PaintPoint::new(26.0, 26.0),
        ));
        layered = layered
            .toggled_layer_visibility_document(hidden_id)
            .expect("hide layer");

        let svg = String::from_utf8(render_document_svg(&layered).expect("svg export"))
            .expect("svg should be utf-8");

        assert!(svg.contains("stroke=\"#dc4040\""));
        assert!(!svg.contains("stroke=\"#4060dc\""));
    }

    #[test]
    fn svg_export_smooths_freehand_stroke_into_cubic_path() {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(64.0, 64.0),
            background: RgbaColor::white(),
            ..PaintDocument::default()
        };
        let mut stroke = Stroke::new(ToolKind::Brush, RgbaColor::new(32, 80, 220, 255), 6.0);
        stroke.push_point(PaintPoint::new(6.0, 18.0));
        stroke.push_point(PaintPoint::new(16.0, 24.0));
        stroke.push_point(PaintPoint::new(28.0, 16.0));
        stroke.push_point(PaintPoint::new(40.0, 28.0));
        stroke.push_point(PaintPoint::new(56.0, 20.0));
        document.push_stroke(stroke);

        let svg = String::from_utf8(render_document_svg(&document).expect("svg export"))
            .expect("svg should be utf-8");

        assert!(svg.contains(r#"<path d="M 6 18 C "#));
        assert!(svg.contains(" C "));
    }

    #[test]
    fn svg_export_simplification_removes_collinear_middle_points() {
        let points = vec![
            PaintPoint::new(0.0, 0.0),
            PaintPoint::new(1.0, 0.0),
            PaintPoint::new(2.0, 0.0),
            PaintPoint::new(3.0, 0.0),
            PaintPoint::new(10.0, 0.0),
        ];

        let simplified = simplify_stroke_points_for_svg(&points, 6.0);

        assert_eq!(simplified.first().copied(), Some(PaintPoint::new(0.0, 0.0)));
        assert_eq!(simplified.last().copied(), Some(PaintPoint::new(10.0, 0.0)));
        assert!(simplified.len() < points.len());
    }

    #[test]
    fn svg_export_reflects_brush_kinds_with_light_pass_differences() {
        let pencil = svg_stroke_export_passes(&Stroke::new(
            ToolKind::Pencil,
            RgbaColor::new(40, 60, 80, 255),
            10.0,
        ));
        let crayon = svg_stroke_export_passes(&Stroke::new(
            ToolKind::Crayon,
            RgbaColor::new(40, 60, 80, 255),
            10.0,
        ));
        let marker = svg_stroke_export_passes(&Stroke::new(
            ToolKind::Marker,
            RgbaColor::new(40, 60, 80, 255),
            10.0,
        ));

        assert_eq!(pencil.len(), 3);
        assert_eq!(crayon.len(), 3);
        assert_eq!(marker.len(), 3);
        assert!(pencil[0].width < crayon[0].width);
        assert!(crayon[0].width < marker[0].width);
        assert!(marker[0].color.a < crayon[0].color.a);
        assert!(pencil[0].width < marker[0].width);
        assert!(marker[0].color.a < pencil[0].color.a);
        assert!(crayon[1].color.a > pencil[1].color.a);
        assert!(marker[1].width > crayon[1].width);
        assert!(marker[0].width > marker[2].width);
        assert!(pencil[2].color.a < pencil[0].color.a);
        assert!(crayon[2].color.a < crayon[0].color.a);
    }

    #[test]
    fn svg_export_omits_eraser_strokes_with_comment() {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(32.0, 32.0),
            background: RgbaColor::white(),
            ..PaintDocument::default()
        };
        let mut stroke = Stroke::new(ToolKind::Eraser, RgbaColor::white(), 8.0);
        stroke.push_point(PaintPoint::new(4.0, 16.0));
        stroke.push_point(PaintPoint::new(28.0, 16.0));
        document.push_stroke(stroke);

        let svg = String::from_utf8(render_document_svg(&document).expect("svg export"))
            .expect("svg should be utf-8");

        assert!(svg.contains("消しゴムストロークは SVG では簡略化せず省略"));
        assert!(!svg.contains("<path"));
    }
}
