# Completion requirements

The active goal is a finished Rust counterpart for every product in the supplied Autodesk catalog, including its features and native file compatibility. The goal remains open. A product entry, launcher button, shared workbench, or neutral-format converter does not prove that a product is implemented.

The source catalog is stored as 140 distinct identities in `crates/forge-core/data/products.json`, including platform editions, extensions, collections, services, and discontinued products. Individual identities must remain available to the launcher and automation interfaces. Work-area routing is an internal code-sharing choice.

For each identity, completion requires an independently reviewed feature inventory tied to a specific Autodesk product release and platform. Each required workflow needs an implementation, reproducible acceptance data, and a recorded result. Collections require the member applications; cloud services require functioning account/project/review/coordination workflows; SDK targets require compatible programming interfaces. A shared editor displaying a different product name is not evidence of these capabilities.

| Requirement | Evidence needed | Current evidence |
| --- | --- | --- |
| All individual products retained | Catalog comparison with supplied A–Z list, unique IDs, product launch routes | 140 catalog records; implementation status is prototype or planned |
| Drafting feature parity | Editing, annotation, dimensions, layouts, blocks, constraints, toolsets, plotting, automation and extension acceptance workflows | Line, rectangle, circle, polyline, arc, text and point authoring; transforms, snaps, layers; reusable blocks with nested INSERT display/picking/explosion; semantic linear and radial dimensions with derived annotation; partial exchange tests |
| Parametric CAD and manufacturing parity | Robust B-rep kernel, feature histories, assemblies, constraints, drawings, CAM operations, simulation and manufacturing validation | Primitive tessellation and mesh exchange |
| BIM and civil parity | Semantic buildings/infrastructure, families, schedules, coordination, analysis, terrain/network/asset workflows | Unimplemented product targets |
| Media parity | Scene graph, rigs, animation, modifiers, dynamics, materials, renderer, compositing and production workflows | Unimplemented product targets |
| Native format parity | Versioned input/output corpus checked in original applications, including metadata/references and editable history | See `compatibility.md`; DWG read/write exists through the acadrust engine with a documented subset; RVT/IPT/F3D/MAX/MA/MB/FBX remain unimplemented |
| Release quality | Successful builds/installers, launch/edit/save/reopen scenarios, recovery and migration checks on Windows/macOS/Linux | Linux workspace builds/tests (111 tests) and representative native startup/import/render checks including dimension and block authoring; installers and other platforms remain unverified |

The product inventory and format ledger are requirements records, not substitutes for implementation. Current tests prove only their stated cases. No full-product completion claim is justified by this repository yet.

## Compatibility acceptance process

1. Record format, originating product version, platform, units, expected objects and external references for each input fixture.
2. Check parsing against independent fixtures, malformed/truncated inputs, precision limits, and resource limits.
3. Edit supported content and save it, preserving all unrelated data and references. Unsupported constructs must be reported before a destructive operation.
4. Open the result in the originating product and compare semantic objects, geometry tolerances, editable history, styles, metadata, references, and rendered output as appropriate.
5. Repeat across the supported product/version matrix. Label partial import/export explicitly until the complete matrix passes.

No Autodesk installation or authoritative native-file fixture corpus is currently connected. This limits verification of vendor-native compatibility, while implementation of documented formats and local workflows can continue.
