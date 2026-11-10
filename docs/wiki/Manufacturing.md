# Manufacturing

The Make workbench stores machining operations separately from drafting entities. Copying selected Draft shapes converts inches, feet, centimetres and metres to millimetres without changing the source document; unitless values are treated as millimetres, and other unit codes are refused. Profiles accept rectangles, circles and simple closed straight polylines. Pockets accept the same closed shapes, optionally with up to 64 non-overlapping closed islands that are retained and never cut; drilling accepts points. Stock/tool setup includes stock bounds, tool diameter, final depth, step-down, pocket step over, clearance Z, cutting/plunge feeds and spindle speed. Coordinates are millimetres, with stock top Z=0 and material extending toward negative Z.

Center, Inside and Outside compensation derive tool-center paths from each profile. The final pass clamps to the requested depth; depth must remain within the stock. Checked tool footprints must fit the stock's XY bounds. Preview distinguishes rapid and feed motion, retracting to clearance before XY repositioning. Circles are currently sampled as 96-segment paths. Time estimates assume a 3,000 mm/min rapid rate and do not model machine acceleration or material removal.

Rectangular pockets use alternating raster rows plus a wall-finishing contour; circular pockets clear outward from the center through concentric rings. Polygon pockets use scanline intersections of the cutter-radius boundary and finish the complete contour; pockets with islands split each raster row around the islands, retract between disconnected spans, and finish both the outer wall and every island clearance contour. Pocket compensation is Inside, and step over cannot exceed tool diameter. Rectangle sides, circle diameters and narrow polygon regions must exceed tool diameter; internal rectangular corners retain cutter-radius fillets and circular islands are circumscribed by their sampled clearance contour so chords cannot encroach on the island. Drilling uses Center compensation and explicit feed plunges with a full vertical clearance retract between depth pecks. These operations do not emit controller-specific canned cycles. See [examples and limits](Advanced-workflows).

```sh
./target/desktop/debug/forge workbench export target/suite.forge.json target/profiles.nc --area make
```

Output is a generic LinuxCNC-style profile/pocket/drill program using XY plane, millimetres, absolute coordinates and units-per-minute feeds. The post emits linear rapid/feed moves, starts the spindle and ends the program. These code meanings follow the [LinuxCNC G-code reference](https://linuxcnc.org/docs/html/gcode/g-code.html). Machine work offsets, tooling and controller setup are external to this project; the generic post has no machine acceptance result.

Current bounds include at most 1,000 operations, 64 islands per pocket, 1,000 depth passes per operation and 500,000 planned motions. Adaptive paths, ramps, tool changes, multi-axis machining, machine-specific posts, stock/collision simulation and manufacturing-native file formats remain unfinished.

[Advanced workflows](Advanced-workflows) · [Feature status](Feature-status)
