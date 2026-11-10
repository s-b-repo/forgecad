# Editable modeling

Model stores primitive parameters, imported meshes and profile extrusions as separate native objects. Native JSON v6 retains each extrusion's profile, name, base elevation, height and identity. Editing parameters derives a fresh preview; it does not replace the source with imported triangles. V1–v5 projects receive an empty extrusion list and retain their original content.

## Desktop extrusion

In Draft, select rectangles, circles or simple closed straight polylines. Switch to Model and expand **Profile extrusion**. Enter a name, base Z and positive height, then choose **Extrude selected Draft profiles**. Every selected profile is copied into an editable feature; the whole action is one undo step. The source Draft entities stay independent. **Add extrusion example** creates a plate without needing a drawing selection.

Select an extrusion in the model tree or canvas. The inspector edits its height, base Z and profile coordinates. **Apply extrusion parameters** validates the entire candidate before one undo step. Invalid dimensions or self-intersecting outlines preserve the existing document and history. **Delete extrusion** and the Delete key remove the feature. Native Save retains the editable definition through reopen.

Coordinates and height use project units. A unit code of 4 denotes millimetres, 6 denotes metres, and 0 denotes unspecified model units. No implicit conversion occurs when copying a Draft profile.

## CLI authoring

```sh
target/desktop/debug/forge model extrude - target/plate.forge.json \
  --rectangle 0 0 80 40 --height 20 --name Plate
target/desktop/debug/forge model inspect target/plate.forge.json --json
target/desktop/debug/forge model set target/plate.forge.json target/taller.forge.json --id 2 --height 30
target/desktop/debug/forge convert target/taller.forge.json target/plate.obj --allow-lossy
```

`--entity ID` copies a profile from an existing native drawing instead of supplying `--rectangle X1 Y1 X2 Y2` or `--circle CX CY RADIUS`. Supply exactly one profile for creation. `model set` accepts a replacement profile as well as `--name`, `--height` and `--base-z`. `model remove INPUT OUTPUT --id ID` deletes one extrusion. Inspect the source to find its actual IDs; a fresh empty document allocates its first feature as ID 2. Existing outputs, including in-place edits, require `--overwrite` or `--force`.

Create the concave bracket and circular boss example:

```sh
target/desktop/debug/forge workbench apply - target/parts.forge.json --script examples/model-parts.json
target/desktop/debug/forge-craft --product inventor --open target/parts.forge.json
```

Typed project edits support `extrusion` with `name`, `profile`, `base_z` and `height`, and `extrusion_remove` with an `id`. New extrusion IDs come from the document allocator so they cannot collide with layers, drawing entities, meshes or primitive solids.

## Geometry and limits

Supported profiles are rectangles, circles and simple closed straight polylines with 3–256 vertices. Concave profiles are triangulated; redundant collinear positions are removed consistently from caps and walls. Clockwise or counterclockwise outlines produce outward-facing closed meshes. Circles retain their analytic native profile and derive 96-sided preview/export boundaries. The resulting volume is the inscribed polygon's volume, rather than an exact analytic circle.

Height is between 0.000001 and 1,000,000,000 model units. Profile coordinates and base elevation stay within ±1,000,000,000, and height must be representable at the chosen base. A project accepts at most 1,000 extrusions, 8,192 total profile vertices and a combined cubic triangulation-work budget of 16,777,216. Limits are checked before expensive geometry work.

Model OBJ/STL export includes primitives, extrusions and imported meshes together. CLI neutral export requires explicit loss authorization for editable parameters, units, drawing content or merged identities. Direct DXF and ordinary drawing plots refuse model content. A saved layout explicitly selects Draft geometry from a mixed project. Native saves retain every workbench together.

Extrusions do not yet supply a general feature dependency graph, sketch constraints, profile holes, boolean cuts/unions, fillets, assemblies or analytic B-rep bodies. The separate `forge brep` STEP commands remain limited to their documented analytic primitive and one-solid exchange workflows. IPT/IAM and F3D/F3Z remain unsupported.
