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
| `RELATION HORIZONTAL`, `RELATION VERTICAL` | `id`; solves that LINE onto the axis |
| `RELATION COINCIDENT` | `id handle id handle`; joins two handles |
| `RELATION FIXPOINT` | `id handle x y`; pins one handle at a coordinate |
| `RELATION DISTX`, `RELATION DISTY` | `id handle id handle value`; axis distance between two handles |
| `RELATION RADIUS` | `id value`; absolute radius of a CIRCLE |
| `RELATION EQUALRAD` | `id id`; equal radii on two CIRCLEs |
| `RELATION REMOVE` | `constraint-id`; removes one relation and re-solves |
| `RELATION CLEAR` | Removes every relation |
| `CONSTRAINT` | `REPORT` (read-only) or `SOLVE`; re-solves the current sketch |
| `HELP` | Show command syntax |

Point handles identify a constrained point on an entity: `START`/`END` on lines, `CENTER` on circles, `POSITION` on points, `CORNERA`/`CORNERB` on rectangles and `VERTEX n` on straight polylines. Handles, relation kinds and command names are case-insensitive. Relation and constraint IDs are their own namespace, independent of drawing object IDs; each new relation takes the next free constraint ID automatically, so `RELATION REMOVE` and `CONSTRAINT REPORT` report those numbers.

Desktop commands also include `UNDO`, `REDO`, `FIT`, `SAVE` and `NEW`. NEW follows the usual unsaved-change decision. Batch commands below exclude these desktop actions. Commands are case-insensitive and accept finite numbers, including decimal/scientific notation. Object IDs appear in the inspector. Names and text use the rest of their command line. This command language is not AutoLISP or an AutoCAD script interpreter.

## Sketch constraints

The **Sketch constraints** section of the Draft inspector authors relations on the current drawing, and `RELATION`/`CONSTRAINT` do the same from the command bar and CLI scripts. Relations persist in native Forge projects; neutral formats (DXF, OBJ/STL, DWG) drop them. `forge constraints handles` lists an entity's valid handles, and `forge constraints report`/`solve` query or re-solve a saved project:

```sh
./target/desktop/debug/forge draft - target/slope.forge.json --command 'LINE 0 0 100 20'
./target/desktop/debug/forge draft - target/plate.forge.json \
  --command 'LINE 0 0 100 20' --command 'RELATION HORIZONTAL 2'
./target/desktop/debug/forge constraints report target/plate.forge.json --json
./target/desktop/debug/forge constraints solve target/plate.forge.json target/flat.forge.json
```

The solver collects the coordinates and radii of every entity a relation references into a linear system and solves it, moving participating geometry the least that satisfies the relations. Horizontal/vertical relations make a LINE axis-aligned, coincident joins two handles, fixed points pin one handle at a coordinate, X/Y distances set the separation between two handles on one axis, radius sets a CIRCLE's absolute radius and equal radius matches two circles. Redundant but consistent relations are reported rather than applied twice.

`CONSTRAINT REPORT` is read-only and `CONSTRAINT SOLVE` re-solves the current sketch. The report carries the sketch **state** (`Satisfied`, `NeedsSolve`, `Conflict` or `NumericalFailure`), the counts of **constraints**, participating **variables**, **equations** and matrix **rank**, the remaining **degrees of freedom**, the number of **redundant equations**, the tolerance, the **maximum error** and the **conflicting constraint IDs** with their per-constraint errors. The command bar and CLI print the summary line plus one line per relation; `--json` returns every field.

Every relation and every solve is one transaction on a candidate copy of the document: a parse error, an unsupported entity, a conflicting set or a numerically invalid result leaves the drawing, the constraint set, the object-ID allocator and the undo history byte-identical, and no output file is written. An already satisfied sketch re-solves as a verified no-op that reports no change.

Supported geometry is LINE, RECTANGLE, CIRCLE, POINT and straight polylines of at most 64 vertices with zero bulge. Only linear relations are solved: there are no angular, general length, diameter, symmetry or driving-dimension relations, and no spline, arc, bulge or 3D sketch constraints. A sketch holds at most 256 constraints and 128 participating coordinate/radius variables, and constrained coordinates, radii and dimensions must be finite and within ±1e9 model units (radii at least 0.000001). Entities referenced by a relation — or by a linked extrusion — cannot be erased or implicitly removed; remove the relations or detach the features first. `RELATION REMOVE` refuses to leave a conflicting set, and clearing all relations is itself validated.

A worked example:

```sh
./target/desktop/debug/forge draft - target/slope.forge.json --command 'LINE 0 0 100 20'
./target/desktop/debug/forge draft target/slope.forge.json target/flat.forge.json \
  --command 'RELATION HORIZONTAL 2' --overwrite
./target/desktop/debug/forge constraints report target/flat.forge.json
```

Relation #1 Horizontal #2 · Satisfied: 1 constraints, 3 remaining degrees of freedom, maximum error 0.000000; conflicting IDs []
  #1 Horizontal #2

The line becomes horizontal with the correction shared between both endpoints: the sloped start `(0,0)` and end `(100,20)` become `(0,10)` and `(100,10)` to floating-point precision. Running `CONSTRAINT SOLVE` once more on the already satisfied sketch sharpens the shared coordinate to its exact midpoint and then verifies as a no-op. Its ID stayed 2, so `RELATION HORIZONTAL 2` addresses it. Conflicting relations name the constraint IDs that cannot hold together:

```sh
./target/desktop/debug/forge draft - target/conflict.forge.json \
  --command 'LINE 0 0 10 0' --command 'LINE 20 0 30 0' \
  --command 'RELATION DISTX 2 START 3 START 50'
./target/desktop/debug/forge draft target/conflict.forge.json target/conflict.forge.json \
  --command 'RELATION DISTX 2 START 3 START 70' --overwrite
```

Relation rejected: Conflict: 2 constraints, 7 remaining degrees of freedom, maximum error 20.000000; conflicting IDs [1, 2]

Both distances cannot hold, so the second relation is refused and `target/conflict.forge.json` keeps exactly the bytes it had before the command. Releasing one side (`RELATION REMOVE 1`) lets the new value commit.

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
