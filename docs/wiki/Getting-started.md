# Getting started

```sh
git clone https://github.com/s-b-repo/forgecad.git
cd forgecad
cargo run -p forge-craft -- --product autocad
cargo run -p forge-cli -- products --json
cargo test --workspace --all-targets
cargo build --workspace --bins
```

Rust 1.95+ and the platform's graphics development libraries are required. The desktop uses native eframe/egui. Family entry binaries remain available as draftcraft, modelcraft, buildcraft, terraincraft, makecraft, motioncraft and connectcraft.

```sh
./target/desktop/debug/forge-craft --product autocad --open examples/office-plan.dxf
./target/desktop/debug/forge-craft --product inventor --open examples/mesh-box.obj
./target/desktop/debug/forge workbench create all target/suite.forge.json
./target/desktop/debug/forge-craft --product revit --open target/suite.forge.json
./target/desktop/debug/forge products
```

The Products selector opens the full searchable catalog. Each target keeps its own identity; shared work areas provide the currently implemented tools. Build, Terrain, Make and Motion have semantic authoring, derived previews and dedicated exports. Their supported workflows and remaining limits are described in the [workbench guide](Workbenches). Connect provides a local issue/review board with priorities, statuses and CSV output; network services remain unimplemented.

[Modeling](Modeling) · [Workbenches](Workbenches) · [Compatibility](Compatibility)
