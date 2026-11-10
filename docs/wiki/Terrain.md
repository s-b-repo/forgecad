# Terrain

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

Surface OBJ exports derived triangles. Contour DXF exports 2D XY line geometry on layers named `Contour {level}`; elevation is retained in that layer name, rather than a Z coordinate. It does not encode 3D contour polylines or a Civil 3D semantic surface. Horizontal line/circular-arc alignments retain editable stations/vertices/bulges, provide left-positive station/offset queries and sample terrain profiles. Cut/fill quantities integrate the TIN exactly against a level design datum. See the [workflow guide](Advanced-workflows) for controls, reports and bounds. Breaklines, constrained triangulation, grading rules, spiral alignments/corridors, drainage, utility networks and Civil 3D native formats remain unfinished.

[Advanced workflows](Advanced-workflows) · [Feature status](Feature-status)
