# Building, terrain, motion and manufacturing

Build, Terrain, Motion and Make store separate semantic models in native Forge JSON v6. Connect retains local review issues, and Draft retains saved print layouts. Each workbench has authoring controls, a derived preview and dedicated exports. Native saves keep all areas together, including unrelated drawing/model content. Earlier v1–v3 projects migrate with empty workbench models. V4 projects retain their workbench data and receive empty alignments/layouts, world-space scene parents, profile-operation defaults and empty pocket-island lists. V5 and earlier projects receive empty extrusions, straight alignment segments, linear scene interpolation, visible objects and empty modifier stacks. Derived OBJ, CSV, DXF and G-code carry only their selected output; native saves retain editable relationships and parameters.

Create a project containing all five semantic examples, then open one of its workbenches:

```sh
./target/desktop/debug/forge workbench create all target/suite.forge.json
./target/desktop/debug/forge-craft --product revit --open target/suite.forge.json
./target/desktop/debug/forge-craft --product civil-3d --open target/suite.forge.json
./target/desktop/debug/forge-craft --product maya --open target/suite.forge.json
./target/desktop/debug/forge-craft --product inventor-cam --open target/suite.forge.json
./target/desktop/debug/forge workbench inspect target/suite.forge.json --json
```

Use `cargo run -p forge-cli --` in place of the binary path when building from source. `workbench create build|terrain|motion|make|connect` creates just one example area. Existing files require `--overwrite` or `--force`. Product names identify compatibility targets; these examples implement the bounded workflows below, without full product parity.

## Building

The Build workbench retains named storeys, straight walls, rectangular slabs and hosted door/window openings. Storey elevations define wall bottoms and slab tops. Wall endpoints are in XY; thickness and height define their section. Slabs extend below their storey's elevation. Openings use distance along their host wall, width, sill elevation above the storey and height. Doors have zero sill. Openings must remain inside their wall and cannot overlap.

The preview derives wall pieces around openings, so a door/window hole changes the geometry and net quantity. Numeric edits validate references, dimensions and host bounds before committing; invalid edits preserve the document and undo history. Storey/wall relationships remain editable in the native project.

```sh
./target/desktop/debug/forge workbench export target/suite.forge.json target/building.obj --area build
./target/desktop/debug/forge workbench export target/suite.forge.json target/quantities.csv --area build
```

Schedules include storeys, wall gross/opening/net areas, net wall volumes, slab areas/volumes and opening host information. Quantities use model units squared/cubed. A millimetre model produces mm²/mm³; they are not automatically converted to m²/m³. Mesh output does not retain editable storeys or host relationships.

Current building limits include 10,000 total elements and bounded openings per wall. Curved/compound walls, arbitrary slab outlines, families, rooms/schedules beyond these quantities, structural/MEP systems, multi-model coordination, engineering analysis and RVT/RFA/IFC exchange remain unfinished.

## Terrain

Add surveyed XYZ points in the Terrain workbench, or replace the survey with the built-in example. Select a point to stage coordinate edits and Apply them. Adding, changing or deleting a survey point clears derived faces; **Rebuild triangulated surface** computes a fresh TIN. Each edit/rebuild is an undo step, and rejected duplicates/degeneracies leave the survey unchanged.

The surface uses deterministic XY Delaunay triangulation, with CCW checked faces. It is a 2.5D surface: one elevation per XY location. Duplicate or numerically indistinguishable XY positions, near-collinear data, inverted/overlapping faces and nonmanifold edges are refused. Surveys are limited to 2,048 points. Finite survey points without triangles can be saved while being edited.

Contours are unsmoothed line segments at multiples of the chosen interval. The inspector queries the exact planar TIN elevation at an XY location and calculates sampled longitudinal profiles between two endpoints. Samples outside the surface remain gaps. Profiles permit 2–10,000 samples; contour requests permit at most 10,000 levels and 100,000 segments.

```sh
./target/desktop/debug/forge workbench export target/suite.forge.json target/surface.obj --area terrain
./target/desktop/debug/forge workbench export target/suite.forge.json target/survey.csv --area terrain
./target/desktop/debug/forge workbench survey target/survey.csv target/survey.forge.json
./target/desktop/debug/forge workbench export target/suite.forge.json target/contours.dxf --area terrain --interval 5
```

You can also supply your own survey CSV at the input path. Rows contain three numeric values, X,Y,Z. The optional header is `x,y,z` or `x,y,elevation`. Blank lines and `#` comment lines are ignored. Derived survey CSV begins with `# units: CODE`, which import preserves; files without that metadata default to millimetres (unit code 4). Metadata must precede point rows. Desktop coordinates follow the project's model units. CSV import immediately triangulates, so it requires at least three non-collinear points.

Surface OBJ exports derived triangles. Contour DXF exports 2D XY line geometry on layers named `Contour {level}`; elevation is retained in that layer name, rather than a Z coordinate. It does not encode 3D contour polylines or a Civil 3D semantic surface. Horizontal line/circular-arc alignments retain editable stations/vertices/bulges, provide left-positive station/offset queries and sample terrain profiles. Cut/fill quantities integrate the TIN exactly against a level design datum. See the [workflow guide](advanced-workflows.md) for controls, reports and bounds. Breaklines, constrained triangulation, grading rules, spiral alignments/corridors, drainage, utility networks and Civil 3D native formats remain unfinished.

## Motion

The Motion workbench stores named box/cylinder/ellipsoid objects with sizes, colors and ordered keyframes. Keys retain time, translation, Euler rotation in degrees and positive XYZ scale. Duration and frame rate define playback sampling; scrubbing or playback changes the viewed time without adding edit history.

By default, translation and scale interpolate linearly. Each Euler angle follows its shortest arc; an exact 180° tie follows the negative direction. Geometry is locally scaled, rotated about X/Y/Z in that order, then translated into its parent space. Ancestor transforms compose through up to 64 levels, retaining shear induced by nonuniform parent scales. Changing parent retains local keys and may change world positions. Cycles and missing parents are refused; deleting an object deletes its descendants in one undo step. Evaluation clamps outside times to the scene range, and the first/last keys hold their poses. A native save preserves colors, keys and parent references.

```sh
./target/desktop/debug/forge workbench export target/suite.forge.json target/frame-at-two.obj --area motion --time 2
```

Evaluated OBJ contains geometry at one time. It does not encode keyframes, colors/materials, scene hierarchy or animation. The current scene has up to 1,000 objects and 10,000 total keyframes, with bounded frame counts. General deformation modifiers, rigging, constraints, dynamics, lighting/shader materials, production rendering, compositing and MAX/MA/MB/FBX exchange remain unfinished.


Objects support Linear, Step and Smoothstep interpolation in the inspector. Step holds the left key until the next authored key; Smoothstep uses an eased interval for position, scale and shortest Euler rotations. Visibility is inherited: hiding a parent hides its descendants without deleting keys.

The editable local-space modifier stack supplies arrays and X/Y/Z mirror planes. Arrays include the original copy. Mirroring reverses face winding so reflected geometry stays outward-facing; Keep original retains both copies. Modifiers apply in list order before object and parent transforms. Use Up/Remove to adjust the stack, then Apply object properties. Limits are eight modifiers and 128 combined copies per object, with a scene-wide derived triangle budget of 5,000,000. Overlapping copies remain separate surfaces; these are not boolean unions.

## Manufacturing

The Make workbench stores machining operations separately from drafting entities. Copying selected Draft shapes converts inches, feet, centimetres and metres to millimetres without changing the source document; unitless values are treated as millimetres, and other unit codes are refused. Profiles accept rectangles, circles and simple closed straight polylines. Pockets accept the same closed shapes, optionally with up to 64 non-overlapping closed islands that are retained and never cut; drilling accepts points. Stock/tool setup includes stock bounds, tool diameter, final depth, step-down, pocket step over, clearance Z, cutting/plunge feeds and spindle speed. Coordinates are millimetres, with stock top Z=0 and material extending toward negative Z.

Center, Inside and Outside compensation derive tool-center paths from each profile. The final pass clamps to the requested depth; depth must remain within the stock. Checked tool footprints must fit the stock's XY bounds. Preview distinguishes rapid and feed motion, retracting to clearance before XY repositioning. Circles are currently sampled as 96-segment paths. Time estimates assume a 3,000 mm/min rapid rate and do not model machine acceleration or material removal.

Rectangular pockets use alternating raster rows plus a wall-finishing contour; circular pockets clear outward from the center through concentric rings. Polygon pockets use scanline intersections of the cutter-radius boundary and finish the complete contour; pockets with islands split each raster row around the islands, retract between disconnected spans, and finish both the outer wall and every island clearance contour. Pocket compensation is Inside, and step over cannot exceed tool diameter. Rectangle sides, circle diameters and narrow polygon regions must exceed tool diameter; internal rectangular corners retain cutter-radius fillets and circular islands are circumscribed by their sampled clearance contour so chords cannot encroach on the island. Drilling uses Center compensation and explicit feed plunges with a full vertical clearance retract between depth pecks. These operations do not emit controller-specific canned cycles. See [examples and limits](advanced-workflows.md).

```sh
./target/desktop/debug/forge workbench export target/suite.forge.json target/profiles.nc --area make
```

Output is a generic LinuxCNC-style profile/pocket/drill program using XY plane, millimetres, absolute coordinates and units-per-minute feeds. The post emits linear rapid/feed moves, starts the spindle and ends the program. These code meanings follow the [LinuxCNC G-code reference](https://linuxcnc.org/docs/html/gcode/g-code.html). Machine work offsets, tooling and controller setup are external to this project; the generic post has no machine acceptance result.

Current bounds include at most 1,000 operations, 64 islands per pocket, 1,000 depth passes per operation and 500,000 planned motions. Adaptive paths, ramps, tool changes, multi-axis machining, machine-specific posts, stock/collision simulation and manufacturing-native file formats remain unfinished.

## Local project review

Connect retains local issues with titles, multiline descriptions, human-readable element references, priorities and Open / In progress / Resolved states. Search and status filters narrow the board. Staged edits, creation and deletion validate before one undo step; native saves preserve issue text and unrelated model data. Element references are descriptive text rather than checked model links.

```sh
./target/desktop/debug/forge workbench create connect target/review.forge.json
./target/desktop/debug/forge-craft --product autodesk-docs --open target/review.forge.json
./target/desktop/debug/forge workbench export target/review.forge.json target/issues.csv --area connect
```

CSV escapes multiline/quoted text and prefixes formula-like user cells for spreadsheet use. The native project retains exact text. This is a local board with no messaging, user accounts, permissions, version-control server or cloud synchronization. Those services and Autodesk service compatibility remain unfinished.

## Remaining suite scope

Draft/Model retain their documented geometry workflows. Connect provides the local review board described above; account, multi-user, cloud synchronization and service-compatible workflows remain unfinished. Autodesk native formats beyond the separate partial DWG engine are still unimplemented. The 140 catalog identities remain separate targets, and every workbench's implementation and file support must be assessed within its stated limits.
