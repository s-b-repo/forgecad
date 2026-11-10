# Architecture

Forge Craft is a multi-workbench desktop suite with a shared document and geometry layer.

```text
apps/craft-suite       Native application shell, individual product targets and editing UI
apps/forge-cli         Headless drawing, native DWG, STEP and semantic workbench commands
crates/forge-core      Validated document, drafting/building/terrain/scene/CAM models and exchange
crates/forge-core/data Individual product catalog and implementation/lifecycle status
validation            Reproducible tests, native smoke runner, logs and screenshots
```

The UI depends on the core crate rather than owning file-format rules. Workbenches share the document, undo, camera, and import/export services while retaining domain-specific semantic models. Building elements and hosted openings are stored independently of their derived meshes; scene keyframes are stored independently of an evaluated animation frame.

Current core modules cover versioned/atomic native storage, strict DXF diagnostics, exact analytic 2D entities, editing transforms and object snaps, primitive tessellation, and geometry-only STL/OBJ exchange. Imported triangle meshes persist in native projects. The CLI and desktop use these common models; the CLI additionally exposes explicit data-loss authorization for batch conversion. Desktop export previews report losses and ask before replacing an existing export.

`forge-core::drafting` implements transactional offsets, arrays and LINE trim/extend. `forge-core::command` parses complete numeric commands for both the desktop command bar and CLI batch scripts. `forge-core::plot` derives physical SVG sheets from visible expanded entities without changing the native document. The desktop's separate drafting controls call that same command layer, recording one document snapshot per edit.

`crates/forge-modeling` wraps the Truck analytic B-rep/STEP kernel with topology checks. The CLI's `brep` workflow creates boxes and elliptic cylinders, inspects one-shell/one-solid STEP, and preserves its original source for unchanged exchange. These B-reps are not yet part of the desktop document or primitive history.

Native JSON v5 retains the five separately validated models in `Document`: `building`, `terrain`, `scene`, `manufacturing` and `review`, and adds saved `layouts`, terrain alignments, scene parent references and machining operation kinds. Missing fields in v1–v4 documents receive compatible defaults. Unrelated workbench data survives edits/native saves. Dedicated workbench/layout exports explicitly select a derived area; ordinary neutral conversions require the loss policy before omitting semantic data or saved sheets.

`forge-core::automation` applies typed project edits in one transaction with per-edit validation. `civil` supplies polyline station/offset evaluation, sampled alignment profiles and exact TIN cut/fill integration after clipping each face at its design datum. Scene meshes use cached affine ancestor transforms, retaining nonuniform-scale composition without changing local keys. CAM preflights operation counts before allocating pocket paths, then validates tool footprints and posted segments. Desktop controls and CLI commands use these core models.

- `forge-core::building` retains storeys, walls, slabs and hosted openings. It derives opening-aware solid mesh pieces and quantity schedules without changing those relationships.
- `forge-core::terrain` retains survey XYZ points and CCW TIN faces. Bounded normalized XY Delaunay triangulation, mesh validation, exact planar interpolation, contour segments and sampled profiles share that surface. Survey edits clear faces until an explicit rebuild.
- `forge-core::scene` retains primitive scene objects and ordered transform keyframes. Evaluation derives colored meshes at a requested time using linear translation/scale and shortest-arc Euler-angle interpolation. Desktop playback changes view time without creating edit history.
- `forge-core::cam` retains stock/tool settings and profile/pocket/drill operations, including optional uncut pocket islands. Planning validates island topology, derives compensated contours, builds zig-zag polygon spans or raster/concentric clearing with disconnected-path retracts, then emits peck depths and rapid/feed motion; G-code generation consumes the checked plan.

Desktop `building_ui.rs`, `terrain_ui.rs`, `civil_ui.rs`, `scene_ui.rs`, `layout_ui.rs` and shared workbench controls stage numeric edits and commit validated candidate documents as one undo step. They render derived meshes/contours/motion rather than inserting surrogate drawing entities. `forge workbench` exposes example creation, survey import, semantic inspection and derived exports using the same core models. `forge-core::review` and `review_ui.rs` implement a local issue board with priorities, statuses, text references and CSV export; cloud account/project/sync services remain unfinished.

The active completion requirements remain in `requirements.md`, including all individual product identities and their native formats. Feature and format status is recorded explicitly. Sharing an editor across targets is not proof of separate product feature parity.
