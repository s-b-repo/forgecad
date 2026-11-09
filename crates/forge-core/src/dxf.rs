//! A strict 2D ASCII DXF subset using Autodesk's documented group codes.
//! Unsupported geometry and attributes are reported instead of silently discarded.
use crate::{BlockDefinition, Document, Entity, Geometry, Point, PolylineVertex, blocks};
use std::{collections::BTreeMap, fmt::Write as _, io};

#[derive(Clone, Debug)]
pub struct ImportReport {
    pub document: Document,
    pub unsupported_entities: BTreeMap<String, usize>,
    pub unsupported_features: Vec<String>,
}
impl ImportReport {
    pub fn summary(&self) -> String {
        self.unsupported_entities
            .iter()
            .map(|(kind, count)| format!("{count} {kind}"))
            .chain(self.unsupported_features.iter().cloned())
            .collect::<Vec<_>>()
            .join("; ")
    }
    fn unsupported(&mut self, value: impl Into<String>) {
        let value = value.into();
        if !self.unsupported_features.contains(&value) {
            self.unsupported_features.push(value);
        }
    }
}
#[derive(Debug)]
struct Group<'a> {
    code: i16,
    value: &'a str,
    line: usize,
}
#[derive(Debug)]
struct Record<'a> {
    kind: &'a str,
    fields: &'a [Group<'a>],
}
impl Record<'_> {
    fn value(&self, code: i16) -> Option<&str> {
        self.fields
            .iter()
            .find(|group| group.code == code)
            .map(|group| group.value)
    }
    fn required(&self, code: i16) -> io::Result<&str> {
        self.value(code)
            .ok_or_else(|| invalid(format!("{} is missing group code {code}", self.kind)))
    }
    fn number(&self, code: i16) -> io::Result<f64> {
        parse_number(self.required(code)?, self.kind, code)
    }
    fn optional_number(&self, code: i16, default: f64) -> io::Result<f64> {
        self.value(code)
            .map(|value| parse_number(value, self.kind, code))
            .unwrap_or(Ok(default))
    }
    fn integer(&self, code: i16, default: i32) -> io::Result<i32> {
        self.value(code)
            .map(|value| {
                value
                    .trim()
                    .parse::<i32>()
                    .map_err(|_| invalid(format!("{} group {code} is not an integer", self.kind)))
            })
            .unwrap_or(Ok(default))
    }
    fn point(&self, x: i16, y: i16) -> io::Result<Point> {
        Ok(Point::new(self.number(x)?, self.number(y)?))
    }
}

pub fn parse(source: &str) -> io::Result<ImportReport> {
    if source.starts_with("AutoCAD Binary DXF") {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Binary DXF is not supported",
        ));
    }
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let lines: Vec<&str> = source.lines().collect();
    if lines.len() % 2 != 0 {
        return Err(invalid("DXF must contain complete group-code/value pairs"));
    }
    let mut groups = Vec::with_capacity(lines.len() / 2);
    for (index, pair) in lines.chunks_exact(2).enumerate() {
        let code = pair[0]
            .trim()
            .parse::<i16>()
            .map_err(|_| invalid(format!("Invalid DXF group code at line {}", index * 2 + 1)))?;
        if !(-5..=1071).contains(&code) {
            return Err(invalid(format!(
                "DXF group code {code} is outside the documented range"
            )));
        }
        groups.push(Group {
            code,
            value: pair[1].trim_end_matches('\r'),
            line: index * 2 + 2,
        });
    }
    let mut records = Vec::new();
    let mut index = 0;
    while index < groups.len() {
        if groups[index].code == 999 {
            index += 1;
            continue;
        }
        if groups[index].code != 0 {
            return Err(invalid(format!(
                "Expected a DXF record at line {}",
                groups[index].line
            )));
        }
        let start = index;
        index += 1;
        while index < groups.len() && groups[index].code != 0 {
            index += 1;
        }
        records.push(Record {
            kind: groups[start].value.trim(),
            fields: &groups[start + 1..index],
        });
    }
    let mut report = ImportReport {
        document: Document::new("Imported DXF"),
        unsupported_entities: BTreeMap::new(),
        unsupported_features: Vec::new(),
    };
    let mut section = None;
    let mut saw_entities = false;
    let mut saw_eof = false;
    let mut index = 0;
    while index < records.len() {
        let record = &records[index];
        match record.kind {
            "SECTION" => {
                if section.is_some() {
                    return Err(invalid("Nested DXF SECTION"));
                }
                let name = record.required(2)?.trim();
                section = Some(name);
                if name == "HEADER" {
                    read_header(record, &mut report)?;
                }
                if name == "ENTITIES" {
                    saw_entities = true;
                }
            }
            "ENDSEC" => {
                if section.take().is_none() {
                    return Err(invalid("ENDSEC outside a section"));
                }
            }
            "EOF" => {
                if section.is_some() {
                    return Err(invalid("DXF section is missing ENDSEC"));
                }
                if index + 1 != records.len() {
                    return Err(invalid("DXF has records after EOF"));
                }
                saw_eof = true;
            }
            "LAYER" if section == Some("TABLES") => read_layer(record, &mut report)?,
            "BLOCK_RECORD" if section == Some("TABLES") => {
                if record.integer(70, 0)? != 0 {
                    report.unsupported("Per-block insertion units");
                }
            }
            "BLOCK" if section == Some("BLOCKS") => {
                let consumed = read_block(&records[index..], &mut report)?;
                index += consumed - 1;
            }
            _ if section == Some("ENTITIES") => {
                let geometry = if record.kind == "POLYLINE" {
                    let (geometry, consumed) =
                        read_legacy_polyline(&records[index..], &mut report)?;
                    index += consumed - 1;
                    geometry
                } else {
                    read_entity(record, &mut report)?
                };
                if let Some(geometry) = geometry {
                    geometry.validate()?;
                    let layer_name = record.value(8).unwrap_or("0").trim();
                    let id = ensure_layer(&mut report.document, layer_name)?;
                    report.document.active_layer_id = id;
                    report.document.add_geometry(geometry);
                }
            }
            _ => {}
        }
        index += 1;
    }
    if !saw_eof || !saw_entities {
        return Err(invalid("DXF requires an ENTITIES section and EOF marker"));
    }
    report.document.active_layer_id = report
        .document
        .layers
        .first()
        .map(|layer| layer.id)
        .unwrap_or(1);
    report.document.validate()?;
    Ok(report)
}

fn read_header(record: &Record<'_>, report: &mut ImportReport) -> io::Result<()> {
    for (index, group) in record.fields.iter().enumerate() {
        if group.code == 9 && group.value.trim() == "$INSUNITS" {
            let unit = record.fields[index + 1..]
                .iter()
                .take_while(|next| next.code != 9)
                .find(|next| next.code == 70)
                .ok_or_else(|| invalid("INSUNITS is missing its value"))?;
            report.document.units = unit
                .value
                .trim()
                .parse::<u16>()
                .map_err(|_| invalid("Invalid INSUNITS value"))?;
            if report.document.units > 24 {
                return Err(invalid("Unsupported INSUNITS code"));
            }
        }
    }
    Ok(())
}
fn read_layer(record: &Record<'_>, report: &mut ImportReport) -> io::Result<()> {
    let name = record.required(2)?.trim();
    let flags = record.integer(70, 0)?;
    let aci = record.integer(62, 7)?;
    if aci == 0 || aci.unsigned_abs() > 255 {
        return Err(invalid("LAYER color must have an ACI value from 1 to 255"));
    }
    let color = if let Some(value) = record.value(420) {
        let number = value
            .trim()
            .parse::<u32>()
            .map_err(|_| invalid("Invalid layer true color"))?;
        if number > 0xFFFFFF {
            return Err(invalid("Layer true color is outside 24-bit range"));
        }
        [(number >> 16) as u8, (number >> 8) as u8, number as u8]
    } else {
        aci_color(aci.unsigned_abs() as u16)
    };
    if flags & !64 != 0 {
        report.unsupported("Layer frozen/locked/xref flags");
    }
    if record
        .value(6)
        .is_some_and(|value| !value.trim().eq_ignore_ascii_case("CONTINUOUS"))
    {
        report.unsupported("Layer noncontinuous linetypes");
    }
    if record.integer(370, -3)? != -3 || record.integer(290, 1)? != 1 {
        report.unsupported("Layer lineweight/plot settings");
    }
    let visible = aci > 0 && flags & 1 == 0;
    let id = ensure_layer(&mut report.document, name)?;
    let layer = report
        .document
        .layers
        .iter_mut()
        .find(|layer| layer.id == id)
        .expect("layer exists");
    layer.color = color;
    layer.visible = visible;
    Ok(())
}
fn ensure_layer(document: &mut Document, name: &str) -> io::Result<u64> {
    if name.is_empty() || name.contains(['\n', '\r', '\0']) {
        return Err(invalid("Invalid DXF layer name"));
    }
    if let Some(layer) = document
        .layers
        .iter()
        .find(|layer| layer.name.eq_ignore_ascii_case(name))
    {
        return Ok(layer.id);
    }
    Ok(document.add_layer(name, [255, 255, 255]))
}
fn common_features(record: &Record<'_>, report: &mut ImportReport) -> io::Result<()> {
    for code in [30, 31, 32, 38, 39] {
        if record.optional_number(code, 0.0)?.abs() > 1e-10 {
            report.unsupported(format!("{} nonplanar/elevated/thick geometry", record.kind));
        }
    }
    for (code, default) in [(210, 0.0), (220, 0.0), (230, 1.0)] {
        if (record.optional_number(code, default)? - default).abs() > 1e-10 {
            report.unsupported(format!("{} nondefault extrusion direction", record.kind));
        }
    }
    if record.integer(67, 0)? != 0 {
        report.unsupported("Paper-space entities");
    }
    if record.integer(60, 0)? != 0 {
        report.unsupported("Per-entity visibility");
    }
    if record.value(420).is_some() || record.integer(62, 256)? != 256 {
        report.unsupported("Per-entity color overrides");
    }
    if record
        .value(6)
        .is_some_and(|value| !value.trim().eq_ignore_ascii_case("BYLAYER"))
    {
        report.unsupported("Per-entity linetypes");
    }
    if record.integer(370, -1)? != -1 || (record.optional_number(48, 1.0)? - 1.0).abs() > 1e-10 {
        report.unsupported("Per-entity lineweight/linetype scale");
    }
    if record
        .fields
        .iter()
        .any(|field| field.code >= 1000 || matches!(field.code, 102 | 360 | 440 | 347 | 390))
    {
        report.unsupported("Entity extended metadata, transparency, or material settings");
    }
    Ok(())
}
fn read_entity(record: &Record<'_>, report: &mut ImportReport) -> io::Result<Option<Geometry>> {
    if !matches!(
        record.kind,
        "LINE" | "CIRCLE" | "ARC" | "POINT" | "LWPOLYLINE" | "TEXT" | "INSERT"
    ) {
        *report
            .unsupported_entities
            .entry(record.kind.to_owned())
            .or_default() += 1;
        return Ok(None);
    }
    common_features(record, report)?;
    Ok(Some(match record.kind {
        "LINE" => Geometry::Line {
            start: record.point(10, 20)?,
            end: record.point(11, 21)?,
        },
        "CIRCLE" => Geometry::Circle {
            center: record.point(10, 20)?,
            radius: record.number(40)?,
        },
        "ARC" => Geometry::Arc {
            center: record.point(10, 20)?,
            radius: record.number(40)?,
            start_angle_deg: record.number(50)?,
            end_angle_deg: record.number(51)?,
        },
        "POINT" => Geometry::Point {
            position: record.point(10, 20)?,
        },
        "LWPOLYLINE" => read_lwpolyline(record, report)?,
        "INSERT" => {
            let x = record.optional_number(41, 1.0)?;
            let y = record.optional_number(42, 1.0)?;
            if x != y {
                report.unsupported("INSERT nonuniform XY scale");
            }
            if record.optional_number(43, 1.0)? != 1.0 {
                report.unsupported("INSERT nondefault Z scale");
            }
            if record.integer(70, 1)? != 1
                || record.integer(71, 1)? != 1
                || record.optional_number(44, 0.0)? != 0.0
                || record.optional_number(45, 0.0)? != 0.0
            {
                report.unsupported("INSERT row/column arrays");
            }
            if record.integer(66, 0)? != 0 {
                report.unsupported("INSERT variable attributes");
            }
            Geometry::Insert {
                block_name: record.required(2)?.trim().to_owned(),
                position: record.point(10, 20)?,
                scale: x,
                rotation_deg: record.optional_number(50, 0.0)?,
            }
        }
        "TEXT" => {
            if record.integer(72, 0)? != 0 || record.integer(73, 0)? != 0 {
                report.unsupported("TEXT nondefault alignment");
            }
            if record.integer(71, 0)? != 0
                || (record.optional_number(41, 1.0)? - 1.0).abs() > 1e-10
                || record.optional_number(51, 0.0)?.abs() > 1e-10
            {
                report.unsupported("TEXT width/oblique/mirroring");
            }
            if record
                .value(7)
                .is_some_and(|style| !style.trim().eq_ignore_ascii_case("STANDARD"))
            {
                report.unsupported("TEXT custom styles");
            }
            Geometry::Text {
                position: record.point(10, 20)?,
                text: record.required(1)?.to_owned(),
                height: record.number(40)?,
                rotation_deg: record.optional_number(50, 0.0)?,
            }
        }
        _ => unreachable!(),
    }))
}
fn read_lwpolyline(record: &Record<'_>, report: &mut ImportReport) -> io::Result<Geometry> {
    let flags = record.integer(70, 0)?;
    if flags & !129 != 0 {
        report.unsupported("LWPOLYLINE unsupported flags");
    }
    if record.optional_number(43, 0.0)?.abs() > 1e-10 {
        report.unsupported("LWPOLYLINE constant width");
    }
    let mut vertices = Vec::new();
    let mut x = None;
    let mut y = None;
    let mut bulge = 0.0;
    for group in record.fields {
        match group.code {
            10 => {
                if let Some(previous_x) = x.take() {
                    vertices.push(PolylineVertex {
                        point: Point::new(
                            previous_x,
                            y.take()
                                .ok_or_else(|| invalid("LWPOLYLINE vertex is missing Y"))?,
                        ),
                        bulge,
                    });
                }
                x = Some(parse_number(group.value, "LWPOLYLINE", 10)?);
                y = None;
                bulge = 0.0;
            }
            20 => {
                if x.is_none() || y.is_some() {
                    return Err(invalid("LWPOLYLINE vertex has mismatched coordinates"));
                }
                y = Some(parse_number(group.value, "LWPOLYLINE", 20)?);
            }
            42 => {
                if x.is_none() {
                    return Err(invalid("LWPOLYLINE bulge precedes its vertex"));
                }
                bulge = parse_number(group.value, "LWPOLYLINE", 42)?;
            }
            40 | 41 => {
                if parse_number(group.value, "LWPOLYLINE", group.code)?.abs() > 1e-10 {
                    report.unsupported("LWPOLYLINE variable widths");
                }
            }
            _ => {}
        }
    }
    if let Some(x) = x {
        vertices.push(PolylineVertex {
            point: Point::new(
                x,
                y.ok_or_else(|| invalid("LWPOLYLINE vertex is missing Y"))?,
            ),
            bulge,
        });
    }
    let declared = record.integer(90, -1)?;
    if declared < 0 || declared as usize != vertices.len() {
        return Err(invalid("LWPOLYLINE vertex count does not match its data"));
    }
    Ok(Geometry::Polyline {
        vertices,
        closed: flags & 1 != 0,
    })
}
fn read_legacy_polyline(
    records: &[Record<'_>],
    report: &mut ImportReport,
) -> io::Result<(Option<Geometry>, usize)> {
    let header = &records[0];
    common_features(header, report)?;
    let flags = header.integer(70, 0)?;
    for code in [40, 41] {
        if header.optional_number(code, 0.0)?.abs() > 1e-10 {
            report.unsupported("POLYLINE widths");
        }
    }
    if flags & !129 != 0 {
        report.unsupported("POLYLINE curve-fit, spline, mesh, or 3D flags");
    }
    let mut vertices = Vec::new();
    let mut index = 1;
    while index < records.len() && records[index].kind == "VERTEX" {
        let vertex = &records[index];
        common_features(vertex, report)?;
        if vertex.integer(70, 0)? != 0 {
            report.unsupported("POLYLINE nonstandard vertex flags");
        }
        for code in [40, 41] {
            if vertex.optional_number(code, 0.0)?.abs() > 1e-10 {
                report.unsupported("POLYLINE widths");
            }
        }
        vertices.push(PolylineVertex {
            point: vertex.point(10, 20)?,
            bulge: vertex.optional_number(42, 0.0)?,
        });
        index += 1;
    }
    if index >= records.len() || records[index].kind != "SEQEND" {
        return Err(invalid("POLYLINE is missing SEQEND"));
    }
    Ok((
        Some(Geometry::Polyline {
            vertices,
            closed: flags & 1 != 0,
        }),
        index + 1,
    ))
}

fn read_block(records: &[Record<'_>], report: &mut ImportReport) -> io::Result<usize> {
    let header = &records[0];
    let name = header.required(2)?.trim().to_owned();
    let base_point = Point::new(
        header.optional_number(10, 0.0)?,
        header.optional_number(20, 0.0)?,
    );
    common_features(header, report)?;
    let flags = header.integer(70, 0)?;
    if flags & !1 != 0 || header.value(1).is_some_and(|path| !path.trim().is_empty()) {
        report.unsupported(format!("BLOCK {name} attributes/xref/dependency flags"));
    }
    if flags & 1 != 0 && !name.starts_with('*') {
        report.unsupported("BLOCK nonstandard anonymous name");
    }
    if header
        .value(4)
        .is_some_and(|description| !description.is_empty())
    {
        report.unsupported("BLOCK descriptions");
    }
    let mut entities = Vec::new();
    let mut index = 1;
    while index < records.len() && records[index].kind != "ENDBLK" {
        let record = &records[index];
        if matches!(record.kind, "ENDSEC" | "BLOCK" | "EOF") {
            return Err(invalid("BLOCK is missing ENDBLK"));
        }
        let geometry = if record.kind == "POLYLINE" {
            let (geometry, consumed) = read_legacy_polyline(&records[index..], report)?;
            index += consumed - 1;
            geometry
        } else {
            read_entity(record, report)?
        };
        if let Some(geometry) = geometry {
            geometry.validate()?;
            let layer = ensure_layer(&mut report.document, record.value(8).unwrap_or("0").trim())?;
            let id = report.document.allocate_id();
            entities.push(Entity {
                id,
                layer_id: layer,
                geometry,
            });
        }
        index += 1;
    }
    if index >= records.len() {
        return Err(invalid("BLOCK is missing ENDBLK"));
    }
    if blocks::is_layout_block(&name) {
        if !entities.is_empty() {
            report.unsupported("Nonempty layout block definitions");
        }
    } else {
        report.document.blocks.push(BlockDefinition {
            name,
            base_point,
            entities,
        });
    }
    Ok(index + 1)
}

/// UTF-8 AutoCAD 2007 ASCII DXF with handles, owner links, and layer tables.
pub fn write(document: &Document) -> io::Result<String> {
    document.validate()?;
    if !document.solids.is_empty() || !document.meshes.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "DXF export supports drafting entities; use STL or OBJ for model geometry",
        ));
    }
    for layer in &document.layers {
        if layer.name.chars().count() > 255 {
            return Err(invalid("DXF layer names cannot exceed 255 characters"));
        }
    }
    for entity in document.entities.iter().chain(
        document
            .blocks
            .iter()
            .flat_map(|block| block.entities.iter()),
    ) {
        if let Geometry::Text { text, .. } = &entity.geometry {
            if text.chars().count() > 255 {
                return Err(invalid(
                    "DXF TEXT cannot exceed 255 characters; it will not be truncated",
                ));
            }
        }
    }
    let mut output = String::new();
    let mut handle = 1u64;
    pair(&mut output, 0, "SECTION");
    pair(&mut output, 2, "HEADER");
    pair(&mut output, 9, "$ACADVER");
    pair(&mut output, 1, "AC1021");
    pair(&mut output, 9, "$INSUNITS");
    pair(&mut output, 70, document.units);
    pair(&mut output, 9, "$HANDSEED");
    pair(&mut output, 5, "FFFFFF");
    pair(&mut output, 0, "ENDSEC");
    pair(&mut output, 0, "SECTION");
    pair(&mut output, 2, "TABLES");
    let ltype_table = next_handle(&mut handle);
    table_start(&mut output, "LTYPE", &ltype_table, 1);
    record_start(
        &mut output,
        "LTYPE",
        &next_handle(&mut handle),
        Some(&ltype_table),
    );
    pair(&mut output, 100, "AcDbSymbolTableRecord");
    pair(&mut output, 100, "AcDbLinetypeTableRecord");
    pair(&mut output, 2, "CONTINUOUS");
    pair(&mut output, 70, 0);
    pair(&mut output, 3, "Solid line");
    pair(&mut output, 72, 65);
    pair(&mut output, 73, 0);
    pair(&mut output, 40, 0);
    pair(&mut output, 0, "ENDTAB");
    let layer_table = next_handle(&mut handle);
    table_start(&mut output, "LAYER", &layer_table, document.layers.len());
    for layer in &document.layers {
        record_start(
            &mut output,
            "LAYER",
            &next_handle(&mut handle),
            Some(&layer_table),
        );
        pair(&mut output, 100, "AcDbSymbolTableRecord");
        pair(&mut output, 100, "AcDbLayerTableRecord");
        pair(&mut output, 2, &layer.name);
        pair(&mut output, 70, 0);
        let aci = nearest_aci(layer.color) as i32;
        pair(&mut output, 62, if layer.visible { aci } else { -aci });
        pair(
            &mut output,
            420,
            ((layer.color[0] as u32) << 16)
                | ((layer.color[1] as u32) << 8)
                | layer.color[2] as u32,
        );
        pair(&mut output, 6, "CONTINUOUS");
    }
    pair(&mut output, 0, "ENDTAB");
    let style_table = next_handle(&mut handle);
    table_start(&mut output, "STYLE", &style_table, 1);
    record_start(
        &mut output,
        "STYLE",
        &next_handle(&mut handle),
        Some(&style_table),
    );
    pair(&mut output, 100, "AcDbSymbolTableRecord");
    pair(&mut output, 100, "AcDbTextStyleTableRecord");
    pair(&mut output, 2, "STANDARD");
    pair(&mut output, 70, 0);
    pair(&mut output, 40, 0);
    pair(&mut output, 41, 1);
    pair(&mut output, 50, 0);
    pair(&mut output, 71, 0);
    pair(&mut output, 42, 2.5);
    pair(&mut output, 3, "txt");
    pair(&mut output, 4, "");
    pair(&mut output, 0, "ENDTAB");
    let block_table = next_handle(&mut handle);
    table_start(
        &mut output,
        "BLOCK_RECORD",
        &block_table,
        2 + document.blocks.len(),
    );
    let model_record = next_handle(&mut handle);
    let paper_record = next_handle(&mut handle);
    let mut block_handles = BTreeMap::new();
    for block in &document.blocks {
        block_handles.insert(block.name.clone(), next_handle(&mut handle));
    }
    for (name, id) in [
        ("*Model_Space", &model_record),
        ("*Paper_Space", &paper_record),
    ]
    .into_iter()
    .chain(block_handles.iter().map(|(name, id)| (name.as_str(), id)))
    {
        record_start(&mut output, "BLOCK_RECORD", id, Some(&block_table));
        pair(&mut output, 100, "AcDbSymbolTableRecord");
        pair(&mut output, 100, "AcDbBlockTableRecord");
        pair(&mut output, 2, name);
        pair(&mut output, 70, 0);
    }
    pair(&mut output, 0, "ENDTAB");
    pair(&mut output, 0, "ENDSEC");
    pair(&mut output, 0, "SECTION");
    pair(&mut output, 2, "BLOCKS");
    let default_layer = &document.layers[0].name;
    for (name, id) in [
        ("*Model_Space", &model_record),
        ("*Paper_Space", &paper_record),
    ] {
        block_start(
            &mut output,
            name,
            Point::default(),
            id,
            &mut handle,
            default_layer,
        );
        block_end(&mut output, id, &mut handle, default_layer);
    }
    for block in &document.blocks {
        let owner = &block_handles[&block.name];
        block_start(
            &mut output,
            &block.name,
            block.base_point,
            owner,
            &mut handle,
            default_layer,
        );
        for entity in &block.entities {
            write_entity(&mut output, document, entity, owner, &mut handle)?;
        }
        block_end(&mut output, owner, &mut handle, default_layer);
    }
    pair(&mut output, 0, "ENDSEC");
    pair(&mut output, 0, "SECTION");
    pair(&mut output, 2, "ENTITIES");
    for entity in &document.entities {
        write_entity(&mut output, document, entity, &model_record, &mut handle)?;
    }
    pair(&mut output, 0, "ENDSEC");
    pair(&mut output, 0, "EOF");
    Ok(output)
}
fn block_start(
    output: &mut String,
    name: &str,
    base: Point,
    owner: &str,
    handle: &mut u64,
    layer: &str,
) {
    record_start(output, "BLOCK", &next_handle(handle), Some(owner));
    pair(output, 100, "AcDbEntity");
    pair(output, 8, layer);
    pair(output, 100, "AcDbBlockBegin");
    pair(output, 2, name);
    pair(
        output,
        70,
        if name.starts_with('*') && !blocks::is_layout_block(name) {
            1
        } else {
            0
        },
    );
    write_point(output, base);
    pair(output, 3, name);
    pair(output, 1, "");
}
fn block_end(output: &mut String, owner: &str, handle: &mut u64, layer: &str) {
    record_start(output, "ENDBLK", &next_handle(handle), Some(owner));
    pair(output, 100, "AcDbEntity");
    pair(output, 8, layer);
    pair(output, 100, "AcDbBlockEnd");
}
fn write_entity(
    output: &mut String,
    document: &Document,
    entity: &Entity,
    owner: &str,
    handle: &mut u64,
) -> io::Result<()> {
    let layer = &document
        .layer(entity.layer_id)
        .expect("validated layer")
        .name;
    let kind = match entity.geometry {
        Geometry::Line { .. } => "LINE",
        Geometry::Rectangle { .. } | Geometry::Polyline { .. } => "LWPOLYLINE",
        Geometry::Circle { .. } => "CIRCLE",
        Geometry::Arc { .. } => "ARC",
        Geometry::Point { .. } => "POINT",
        Geometry::Text { .. } => "TEXT",
        Geometry::Insert { .. } => "INSERT",
        Geometry::Dimension { .. } => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Semantic DIMENSION DXF export is not implemented; annotation is retained in native projects",
            ));
        }
    };
    record_start(output, kind, &next_handle(handle), Some(owner));
    pair(output, 100, "AcDbEntity");
    pair(output, 8, layer);
    match &entity.geometry {
        Geometry::Dimension { .. } => unreachable!("dimensions rejected above"),
        Geometry::Line { start, end } => {
            pair(output, 100, "AcDbLine");
            write_point(output, *start);
            pair(output, 11, end.x);
            pair(output, 21, end.y);
            pair(output, 31, 0);
        }
        Geometry::Circle { center, radius } => {
            pair(output, 100, "AcDbCircle");
            write_point(output, *center);
            pair(output, 40, *radius);
        }
        Geometry::Arc {
            center,
            radius,
            start_angle_deg,
            end_angle_deg,
        } => {
            pair(output, 100, "AcDbCircle");
            write_point(output, *center);
            pair(output, 40, *radius);
            pair(output, 100, "AcDbArc");
            pair(output, 50, *start_angle_deg);
            pair(output, 51, *end_angle_deg);
        }
        Geometry::Point { position } => {
            pair(output, 100, "AcDbPoint");
            write_point(output, *position);
        }
        Geometry::Rectangle { a, b } => {
            pair(output, 100, "AcDbPolyline");
            pair(output, 90, 4);
            pair(output, 70, 1);
            for point in [*a, Point::new(b.x, a.y), *b, Point::new(a.x, b.y)] {
                pair(output, 10, point.x);
                pair(output, 20, point.y);
            }
        }
        Geometry::Polyline { vertices, closed } => {
            pair(output, 100, "AcDbPolyline");
            pair(output, 90, vertices.len());
            pair(output, 70, if *closed { 1 } else { 0 });
            for vertex in vertices {
                pair(output, 10, vertex.point.x);
                pair(output, 20, vertex.point.y);
                if vertex.bulge != 0.0 {
                    pair(output, 42, vertex.bulge);
                }
            }
        }
        Geometry::Text {
            position,
            text,
            height,
            rotation_deg,
        } => {
            pair(output, 100, "AcDbText");
            write_point(output, *position);
            pair(output, 40, *height);
            pair(output, 1, text);
            pair(output, 50, *rotation_deg);
            pair(output, 41, 1);
            pair(output, 7, "STANDARD");
            pair(output, 72, 0);
            pair(output, 100, "AcDbText");
            pair(output, 73, 0);
        }
        Geometry::Insert {
            block_name,
            position,
            scale,
            rotation_deg,
        } => {
            pair(output, 100, "AcDbBlockReference");
            pair(output, 2, block_name);
            write_point(output, *position);
            pair(output, 41, *scale);
            pair(output, 42, *scale);
            pair(output, 43, 1);
            pair(output, 50, *rotation_deg);
        }
    }
    Ok(())
}
fn pair(output: &mut String, code: i16, value: impl std::fmt::Display) {
    let _ = writeln!(output, "{code}\n{value}");
}
fn next_handle(handle: &mut u64) -> String {
    let value = format!("{:X}", *handle);
    *handle += 1;
    value
}
fn record_start(output: &mut String, kind: &str, handle: &str, owner: Option<&str>) {
    pair(output, 0, kind);
    pair(output, 5, handle);
    if let Some(owner) = owner {
        pair(output, 330, owner);
    }
}
fn table_start(output: &mut String, name: &str, handle: &str, count: usize) {
    record_start(output, "TABLE", handle, None);
    pair(output, 2, name);
    pair(output, 100, "AcDbSymbolTable");
    pair(output, 70, count);
}
fn write_point(output: &mut String, point: Point) {
    pair(output, 10, point.x);
    pair(output, 20, point.y);
    pair(output, 30, 0);
}
fn parse_number(value: &str, kind: &str, code: i16) -> io::Result<f64> {
    let number = value
        .trim()
        .parse::<f64>()
        .map_err(|_| invalid(format!("{kind} group {code} is not a number")))?;
    if !number.is_finite() {
        return Err(invalid(format!("{kind} group {code} is not finite")));
    }
    Ok(number)
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn nearest_aci(color: [u8; 3]) -> u16 {
    (1..=255)
        .min_by_key(|index| {
            let aci = aci_color(*index);
            (0..3)
                .map(|axis| {
                    let d = color[axis] as i32 - aci[axis] as i32;
                    d * d
                })
                .sum::<i32>()
        })
        .unwrap_or(7)
}
fn aci_color(index: u16) -> [u8; 3] {
    match index {
        1 => [255, 0, 0],
        2 => [255, 255, 0],
        3 => [0, 255, 0],
        4 => [0, 255, 255],
        5 => [0, 0, 255],
        6 => [255, 0, 255],
        7 => [255, 255, 255],
        8 => [128, 128, 128],
        9 => [192, 192, 192],
        10..=249 => {
            let group = (index - 10) / 10;
            let shade = (index - 10) % 10;
            let value = [255.0, 165.0, 127.0, 76.0, 38.0][shade as usize / 2];
            let saturation = if shade % 2 == 0 { 1.0 } else { 0.5 };
            let hue = group as f64 / 4.0;
            let sector = hue.floor() as usize;
            let fraction = hue - hue.floor();
            let p = value * (1.0 - saturation);
            let q = value * (1.0 - saturation * fraction);
            let t = value * (1.0 - saturation * (1.0 - fraction));
            let rgb = match sector % 6 {
                0 => [value, t, p],
                1 => [q, value, p],
                2 => [p, value, t],
                3 => [p, q, value],
                4 => [t, p, value],
                _ => [value, p, q],
            };
            rgb.map(|channel| channel.floor() as u8)
        }
        250..=255 => {
            let gray = [51, 80, 105, 130, 190, 255][index as usize - 250];
            [gray; 3]
        }
        _ => [255, 255, 255],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = include_str!("../tests/fixtures/drawing_subset.dxf");
    const LEGACY: &str = include_str!("../tests/fixtures/legacy_polyline.dxf");
    fn drawing(entities: &str) -> String {
        format!("0\nSECTION\n2\nENTITIES\n{entities}0\nENDSEC\n0\nEOF\n")
    }

    #[test]
    fn independent_fixture_reads_units_layers_geometry_and_text_spacing() {
        let report = parse(FIXTURE).unwrap();
        assert!(report.unsupported_entities.is_empty());
        assert!(report.unsupported_features.is_empty());
        assert_eq!(report.document.units, 4);
        assert_eq!(report.document.entities.len(), 6);
        let layer = report
            .document
            .layers
            .iter()
            .find(|layer| layer.name == "Construction")
            .unwrap();
        assert_eq!(layer.color, [0x12, 0x34, 0x56]);
        assert!(!layer.visible);
        assert!(
            matches!(&report.document.entities[5].geometry, Geometry::Text { text, rotation_deg:30.0, .. } if text == "  Dimension ± 0.2  ")
        );
        assert!(
            matches!(&report.document.entities[4].geometry, Geometry::Polyline { vertices,closed:true } if vertices.len()==3 && (vertices[0].bulge-0.41421356237309503).abs()<1e-15)
        );
    }
    #[test]
    fn writer_roundtrips_supported_geometry_and_exact_layer_rgb() {
        let original = parse(FIXTURE).unwrap().document;
        let output = write(&original).unwrap();
        assert!(output.contains("AC1021"));
        let report = parse(&output).unwrap();
        assert!(
            report.unsupported_features.is_empty(),
            "{}",
            report.summary()
        );
        assert_eq!(report.document.units, original.units);
        assert_eq!(
            report
                .document
                .entities
                .iter()
                .map(|entity| &entity.geometry)
                .collect::<Vec<_>>(),
            original
                .entities
                .iter()
                .map(|entity| &entity.geometry)
                .collect::<Vec<_>>()
        );
        for layer in &original.layers {
            let reloaded = report
                .document
                .layers
                .iter()
                .find(|other| other.name == layer.name)
                .unwrap();
            assert_eq!(
                (reloaded.color, reloaded.visible),
                (layer.color, layer.visible)
            );
        }
    }
    #[test]
    fn legacy_vertices_preserve_closure_and_signed_bulge() {
        let report = parse(LEGACY).unwrap();
        assert!(report.unsupported_entities.is_empty());
        assert!(report.unsupported_features.is_empty());
        assert_eq!(report.document.entities.len(), 1);
        assert!(
            matches!(&report.document.entities[0].geometry, Geometry::Polyline { vertices, closed:true } if vertices[0].bulge == -1.0 && vertices.len()==3)
        );
    }
    #[test]
    fn malformed_pair_coordinates_counts_and_numbers_are_errors() {
        for input in [
            "0\nSECTION\n2",
            "garbage\nSECTION\n",
            &drawing("0\nLWPOLYLINE\n90\n2\n10\n1\n20\n2\n"),
            &drawing("0\nLWPOLYLINE\n90\n2\n10\n1\n10\n2\n20\n3\n"),
            &drawing("0\nCIRCLE\n10\n1\n20\n2\n40\n-1\n"),
            &drawing("0\nLINE\n10\nNaN\n20\n0\n11\n1\n21\n2\n"),
            &drawing("0\nPOLYLINE\n70\n0\n0\nVERTEX\n10\n0\n20\n1\n"),
        ] {
            assert!(parse(input).is_err(), "unexpectedly accepted {input:?}");
        }
    }
    #[test]
    fn unsupported_entities_and_nonplanar_attributes_are_reported() {
        let source = drawing(
            "0\nSPLINE\n8\n0\n0\nHATCH\n2\nPattern\n0\nLINE\n10\n0\n20\n0\n30\n9\n11\n1\n21\n1\n210\n1\n220\n0\n230\n0\n",
        );
        let report = parse(&source).unwrap();
        assert_eq!(report.unsupported_entities.get("SPLINE"), Some(&1));
        assert_eq!(report.unsupported_entities.get("HATCH"), Some(&1));
        assert!(
            report
                .unsupported_features
                .iter()
                .any(|warning| warning.contains("nonplanar"))
        );
        assert!(
            report
                .unsupported_features
                .iter()
                .any(|warning| warning.contains("extrusion"))
        );
    }
    #[test]
    fn text_alignment_and_polyline_widths_are_reported() {
        let source = drawing(
            "0\nTEXT\n10\n0\n20\n0\n40\n3\n1\nHello\n72\n1\n0\nLWPOLYLINE\n90\n2\n43\n4\n10\n0\n20\n0\n10\n1\n20\n1\n",
        );
        let report = parse(&source).unwrap();
        assert!(
            report
                .unsupported_features
                .iter()
                .any(|value| value.contains("alignment"))
        );
        assert!(
            report
                .unsupported_features
                .iter()
                .any(|value| value.contains("constant width"))
        );
    }
    #[test]
    fn sections_prevent_block_definition_geometry_being_imported_as_instances() {
        let source = "0\nSECTION\n2\nBLOCKS\n0\nLINE\n10\n0\n20\n0\n11\n1\n21\n1\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n";
        assert!(parse(source).unwrap().document.entities.is_empty());
        assert!(parse(&drawing("").replace("0\nEOF\n", "")).is_err());
    }
    #[test]
    fn rectangle_exports_as_closed_polyline_without_losing_extents() {
        let mut document = Document::default();
        document.add_geometry(Geometry::Rectangle {
            a: Point::new(-3.0, 7.0),
            b: Point::new(4.0, -2.0),
        });
        let reloaded = parse(&write(&document).unwrap()).unwrap().document;
        assert!(
            matches!(&reloaded.entities[0].geometry,Geometry::Polyline { vertices,closed:true } if vertices.len()==4 && vertices[0].point == Point::new(-3.0,7.0) && vertices[2].point==Point::new(4.0,-2.0))
        );
    }
    #[test]
    fn writer_refuses_to_discard_model_content() {
        let mut document = Document::default();
        document.add_solid("Box", crate::SolidKind::Box);
        assert_eq!(
            write(&document).unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
    }
}

#[cfg(test)]
mod block_tests {
    use super::*;
    const FIXTURE: &str = include_str!("../tests/fixtures/nested_blocks.dxf");
    #[test]
    fn independently_authored_nested_blocks_preserve_definitions_and_instances() {
        let report = parse(FIXTURE).unwrap();
        assert!(report.unsupported_entities.is_empty());
        assert!(
            report.unsupported_features.is_empty(),
            "{}",
            report.summary()
        );
        let document = report.document;
        assert_eq!(document.blocks.len(), 2);
        assert_eq!(document.entities.len(), 1);
        assert!(
            matches!(&document.entities[0].geometry,Geometry::Insert{block_name,..} if block_name=="Assembly")
        );
        let leaves = document.expanded_entities().unwrap();
        assert_eq!(leaves.len(), 2);
        if let Geometry::Line { start, end } = leaves[0].geometry {
            assert!(start.distance(Point::new(10.0, 20.0)) < 1e-10);
            assert!(end.distance(Point::new(-2.0, 20.0)) < 1e-10);
        } else {
            panic!("expected line");
        }
        if let Geometry::Circle { center, radius } = leaves[1].geometry {
            assert!(center.distance(Point::new(4.0, 14.0)) < 1e-10);
            assert!((radius - 3.0).abs() < 1e-10);
        } else {
            panic!("expected circle");
        }
        let encoded = write(&document).unwrap();
        let reloaded = parse(&encoded).unwrap();
        assert!(
            reloaded.unsupported_features.is_empty(),
            "{}",
            reloaded.summary()
        );
        assert_eq!(
            reloaded
                .document
                .blocks
                .iter()
                .map(|block| (&block.name, block.base_point))
                .collect::<Vec<_>>(),
            document
                .blocks
                .iter()
                .map(|block| (&block.name, block.base_point))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            reloaded.document.entities[0].geometry,
            document.entities[0].geometry
        );
        let native = Document::from_json(&document.to_json().unwrap()).unwrap();
        assert_eq!(native, document);
    }
    #[test]
    fn nonuniform_arrays_attributes_zscale_and_xrefs_are_explicit_diagnostics() {
        let source = FIXTURE
            .replace(
                "41\n3\n42\n3\n50\n90",
                "41\n3\n42\n4\n43\n2\n66\n1\n70\n2\n50\n90",
            )
            .replace("2\nLeaf\n70\n0", "2\nLeaf\n70\n4\n1\nexternal.dwg");
        let report = parse(&source).unwrap();
        for expected in ["nonuniform", "Z scale", "attributes", "arrays", "xref"] {
            assert!(
                report
                    .unsupported_features
                    .iter()
                    .any(|warning| warning.contains(expected)),
                "missing {expected}: {}",
                report.summary()
            );
        }
    }
    #[test]
    fn missing_definition_cycle_and_truncated_block_are_rejected() {
        assert!(parse(&FIXTURE.replace("2\nLeaf\n10\n5", "2\nMissing\n10\n5")).is_err());
        assert!(parse(&FIXTURE.replace("2\nLeaf\n10\n5", "2\nAssembly\n10\n5")).is_err());
        assert!(parse(&FIXTURE.replacen("0\nENDBLK\n", "", 1)).is_err());
    }
    #[test]
    fn all_output_handles_are_unique_and_owner_references_resolve() {
        let document = parse(FIXTURE).unwrap().document;
        let text = write(&document).unwrap();
        let lines: Vec<_> = text.lines().collect();
        let groups: Vec<_> = lines
            .chunks_exact(2)
            .map(|pair| (pair[0], pair[1]))
            .collect();
        let mut handles = std::collections::BTreeSet::new();
        for (code, value) in &groups {
            if *code == "5" && *value != "FFFFFF" {
                assert!(handles.insert(*value), "duplicate handle {value}");
            }
        }
        for (code, value) in &groups {
            if *code == "330" {
                assert!(handles.contains(value), "missing owner {value}");
            }
        }
    }
}
