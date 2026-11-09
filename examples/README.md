# Example projects

`office-plan.dxf` is an independently authored 2D drawing in millimetres with named layers, a closed polyline, line, arc, circle and text. It is a format/editing example, not a BIM model or construction design.

`mesh-box.obj` is an independently authored outward-facing box with six quadrilateral faces. It exercises polygon triangulation and imported mesh rendering. It contains no texture or material metadata.

Launch a product target with a document:

```sh
cargo run -p forge-studio -- --product autocad --open examples/office-plan.dxf
cargo run -p forge-studio -- --product inventor --open examples/mesh-box.obj
```

Inspect or convert through the headless application:

```sh
cargo run -p forge-cli -- inspect examples/office-plan.dxf --json
cargo run -p forge-cli -- convert examples/mesh-box.obj target/imported-box.forge.json
```

The source examples are preserved when opened; native saves use a `.forge.json` project path.
