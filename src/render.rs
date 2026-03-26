use std::error::Error;
use std::fmt::{self, Display, Formatter};

use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke as TinyStroke, Transform};

use crate::model::{PaintDocument, RgbaColor, Stroke, ToolKind};

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

    for stroke in &document.strokes {
        render_stroke(&mut pixmap, stroke, document.background);
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
                let stroke_style = TinyStroke {
                    width: stroke.width.max(1.0),
                    line_cap: tiny_skia::LineCap::Round,
                    line_join: tiny_skia::LineJoin::Round,
                    ..TinyStroke::default()
                };
                pixmap.stroke_path(&path, &paint, &stroke_style, Transform::identity(), None);
            }
        }
    }
}

fn color_from_rgba(color: RgbaColor) -> Color {
    Color::from_rgba8(color.r, color.g, color.b, color.a)
}

#[cfg(test)]
mod tests {
    use super::{render_document_pixmap, render_document_png};
    use crate::model::{CanvasSize, PaintDocument, PaintPoint, RgbaColor, Stroke, ToolKind};
    use tiny_skia::Pixmap;

    fn sample_document() -> PaintDocument {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(64.0, 32.0),
            background: RgbaColor::white(),
            strokes: Vec::new(),
        };

        let mut stroke = Stroke::new(ToolKind::Brush, RgbaColor::charcoal(), 6.0);
        stroke.push_point(PaintPoint::new(8.0, 8.0));
        stroke.push_point(PaintPoint::new(40.0, 8.0));
        document.push_stroke(stroke);
        document
    }

    #[test]
    fn render_pixmap_contains_background_and_stroke() {
        let pixmap = render_document_pixmap(&sample_document()).expect("document should render");

        let background = pixmap
            .pixel(60, 28)
            .expect("background pixel should exist")
            .demultiply();
        let stroke = pixmap
            .pixel(20, 8)
            .expect("stroke pixel should exist")
            .demultiply();

        assert_eq!(
            (background.red(), background.green(), background.blue()),
            (255, 255, 255)
        );
        assert_ne!(
            (stroke.red(), stroke.green(), stroke.blue()),
            (255, 255, 255)
        );
    }

    #[test]
    fn render_png_can_be_decoded() {
        let png = render_document_png(&sample_document()).expect("png export should succeed");
        let decoded = Pixmap::decode_png(&png).expect("exported png should decode");

        assert_eq!(decoded.width(), 64);
        assert_eq!(decoded.height(), 32);
    }
}
