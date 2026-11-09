# Forge Studio

An independent, MIT-licensed Rust project pursuing a counterpart for every Autodesk product supplied in the original catalog, including complete features and native format compatibility. The goal is active and unfinished. The repository retains **140 individual product targets**, including AutoCAD, Inventor, Revit, Fusion, Maya, 3ds Max, and Civil 3D. Target status is explicitly prototype or planned.

## Run the built Linux applications

```sh
./target/desktop/debug/forge-studio --product autocad --open examples/office-plan.dxf
./target/desktop/debug/forge-studio --product inventor --open examples/mesh-box.obj
./target/desktop/debug/forge products
```

The Products selector opens the full searchable catalog. Each target keeps its own identity; shared work areas provide the currently implemented tools. BIM, civil, manufacturing, animation and service-specific workflows remain incomplete.

## Build from source

```sh
cargo run -p forge-studio -- --product autocad
cargo run -p forge-cli -- products --json
cargo test --workspace --all-targets
cargo build --workspace --bins
```

Rust 1.95+ and the platform's graphics development libraries are required. The desktop uses native eframe/egui. Family entry binaries remain available as draftcraft, modelcraft, buildcraft, terraincraft, makecraft, motioncraft and connectcraft.

## Implemented workflows

- Drafting: lines, rectangles, circles, polylines, center/start/end-angle arcs, text and points; grid/object snapping; multi-selection; layer visibility/assignment; move/copy/rotate/uniform-scale; undo/redo; fit/pan/zoom; native save/open and unsaved-change decisions.
- Blocks: reusable definitions from a selection (⌃G), nested INSERT display and picking through expanded geometry, and one-transaction explosion back to ordinary entities.
- Annotation: semantic horizontal, vertical, aligned and rotated linear dimensions plus radius and diameter dimensions, with extension lines, arrows and measured text derived from the stored references.
- Modeling: dimension/position editing for box, cylinder and ellipsoid primitives; orbit/pan/zoom; imported triangle meshes; native persistence; tessellated STL/OBJ export with explicit loss/replacement decisions.
- Exchange: a strict ASCII DXF subset including layers, units, basic text, arcs, curved polylines, blocks and INSERT references; ASCII/binary STL; geometry-only OBJ with polygon triangulation; native JSON v3, v2 and v1 migration.
- Automation: inspect, convert, transform, primitive creation, native DWG commands and individual-product catalog commands. Batch conversions refuse unapproved data loss and existing output replacement.

```sh
./target/desktop/debug/forge inspect examples/office-plan.dxf --json
./target/desktop/debug/forge convert examples/mesh-box.obj target/box.forge.json
./target/desktop/debug/forge transform examples/office-plan.dxf target/moved.dxf --translate 100 200
./target/desktop/debug/forge native --help
./target/desktop/debug/forge native convert examples/office-plan.dxf target/plan.dwg --version r2018
./target/desktop/debug/forge native inspect target/plan.dwg --json
```

DWG drawings are read and written through the unmodified acadrust engine by the `native` commands, with a documented entity subset, R2000/R2018 write targets, and saves refused when the engine reports unsupported or unrecovered content. The RVT/RFA, IPT/IAM, F3D/F3Z, MAX, MA/MB and FBX formats are **not implemented**. The current exchange support does not establish native compatibility or full product parity. Detailed requirements and remaining format gaps are in [requirements](docs/requirements.md), [compatibility](docs/compatibility.md) and [product mapping](docs/product-map.md).

## Verification and artifacts

All source, test harnesses, build output, logs and screenshots are kept in this workspace. Test/build logs are in `validation/results`; the native startup runner is `validation/native_smoke.py`. It launches representative targets on an owned Xvfb display and captures the drafting/modeling views. This proves startup/import/render paths only. It does not prove every user workflow or Autodesk parity.

```sh
python3 validation/native_smoke.py
```

The `validation/core`, `validation/mesh` and `validation/cli` standalone manifests allow testing actual source without resolving desktop dependencies. Normal workspace tests are the primary check once dependencies are available.

See [LICENSE](LICENSE). No Autodesk code or product branding is included; Autodesk product names identify compatibility targets.
