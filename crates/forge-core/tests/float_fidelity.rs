//! Bit-exact float fidelity through the project serialisation format.
//!
//! Finding (diagnosed on native Linux, not just under Miri): serde_json's
//! default float parser (`f64_from_parts`, non-`float_roundtrip` path) recomputes
//! `significand as f64` times powers of ten and loses 1 ulp for many
//! shortest-representation decimals near 1.0 and around 1e-16. `str::parse::<f64>()`
//! is correct. Because every project coordinate is stored through serde_json,
//! reopening a saved project could silently shift a coordinate by 1 ulp — a real
//! data-fidelity defect for a CAD tool.
//!
//! Fix: `serde_json` is now built with the `float_roundtrip` feature (see the
//! root `Cargo.toml`), which routes parsing through the correctly rounded
//! `lexical` path for both the fast and the long parse paths. These tests lock
//! that fix in: before the feature was enabled they fail outright on several
//! sweep points, so they must never be weakened to make a regression pass.
use forge_core::{Document, Geometry, Point};

/// Magnitudes whose shortest decimal representation previously parsed back
/// 1 ulp low (verified against `str::parse::<f64>()`). The literals are the
/// diagnosed decimal texts verbatim, so they are kept at full precision even
/// when some of them denote a value that has a shorter spelling.
#[allow(clippy::excessive_precision)]
fn diagnosed() -> Vec<f64> {
    vec![
        0.9999999999999993,
        0.9999999999999999,
        1.0009999999999998,
        1.0000000000000007,
        3.0616169978683826e-16,
        6.123233995736765e-17,
        0.9999999999999992,
    ]
}

/// 200 generated values straddling the two diagnosed bands: one ulp steps
/// around 1.0 and 1e-16 steps around the sub-normal-adjacent decade.
fn sweep() -> Vec<f64> {
    let mut values = Vec::with_capacity(200);
    for index in 1..=100 {
        values.push(1.0 + index as f64 * 2f64.powi(-53));
    }
    for index in 1..=100 {
        values.push(index as f64 * 1e-16);
    }
    values
}

fn assert_bit_exact(value: f64, label: &str) {
    let text = serde_json::to_string(&value).expect("serialize float");
    let parsed: f64 = serde_json::from_str(&text).expect("parse float");
    assert_eq!(
        parsed.to_bits(),
        value.to_bits(),
        "{label}: {value:?} round-tripped through JSON as {parsed:?} (text {text})"
    );
}

#[test]
fn diagnosed_magnitudes_round_trip_bit_exactly() {
    for (index, value) in diagnosed().into_iter().enumerate() {
        assert_bit_exact(value, &format!("diagnosed[{index}]"));
    }
}

#[test]
fn generated_sweep_round_trips_bit_exactly() {
    for (index, value) in sweep().into_iter().enumerate() {
        assert_bit_exact(value, &format!("sweep[{index}]"));
    }
}

#[test]
fn project_document_round_trip_preserves_coordinate_bits() {
    let values = diagnosed();
    let mut document = Document::new("float fidelity");
    document
        .add_geometry(Geometry::Line {
            start: Point::new(values[0], values[1]),
            end: Point::new(values[2], values[3]),
        })
        .expect("first line");
    // Carry the remaining diagnosed magnitudes through the same project file so
    // every lossy value is asserted at document level, not just in isolation.
    document
        .add_geometry(Geometry::Line {
            start: Point::new(values[4], values[5]),
            end: Point::new(values[6], values[0]),
        })
        .expect("second line");

    let text = document.to_json().expect("serialize project");
    let reopened = Document::from_json(&text).expect("reopen project");

    // Exact document equality: no tolerance is allowed for a coordinate that was
    // already representable before the file was written.
    assert_eq!(reopened, document);

    let original = &document.entities[0];
    let Geometry::Line { start, end } = original.geometry else {
        panic!("first entity is a line")
    };
    let restored = reopened
        .entities
        .iter()
        .find(|entity| entity.id == original.id)
        .expect("first entity survives the round trip");
    let Geometry::Line {
        start: start_restored,
        end: end_restored,
    } = restored.geometry
    else {
        panic!("restored entity is a line")
    };
    for (coord, restored, label) in [
        (start.x, start_restored.x, "start.x"),
        (start.y, start_restored.y, "start.y"),
        (end.x, end_restored.x, "end.x"),
        (end.y, end_restored.y, "end.y"),
    ] {
        assert_eq!(
            restored.to_bits(),
            coord.to_bits(),
            "entities[0].geometry {label} shifted: {coord:?} -> {restored:?}"
        );
    }
}
