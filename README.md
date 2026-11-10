# Forge Craft

A native Rust CAD and engineering suite with seven workbenches for drafting, modeling, buildings, terrain, manufacturing, animation and local project review.

[Wiki](https://github.com/s-b-repo/forgecad/wiki) · [All features](docs/features.md) · [Progress](docs/progress.md) · [Roadmap](docs/roadmap.md) · [Documentation](docs/README.md)

An independent, MIT-licensed Rust project pursuing a counterpart for every Autodesk product supplied in the original catalog, including complete features and native format compatibility. The goal is active and unfinished. The repository retains **140 individual product targets**, including AutoCAD, Inventor, Revit, Fusion, Maya, 3ds Max, and Civil 3D. Target status is explicitly prototype or planned.

## Run the built Linux applications

```sh
./target/desktop/debug/forge-craft --product autocad --open examples/office-plan.dxf
./target/desktop/debug/forge-craft --product inventor --open examples/mesh-box.obj
./target/desktop/debug/forge workbench create all target/suite.forge.json
./target/desktop/debug/forge-craft --product revit --open target/suite.forge.json
./target/desktop/debug/forge products
```

The Products selector opens the full searchable catalog. Each target keeps its own identity; shared work areas provide the currently implemented tools. Build, Terrain, Make and Motion have semantic authoring, derived previews and dedicated exports. Their supported workflows and remaining limits are described in the [workbench guide](docs/workbenches.md). Connect provides a local issue/review board with priorities, statuses and CSV output; network services remain unimplemented.

## Build from source

```sh
git clone https://github.com/s-b-repo/forgecad.git
cd forgecad
cargo run -p forge-craft -- --product autocad
cargo run -p forge-cli -- products --json
cargo test --workspace --all-targets
cargo build --workspace --bins
```

Rust 1.95+ and the platform's graphics development libraries are required. The desktop uses native eframe/egui. Family entry binaries remain available as draftcraft, modelcraft, buildcraft, terraincraft, makecraft, motioncraft and connectcraft.

## Implemented workflows

- Drafting: lines, rectangles, circles, polylines, center/start/end-angle arcs, text and points; grid/object snapping; multi-selection; layer visibility/assignment; move/copy/rotate/uniform-scale/mirror; undo/redo; fit/pan/zoom; native save/open and unsaved-change decisions.
- Precision editing: numeric command bar with history, parallel offsets of analytic entities and straight or tangent-continuous curved polylines, rectangular/polar arrays, and LINE trim/extend against finite LINE boundaries. Desktop controls and batch scripts use the same transactional operations. Invalid operations leave the drawing unchanged.
- Blocks: reusable definitions from a selection (⌃G), nested INSERT display and picking through expanded geometry, and one-transaction explosion back to ordinary entities.
- Annotation: semantic horizontal, vertical, aligned and rotated linear dimensions plus radius and diameter dimensions, with extension lines, arrows and measured text derived from the stored references.
- Modeling: editable rectangle/circle/concave-profile extrusions with retained height/base/profile parameters; dimension/position editing for box, cylinder and ellipsoid primitives; orbit/pan/zoom; imported triangle meshes; native persistence; tessellated STL/OBJ export with explicit loss/replacement decisions.
- Analytic modeling: checked STEP box and circular/elliptic cylinder authoring, topology inspection and source-preserving exchange through `forge brep`. This is a separate CLI workflow, limited to one closed manifold solid with one shell; STEP assemblies and editable feature histories are not supported.
- Plotting: physical SVG sheets with page size, margins, fit or explicit scale, monochrome/layer colors, visible nested blocks, dimensions and exact circular arcs. Saved layouts add multiple clipped viewports, individual scales/layer filters/clockwise model-view rotation and paper notes. Use **Saved print layouts**, **Plot to SVG**, `forge layout` or `forge plot --layout NAME`. Text metrics are approximate; DXF paper-space exchange and vendor plot styles remain unfinished.
- Building: storeys, straight walls, rectangular slabs and hosted door/window openings; opening-aware wall geometry, numeric editing, quantity schedules and derived OBJ export. The building model retains host/storey relationships in native projects.
- Terrain: editable XYZ surveys, checked TIN triangulation, contours, elevation queries and profiles; persistent line/circular-arc alignments with analytic stationing, station/offset queries and sampled alignment profiles; exact cut/fill integration against a level datum; CSV reports/survey exchange, OBJ surface and DXF contours.
- Motion: colored primitive objects, translation/rotation/scale keyframes, parent hierarchies, linear/step/smoothstep interpolation, inherited visibility, editable array/mirror modifiers and timeline playback and evaluated-frame OBJ. Local poses inherit ancestor transforms, including nonuniform scale. Cycles and dangling parents are refused; subtree deletion is undoable.
- Manufacturing: stock/tool setup, compensated profiles, rectangular raster/circular concentric/polygon scanline pockets with optional uncut islands, point-based peck drilling, depth passes, motion preview and generic LinuxCNC-style G-code. Pocket step over and cutter footprints are validated; XY positioning follows vertical clearance retracts. Machine-specific posts and stock/collision simulation remain unfinished.
- Project review: local issue board, title/description/reference editing, search, status/priority filters, resolution lifecycle, undo/redo and CSV export. No cloud account or synchronization is implied.
- Exchange: a strict ASCII DXF subset including layers, units, basic text, arcs, curved polylines, blocks and INSERT references; ASCII/binary STL; geometry-only OBJ with polygon triangulation; native JSON v6 with v1–v5 migration, saved layouts and separate workbench models.
- Automation: inspect, convert, transform, primitive creation, native DWG commands, typed transactional project scripts, civil reports, saved-layout commands and individual-product catalog commands. Batch conversions refuse unapproved data loss and existing output replacement.

```sh
./target/desktop/debug/forge inspect examples/office-plan.dxf --json
./target/desktop/debug/forge convert examples/mesh-box.obj target/box.forge.json
./target/desktop/debug/forge transform examples/office-plan.dxf target/moved.dxf --translate 100 200
./target/desktop/debug/forge native --help
./target/desktop/debug/forge native convert examples/office-plan.dxf target/plan.dwg --version r2018
./target/desktop/debug/forge native inspect target/plan.dwg --json
./target/desktop/debug/forge draft - target/plan.forge.json --script examples/drafting.forgecmd
./target/desktop/debug/forge plot target/plan.forge.json target/plan.svg
./target/desktop/debug/forge brep primitive cylinder target/part.step --size 40 40 80
./target/desktop/debug/forge brep inspect target/part.step --json
./target/desktop/debug/forge workbench inspect target/suite.forge.json --json
./target/desktop/debug/forge workbench export target/suite.forge.json target/quantities.csv --area build
./target/desktop/debug/forge workbench export target/suite.forge.json target/terrain.obj --area terrain
./target/desktop/debug/forge workbench export target/suite.forge.json target/frame.obj --area motion --time 2
./target/desktop/debug/forge workbench export target/suite.forge.json target/profiles.nc --area make
```

See the [editable modeling guide](docs/modeling.md), [code audit](docs/code-audit.md) and [drafting and plotting guide](docs/drafting.md) for command syntax, scripts, operation limits and print scales, and the [workbench guide](docs/workbenches.md) for building, terrain, scene and CAM workflows. Output files are protected against replacement unless explicitly authorized. Dedicated workbench exports select one area's derived output; native saves retain all areas together.

The [new workflow guide](docs/advanced-workflows.md) includes runnable examples for saved layouts, scene parenting, pocket/drill operations, alignment profiles and earthwork reports. The [completion requirements](docs/requirements.md) continue to track the wider unfinished suite.

DWG drawings are read and written through the unmodified acadrust engine by the `native` commands, with a documented entity subset, R2000/R2018 write targets, and saves refused when the engine reports unsupported or unrecovered content. The RVT/RFA, IPT/IAM, F3D/F3Z, MAX, MA/MB and FBX formats are **not implemented**. The current exchange support does not establish native compatibility or full product parity. Detailed requirements and remaining format gaps are in [requirements](docs/requirements.md), [compatibility](docs/compatibility.md) and [product mapping](docs/product-map.md).

## Verification and artifacts

All source, test harnesses, build output, logs and screenshots are kept in this workspace. Test/build logs are in `validation/results`; the native startup runner is `validation/native_smoke.py`. It launches representative targets on an owned Xvfb display and captures all seven populated workbench views. This proves startup/import/render paths only. It does not prove every user workflow or Autodesk parity.

```sh
python3 validation/native_smoke.py
```

The `validation/core`, `validation/mesh` and `validation/cli` standalone manifests allow testing actual source without resolving desktop dependencies. Normal workspace tests are the primary check once dependencies are available.

See [LICENSE](LICENSE). No Autodesk code or product branding is included; Autodesk product names identify compatibility targets.
