//! Vector plotting of the visible model-space drawing to a physical SVG page.
//!
//! Geometry uses document units. An explicit scale is paper millimetres per
//! document unit, independent of INSUNITS (for example 0.01 plots millimetre
//! geometry at 1:100). `None` fits and centres the drawing within the margins.
//! Arcs and polyline bulges remain SVG circular arcs. Dimensions and nested
//! blocks are expanded only for this derived output; the document is untouched.
//! Text uses an ordinary sans-serif font and approximate CAD glyph advances,
//! rather than the vendor's fonts, text styles, or a paper-space layout.

use crate::{Document, Geometry, Point, arc_point, bulge_arc, edit::Bounds2d};
use serde::{Deserialize, Serialize};
use std::{fmt::Write, io};

/// Page settings for a standalone, printable SVG. Defaults to landscape A4.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SvgOptions {
    pub page_width_mm: f64,
    pub page_height_mm: f64,
    pub margin_mm: f64,
    /// Paper millimetres per model unit; `None` fits the visible drawing.
    pub scale: Option<f64>,
    /// Print black strokes and text; false uses effective layer colours.
    pub monochrome: bool,
}

/// A native printable sheet. Paper positions are millimetres from top left;
/// model coordinates retain the drawing's units and upward Y axis.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrintLayout {
    pub name: String,
    pub options: SvgOptions,
    pub viewports: Vec<Viewport>,
    #[serde(default)]
    pub notes: Vec<PaperText>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    pub name: String,
    pub paper_position: Point,
    pub paper_size: [f64; 2],
    pub model_center: Point,
    /// Paper millimetres per drawing unit. Viewports intentionally clip.
    pub scale: f64,
    /// Clockwise rotation of the model view about its center, in degrees.
    /// The clipped paper frame remains axis-aligned.
    #[serde(default)]
    pub rotation_deg: f64,
    pub border: bool,
    #[serde(default)]
    pub hidden_layer_ids: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaperText {
    pub position: Point,
    pub text: String,
    pub height_mm: f64,
}

impl PrintLayout {
    pub fn fitted(
        name: impl Into<String>,
        document: &Document,
        options: SvgOptions,
    ) -> io::Result<Self> {
        document.validate()?;
        validate_options(options)?;
        let bounds = document
            .drawing_bounds(true)
            .unwrap_or_else(|| Bounds2d::from_point(Point::default()));
        let size = [
            options.page_width_mm - 2.0 * options.margin_mm,
            options.page_height_mm - 2.0 * options.margin_mm,
        ];
        let width = (bounds.max.x - bounds.min.x).max(1.0);
        let height = (bounds.max.y - bounds.min.y).max(1.0);
        let scale = options.scale.unwrap_or(
            ((size[0] - 2.0 * PLOT_PAD_MM) / width).min((size[1] - 2.0 * PLOT_PAD_MM) / height),
        );
        let layout = Self {
            name: name.into(),
            options: SvgOptions {
                scale: None,
                ..options
            },
            viewports: vec![Viewport {
                name: "Overall".into(),
                paper_position: Point::new(options.margin_mm, options.margin_mm),
                paper_size: size,
                model_center: Point::new(
                    bounds.min.x / 2.0 + bounds.max.x / 2.0,
                    bounds.min.y / 2.0 + bounds.max.y / 2.0,
                ),
                scale,
                rotation_deg: 0.0,
                border: true,
                hidden_layer_ids: Vec::new(),
            }],
            notes: Vec::new(),
        };
        layout.validate(document)?;
        Ok(layout)
    }

    pub fn validate(&self, document: &Document) -> io::Result<()> {
        validate_options(self.options)?;
        if !valid_name(&self.name)
            || self.viewports.len() > 32
            || self.notes.len() > 256
            || self.options.scale.is_some()
        {
            return Err(invalid(
                "Layouts require a single-line name, at most 32 viewports/256 notes and viewport-specific scales",
            ));
        }
        let mut names = std::collections::BTreeSet::new();
        for viewport in &self.viewports {
            let options = self.options;
            if !valid_name(&viewport.name)
                || !names.insert(viewport.name.to_lowercase())
                || !viewport.paper_position.is_finite()
                || !viewport.model_center.is_finite()
                || viewport
                    .paper_size
                    .iter()
                    .any(|v| !v.is_finite() || *v <= 0.0)
                || !viewport.scale.is_finite()
                || viewport.scale <= 0.0
                || !viewport.rotation_deg.is_finite()
                || viewport.paper_position.x < options.margin_mm
                || viewport.paper_position.y < options.margin_mm
                || viewport.paper_position.x + viewport.paper_size[0]
                    > options.page_width_mm - options.margin_mm + 1e-9
                || viewport.paper_position.y + viewport.paper_size[1]
                    > options.page_height_mm - options.margin_mm + 1e-9
                || viewport.hidden_layer_ids.len() > document.layers.len()
                || viewport
                    .hidden_layer_ids
                    .iter()
                    .any(|id| document.layer(*id).is_none())
            {
                return Err(invalid(
                    "Viewports require unique names, finite positive scales, existing layers and frames within sheet margins",
                ));
            }
            let (a, b, c, d) = viewport_matrix(viewport);
            let tx = viewport.paper_position.x + viewport.paper_size[0] / 2.0
                - a * viewport.model_center.x
                - c * viewport.model_center.y;
            let ty = viewport.paper_position.y + viewport.paper_size[1] / 2.0
                - b * viewport.model_center.x
                - d * viewport.model_center.y;
            if [a, b, c, d, tx, ty].iter().any(|value| !value.is_finite())
                || !(STROKE_MM / viewport.scale).is_finite()
                || !(POINT_RADIUS_MM / viewport.scale).is_finite()
            {
                return Err(invalid("Viewport transform exceeds numeric range"));
            }
        }
        for note in &self.notes {
            if !note.position.is_finite()
                || note.position.x < 0.0
                || note.position.y < 0.0
                || note.position.x > self.options.page_width_mm
                || note.position.y > self.options.page_height_mm
                || note.text.len() > 4096
                || note.text.contains(['\r', '\n', '\0'])
                || !note.height_mm.is_finite()
                || note.height_mm <= 0.0
                || note.height_mm > self.options.page_height_mm
            {
                return Err(invalid(
                    "Paper notes require finite sheet positions, positive heights and single-line text up to 4096 bytes",
                ));
            }
        }
        Ok(())
    }
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 255 && !name.contains(['\r', '\n', '\0'])
}

/// SVG matrix coefficients for model coordinates to paper millimetres.
/// Positive angles rotate clockwise on the printed sheet because SVG y grows
/// downward; the model's upward Y axis is reversed before rotation.
fn viewport_matrix(viewport: &Viewport) -> (f64, f64, f64, f64) {
    let (sin, cos) = viewport.rotation_deg.to_radians().sin_cos();
    let scale = viewport.scale;
    (cos * scale, sin * scale, sin * scale, -cos * scale)
}

/// Plot a saved sheet with independently scaled/clipped model viewports and
/// paper notes. A derived sheet selects drafting content from a mixed project.
pub fn export_layout_svg(document: &Document, name: &str) -> io::Result<String> {
    document.validate()?;
    let layout = document
        .layouts
        .iter()
        .find(|l| l.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| invalid("Print layout does not exist"))?;
    let primitives = drawing_primitives(document)?;
    if primitives
        .len()
        .checked_mul(layout.viewports.len())
        .is_none_or(|n| n > MAX_PLOT_PRIMITIVES)
    {
        return Err(invalid("Layout exceeds the plotted primitive limit"));
    }
    let mut out = String::new();
    writeln!(out,"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"0 0 {} {}\">",n(layout.options.page_width_mm),n(layout.options.page_height_mm),n(layout.options.page_width_mm),n(layout.options.page_height_mm)).unwrap();
    writeln!(
        out,
        "<title>{} · {}</title><rect width=\"100%\" height=\"100%\" fill=\"white\"/>",
        xml(&document.name),
        xml(&layout.name)
    )
    .unwrap();
    for (index, v) in layout.viewports.iter().enumerate() {
        let (x, y, w, h) = (
            v.paper_position.x,
            v.paper_position.y,
            v.paper_size[0],
            v.paper_size[1],
        );
        let (a, b, c, d) = viewport_matrix(v);
        let tx = x + w / 2.0 - a * v.model_center.x - c * v.model_center.y;
        let ty = y + h / 2.0 - b * v.model_center.x - d * v.model_center.y;
        writeln!(out,"<defs><clipPath id=\"viewport-{index}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath></defs>",n(x),n(y),n(w),n(h)).unwrap();
        writeln!(out,"<g clip-path=\"url(#viewport-{index})\"><title>{}</title><g transform=\"matrix({} {} {} {} {} {})\" fill=\"none\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\">",xml(&v.name),n(a),n(b),n(c),n(d),n(tx),n(ty),n(STROKE_MM/v.scale)).unwrap();
        for primitive in &primitives {
            if v.hidden_layer_ids.contains(&primitive.layer_id) {
                continue;
            }
            let layer = document.layer(primitive.layer_id).expect("validated layer");
            let [r, g, b] = if layout.options.monochrome {
                [17, 17, 17]
            } else {
                layer.color
            };
            let color = format!("#{r:02x}{g:02x}{b:02x}");
            writeln!(
                out,
                "<g data-source-id=\"{}\" data-layer=\"{}\" stroke=\"{}\">",
                primitive.source_id,
                xml(&layer.name),
                color
            )
            .unwrap();
            append_geometry(&mut out, &primitive.geometry, &color, v.scale)?;
            out.push_str("</g>\n");
        }
        out.push_str("</g></g>\n");
        if v.border {
            writeln!(out,"<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#111111\" stroke-width=\"0.25\"/>",n(x),n(y),n(w),n(h)).unwrap();
        }
    }
    for note in &layout.notes {
        writeln!(out,"<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"{}\" fill=\"#111111\" xml:space=\"preserve\">{}</text>",n(note.position.x),n(note.position.y),n(note.height_mm),xml(&note.text)).unwrap();
    }
    out.push_str("</svg>\n");
    Ok(out)
}

fn drawing_primitives(document: &Document) -> io::Result<Vec<Primitive>> {
    let mut primitives = Vec::new();
    for entity in document.expanded_entities()? {
        if !entity.visible {
            continue;
        }
        let geometries = match entity.geometry {
            Geometry::Dimension { dimension } => dimension.annotation_geometry()?,
            geometry => vec![geometry],
        };
        for geometry in geometries {
            if let Some(bounds) = plot_bounds(&geometry)?
                && (!bounds.min.is_finite() || !bounds.max.is_finite())
            {
                return Err(invalid("Plot geometry exceeds numeric range"));
            }
            primitives.push(Primitive {
                geometry,
                layer_id: entity.layer_id,
                source_id: entity.source_id,
            });
            if primitives.len() > MAX_PLOT_PRIMITIVES {
                return Err(invalid("SVG plot exceeds the primitive resource limit"));
            }
        }
    }
    Ok(primitives)
}

impl Default for SvgOptions {
    fn default() -> Self {
        Self {
            page_width_mm: 297.0,
            page_height_mm: 210.0,
            margin_mm: 10.0,
            scale: None,
            monochrome: true,
        }
    }
}

const STROKE_MM: f64 = 0.25;
const POINT_RADIUS_MM: f64 = 0.5;
const PLOT_PAD_MM: f64 = POINT_RADIUS_MM + STROKE_MM;
const MAX_PLOT_PRIMITIVES: usize = 1_000_000;

struct Primitive {
    geometry: Geometry,
    layer_id: u64,
    source_id: u64,
}

/// Export a white physical page containing the visible 2D drawing.
///
/// A scale that would crop content is refused. Any solid or mesh is also
/// refused, rather than silently excluding 3D content from a mixed document.
/// Empty and fully hidden drawings produce a blank page.
pub fn export_svg(document: &Document, options: SvgOptions) -> io::Result<String> {
    document.validate()?;
    validate_options(options)?;
    if document.has_workbench_data() {
        return Err(invalid(
            "Drawing plots cannot omit semantic workbench data; use the dedicated workbench export or a separate drawing",
        ));
    }
    if !document.solids.is_empty() || !document.meshes.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "SVG plotting supports 2D drawings only; the document contains solids or meshes",
        ));
    }
    let mut primitives = Vec::new();
    for entity in document.expanded_entities()? {
        if !entity.visible {
            continue;
        }
        match entity.geometry {
            Geometry::Dimension { dimension } => {
                for geometry in dimension.annotation_geometry()? {
                    primitives.push(Primitive {
                        geometry,
                        layer_id: entity.layer_id,
                        source_id: entity.source_id,
                    });
                }
            }
            geometry => primitives.push(Primitive {
                geometry,
                layer_id: entity.layer_id,
                source_id: entity.source_id,
            }),
        }
        if primitives.len() > MAX_PLOT_PRIMITIVES {
            return Err(invalid("SVG plot exceeds the primitive resource limit"));
        }
    }

    let mut bounds: Option<Bounds2d> = None;
    for primitive in &primitives {
        if let Some(item) = plot_bounds(&primitive.geometry)? {
            if !item.min.is_finite() || !item.max.is_finite() {
                return Err(invalid(
                    "Drawing bounds exceed the plotting coordinate range",
                ));
            }
            if let Some(total) = &mut bounds {
                total.include(item.min);
                total.include(item.max);
            } else {
                bounds = Some(item);
            }
        }
    }
    let bounds = bounds.unwrap_or_else(|| Bounds2d::from_point(Point::default()));
    let width = bounds.max.x - bounds.min.x;
    let height = bounds.max.y - bounds.min.y;
    if !width.is_finite() || !height.is_finite() {
        return Err(invalid(
            "Drawing extents exceed the plotting coordinate range",
        ));
    }
    let usable_width = options.page_width_mm - 2.0 * (options.margin_mm + PLOT_PAD_MM);
    let usable_height = options.page_height_mm - 2.0 * (options.margin_mm + PLOT_PAD_MM);
    let fit = match (width > 0.0, height > 0.0) {
        (true, true) => (usable_width / width).min(usable_height / height),
        (true, false) => usable_width / width,
        (false, true) => usable_height / height,
        (false, false) => 1.0,
    };
    let scale = options.scale.unwrap_or(fit);
    if !scale.is_finite() || scale <= 0.0 {
        return Err(invalid(
            "Drawing cannot be fitted at a finite positive plot scale",
        ));
    }
    if width * scale > usable_width * (1.0 + 1e-10)
        || height * scale > usable_height * (1.0 + 1e-10)
    {
        return Err(invalid(
            "Drawing exceeds the printable page at this scale; choose Fit, a smaller scale, or a larger page",
        ));
    }
    // Divide before adding to avoid overflow when both endpoints are large.
    let center = Point::new(
        bounds.min.x / 2.0 + bounds.max.x / 2.0,
        bounds.min.y / 2.0 + bounds.max.y / 2.0,
    );
    let tx = options.page_width_mm / 2.0 - center.x * scale;
    let ty = options.page_height_mm / 2.0 + center.y * scale;
    if !tx.is_finite()
        || !ty.is_finite()
        || !(STROKE_MM / scale).is_finite()
        || !(POINT_RADIUS_MM / scale).is_finite()
    {
        return Err(invalid(
            "Plot transform exceeds the supported coordinate range",
        ));
    }

    let mut output = String::new();
    writeln!(output, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>").unwrap();
    writeln!(
        output,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"0 0 {} {}\">",
        n(options.page_width_mm), n(options.page_height_mm),
        n(options.page_width_mm), n(options.page_height_mm),
    ).unwrap();
    writeln!(output, "<title>{}</title>", xml(&document.name)).unwrap();
    writeln!(output, "<desc>Forge Craft model-space drawing; {} paper millimetres per document unit. Font metrics are approximate.</desc>", n(scale)).unwrap();
    writeln!(
        output,
        "<rect width=\"100%\" height=\"100%\" fill=\"white\"/>"
    )
    .unwrap();
    writeln!(
        output,
        "<g transform=\"matrix({} 0 0 {} {} {})\" fill=\"none\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\">",
        n(scale), n(-scale), n(tx), n(ty), n(STROKE_MM / scale),
    ).unwrap();
    for primitive in &primitives {
        let layer = document.layer(primitive.layer_id).expect("validated layer");
        let [r, g, b] = if options.monochrome {
            [17, 17, 17]
        } else {
            layer.color
        };
        let color = format!("#{r:02x}{g:02x}{b:02x}");
        writeln!(
            output,
            "<g data-source-id=\"{}\" data-layer=\"{}\" stroke=\"{}\">",
            primitive.source_id,
            xml(&layer.name),
            color
        )
        .unwrap();
        append_geometry(&mut output, &primitive.geometry, &color, scale)?;
        output.push_str("</g>\n");
    }
    output.push_str("</g>\n</svg>\n");
    Ok(output)
}

fn validate_options(options: SvgOptions) -> io::Result<()> {
    if !options.page_width_mm.is_finite()
        || !options.page_height_mm.is_finite()
        || !options.margin_mm.is_finite()
        || options.margin_mm < 0.0
        || options.page_width_mm <= 2.0 * (options.margin_mm + PLOT_PAD_MM)
        || options.page_height_mm <= 2.0 * (options.margin_mm + PLOT_PAD_MM)
        || options
            .scale
            .is_some_and(|scale| !scale.is_finite() || scale <= 0.0)
    {
        return Err(invalid(
            "SVG page dimensions, margins, and scale must be finite and leave a positive printable area",
        ));
    }
    Ok(())
}

fn plot_bounds(geometry: &Geometry) -> io::Result<Option<Bounds2d>> {
    if let Geometry::Text {
        position,
        text,
        height,
        rotation_deg,
    } = geometry
    {
        let width = height * 0.6 * text.chars().count() as f64;
        let (s, c) = rotation_deg.to_radians().sin_cos();
        let mut bounds = Bounds2d::from_point(*position);
        // Include descenders as well as the nominal glyph height. SVG fonts
        // differ, so this remains an estimate rather than font-shaper extents.
        for (x, y) in [
            (0.0, -0.3 * height),
            (width, -0.3 * height),
            (width, *height),
            (0.0, *height),
        ] {
            let point = Point::new(position.x + x * c - y * s, position.y + x * s + y * c);
            if !point.is_finite() {
                return Err(invalid("Text extents exceed the plotting coordinate range"));
            }
            bounds.include(point);
        }
        return Ok(Some(bounds));
    }
    Ok(geometry.bounds())
}

fn append_geometry(
    output: &mut String,
    geometry: &Geometry,
    color: &str,
    scale: f64,
) -> io::Result<()> {
    match geometry {
        Geometry::Line { start, end } => {
            writeln!(
                output,
                "<path d=\"M {} {} L {} {}\"/>",
                n(start.x),
                n(start.y),
                n(end.x),
                n(end.y)
            )
            .unwrap();
        }
        Geometry::Rectangle { a, b } => {
            writeln!(
                output,
                "<path d=\"M {} {} L {} {} L {} {} L {} {} Z\"/>",
                n(a.x),
                n(a.y),
                n(b.x),
                n(a.y),
                n(b.x),
                n(b.y),
                n(a.x),
                n(b.y)
            )
            .unwrap();
        }
        Geometry::Circle { center, radius } => {
            writeln!(
                output,
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\"/>",
                n(center.x),
                n(center.y),
                n(*radius)
            )
            .unwrap();
        }
        Geometry::Arc {
            center,
            radius,
            start_angle_deg,
            end_angle_deg,
        } => {
            let sweep = (end_angle_deg - start_angle_deg).rem_euclid(360.0);
            if !sweep.is_finite() {
                return Err(invalid("Arc sweep exceeds the plotting coordinate range"));
            }
            let start = arc_point(*center, *radius, *start_angle_deg);
            let end = arc_point(*center, *radius, *end_angle_deg);
            if !start.is_finite() || !end.is_finite() {
                return Err(invalid(
                    "Arc endpoints exceed the plotting coordinate range",
                ));
            }
            writeln!(
                output,
                "<path d=\"M {} {} A {} {} 0 {} 1 {} {}\"/>",
                n(start.x),
                n(start.y),
                n(*radius),
                n(*radius),
                u8::from(sweep > 180.0),
                n(end.x),
                n(end.y)
            )
            .unwrap();
        }
        Geometry::Polyline { vertices, closed } => {
            let start = vertices.first().expect("validated polyline").point;
            write!(output, "<path d=\"M {} {}", n(start.x), n(start.y)).unwrap();
            for index in 0..vertices.len() - usize::from(!closed) {
                let a = vertices[index];
                let b = vertices[(index + 1) % vertices.len()];
                if a.bulge == 0.0 {
                    write!(output, " L {} {}", n(b.point.x), n(b.point.y)).unwrap();
                } else {
                    let (_, radius, _, sweep) = bulge_arc(a.point, b.point, a.bulge)
                        .ok_or_else(|| invalid("Invalid curved polyline plot segment"))?;
                    write!(
                        output,
                        " A {} {} 0 {} {} {} {}",
                        n(radius),
                        n(radius),
                        u8::from(sweep.abs() > 180.0),
                        u8::from(sweep > 0.0),
                        n(b.point.x),
                        n(b.point.y)
                    )
                    .unwrap();
                }
            }
            if *closed {
                output.push_str(" Z");
            }
            output.push_str("\"/>\n");
        }
        Geometry::Point { position } => {
            writeln!(
                output,
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\"/>",
                n(position.x),
                n(position.y),
                n(POINT_RADIUS_MM / scale)
            )
            .unwrap();
        }
        Geometry::Text {
            position,
            text,
            height,
            rotation_deg,
        } => {
            if !text.is_empty() {
                let width = height * 0.6 * text.chars().count() as f64;
                if !width.is_finite() {
                    return Err(invalid("Text width exceeds the plotting coordinate range"));
                }
                writeln!(output, "<text transform=\"translate({} {}) rotate({}) scale(1 -1)\" font-family=\"sans-serif\" font-size=\"{}\" textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\" fill=\"{}\" stroke=\"none\" xml:space=\"preserve\">{}</text>", n(position.x), n(position.y), n(*rotation_deg), n(*height), n(width), color, xml(text)).unwrap();
            }
        }
        Geometry::Insert { .. } | Geometry::Dimension { .. } => {
            return Err(invalid(
                "Unexpanded block or dimension reached SVG plot output",
            ));
        }
    }
    Ok(())
}

fn n(value: f64) -> String {
    if value == 0.0 {
        "0".to_owned()
    } else {
        value.to_string()
    }
}

fn xml(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            // Native names permit more controls than XML 1.0 does. Keep the
            // export well formed without inserting executable markup.
            character
                if character == '\t'
                    || character == '\n'
                    || character == '\r'
                    || ('\u{20}'..='\u{D7FF}').contains(&character)
                    || ('\u{E000}'..='\u{FFFD}').contains(&character)
                    || character >= '\u{10000}' =>
            {
                result.push(character)
            }
            _ => result.push('\u{FFFD}'),
        }
    }
    result
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Entity, PolylineVertex, SolidKind,
        dimension::{Dimension, LinearDimension, LinearDimensionKind},
    };

    fn plot(document: &Document) -> String {
        export_svg(document, SvgOptions::default()).unwrap()
    }

    #[test]
    fn viewport_rotation_matrix_rotates_the_model_inside_an_axis_aligned_frame() {
        let viewport = |rotation_deg| Viewport {
            name: "Rotated".into(),
            paper_position: Point::new(10.0, 10.0),
            paper_size: [100.0, 80.0],
            model_center: Point::new(4.0, 5.0),
            scale: 2.0,
            rotation_deg,
            border: false,
            hidden_layer_ids: Vec::new(),
        };
        let (a, b, c, d) = viewport_matrix(&viewport(90.0));
        assert!(a.abs() < 1e-12);
        assert!((b - 2.0).abs() < 1e-12);
        assert!((c - 2.0).abs() < 1e-12);
        assert!(d.abs() < 1e-12);
        let (a, b, c, d) = viewport_matrix(&viewport(45.0));
        let diagonal = 2.0_f64.sqrt();
        assert!((a - diagonal).abs() < 1e-12);
        assert!((b - diagonal).abs() < 1e-12);
        assert!((c - diagonal).abs() < 1e-12);
        assert!((d + diagonal).abs() < 1e-12);
        let document = Document {
            layouts: vec![PrintLayout {
                name: "Rotated sheet".into(),
                options: SvgOptions::default(),
                viewports: vec![viewport(90.0)],
                notes: Vec::new(),
            }],
            ..Document::default()
        };
        document.validate().unwrap();
        let output = export_layout_svg(&document, "Rotated sheet").unwrap();
        assert!(output.contains("clip-path=\"url(#viewport-0)\""));
        let matrix = output
            .split("<g transform=\"matrix(")
            .nth(1)
            .unwrap()
            .split(')')
            .next()
            .unwrap();
        let values: Vec<f64> = matrix
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        assert_eq!(values.len(), 6);
        assert!(values[0].abs() < 1e-12);
        assert!((values[1] - 2.0).abs() < 1e-12);
        assert!((values[2] - 2.0).abs() < 1e-12);
        assert!(values[3].abs() < 1e-12);
        assert!((values[4] - 50.0).abs() < 1e-12);
        assert!((values[5] - 42.0).abs() < 1e-12);
        let mut invalid = document.clone();
        invalid.layouts[0].viewports[0].rotation_deg = f64::NAN;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn fixed_scale_centres_geometry_and_reverses_only_the_world_y_axis() {
        let mut document = Document::new("Scale test");
        document.add_geometry(Geometry::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(100.0, 50.0),
        });
        let output = export_svg(
            &document,
            SvgOptions {
                scale: Some(1.0),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(output.contains("width=\"297mm\" height=\"210mm\""));
        assert!(output.contains("transform=\"matrix(1 0 0 -1 98.5 130)\""));
        assert!(output.contains("M 0 0 L 100 50"));
        assert!(output.contains("stroke-width=\"0.25\""));
        assert!(
            export_svg(
                &document,
                SvgOptions {
                    scale: Some(10.0),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn analytic_arcs_circles_and_signed_bulges_are_not_tessellated() {
        let mut document = Document::default();
        document.add_geometry(Geometry::Circle {
            center: Point::new(3.0, 4.0),
            radius: 2.0,
        });
        document.add_geometry(Geometry::Arc {
            center: Point::default(),
            radius: 10.0,
            start_angle_deg: 0.0,
            end_angle_deg: 270.0,
        });
        document.add_geometry(Geometry::Polyline {
            vertices: vec![
                PolylineVertex {
                    point: Point::new(0.0, 0.0),
                    bulge: -1.0,
                },
                PolylineVertex::new(Point::new(10.0, 0.0)),
            ],
            closed: false,
        });
        let output = plot(&document);
        assert!(output.contains("<circle cx=\"3\" cy=\"4\" r=\"2\"/>"));
        assert!(output.contains("A 10 10 0 1 1"));
        assert!(output.contains("M 0 0 A 5 5 0 0 0 10 0"));
    }

    #[test]
    fn layers_and_visibility_resolve_through_block_instances() {
        let mut document = Document::default();
        let red = document.add_layer("Red & \"visible\"", [230, 10, 20]);
        document
            .add_block(
                "Part",
                Point::default(),
                vec![Entity {
                    id: 0,
                    layer_id: 1,
                    geometry: Geometry::Circle {
                        center: Point::default(),
                        radius: 2.0,
                    },
                }],
            )
            .unwrap();
        let source = document.add_geometry(Geometry::Insert {
            block_name: "Part".into(),
            position: Point::new(20.0, 30.0),
            scale: 3.0,
            rotation_deg: 45.0,
        });
        let output = export_svg(
            &document,
            SvgOptions {
                monochrome: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(output.contains("cx=\"20\" cy=\"30\" r=\"6\""));
        assert!(output.contains("stroke=\"#e60a14\""));
        assert!(output.contains("Red &amp; &quot;visible&quot;"));
        assert!(output.contains(&format!("data-source-id=\"{source}\"")));
        document
            .layers
            .iter_mut()
            .find(|layer| layer.id == red)
            .unwrap()
            .visible = false;
        let hidden = plot(&document);
        assert!(!hidden.contains("<circle"));
        assert!(!hidden.contains("data-source-id"));
    }

    #[test]
    fn dimension_annotation_is_exported_without_changing_semantics() {
        let mut document = Document::default();
        document.add_geometry(Geometry::Dimension {
            dimension: Dimension::Linear(LinearDimension {
                kind: LinearDimensionKind::Horizontal,
                a: Point::default(),
                b: Point::new(100.0, 0.0),
                line_point: Point::new(0.0, 15.0),
                style: Default::default(),
            }),
        });
        let before = document.clone();
        let output = plot(&document);
        assert!(output.contains("100.00</text>"));
        assert!(output.contains("<path"));
        assert_eq!(document, before);
    }

    #[test]
    fn text_is_upright_rotated_and_xml_is_escaped() {
        let mut document = Document::new("Drawing <script> & \"quotes\"\u{1}");
        document.add_geometry(Geometry::Text {
            position: Point::new(3.0, 4.0),
            text: "<script> & 'quoted' \"test\"".into(),
            height: 2.5,
            rotation_deg: 30.0,
        });
        let output = plot(&document);
        assert!(!output.contains("<script>"));
        assert!(!output.contains('\u{1}'));
        assert!(output.contains("Drawing &lt;script&gt; &amp; &quot;quotes&quot;�"));
        let rotation = output
            .split("translate(3 4) rotate(")
            .nth(1)
            .unwrap()
            .split(')')
            .next()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!((rotation - 30.0).abs() < 1e-10);
        assert!(output.contains("scale(1 -1)\" font-family="));
        assert!(output.contains("&lt;script&gt; &amp; &apos;quoted&apos; &quot;test&quot;"));
    }

    #[test]
    fn invalid_pages_scales_and_silently_dropped_3d_are_refused() {
        let document = Document::default();
        for options in [
            SvgOptions {
                page_width_mm: 0.0,
                ..Default::default()
            },
            SvgOptions {
                page_height_mm: f64::INFINITY,
                ..Default::default()
            },
            SvgOptions {
                margin_mm: -1.0,
                ..Default::default()
            },
            SvgOptions {
                margin_mm: 200.0,
                ..Default::default()
            },
            SvgOptions {
                scale: Some(0.0),
                ..Default::default()
            },
            SvgOptions {
                scale: Some(f64::NAN),
                ..Default::default()
            },
            SvgOptions {
                scale: Some(2e-309),
                ..Default::default()
            },
        ] {
            assert!(export_svg(&document, options).is_err());
        }
        let mut mixed = document;
        mixed.add_solid("Box", SolidKind::Box);
        assert_eq!(
            export_svg(&mixed, Default::default()).unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
    }

    #[test]
    fn empty_and_single_point_drawings_have_finite_plot_transforms() {
        let mut document = Document::default();
        assert!(plot(&document).contains("matrix(1 0 0 -1 148.5 105)"));
        document.add_geometry(Geometry::Point {
            position: Point::new(-500.0, 600.0),
        });
        let output = plot(&document);
        assert!(output.contains("matrix(1 0 0 -1 648.5 705)"));
        assert!(output.contains("r=\"0.5\""));
        assert!(!output.contains("NaN"));
        assert!(!output.contains("inf"));
    }
}
