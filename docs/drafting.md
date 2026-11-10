# Precision drafting and plotting

Open Forge Craft in the Draft workbench. The command bar at the bottom accepts complete numeric commands in document model units. Press Enter or Run to execute; Up/Down recalls commands. Each successful edit is one undo step. Selection, measurements, Help and failed commands do not add history. Creation commands select the new objects, so `RECTANGLE 0 0 120 80`, `OFFSET -5`, then `ARRAYRECT 3 2 150 100` produces two rectangles and five additional copies of the inner rectangle.

Selection controls in the inspector expose move, copy, rotate, scale, offset, mirror, rectangular/polar arrays and line trim/extend. Expand **Offset, mirror & arrays** or **Trim / extend lines** to use their numeric controls. The pivot fields above these controls supply the rotation center, polar center and first point of the mirror axis. Mirroring uses the **Create a copy** checkbox. Press Help in the command bar for the full reference.

| Command | Arguments and behavior |
| --- | --- |
| `LINE` | `x1 y1 x2 y2` |
| `RECTANGLE` | `x1 y1 x2 y2` |
| `CIRCLE` | `cx cy radius` |
| `ARC` | `cx cy radius start-deg end-deg`; counterclockwise |
| `POINT` | `x y` |
| `PLINE` | `x y x y … [CLOSE]`; straight segments |
| `TEXT` | `x y height rotation-deg text…` |
| `DIM` | `H\|V\|A x1 y1 x2 y2 line-x line-y`; semantic horizontal, vertical or aligned dimension |
| `SELECT` | `ALL`, `NONE` or space-separated object IDs; ALL includes visible top-level objects |
| `MOVE`, `COPY` | `dx dy`; act on the selection |
| `ROTATE`, `SCALE` | `angle-deg [pivot-x pivot-y]` or `factor [pivot-x pivot-y]`; default pivot is origin |
| `MIRROR` | `axis-x1 axis-y1 axis-x2 axis-y2 [COPY]` |
| `OFFSET` | `signed-distance`; creates selected parallel copies |
| `ARRAYRECT` | `columns rows dx dy`; signed axis spacing |
| `ARRAYPOLAR` | `count cx cy sweep-deg [NOROTATE]` |
| `TRIM`, `EXTEND` | `target-id boundary-id pick-x pick-y` |
| `ERASE` | Remove selected drawing objects |
| `BLOCK` | `name…`; replace selection with one reusable reference, using origin as definition base |
| `EXPLODE` | Replace selected block references with their expanded geometry |
| `LAYER` | `name…`; create/select the current layer |
| `UNITS` | DXF unit code `0..24` (0 unitless, 1 inches, 4 millimetres, 6 metres); sets metadata without rescaling coordinates |
| `LENGTH`, `DIST` | Selected curve length, or `x1 y1 x2 y2` distance |
| `HELP` | Show command syntax |

Desktop commands also include `UNDO`, `REDO`, `FIT`, `SAVE` and `NEW`. NEW follows the usual unsaved-change decision. Batch commands below exclude these desktop actions. Commands are case-insensitive and accept finite numbers, including decimal/scientific notation. Object IDs appear in the inspector. Names and text use the rest of their command line. This command language is not AutoLISP or an AutoCAD script interpreter.

## Operation limits

Positive offsets mean left of a directed line/open polyline, or outward from a circle, arc, rectangle or closed polyline. Negative distances reverse that direction. Straight polyline offsets use miter joins. Curved polyline offsets retain exact arc bulges at tangent-continuous joins; sharp curved joins, self-intersecting, collapsing or reversing contours are refused. Polyline offset is limited to 2,048 vertices.

Array counts include the original objects. A full 360° polar sweep omits a duplicated final position; a partial sweep includes its final angle. NOROTATE moves the selection as a group while retaining orientation. Arrays create at most 10,000 new entities per operation. Unsupported or unrepresentable transforms are refused. Mirroring block references and text is currently refused; explode a reference before mirroring supported contents.

Trim and extend operate on ordinary LINE objects. The intersection must lie on the finite boundary LINE. Trim removes the picked side of an interior intersection. Extend chooses the nearest target endpoint and moves it to an intersection beyond that endpoint. Parallel, coincident and shortening extensions are refused.

## Batch editing

Use `cargo run -p forge-cli --` in place of the binary path if it has not been built.

```sh
./target/desktop/debug/forge draft - target/plan.forge.json --script examples/drafting.forgecmd
./target/desktop/debug/forge draft examples/office-plan.dxf target/shifted.dxf \
  --command 'SELECT ALL' --command 'MOVE 100 200'
```

`-` starts a new document. Repeated `--command` and `--script` options execute in their supplied order, preserving selection between commands. Scripts contain one command per line; blank lines and lines starting with `#` are ignored. Parsing, every operation, and output encoding must succeed before any output is written. Existing destinations require `--overwrite` (or `--force`). Native JSON retains blocks and dimensions; the strict DXF subset refuses semantic dimensions it cannot represent. Mesh/native Autodesk formats are not accepted by this drafting command.

## Vector plotting

Expand **Plot to SVG** below the draft file controls. Choose page width/height and margins in millimetres, Fit or a fixed scale, and monochrome or layer colors. Export writes a sibling `.svg` file; existing output requires a replacement decision. The source drawing and undo history are unchanged.

```sh
./target/desktop/debug/forge plot target/plan.forge.json target/plan.svg
./target/desktop/debug/forge plot target/plan.forge.json target/plan-1-10.svg \
  --page 297 210 --margin 10 --scale 0.1 --color
```

Scale is paper millimetres per model unit, independent of the document unit code. For a millimetre drawing, `1` means 1:1, `0.1` means 1:10, and `0.01` means 1:100. Fit centers the visible drawing within the printable page. Fixed scales that exceed that page are refused. Print SVG at actual size to preserve the specified scale.

Plot output includes visible layers, nested block contents and derived dimension strokes/text. Circular arcs and polyline bulges stay exact SVG arcs. Text is escaped and rotated but uses ordinary sans-serif fonts with approximate metrics. A drawing containing any 3D solids/meshes is refused. SVG is a derived drawing sheet; it does not preserve CAD editing semantics, hidden objects, feature history, paper-space layouts or CTB/STB plot styles.

## Analytic STEP exchange

```sh
./target/desktop/debug/forge brep primitive box target/box.step --size 80 40 20 --center 0 0 10
./target/desktop/debug/forge brep primitive cylinder target/cylinder.step --size 40 40 80
./target/desktop/debug/forge brep inspect target/cylinder.step --json
./target/desktop/debug/forge brep convert target/cylinder.step target/copy.step
```

Sizes/centers are in millimetres. Cylinder sizes specify X/Y diameters and Z height, so unequal diameters produce an elliptic cylinder. These are analytic B-reps, separate from desktop mesh primitives. The checked reader accepts one closed manifold solid with one shell and audits topology to refuse missing converted geometry. Assemblies, open shells, multi-body files and external references are refused.

STEP conversion preserves the exact original source by default. `--regenerate --allow-loss` writes geometry-only STEP, explicitly authorizing replacement of metadata. Existing files require `--overwrite` or `--force`. STEP does not establish Inventor/Fusion native format compatibility, editable feature histories, assemblies or full solid-modeling functionality.
