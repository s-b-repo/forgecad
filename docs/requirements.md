# Completion requirements

The active goal is a finished Rust counterpart for every product in the supplied Autodesk catalog, including its features and native file compatibility. The goal remains open. A product entry, launcher button, shared workbench, or neutral-format converter does not prove that a product is implemented.

The source catalog is stored as 140 distinct identities in `crates/forge-core/data/products.json`, including platform editions, extensions, collections, services, and discontinued products. Individual identities must remain available to the launcher and automation interfaces. Work-area routing is an internal code-sharing choice.

For each identity, completion requires an independently reviewed feature inventory tied to a specific Autodesk product release and platform. Each required workflow needs an implementation, reproducible acceptance data, and a recorded result. Collections require the member applications; cloud services require functioning account/project/review/coordination workflows; SDK targets require compatible programming interfaces. A shared editor displaying a different product name is not evidence of these capabilities.

| Requirement | Evidence needed | Current evidence |
| --- | --- | --- |
| All individual products retained | Catalog comparison with supplied A–Z list, unique IDs, product launch routes | 140 catalog records; implementation status is prototype or planned |
| Drafting feature parity | Editing, annotation, dimensions, layouts, blocks, constraints, toolsets, plotting, automation and extension acceptance workflows | Analytic 2D authoring; transforms, snaps, layers; nested blocks and semantic dimensions; numeric commands/scripts; offsets, arrays, LINE trim/extend; physical SVG and saved multi-viewport sheets with layer filters, paper notes and model-view rotation. Constraints, DXF paper-space exchange, toolsets and extension compatibility remain unfinished |
| Parametric CAD and manufacturing parity | Robust B-rep kernel, feature histories, assemblies, constraints, drawings, CAM operations, simulation and manufacturing validation | Editable profile extrusions and primitive tessellation/mesh exchange; checked analytic STEP boxes/cylinders and one-shell/one-solid CLI exchange; compensated profiles, rectangular/circular/polygon pockets with optional uncut islands and point-based peck drilling with stepdowns, stock bounds, motion preview and generic G-code. Feature histories, assemblies, constraints, tool changes, machine-specific posts and stock/collision simulation remain unfinished |
| BIM and civil parity | Semantic buildings/infrastructure, families, schedules, coordination, analysis, terrain/network/asset workflows | Storeys, walls, slabs, hosted openings with mesh cutouts and quantities; surveyed TINs, contours, elevation/profile queries; persistent line/circular-arc alignments, station/offset reports and exact TIN cut/fill against a level datum. Families, multi-discipline systems, coordination, structural analysis, spiral alignments/corridors, drainage/network/assets and native BIM/civil formats remain unfinished |
| Media parity | Scene graph, rigs, animation, modifiers, dynamics, materials, renderer, compositing and production workflows | Colored primitive scenes with eased/stepped transform keys, array/mirror modifiers and checked parent hierarchies, composed affine evaluation, playback/scrubbing and evaluated-frame OBJ. Rigs, general deformation, dynamics, shader materials, production rendering, compositing and media-native formats remain unfinished |
| Project services parity | Accounts, permissions, project coordination, reviews and synchronization with independent acceptance data | Local issue board with titles, descriptions, text references, priorities, status lifecycle, search, native persistence and CSV; accounts, multi-user coordination, permissions and cloud services remain unfinished |
| Native format parity | Versioned input/output corpus checked in original applications, including metadata/references and editable history | See `compatibility.md`; DWG read/write exists through the acadrust engine with a documented subset; RVT/IPT/F3D/MAX/MA/MB/FBX remain unimplemented |
| Release quality | Successful builds/installers, launch/edit/save/reopen scenarios, recovery and migration checks on Windows/macOS/Linux | Linux workspace tests and native startup/edit/save/reopen runners; semantic-model geometry, relationships, interpolation, toolpath and migration tests. Exact current counts and artifact hashes are recorded in `validation/results/verification.json`; installers and other platforms remain unverified |

The product inventory and format ledger are requirements records, not substitutes for implementation. Current tests prove only their stated cases. No full-product completion claim is justified by this repository yet.

The [advanced workflow guide](advanced-workflows.md) records the saved-sheet, parenting, pocket/drill, alignment and earthwork implementations with runnable examples and explicit limits. Tests include exact geometry/volume expectations, independent pocket-coverage samples, migration and atomic rejection cases. These bounded additions do not mark any catalog product as complete.

## Compatibility acceptance process

1. Record format, originating product version, platform, units, expected objects and external references for each input fixture.
2. Check parsing against independent fixtures, malformed/truncated inputs, precision limits, and resource limits.
3. Edit supported content and save it, preserving all unrelated data and references. Unsupported constructs must be reported before a destructive operation.
4. Open the result in the originating product and compare semantic objects, geometry tolerances, editable history, styles, metadata, references, and rendered output as appropriate.
5. Repeat across the supported product/version matrix. Label partial import/export explicitly until the complete matrix passes.

No Autodesk installation or authoritative native-file fixture corpus is currently connected. This limits verification of vendor-native compatibility, while implementation of documented formats and local workflows can continue.
