use std::error::Error;
use std::fmt::{self, Display, Formatter};

use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke as TinyStroke, Transform};

use crate::model::{
    GroupElement, PaintDocument, PaintElement, PaintPoint, PaintVector, RgbaColor, ShapeElement,
    ShapeKind, Stroke, ToolKind,
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
    let mut stroke_paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    stroke_paint.set_color_rgba8(shape.color.r, shape.color.g, shape.color.b, shape.color.a);

    let Some(path) = (match shape.kind {
        ShapeKind::Line => line_path(shape.start, shape.end),
        ShapeKind::Rectangle => polygon_path(&shape.rotated_box_corners()),
        ShapeKind::Ellipse => ellipse_path(shape),
    }) else {
        return;
    };

    if shape.kind.supports_fill()
        && let Some(fill_color) = shape.fill_color
    {
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
        &stroke_style(shape.width),
        Transform::identity(),
        None,
    );
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
    let center = shape.center();
    let half = shape.half_extents();
    if half.dx <= f32::EPSILON || half.dy <= f32::EPSILON {
        return None;
    }

    let points: Vec<PaintPoint> = (0..96)
        .map(|step| {
            let t = step as f32 / 96.0 * std::f32::consts::TAU;
            let local = PaintVector::new(half.dx * t.cos(), half.dy * t.sin());
            center.offset(rotate_vector(local, shape.rotation_radians))
        })
        .collect();
    polygon_path(&points)
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
        render_document_png, sample_document_color,
    };
    use crate::model::{
        CanvasSize, GroupElement, GuideAxis, PaintDocument, PaintElement, PaintPoint, RgbaColor,
        ShapeElement, ShapeKind, Stroke, ToolKind,
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
}
