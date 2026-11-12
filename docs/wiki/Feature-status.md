# Feature inventory

Repository workflow inventory; implemented means the bounded capability described, not Autodesk product parity.

This inventory covers current repository workflows and planned capability groups. The separate product catalog retains all 140 target identities; each still needs a release/platform-specific acceptance inventory.

Status means: **Implemented** = usable within the stated limits; **Partial** = part of the named capability is available; **Planned** = implementation remains; **External evidence** = acceptance data or originating applications are still required.

## Draft

| ID / feature | Status | Current coverage | Limits / remaining work | Evidence |
| --- | --- | --- | --- | --- |
| `draft-authoring` · 2D entity authoring | Implemented | Lines, rectangles, circles, arcs, points, straight/curved polylines and text | No spline or hatch authoring | [lib.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/lib.rs), [app.rs](https://github.com/s-b-repo/forgecad/blob/main/apps/craft-suite/src/app.rs) |
| `draft-transforms` · Selection and transforms | Implemented | Move, copy, rotate, uniform scale, mirror and multi-selection | Unsupported text/INSERT mirror cases are refused | [edit.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/edit.rs) |
| `draft-snaps` · Grid and object snapping | Implemented | Grid and supported geometry reference points | No complete vendor drafting-aid set | [edit.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/edit.rs) |
| `draft-offset-array` · Offsets and arrays | Implemented | Analytic/straight and tangent-continuous curved offsets; rectangular and polar arrays | Invalid joins, collapsing offsets and excessive arrays are refused | [drafting.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/drafting.rs) |
| `draft-trim` · LINE trim and extend | Implemented | Picked target LINE against finite LINE boundaries | General curve trimming remains planned | [drafting.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/drafting.rs) |
| `draft-layers` · Layers | Implemented | Names, colors, visibility and assignment | Full linetype/plot-style systems are not implemented | [lib.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/lib.rs) |
| `draft-blocks` · Reusable nested blocks | Implemented | Definitions, references, expansion, picking and transactional explosion | No dynamic blocks or vendor custom objects | [blocks.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/blocks.rs) |
| `draft-dimensions` · Semantic dimensions | Implemented | Horizontal, vertical, aligned, rotated, radius and diameter annotations | DXF DIMENSION export is refused; full styles remain planned | [dimension.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/dimension.rs) |
| `draft-commands` · Numeric commands and batch scripts | Implemented | Shared transactional desktop command bar, CLI scripts and sketch relation commands | Not AutoLISP, ObjectARX or vendor script compatibility | [command.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/command.rs) |
| `draft-layouts` · Saved viewport sheets | Implemented | Clipping, independent scale/rotation, hidden layers and paper notes | No DXF paper-space exchange or CTB/STB styles | [plot.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/plot.rs) |
| `draft-constraints` · Sketch constraints | Implemented | Persistent sketch relations with a least-squares linear solver: horizontal/vertical lines, coincident/fixed points, X/Y distances, equal and absolute radii on lines, rectangles, circles, points and straight polylines (≤64 vertices) | Linear relations only — no angular, general length/diameter dimensions, spline/arc/bulge relations or 3D sketch constraints; solves linear systems only | [constraints.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/constraints.rs), [command.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/command.rs), [constraints.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/tests/constraints.rs), [constraints_ui.rs](https://github.com/s-b-repo/forgecad/blob/main/apps/craft-suite/src/constraints_ui.rs), [constraints.rs](https://github.com/s-b-repo/forgecad/blob/main/apps/forge-cli/src/constraints.rs) |
| `draft-toolsets` · Specialist toolsets and extensions | Planned | Electrical, mechanical, MEP, plant, mapping, raster and extension workflows | Catalog routing is not specialist toolset implementation | — |

## Model

| ID / feature | Status | Current coverage | Limits / remaining work | Evidence |
| --- | --- | --- | --- | --- |
| `model-primitives` · Parametric primitives | Implemented | Editable boxes, cylinders and ellipsoids | Tessellated preview/export; no general feature graph | [mesh.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/mesh.rs) |
| `model-extrusions` · Editable profile extrusions | Implemented | Concave straight profiles, rectangles and circles; native parameters, desktop/CLI edits | No holes, sketch dependencies or B-rep extrusion | [model.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/model.rs), [model_motion_civil.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/tests/model_motion_civil.rs) |
| `model-meshes` · Imported triangle meshes | Implemented | Native persistence, display/picking and model-area exports | Geometry only; no retained imported materials/textures | [mesh.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/mesh.rs) |
| `model-step` · Analytic STEP exchange | Partial | Box/cylinder authoring and checked one-shell, one-solid exchange | Separate CLI; no assemblies, multi-body files or desktop B-rep history | [lib.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-modeling/src/lib.rs) |
| `model-kernel` · General solid feature history | Planned | Dependent sketches, booleans, fillets, shells, revolutions and robust topology | Primitive/extrusion parameters do not establish a full modeling kernel | — |
| `model-assemblies` · Assemblies and manufacturing drawings | Planned | Components, joints/constraints, linked drawings and revisions | Not implemented | — |

## Build

| ID / feature | Status | Current coverage | Limits / remaining work | Evidence |
| --- | --- | --- | --- | --- |
| `build-elements` · Semantic building elements | Implemented | Storeys, straight walls, rectangular slabs and hosted door/window openings | No curved/compound walls, arbitrary slab outlines or full families | [building.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/building.rs) |
| `build-quantities` · Opening-aware geometry and quantities | Implemented | Actual wall cutouts, net areas/volumes, native relationships and CSV schedules | Model-unit quantities; no takeoff product parity | [building.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/building.rs) |
| `build-families` · Families, rooms and advanced schedules | Planned | Parameterized reusable building systems and space semantics | Not implemented | — |
| `build-analysis` · Structural, MEP and engineering analysis | Planned | Structural/energy systems, routing and independently validated analysis | No engineering solver or analysis acceptance corpus | — |
| `build-coordination` · Multi-model coordination | Planned | Clash detection, linked models and coordination workflows | Not implemented | — |

## Terrain

| ID / feature | Status | Current coverage | Limits / remaining work | Evidence |
| --- | --- | --- | --- | --- |
| `terrain-tin` · Survey and checked TIN surfaces | Implemented | XYZ editing, deterministic XY Delaunay triangulation and survey CSV | 2.5D surface; no breaklines or constrained triangulation | [terrain.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/terrain.rs) |
| `terrain-contours` · Contours, elevations and profiles | Implemented | TIN interpolation, contour segments, longitudinal profiles and surface exports | Contours are unsmoothed segments; outside-surface gaps remain explicit | [terrain.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/terrain.rs) |
| `terrain-alignments` · Line and circular-arc alignments | Implemented | Analytic station/offset queries, left-positive offsets and terrain profiles | No spirals, station equations, design standards or corridors | [civil.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/civil.rs), [model_motion_civil.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/tests/model_motion_civil.rs) |
| `terrain-earthwork` · Cut/fill against a level datum | Implemented | Exact integration after clipping each TIN triangle by datum height | No arbitrary TIN-to-TIN comparison or grading optimizer | [civil.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/civil.rs) |
| `terrain-grading` · Corridors, grading and drainage | Planned | Road assemblies, grading rules, hydrology and drainage networks | Not implemented | — |
| `terrain-assets` · Utility networks, assets and geospatial systems | Planned | Network/asset workflows, CRS transformation and project standards | Not implemented | — |

## Make

| ID / feature | Status | Current coverage | Limits / remaining work | Evidence |
| --- | --- | --- | --- | --- |
| `make-profiles` · Compensated profile machining | Implemented | Rectangle/circle/straight-polygon contours, stock/tool setup and depth passes | Single-tool bounded 2.5D operations | [cam.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/cam.rs) |
| `make-pockets` · Pocket milling with islands | Implemented | Rectangular, circular and polygon clearing with checked uncut islands | No adaptive strategy, ramping, tool changes or multi-axis paths | [cam.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/cam.rs) |
| `make-drilling` · Peck drilling | Implemented | Point-based incremental depth plunges with full clearance retracts | Explicit moves; no controller-specific canned cycles | [cam.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/cam.rs) |
| `make-preview` · Motion preview and generic posting | Implemented | Checked cutter footprints, travel/depth limits and generic millimetre G-code | No machine acceptance, acceleration model or stock-removal simulation | [cam.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/cam.rs) |
| `make-production` · Production CAM and postprocessors | Planned | Tools, machine posts, adaptive/ramped and multi-axis operations | Not implemented | — |
| `make-simulation` · Stock, collision and engineering simulation | Planned | Stock removal, fixtures, collisions, FEA/CFD and manufacturing validation | Motion previews do not provide these solvers | — |
| `make-additive` · Additive, nesting and manufacturing management | Planned | Slicing, build preparation, nesting, inspection and manufacturing services | Not implemented | — |

## Motion

| ID / feature | Status | Current coverage | Limits / remaining work | Evidence |
| --- | --- | --- | --- | --- |
| `motion-scene` · Colored primitive scene graph | Implemented | Persistent objects, checked parent references and composed affine transforms | No vendor dependency graph or scene format compatibility | [scene.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/scene.rs) |
| `motion-animation` · Transform animation and playback | Implemented | Translation/scale/Euler keys, linear/step/smoothstep interpolation and visibility inheritance | No quaternion curves, per-channel tangents or rig animation | [scene.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/scene.rs), [model_motion_civil.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/tests/model_motion_civil.rs) |
| `motion-modifiers` · Editable modifier stack | Partial | Local-space arrays and mirrored copies with outward winding | Eight modifiers and 128 copies per object; no general deformation stack | [scene.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/scene.rs) |
| `motion-rigs` · Rigging, skinning and animation constraints | Planned | Skeletons, deformation, animation controllers and production animation | Not implemented | — |
| `motion-rendering` · Materials, lighting and production rendering | Planned | Shaders, lighting, cameras and offline rendering | Colored viewport primitives are not a production renderer | — |
| `motion-dynamics` · Dynamics and compositing | Planned | Physics, particles, effects, compositing and production interchange | Not implemented | — |

## Connect

| ID / feature | Status | Current coverage | Limits / remaining work | Evidence |
| --- | --- | --- | --- | --- |
| `connect-review` · Local issue/review board | Implemented | Descriptions, references, priority/status lifecycle, search and CSV | References are descriptive text; no cloud identity or synchronization | [review.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/review.rs) |
| `connect-services` · Accounts, permissions and synchronization | Planned | Multi-user projects, identity, permissions, revisions and coordination services | Not implemented | — |
| `connect-vendor` · Vendor cloud/service compatibility | Planned | Connected Autodesk account/project/service workflows | No vendor integration or acceptance data | — |

## Exchange

| ID / feature | Status | Current coverage | Limits / remaining work | Evidence |
| --- | --- | --- | --- | --- |
| `exchange-native` · Native JSON persistence | Implemented | Version 7, validated atomic writes and v1-v6 migration | Project-specific format; no Autodesk native compatibility implied | [lib.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/lib.rs), [storage.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/storage.rs) |
| `exchange-dxf` · ASCII DXF subset | Partial | Analytic drawing entities, units, layers, nested blocks and strict loss diagnostics | No binary DXF, semantic dimension export, paper space, hatches, splines or proxies | [dxf.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/dxf.rs) |
| `exchange-dwg` · DWG/DWT/DWS engine workflow | Partial | acadrust-supported entities; CLI read/write with R2000/R2018 output checks | Desktop DWG editing and full vendor fidelity remain unverified | [lib.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-dwg/src/lib.rs) |
| `exchange-mesh` · Geometry-only OBJ/STL | Implemented | ASCII/binary STL, OBJ polygon triangulation and checked mesh output | No materials, animation, unit metadata or editable CAD parameters | [mesh.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/mesh.rs) |
| `exchange-proprietary` · Remaining Autodesk native formats | Planned | RVT/RFA, IPT/IAM, F3D/F3Z, MAX, MA/MB, FBX, NWD/NWC, RCS/RCP and DWF | No integrated implementation or native acceptance corpus | — |
| `exchange-open` · Further open interchange | Planned | IFC, IGES, SAT, USD and Alembic workflows | Not implemented | — |

## Quality

| ID / feature | Status | Current coverage | Limits / remaining work | Evidence |
| --- | --- | --- | --- | --- |
| `quality-history` · Undo/redo and transactional edits | Implemented | Document snapshots and validated candidate edits across native workbenches | No cross-session recovery journal | [app.rs](https://github.com/s-b-repo/forgecad/blob/main/apps/craft-suite/src/app.rs), [automation.rs](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/automation.rs) |
| `quality-linux` · Linux verification | Implemented | Workspace tests, native launch/edit/reopen cases and artifact hashes | Tests prove their stated bounded cases | [native_smoke.py](https://github.com/s-b-repo/forgecad/blob/main/validation/native_smoke.py), [summarize.py](https://github.com/s-b-repo/forgecad/blob/main/validation/summarize.py) |
| `quality-release` · Installers and cross-platform releases | Planned | Packaged Windows/macOS/Linux installs, migration and recovery acceptance | Only local Linux binaries have been verified | — |
| `quality-native-acceptance` · Originating-product acceptance | External evidence | Versioned fixtures checked in each originating application | No Autodesk installation or authoritative fixture corpus connected | [requirements.md](https://github.com/s-b-repo/forgecad/blob/main/docs/requirements.md) |
| `quality-audit` · Supplied Rust pattern audit | Partial | All workspace Rust files scanned; focused new-code remediation: fallible authoring APIs (add_geometry/add_solid/add_layer/add_mesh/add_block/duplicate_entities), reachable panic hardening and recorded hashes/findings | Raw strict gate remains non-clean (test assertions and legacy invariants); the confirmed object-ID exhaustion panic is fixed and covered by tests | [audit-bad-patterns.sh](https://github.com/s-b-repo/forgecad/blob/main/audit-bad-patterns.sh), [run_code_audit.py](https://github.com/s-b-repo/forgecad/blob/main/scripts/run_code_audit.py), [code-audit.md](https://github.com/s-b-repo/forgecad/blob/main/docs/code-audit.md), [code-audit-id-exhaustion.json](https://github.com/s-b-repo/forgecad/blob/main/validation/results/code-audit-id-exhaustion.json) |
