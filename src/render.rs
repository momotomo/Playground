use std::error::Error;
use std::fmt::{self, Display, Formatter};

use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke as TinyStroke, Transform};

use crate::model::{
    GroupElement, PaintDocument, PaintElement, PaintPoint, PaintVector, RgbaColor, ShapeElement,
    ShapeKind, Stroke, ToolKind,
};

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

pub fn render_document_pixmap(document: &PaintDocument) -> Result<Pixmap, RenderError> {
    let (width, height) = raster_dimensions(document)?;
    let mut pixmap = Pixmap::new(width, height).ok_or(RenderError::InvalidCanvasSize)?;
    pixmap.fill(color_from_rgba(document.background));

    for element in &document.elements {
        render_element(&mut pixmap, element, document.background);
    }

    Ok(pixmap)
}

pub fn render_document_png(document: &PaintDocument) -> Result<Vec<u8>, RenderError> {
    let pixmap = render_document_pixmap(document)?;
    pixmap
        .encode_png()
        .map_err(|error| RenderError::PngEncode(error.to_string()))
}

fn raster_dimensions(document: &PaintDocument) -> Result<(u32, u32), RenderError> {
    let width = document.canvas_size.width;
    let height = document.canvas_size.height;

    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(RenderError::InvalidCanvasSize);
    }

    Ok((width.round() as u32, height.round() as u32))
}

fn render_element(pixmap: &mut Pixmap, element: &PaintElement, background: RgbaColor) {
    match element {
        PaintElement::Stroke(stroke) => render_stroke(pixmap, stroke, background),
        PaintElement::Shape(shape) => render_shape(pixmap, shape),
        PaintElement::Group(group) => render_group(pixmap, group, background),
    }
}

fn render_group(pixmap: &mut Pixmap, group: &GroupElement, background: RgbaColor) {
    for element in &group.elements {
        render_element(pixmap, element, background);
    }
}

fn render_stroke(pixmap: &mut Pixmap, stroke: &Stroke, background: RgbaColor) {
    let color = match stroke.tool {
        ToolKind::Brush => stroke.color,
        ToolKind::Eraser => background,
    };

    let mut paint = Paint::default();
    paint.set_color_rgba8(color.r, color.g, color.b, color.a);
    paint.anti_alias = true;

    match stroke.points.as_slice() {
        [] => {}
        [point] => {
            if let Some(path) = PathBuilder::from_circle(point.x, point.y, stroke.width * 0.5) {
                pixmap.fill_path(
                    &path,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
        [first, rest @ ..] => {
            let mut builder = PathBuilder::new();
            builder.move_to(first.x, first.y);
            for point in rest {
                builder.line_to(point.x, point.y);
            }

            if let Some(path) = builder.finish() {
                pixmap.stroke_path(
                    &path,
                    &paint,
                    &stroke_style(stroke.width),
                    Transform::identity(),
                    None,
                );
            }
        }
    }
}

fn render_shape(pixmap: &mut Pixmap, shape: &ShapeElement) {
    let mut paint = Paint::default();
    paint.set_color_rgba8(shape.color.r, shape.color.g, shape.color.b, shape.color.a);
    paint.anti_alias = true;

    let Some(path) = (match shape.kind {
        ShapeKind::Line => line_path(shape.start, shape.end),
        ShapeKind::Rectangle => polygon_path(&shape.rotated_box_corners()),
        ShapeKind::Ellipse => ellipse_path(shape),
    }) else {
        return;
    };

    pixmap.stroke_path(
        &path,
        &paint,
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
    use super::{render_document_pixmap, render_document_png};
    use crate::model::{
        CanvasSize, GroupElement, PaintDocument, PaintElement, PaintPoint, RgbaColor, ShapeElement,
        ShapeKind, Stroke, ToolKind,
    };
    use tiny_skia::Pixmap;

    fn sample_document() -> PaintDocument {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(64.0, 64.0),
            background: RgbaColor::white(),
            elements: Vec::new(),
        };

        let mut stroke = Stroke::new(ToolKind::Brush, RgbaColor::charcoal(), 6.0);
        stroke.push_point(PaintPoint::new(8.0, 8.0));
        stroke.push_point(PaintPoint::new(28.0, 8.0));
        document.push_stroke(stroke);
        document.push_shape(ShapeElement::with_rotation(
            ShapeKind::Rectangle,
            RgbaColor::new(220, 64, 64, 255),
            4.0,
            PaintPoint::new(24.0, 24.0),
            PaintPoint::new(44.0, 40.0),
            std::f32::consts::FRAC_PI_4,
        ));
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
    fn render_respects_element_stack_order() {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(48.0, 48.0),
            background: RgbaColor::white(),
            elements: Vec::new(),
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
        let document = PaintDocument {
            canvas_size: CanvasSize::new(64.0, 64.0),
            background: RgbaColor::white(),
            elements: vec![PaintElement::Group(GroupElement {
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
        };

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
}
