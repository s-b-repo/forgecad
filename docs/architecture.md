# Architecture

Forge Studio is a multi-workbench desktop suite with a shared document and geometry layer.

```text
apps/craft-suite       Native application shell, individual product targets and editing UI
apps/forge-cli         Headless inspect/convert/transform/primitive/product commands
crates/forge-core      Validated document model, geometry, native files, DXF and mesh exchange
crates/forge-core/data Individual product catalog and implementation/lifecycle status
validation            Reproducible tests, native smoke runner, logs and screenshots
```

The UI depends on the core crate rather than owning file-format rules. Each future workbench can use the shared document, selection, undo, camera, and import/export services while keeping domain-specific models in its own crate. For example, BIM elements and relationships should not be flattened into drafting lines; media timelines should not be represented as solids.

Current core modules cover versioned/atomic native storage, strict DXF diagnostics, exact analytic 2D entities, editing transforms and object snaps, primitive tessellation, and geometry-only STL/OBJ exchange. Imported triangle meshes persist in native projects. The CLI and desktop use these common models; the CLI additionally exposes explicit data-loss authorization for batch conversion. Desktop export previews report losses and ask before replacing an existing export.

The active completion requirements remain in `requirements.md`, including all individual product identities and their native formats. Feature and format status is recorded explicitly. Sharing an editor across targets is not proof of separate product feature parity.
