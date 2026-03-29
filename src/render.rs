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

    let color = stroke.tool.styled_color(stroke.color);
    let width = stroke.effective_width();
    match stroke.points.as_slice() {
        [] => {}
        [point] => {
            writeln!(
                svg,
                r#"<circle cx="{}" cy="{}" r="{}" fill="{}" fill-opacity="{}" />"#,
                svg_scalar(point.x),
                svg_scalar(point.y),
                svg_scalar((width * 0.5).max(0.5)),
                svg_rgb(color),
                svg_opacity(color),
            )
            .expect("write into String should succeed");
        }
        [first, rest @ ..] => {
            let mut data = format!("M {} {}", svg_scalar(first.x), svg_scalar(first.y));
            for point in rest {
                write!(data, " L {} {}", svg_scalar(point.x), svg_scalar(point.y))
                    .expect("write into String should succeed");
            }
            writeln!(
                svg,
                r#"<path d="{}" fill="none" stroke="{}" stroke-opacity="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round" />"#,
                data,
                svg_rgb(color),
                svg_opacity(color),
                svg_scalar(width),
            )
            .expect("write into String should succeed");
        }
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
    // SVG では 1px 高の矩形列へ素直に落として安全側で持ち出します。
    let fill_opacity = svg_opacity(fill.color);
    let fill_rgb = svg_rgb(fill.color);
    svg.push_str("<g data-fill=\"bucket\">\n");
    for span in &fill.spans {
        let width = span.width();
        if width == 0 {
            continue;
        }
        writeln!(
            svg,
            r#"<rect x="{}" y="{}" width="{}" height="1" fill="{}" fill-opacity="{}" stroke="none" />"#,
            svg_scalar(fill.origin.x + span.x_start as f32),
            svg_scalar(fill.origin.y + span.y as f32),
            svg_scalar(width as f32),
            fill_rgb,
            fill_opacity,
        )
        .expect("write into String should succeed");
    }
    svg.push_str("</g>\n");
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
        ToolKind::Brush | ToolKind::Pencil | ToolKind::Marker => {
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
    fn svg_export_serializes_bucket_fill_as_scanline_rects() {
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
        assert!(svg.contains(r#"<rect x="6" y="7" width="4" height="1""#));
        assert!(svg.contains(r#"fill-opacity="0.7059""#));
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
