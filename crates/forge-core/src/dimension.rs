//! Semantic drafting dimensions. References, placement and style are stored;
//! annotation strokes and text are derived whenever the dimension is displayed.
//!
//! Linear dimensions measure a projection onto their dimension axis. Aligned
//! dimensions instead measure the true distance between their reference points.
//! This models the public DIMLINEAR/DIMALIGNED and DIMRADIUS/DIMDIAMETER behavior.

use crate::{Geometry, Point, PolylineVertex, edit::Transform2d};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DimensionStyle {
    pub text_height: f64,
    pub arrow_size: f64,
    pub extension_offset: f64,
    pub extension_overshoot: f64,
    pub text_gap: f64,
    pub precision: u8,
    /// `None`, an empty string, or `<>` uses the computed measurement. A single
    /// space suppresses text. Other strings replace `<>` with that measurement.
    pub text_override: Option<String>,
}

impl Default for DimensionStyle {
    fn default() -> Self {
        Self {
            text_height: 2.5,
            arrow_size: 2.5,
            extension_offset: 0.625,
            extension_overshoot: 1.25,
            text_gap: 0.625,
            precision: 2,
            text_override: None,
        }
    }
}

impl DimensionStyle {
    pub fn validate(&self) -> io::Result<()> {
        for (name, value) in [
            ("text height", self.text_height),
            ("arrow size", self.arrow_size),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(invalid(format!(
                    "Dimension {name} must be finite and positive"
                )));
            }
        }
        for (name, value) in [
            ("extension offset", self.extension_offset),
            ("extension overshoot", self.extension_overshoot),
            ("text gap", self.text_gap),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(invalid(format!(
                    "Dimension {name} must be finite and nonnegative"
                )));
            }
        }
        if self.precision > 12 {
            return Err(invalid("Dimension precision must be between 0 and 12"));
        }
        if let Some(text) = &self.text_override {
            if text.contains(['\n', '\r', '\0']) {
                return Err(invalid(
                    "Dimension text must be a single line without null characters",
                ));
            }
        }
        Ok(())
    }

    fn text(&self, measurement: f64, prefix: &str) -> String {
        let precision = self.precision as usize;
        let automatic = format!("{prefix}{measurement:.precision$}");
        match self.text_override.as_deref() {
            None | Some("") => automatic,
            Some(" ") => String::new(),
            Some(value) => value.replace("<>", &automatic),
        }
    }

    fn scaled(&self, scale: f64) -> Self {
        Self {
            text_height: self.text_height * scale,
            arrow_size: self.arrow_size * scale,
            extension_offset: self.extension_offset * scale,
            extension_overshoot: self.extension_overshoot * scale,
            text_gap: self.text_gap * scale,
            precision: self.precision,
            text_override: self.text_override.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum LinearDimensionKind {
    Horizontal,
    Vertical,
    Aligned,
    Rotated { angle_deg: f64 },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LinearDimension {
    pub kind: LinearDimensionKind,
    pub a: Point,
    pub b: Point,
    /// A point on the dimension line. Its component parallel to that line does
    /// not affect the measurement or extension-line endpoints.
    pub line_point: Point,
    pub style: DimensionStyle,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RadialDimension {
    pub center: Point,
    pub point_on_circle: Point,
    pub diameter: bool,
    pub leader_length: f64,
    /// Center of the text in model coordinates. `None` derives its location
    /// from the radius line and leader length.
    pub text_position: Option<Point>,
    pub style: DimensionStyle,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "dimension_type", rename_all = "snake_case")]
pub enum Dimension {
    Linear(LinearDimension),
    Radial(RadialDimension),
}

#[derive(Clone, Debug)]
pub struct DimensionLayout {
    /// Only Line, Polyline and Text primitives are emitted. Semantic dimensions
    /// never recursively contain flattened dimensions.
    pub primitives: Vec<Geometry>,
    pub measurement: f64,
    pub text: String,
    /// Center of the laid-out text; the emitted Text primitive has a baseline
    /// insertion point derived using approximate glyph advances.
    pub text_position: Point,
    pub text_rotation_deg: f64,
    pub dimension_start: Point,
    pub dimension_end: Point,
}

impl Dimension {
    pub fn validate(&self) -> io::Result<()> {
        match self {
            Self::Linear(value) => value.validate(),
            Self::Radial(value) => value.validate(),
        }
    }

    pub fn measurement(&self) -> io::Result<f64> {
        match self {
            Self::Linear(value) => value.measurement(),
            Self::Radial(value) => value.measurement(),
        }
    }

    pub fn text(&self) -> io::Result<String> {
        match self {
            Self::Linear(value) => value.text(),
            Self::Radial(value) => value.text(),
        }
    }

    pub fn layout(&self) -> io::Result<DimensionLayout> {
        match self {
            Self::Linear(value) => value.layout(),
            Self::Radial(value) => value.layout(),
        }
    }

    pub fn annotation_geometry(&self) -> io::Result<Vec<Geometry>> {
        Ok(self.layout()?.primitives)
    }

    pub fn transformed(&self, transform: Transform2d) -> io::Result<Self> {
        match self {
            Self::Linear(value) => Ok(Self::Linear(value.transformed(transform)?)),
            Self::Radial(value) => Ok(Self::Radial(value.transformed(transform)?)),
        }
    }
}

impl LinearDimension {
    pub fn new(
        kind: LinearDimensionKind,
        a: Point,
        b: Point,
        line_point: Point,
    ) -> io::Result<Self> {
        let dimension = Self {
            kind,
            a,
            b,
            line_point,
            style: DimensionStyle::default(),
        };
        dimension.validate()?;
        Ok(dimension)
    }

    pub fn validate(&self) -> io::Result<()> {
        self.style.validate()?;
        for point in [self.a, self.b, self.line_point] {
            finite_point(point)?;
        }
        self.frame()?;
        Ok(())
    }

    pub fn measurement(&self) -> io::Result<f64> {
        self.validate()?;
        Ok(self.frame()?.measurement)
    }

    pub fn text(&self) -> io::Result<String> {
        Ok(self.style.text(self.measurement()?, ""))
    }

    pub fn layout(&self) -> io::Result<DimensionLayout> {
        self.validate()?;
        let frame = self.frame()?;
        let text = self.style.text(frame.measurement, "");
        let readable_angle = readable_angle(frame.axis.y.atan2(frame.axis.x).to_degrees());
        let text_axis = axis_from_angle(readable_angle);
        let text_normal = perpendicular(text_axis);
        let text_position = add(
            midpoint(frame.start, frame.end),
            mul(
                text_normal,
                self.style.text_gap + self.style.text_height * 0.5,
            ),
        );
        let text_width = approximate_text_width(&text, self.style.text_height);
        let inside_arrows = frame.measurement
            >= 2.0 * self.style.arrow_size + text_width + 2.0 * self.style.text_gap;
        let arrow_direction = if inside_arrows {
            frame.direction
        } else {
            mul(frame.direction, -1.0)
        };
        let extension = if inside_arrows {
            0.0
        } else {
            self.style.arrow_size * 1.5
        };
        let mut primitives = vec![Geometry::Line {
            start: sub(frame.start, mul(frame.direction, extension)),
            end: add(frame.end, mul(frame.direction, extension)),
        }];
        add_extension(
            &mut primitives,
            self.a,
            frame.start,
            frame.normal,
            &self.style,
        );
        add_extension(
            &mut primitives,
            self.b,
            frame.end,
            frame.normal,
            &self.style,
        );
        add_arrow(
            &mut primitives,
            frame.start,
            arrow_direction,
            self.style.arrow_size,
        );
        add_arrow(
            &mut primitives,
            frame.end,
            mul(arrow_direction, -1.0),
            self.style.arrow_size,
        );
        add_text(
            &mut primitives,
            &text,
            text_position,
            readable_angle,
            self.style.text_height,
        );
        validated_layout(DimensionLayout {
            primitives,
            measurement: frame.measurement,
            text,
            text_position,
            text_rotation_deg: readable_angle,
            dimension_start: frame.start,
            dimension_end: frame.end,
        })
    }

    pub fn transformed(&self, transform: Transform2d) -> io::Result<Self> {
        self.validate()?;
        let scale = similarity_scale(transform)?;
        let kind = match self.kind {
            LinearDimensionKind::Aligned => LinearDimensionKind::Aligned,
            _ => {
                let axis = self.frame()?.axis;
                let axis = Point::new(
                    transform.matrix[0][0] * axis.x + transform.matrix[0][1] * axis.y,
                    transform.matrix[1][0] * axis.x + transform.matrix[1][1] * axis.y,
                );
                let angle = axis.y.atan2(axis.x).to_degrees().rem_euclid(180.0);
                if angle.min((180.0 - angle).abs()) < 1e-9 {
                    LinearDimensionKind::Horizontal
                } else if (angle - 90.0).abs() < 1e-9 {
                    LinearDimensionKind::Vertical
                } else {
                    LinearDimensionKind::Rotated { angle_deg: angle }
                }
            }
        };
        let result = Self {
            kind,
            a: transform.apply(self.a),
            b: transform.apply(self.b),
            line_point: transform.apply(self.line_point),
            style: self.style.scaled(scale),
        };
        result.validate()?;
        Ok(result)
    }

    fn frame(&self) -> io::Result<LinearFrame> {
        let delta = sub(self.b, self.a);
        finite_point(delta)?;
        let axis = match self.kind {
            LinearDimensionKind::Horizontal => Point::new(1.0, 0.0),
            LinearDimensionKind::Vertical => Point::new(0.0, 1.0),
            LinearDimensionKind::Aligned => unit(delta)?,
            LinearDimensionKind::Rotated { angle_deg } => {
                if !angle_deg.is_finite() {
                    return Err(invalid("Dimension rotation must be finite"));
                }
                axis_from_angle(angle_deg.rem_euclid(360.0))
            }
        };
        let signed_measurement = dot(delta, axis);
        let projection_resolution =
            8.0 * f64::EPSILON * (delta.x.abs() * axis.x.abs() + delta.y.abs() * axis.y.abs());
        if !signed_measurement.is_finite() || signed_measurement.abs() <= projection_resolution {
            return Err(invalid(
                "Dimension reference points must have a finite, nonzero distance along the dimension axis",
            ));
        }
        let measurement = signed_measurement.abs();
        let normal = perpendicular(axis);
        // Subtract before projecting, which retains small offsets when model
        // coordinates are large and avoids summing large absolute projections.
        let start = add(
            self.a,
            mul(normal, dot(sub(self.line_point, self.a), normal)),
        );
        let end = add(
            self.b,
            mul(normal, dot(sub(self.line_point, self.b), normal)),
        );
        finite_point(start)?;
        finite_point(end)?;
        Ok(LinearFrame {
            axis,
            normal,
            direction: mul(axis, signed_measurement.signum()),
            start,
            end,
            measurement,
        })
    }
}

impl RadialDimension {
    pub fn new(center: Point, point_on_circle: Point, diameter: bool) -> io::Result<Self> {
        let result = Self {
            center,
            point_on_circle,
            diameter,
            leader_length: 0.0,
            text_position: None,
            style: DimensionStyle::default(),
        };
        result.validate()?;
        Ok(result)
    }

    pub fn validate(&self) -> io::Result<()> {
        self.style.validate()?;
        finite_point(self.center)?;
        finite_point(self.point_on_circle)?;
        if let Some(position) = self.text_position {
            finite_point(position)?;
        }
        if !self.leader_length.is_finite() || self.leader_length < 0.0 {
            return Err(invalid(
                "Radial dimension leader length must be finite and nonnegative",
            ));
        }
        let radius = length(sub(self.point_on_circle, self.center));
        if !radius.is_finite() || radius <= 0.0 || (self.diameter && !(radius * 2.0).is_finite()) {
            return Err(invalid(
                "Radial dimension requires a finite positive radius",
            ));
        }
        Ok(())
    }

    pub fn measurement(&self) -> io::Result<f64> {
        self.validate()?;
        Ok(length(sub(self.point_on_circle, self.center)) * if self.diameter { 2.0 } else { 1.0 })
    }

    pub fn text(&self) -> io::Result<String> {
        Ok(self
            .style
            .text(self.measurement()?, if self.diameter { "⌀" } else { "R" }))
    }

    pub fn layout(&self) -> io::Result<DimensionLayout> {
        self.validate()?;
        let radius_vector = sub(self.point_on_circle, self.center);
        let direction = unit(radius_vector)?;
        let start = if self.diameter {
            sub(self.center, radius_vector)
        } else {
            self.center
        };
        let end = self.point_on_circle;
        let measurement = self.measurement()?;
        let text = self.text()?;
        let angle = readable_angle(direction.y.atan2(direction.x).to_degrees());
        let text_normal = perpendicular(axis_from_angle(angle));
        let landing = if self.leader_length > 0.0 {
            add(end, mul(direction, self.leader_length))
        } else {
            midpoint(start, end)
        };
        let text_position = self.text_position.unwrap_or_else(|| {
            add(
                landing,
                mul(
                    text_normal,
                    self.style.text_height * 0.5 + self.style.text_gap,
                ),
            )
        });
        let mut primitives = vec![Geometry::Line { start, end }];
        if self.leader_length > 0.0 || self.text_position.is_some() {
            let leader_end = sub(
                text_position,
                mul(
                    text_normal,
                    self.style.text_height * 0.5 + self.style.text_gap,
                ),
            );
            primitives.push(Geometry::Line {
                start: end,
                end: leader_end,
            });
        }
        let inside = measurement >= 2.0 * self.style.arrow_size;
        let base_direction = if inside {
            direction
        } else {
            mul(direction, -1.0)
        };
        if self.diameter {
            add_arrow(
                &mut primitives,
                start,
                base_direction,
                self.style.arrow_size,
            );
        }
        add_arrow(
            &mut primitives,
            end,
            mul(base_direction, -1.0),
            self.style.arrow_size,
        );
        if !self.diameter {
            let size = self.style.arrow_size * 0.3;
            primitives.push(Geometry::Line {
                start: add(self.center, Point::new(-size, 0.0)),
                end: add(self.center, Point::new(size, 0.0)),
            });
            primitives.push(Geometry::Line {
                start: add(self.center, Point::new(0.0, -size)),
                end: add(self.center, Point::new(0.0, size)),
            });
        }
        add_text(
            &mut primitives,
            &text,
            text_position,
            angle,
            self.style.text_height,
        );
        validated_layout(DimensionLayout {
            primitives,
            measurement,
            text,
            text_position,
            text_rotation_deg: angle,
            dimension_start: start,
            dimension_end: end,
        })
    }

    pub fn transformed(&self, transform: Transform2d) -> io::Result<Self> {
        self.validate()?;
        let scale = similarity_scale(transform)?;
        let result = Self {
            center: transform.apply(self.center),
            point_on_circle: transform.apply(self.point_on_circle),
            diameter: self.diameter,
            leader_length: self.leader_length * scale,
            text_position: self.text_position.map(|point| transform.apply(point)),
            style: self.style.scaled(scale),
        };
        result.validate()?;
        Ok(result)
    }
}

struct LinearFrame {
    axis: Point,
    normal: Point,
    direction: Point,
    start: Point,
    end: Point,
    measurement: f64,
}

fn add_extension(
    primitives: &mut Vec<Geometry>,
    origin: Point,
    end: Point,
    normal: Point,
    style: &DimensionStyle,
) {
    let distance = dot(sub(end, origin), normal);
    if distance == 0.0 {
        return;
    }
    let direction = mul(normal, distance.signum());
    primitives.push(Geometry::Line {
        start: add(
            origin,
            mul(direction, style.extension_offset.min(distance.abs())),
        ),
        end: add(end, mul(direction, style.extension_overshoot)),
    });
}

fn add_arrow(primitives: &mut Vec<Geometry>, tip: Point, base_direction: Point, size: f64) {
    let base = add(tip, mul(base_direction, size));
    let half_width = mul(perpendicular(base_direction), size * 0.2);
    primitives.push(Geometry::Polyline {
        vertices: [tip, add(base, half_width), sub(base, half_width)]
            .into_iter()
            .map(PolylineVertex::new)
            .collect(),
        closed: true,
    });
}

fn add_text(primitives: &mut Vec<Geometry>, text: &str, center: Point, angle: f64, height: f64) {
    if text.is_empty() {
        return;
    }
    let axis = axis_from_angle(angle);
    let normal = perpendicular(axis);
    let position = sub(
        sub(
            center,
            mul(axis, approximate_text_width(text, height) * 0.5),
        ),
        mul(normal, height * 0.5),
    );
    primitives.push(Geometry::Text {
        position,
        text: text.to_owned(),
        height,
        rotation_deg: angle,
    });
}

fn validated_layout(layout: DimensionLayout) -> io::Result<DimensionLayout> {
    finite_point(layout.text_position)?;
    finite_point(layout.dimension_start)?;
    finite_point(layout.dimension_end)?;
    for primitive in &layout.primitives {
        primitive.validate()?;
    }
    Ok(layout)
}

fn similarity_scale(transform: Transform2d) -> io::Result<f64> {
    // Reuse the core's similarity-transform validation. A point primitive never
    // recursively invokes dimension layout or transformations.
    Geometry::Point {
        position: Point::default(),
    }
    .transformed(transform)?;
    Ok(transform.matrix[0][0].hypot(transform.matrix[1][0]))
}

fn approximate_text_width(text: &str, height: f64) -> f64 {
    text.chars().count() as f64 * height * 0.6
}
fn readable_angle(angle: f64) -> f64 {
    let angle = angle.rem_euclid(360.0);
    if angle > 90.0 && angle <= 270.0 {
        (angle + 180.0).rem_euclid(360.0)
    } else {
        angle
    }
}
fn axis_from_angle(angle: f64) -> Point {
    match angle.rem_euclid(360.0) {
        0.0 => Point::new(1.0, 0.0),
        90.0 => Point::new(0.0, 1.0),
        180.0 => Point::new(-1.0, 0.0),
        270.0 => Point::new(0.0, -1.0),
        angle => {
            let (sin, cos) = angle.to_radians().sin_cos();
            Point::new(cos, sin)
        }
    }
}
fn perpendicular(point: Point) -> Point {
    Point::new(-point.y, point.x)
}
fn add(a: Point, b: Point) -> Point {
    Point::new(a.x + b.x, a.y + b.y)
}
fn sub(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}
fn mul(point: Point, scale: f64) -> Point {
    Point::new(point.x * scale, point.y * scale)
}
fn midpoint(a: Point, b: Point) -> Point {
    add(a, mul(sub(b, a), 0.5))
}
fn dot(a: Point, b: Point) -> f64 {
    a.x * b.x + a.y * b.y
}
fn length(point: Point) -> f64 {
    point.x.hypot(point.y)
}
fn unit(point: Point) -> io::Result<Point> {
    let length = length(point);
    if !length.is_finite() || length <= 0.0 {
        return Err(invalid(
            "Dimension reference points must be finite and distinct",
        ));
    }
    Ok(Point::new(point.x / length, point.y / length))
}
fn finite_point(point: Point) -> io::Result<()> {
    if point.is_finite() {
        Ok(())
    } else {
        Err(invalid("Dimension coordinates must be finite"))
    }
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear(
        kind: LinearDimensionKind,
        a: [f64; 2],
        b: [f64; 2],
        line: [f64; 2],
    ) -> LinearDimension {
        LinearDimension::new(
            kind,
            Point::new(a[0], a[1]),
            Point::new(b[0], b[1]),
            Point::new(line[0], line[1]),
        )
        .unwrap()
    }
    fn near(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    #[test]
    fn horizontal_vertical_and_aligned_measure_distinct_quantities() {
        near(
            linear(
                LinearDimensionKind::Horizontal,
                [1.0, 2.0],
                [4.0, 6.0],
                [0.0, 10.0],
            )
            .measurement()
            .unwrap(),
            3.0,
        );
        near(
            linear(
                LinearDimensionKind::Vertical,
                [1.0, 2.0],
                [4.0, 6.0],
                [10.0, 0.0],
            )
            .measurement()
            .unwrap(),
            4.0,
        );
        near(
            linear(
                LinearDimensionKind::Aligned,
                [1.0, 2.0],
                [4.0, 6.0],
                [-5.0, 8.0],
            )
            .measurement()
            .unwrap(),
            5.0,
        );
    }

    #[test]
    fn reversed_reference_order_keeps_positive_measurement_and_correct_arrows() {
        let dimension = linear(
            LinearDimensionKind::Horizontal,
            [20.0, 3.0],
            [-30.0, 7.0],
            [0.0, -12.0],
        );
        let layout = dimension.layout().unwrap();
        near(layout.measurement, 50.0);
        assert_eq!(layout.dimension_start, Point::new(20.0, -12.0));
        assert_eq!(layout.dimension_end, Point::new(-30.0, -12.0));
        let arrow_vertices: Vec<_> = layout
            .primitives
            .iter()
            .filter_map(|geometry| {
                if let Geometry::Polyline { vertices, .. } = geometry {
                    Some(vertices)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(arrow_vertices.len(), 2);
        assert!(arrow_vertices[0][1].point.x < arrow_vertices[0][0].point.x);
        assert!(arrow_vertices[1][1].point.x > arrow_vertices[1][0].point.x);
        assert_eq!(layout.text_rotation_deg, 0.0);
    }

    #[test]
    fn dimension_placement_changes_annotation_but_never_measurement() {
        let first = linear(
            LinearDimensionKind::Aligned,
            [0.0, 0.0],
            [30.0, 40.0],
            [0.0, 20.0],
        );
        let second = LinearDimension {
            line_point: Point::new(80.0, -30.0),
            ..first.clone()
        };
        near(first.measurement().unwrap(), second.measurement().unwrap());
        assert_ne!(
            first.layout().unwrap().dimension_start,
            second.layout().unwrap().dimension_start
        );
        near(
            first
                .layout()
                .unwrap()
                .dimension_start
                .distance(first.layout().unwrap().dimension_end),
            50.0,
        );
    }

    #[test]
    fn rotated_dimension_projects_onto_requested_axis() {
        let dimension = linear(
            LinearDimensionKind::Rotated { angle_deg: 45.0 },
            [0.0, 0.0],
            [10.0, 0.0],
            [0.0, 10.0],
        );
        near(dimension.measurement().unwrap(), 5.0 * 2.0_f64.sqrt());
        let layout = dimension.layout().unwrap();
        near(layout.text_rotation_deg, 45.0);
        near(
            layout.dimension_start.distance(layout.dimension_end),
            layout.measurement,
        );
    }

    #[test]
    fn references_and_style_round_trip_semantically_without_flattening() {
        let mut dimension = linear(
            LinearDimensionKind::Horizontal,
            [0.0, 0.0],
            [10.0, 5.0],
            [0.0, 9.0],
        );
        dimension.style.precision = 3;
        dimension.style.text_override = Some("TYP <> mm".to_owned());
        let dimension = Dimension::Linear(dimension);
        let source = serde_json::to_string(&dimension).unwrap();
        assert!(!source.contains("primitives"));
        let loaded: Dimension = serde_json::from_str(&source).unwrap();
        assert_eq!(dimension, loaded);
        assert_eq!(loaded.text().unwrap(), "TYP 10.000 mm");
        assert!(!loaded.annotation_geometry().unwrap().is_empty());
    }

    #[test]
    fn overrides_update_with_measurement_and_blank_suppresses_text() {
        let mut dimension = linear(
            LinearDimensionKind::Horizontal,
            [0.0, 0.0],
            [10.0, 0.0],
            [0.0, 5.0],
        );
        dimension.style.text_override = Some("<> ±0.1".to_owned());
        dimension.b.x = 12.5;
        assert_eq!(dimension.text().unwrap(), "12.50 ±0.1");
        dimension.style.text_override = Some(" ".to_owned());
        assert_eq!(dimension.text().unwrap(), "");
        assert!(
            dimension
                .layout()
                .unwrap()
                .primitives
                .iter()
                .all(|primitive| !matches!(primitive, Geometry::Text { .. }))
        );
        dimension.style.text_override = Some("MANUAL".to_owned());
        assert_eq!(dimension.text().unwrap(), "MANUAL");
    }

    #[test]
    fn zero_and_nonfinite_measurements_are_rejected() {
        for (kind, a, b) in [
            (
                LinearDimensionKind::Aligned,
                Point::default(),
                Point::default(),
            ),
            (
                LinearDimensionKind::Horizontal,
                Point::new(1.0, 1.0),
                Point::new(1.0, 20.0),
            ),
            (
                LinearDimensionKind::Vertical,
                Point::new(1.0, 1.0),
                Point::new(20.0, 1.0),
            ),
        ] {
            assert!(LinearDimension::new(kind, a, b, Point::new(0.0, 9.0)).is_err());
        }
        let mut dimension = linear(
            LinearDimensionKind::Aligned,
            [0.0, 0.0],
            [1.0, 1.0],
            [0.0, 9.0],
        );
        dimension.line_point.x = f64::NAN;
        assert!(dimension.layout().is_err());
        dimension.line_point.x = 1.0;
        dimension.kind = LinearDimensionKind::Rotated {
            angle_deg: f64::INFINITY,
        };
        assert!(dimension.measurement().is_err());
    }

    #[test]
    fn invalid_styles_are_rejected_before_annotation() {
        let baseline = DimensionStyle::default();
        for style in [
            DimensionStyle {
                arrow_size: 0.0,
                ..baseline.clone()
            },
            DimensionStyle {
                text_height: -1.0,
                ..baseline.clone()
            },
            DimensionStyle {
                text_gap: f64::NAN,
                ..baseline.clone()
            },
            DimensionStyle {
                precision: 13,
                ..baseline.clone()
            },
            DimensionStyle {
                text_override: Some("bad\ntext".to_owned()),
                ..baseline.clone()
            },
        ] {
            assert!(style.validate().is_err());
        }
    }

    #[test]
    fn layout_moves_short_dimension_arrows_outside_and_never_emits_nested_dimensions() {
        let dimension = linear(
            LinearDimensionKind::Horizontal,
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 5.0],
        );
        let layout = dimension.layout().unwrap();
        let arrows: Vec<_> = layout
            .primitives
            .iter()
            .filter_map(|geometry| {
                if let Geometry::Polyline { vertices, .. } = geometry {
                    Some(vertices)
                } else {
                    None
                }
            })
            .collect();
        assert!(arrows[0][1].point.x < arrows[0][0].point.x);
        assert!(arrows[1][1].point.x > arrows[1][0].point.x);
        for primitive in layout.primitives {
            assert!(matches!(
                primitive,
                Geometry::Line { .. } | Geometry::Polyline { .. } | Geometry::Text { .. }
            ));
        }
    }

    #[test]
    fn radial_and_diameter_dimensions_measure_original_circle() {
        let radial =
            RadialDimension::new(Point::new(10.0, -20.0), Point::new(13.0, -16.0), false).unwrap();
        assert_eq!(radial.text().unwrap(), "R5.00");
        let mut diameter = radial.clone();
        diameter.diameter = true;
        diameter.leader_length = 10.0;
        assert_eq!(diameter.text().unwrap(), "⌀10.00");
        let layout = diameter.layout().unwrap();
        near(layout.dimension_start.distance(layout.dimension_end), 10.0);
        assert_eq!(layout.dimension_start, Point::new(7.0, -24.0));
        diameter.point_on_circle = diameter.center;
        assert!(diameter.validate().is_err());
    }

    #[test]
    fn radial_text_position_is_preserved_and_annotation_remains_valid() {
        let mut radial =
            RadialDimension::new(Point::default(), Point::new(15.0, 0.0), false).unwrap();
        radial.text_position = Some(Point::new(32.0, 11.0));
        let layout = radial.layout().unwrap();
        assert_eq!(layout.text_position, radial.text_position.unwrap());
        near(layout.measurement, 15.0);
        for primitive in layout.primitives {
            primitive.validate().unwrap();
        }
    }

    #[test]
    fn transforms_update_measurement_direction_and_style() {
        let original = linear(
            LinearDimensionKind::Horizontal,
            [0.0, 0.0],
            [20.0, 0.0],
            [0.0, 10.0],
        );
        let transform = Transform2d::uniform_scale(2.0, Point::default())
            .then(Transform2d::rotation(90.0, Point::default()))
            .then(Transform2d::translation(100.0, 200.0));
        let transformed = original.transformed(transform).unwrap();
        assert_eq!(transformed.kind, LinearDimensionKind::Vertical);
        near(transformed.measurement().unwrap(), 40.0);
        near(transformed.style.text_height, 5.0);
        near(transformed.a.x, 100.0);
        near(transformed.a.y, 200.0);
        transformed.layout().unwrap();
        let mirror = Transform2d::reflection(Point::default(), Point::new(0.0, 1.0)).unwrap();
        near(
            original.transformed(mirror).unwrap().measurement().unwrap(),
            20.0,
        );
    }

    #[test]
    fn large_coordinates_keep_small_dimline_offsets() {
        let dimension = linear(
            LinearDimensionKind::Horizontal,
            [1e12, 1e12],
            [1e12 + 10.0, 1e12],
            [1e12, 1e12 + 2.0],
        );
        let layout = dimension.layout().unwrap();
        near(layout.measurement, 10.0);
        near(layout.dimension_start.y - 1e12, 2.0);
        near(layout.dimension_end.y - 1e12, 2.0);
    }

    #[test]
    fn exactly_perpendicular_rotated_dimensions_are_zero_length() {
        for (angle, b) in [
            (90.0, [20.0, 0.0]),
            (270.0, [20.0, 0.0]),
            (45.0, [-20.0, 20.0]),
        ] {
            assert!(
                LinearDimension::new(
                    LinearDimensionKind::Rotated { angle_deg: angle },
                    Point::default(),
                    Point::new(b[0], b[1]),
                    Point::new(10.0, 10.0)
                )
                .is_err()
            );
        }
    }

    #[test]
    fn extensions_follow_negative_dimline_placement_with_style_gap() {
        let mut dimension = linear(
            LinearDimensionKind::Horizontal,
            [0.0, 0.0],
            [30.0, 0.0],
            [0.0, -10.0],
        );
        dimension.style.extension_offset = 1.0;
        dimension.style.extension_overshoot = 2.0;
        let layout = dimension.layout().unwrap();
        let Geometry::Line { start, end } = &layout.primitives[1] else {
            panic!("first extension is a line");
        };
        assert_eq!(*start, Point::new(0.0, -1.0));
        assert_eq!(*end, Point::new(0.0, -12.0));
    }

    #[test]
    fn reversed_aligned_dimensions_keep_text_upright() {
        for (a, b, expected) in [
            ([30.0, 40.0], [0.0, 0.0], 53.13010235415598),
            ([0.0, 20.0], [0.0, 0.0], 90.0),
        ] {
            let dimension = linear(LinearDimensionKind::Aligned, a, b, [30.0, 30.0]);
            near(dimension.layout().unwrap().text_rotation_deg, expected);
        }
    }

    #[test]
    fn tiny_aligned_dimensions_do_not_overflow_direction_normalization() {
        let dimension = linear(
            LinearDimensionKind::Aligned,
            [0.0, 0.0],
            [1e-320, 0.0],
            [0.0, 5.0],
        );
        assert_eq!(dimension.measurement().unwrap(), 1e-320);
        dimension.layout().unwrap();
    }

    #[test]
    fn semantic_json_rejects_unrecognized_dimension_fields() {
        let dimension = Dimension::Linear(linear(
            LinearDimensionKind::Horizontal,
            [0.0, 0.0],
            [20.0, 0.0],
            [0.0, 10.0],
        ));
        let mut value = serde_json::to_value(&dimension).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unrecognized_field".into(), true.into());
        assert!(serde_json::from_value::<Dimension>(value).is_err());
    }

    #[test]
    fn radial_transforms_scale_true_radius_and_explicit_text_placement() {
        let mut dimension =
            RadialDimension::new(Point::new(1.0, 2.0), Point::new(4.0, 6.0), false).unwrap();
        dimension.text_position = Some(Point::new(10.0, 20.0));
        dimension.leader_length = 3.0;
        let transformed = dimension
            .transformed(Transform2d::uniform_scale(3.0, Point::default()))
            .unwrap();
        near(transformed.measurement().unwrap(), 15.0);
        near(transformed.leader_length, 9.0);
        assert_eq!(transformed.text_position, Some(Point::new(30.0, 60.0)));
    }
}
