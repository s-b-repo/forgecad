# Saved layouts, hierarchies, machining and civil analysis

These workflows extend the existing suite. They do not complete the larger [140-product scope](Requirements). Native JSON v6 retains layouts, parent references, alignment definitions and machining operation types. V1–v5 files remain readable; a v6 save preserves older content and adds default values for missing fields.

Commands below use the built `target/desktop/debug/forge` binary. Run `cargo build --workspace --bins --target-dir target/desktop` to build it, or substitute `cargo run -p forge-cli --`. Each command refuses an existing output unless `--overwrite`/`--force` is supplied.

## Saved print layouts

In Draft, expand **Saved print layouts** under the file controls. Enter a sheet name and choose **Create fitted A4 layout**. Select it, edit the page, viewport frames, model centers/scales and per-viewport hidden layers, then **Apply layout changes**. Add paper notes for titles or annotations. Each Apply is one undo step. **Export saved layout to SVG** uses the last applied values; save the native project to retain the editable sheet.

```sh
target/desktop/debug/forge draft - target/plan.forge.json --script examples/drafting.forgecmd
target/desktop/debug/forge layout create target/plan.forge.json target/sheet.forge.json --name 'Plan A'
target/desktop/debug/forge layout viewport target/sheet.forge.json target/detail.forge.json \
  --layout 'Plan A' --name Detail --paper 170 20 100 100 --center 30 30 --scale 1 --rotation 90
target/desktop/debug/forge layout note target/detail.forge.json target/annotated.forge.json \
  --layout 'Plan A' --position 20 195 --height 4 --text 'Plan and detail'
target/desktop/debug/forge layout list target/annotated.forge.json --json
target/desktop/debug/forge plot target/annotated.forge.json target/sheet.svg --layout 'Plan A'
```

Paper positions are millimetres from the top left. Frames must fit within sheet margins. Each viewport has its own model center and positive scale in paper millimetres per model unit. A millimetre model at `0.01` plots at 1:100. `--rotation DEG` turns the model view clockwise about its center while the clipped paper frame remains axis-aligned. Viewports intentionally clip geometry to their frames. The `viewport` command adds or updates the named frame; repeated `--hide-layer ID` flags filter layers for that frame, and `--no-border` suppresses its outline. Notes use paper coordinates and independent text heights.

SVG export expands visible blocks/dimensions without changing the drawing. Saved sheets explicitly select drafting geometry from a mixed project. They do not project 3D solids or workbench models. There are at most 100 sheets per project, 32 viewports and 256 paper notes per sheet, and 1,000,000 plotted primitives across all viewports. Text uses approximate sans-serif metrics. Vendor plot styles and DXF paper-space exchange remain unfinished. Neutral conversions report layout loss and require `--allow-lossy`; native saves retain all sheet data.

## Scene parenting

In Motion, select an object and choose its parent in the inspector, then **Apply object properties**. Poses and keys use parent coordinates. Ancestor scale, XYZ rotation and translation compose when displaying, playing or exporting the scene. Nonuniform scales retain the resulting affine transforms, including shear. Object order does not affect evaluation.

```sh
target/desktop/debug/forge workbench create motion target/motion.forge.json
target/desktop/debug/forge workbench apply target/motion.forge.json target/parented.forge.json \
  --script examples/scene-parenting.json
target/desktop/debug/forge workbench export target/parented.forge.json target/frame.obj --area motion --time 2.5
```

Changing parent keeps the local keyframes and can move the object in world space. Missing parents, cycles and chains beyond 64 levels are refused. **Delete selected object and descendants** removes the subtree as one undo step. Evaluated OBJ contains the resulting frame geometry; it does not retain the hierarchy or animation. Linear/Step/Smoothstep interpolation, inherited visibility and editable array/mirror stacks are available in the inspector. Apply `examples/scene-modifiers.json` after the parenting example to add an animated modifier stack. General deformation, rigs, constraints and production rendering remain unfinished.

## Pocket milling and drilling

In Draft, select rectangles/circles/closed straight polylines for pocket milling or points for drilling. Switch to Make, select **Pocket** or **Drill** as the operation and choose **Add selected Draft operations**. Select the saved pocket, then use **Set islands from Draft selection** to attach selected closed shapes as uncut islands, or **Clear islands** to remove them. Apply tool/stock settings in the inspector. The existing Profile operation continues to support rectangle/circle/closed straight-polyline contours and tool compensation.

```sh
target/desktop/debug/forge workbench apply - target/machining.forge.json --script examples/machining-operations.json
target/desktop/debug/forge workbench export target/machining.forge.json target/machining.nc --area make
target/desktop/debug/forge-craft --product inventor-cam --open target/machining.forge.json
```

Rectangular pockets use raster rows followed by a finishing contour around every wall. Circular pockets start at the center and expand through concentric rings. Polygon pockets use scanline spans of the cutter-radius boundary and finish the complete contour. Pockets with islands split raster spans around each island, retract between disconnected cuts, and finish the outer wall plus every island clearance contour. Step over is positive and cannot exceed tool diameter. Boundaries, narrow polygon regions and island spacing must leave room for the tool. Tool paths stay inside the pocket boundary and outside every island, and rectangular internal corners have cutter-radius fillets. Circular paths use 96 linear segments per ring, leaving the corresponding small chord approximation at the wall; island clearance circles are circumscribed so chord midpoints cannot cut inward. At most 64 non-overlapping islands are accepted per pocket, and islands are refused for non-pocket operations.

Drilling uses an XY point, feeds to each successive depth and retracts vertically to full clearance between pecks. The shared step-down controls peck depth, with the last pass ending at the exact requested final depth. There are no machine-specific canned cycles or tool changes. All operations position in XY at clearance and retract vertically before travel. Stock footprint, depth, numeric posting and motion-count checks run before output. Limits remain 1,000 operations, 1,000 depth passes per operation and 500,000 motions. Output is generic millimetre G-code with no machine acceptance, stock-removal or collision simulation.

## Alignments and stationing

In Terrain, set the existing longitudinal-profile endpoints. Under **Horizontal alignments**, enter a name and choose **Create from profile endpoints**. Select the alignment, edit its start station and ordered XY vertices and segment bulges, and **Apply alignment**. Survey edits and surface rebuilds retain alignment definitions. The blue canvas lines show XY alignments on a reference plane at the highest survey elevation; evaluated profiles provide actual TIN elevations.

Deleting an internal vertex replaces its two adjacent segments with a straight join and preserves every unaffected arc. Deleting an endpoint removes only its adjacent segment. Changes remain staged until Apply and can then be undone together.

```sh
target/desktop/debug/forge workbench create terrain target/survey.forge.json
target/desktop/debug/forge workbench apply target/survey.forge.json target/road.forge.json --script examples/civil-alignment.json
target/desktop/debug/forge workbench station target/road.forge.json --alignment 1 --point 60 55 --json
target/desktop/debug/forge workbench export target/road.forge.json target/profile.csv \
  --area terrain --alignment 1 --samples 101 --offset 0
```

Station increases by true line or circular-arc length from the authored start station. Offsets are positive to the left and negative to the right. At an internal vertex, `point_at` uses the outgoing segment's normal. Nearest-point queries use the earliest segment to break ties, clamp to endpoints and report the perpendicular offset plus full distance to the alignment. Beyond an endpoint these quantities can differ; no extrapolation is implied. Profiles sample stations uniformly, include both ends and retain blank elevations outside the TIN. The CSV columns are `station,x,y,elevation` in project units. Each alignment supports 2–2,048 vertices; a project supports 100 alignments with 20,000 total vertices. Profiles support 2–10,000 samples. Circular segments use one bulge per segment, `tan(signed sweep/4)`; an empty bulge list means straight segments. Positive bulges turn counterclockwise. Offsets that collapse/reverse a circular radius, including its numerical precision limit, are refused. Preview curves are tessellated, but stationing/projection stay analytic. Spiral segments, station equations, corridors and drainage remain unfinished.

## Cut and fill

Under Terrain's **Cut / fill to level datum**, enter a design elevation and calculate the quantities. The result splits each TIN face where it crosses the datum, then integrates positive and negative linear heights separately. This is exact integration for the authored triangulated surface against a horizontal design plane. Voids outside the TIN are excluded.

```sh
target/desktop/debug/forge workbench earthwork target/road.forge.json --datum 15 --json
target/desktop/debug/forge workbench export target/road.forge.json target/earthwork.csv --area terrain --datum 15
```

Cut is material above the datum to remove; fill is material below it to add. Net cut equals cut minus fill. Plan area is in model units squared and volumes in model units cubed: a metre model produces m²/m³; a millimetre model produces mm²/mm³. The CSV includes the project's unit code. Queries and reports do not edit the terrain or add undo history. Comparisons between two arbitrary TIN surfaces and grading optimization remain unfinished.

## Typed project edit scripts

`forge workbench apply INPUT|- OUTPUT --script FILE.json` applies a JSON array of 1–1,000 typed edits, limited to 1 MiB. `-` starts an empty project. The entire script is atomic: a parse, validation or output error leaves input and destination untouched. Edits run in order and validate after each operation, so create referenced objects before assigning parents.

Supported `type` values are `extrusion`, `extrusion_remove`, `scene_object`, `scene_parent`, `scene_keyframe`, `scene_remove`, `alignment`, `alignment_remove`, `cam_operation`, `cam_remove`, `cam_setup`, `layout` and `building`. Object/operation/alignment edits upsert by ID; layouts upsert by case-insensitive name. Unknown fields, invalid references and unsupported geometries are refused. A `cam_operation` pocket profile may include an `islands` array of closed rectangle, circle or straight-polyline geometries. `cam_setup` accepts a `settings` object containing any of `tool_diameter`, `depth`, `step_down`, `step_over`, `safe_z`, `feed`, `plunge_feed`, `spindle_rpm`, `stock_min` and `stock_max`; omitted settings retain their current values. All unrelated project areas remain intact.

Core and CLI acceptance cases are in `crates/forge-core/tests/remaining_features.rs` and `apps/forge-cli/tests/remaining_cli.rs`. Desktop tests cover reversible edits and rejected changes. Native launch checks cover the seven workbench views; native pointer/keyboard automation primarily exercises drafting. These checks establish the documented cases, not full product or machine compatibility.

Run `python3 validation/advanced_workflows.py` after building to regenerate the documented examples and independently render the two-viewport SVG using system librsvg/Cairo. It records artifact hashes in `validation/results/advanced-workflows.json` and writes the printable sheet/PNG preview, native projects, G-code, CSV reports and evaluated OBJ under `target/feature-acceptance`.
