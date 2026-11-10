# Example projects

`office-plan.dxf` is an independently authored 2D drawing in millimetres with named layers, a closed polyline, line, arc, circle and text. It is a format/editing example, not a BIM model or construction design.

`mesh-box.obj` is an independently authored outward-facing box with six quadrilateral faces. It exercises polygon triangulation and imported mesh rendering. It contains no texture or material metadata.

`drafting.forgecmd` creates a dimensioned plan with offset wall rectangles and an array of twelve circular columns. It exercises numeric drafting, named layers, semantic dimensions and SVG plotting. It is a CAD workflow example, not a construction design.

`machining-operations.json` creates rectangular, circular and polygon pockets (the polygon includes an uncut circular island) and a peck-drilled hole through `forge workbench apply - OUTPUT --script FILE`. `civil-alignment.json` adds a stationed polyline to a terrain project; `scene-parenting.json` parents the second object in the Motion example. See the [workflow guide](../docs/advanced-workflows.md) for complete commands and saved-sheet examples.

Launch a product target with a document:

```sh
cargo run -p forge-craft -- --product autocad --open examples/office-plan.dxf
cargo run -p forge-craft -- --product inventor --open examples/mesh-box.obj
```

Inspect or convert through the headless application:

```sh
cargo run -p forge-cli -- inspect examples/office-plan.dxf --json
cargo run -p forge-cli -- convert examples/mesh-box.obj target/imported-box.forge.json
cargo run -p forge-cli -- draft - target/plan.forge.json --script examples/drafting.forgecmd
cargo run -p forge-cli -- plot target/plan.forge.json target/plan.svg
```

The source examples are preserved when opened; native saves use a `.forge.json` project path.
