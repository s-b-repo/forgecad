# Building

The Build workbench retains named storeys, straight walls, rectangular slabs and hosted door/window openings. Storey elevations define wall bottoms and slab tops. Wall endpoints are in XY; thickness and height define their section. Slabs extend below their storey's elevation. Openings use distance along their host wall, width, sill elevation above the storey and height. Doors have zero sill. Openings must remain inside their wall and cannot overlap.

The preview derives wall pieces around openings, so a door/window hole changes the geometry and net quantity. Numeric edits validate references, dimensions and host bounds before committing; invalid edits preserve the document and undo history. Storey/wall relationships remain editable in the native project.

```sh
./target/desktop/debug/forge workbench export target/suite.forge.json target/building.obj --area build
./target/desktop/debug/forge workbench export target/suite.forge.json target/quantities.csv --area build
```

Schedules include storeys, wall gross/opening/net areas, net wall volumes, slab areas/volumes and opening host information. Quantities use model units squared/cubed. A millimetre model produces mm²/mm³; they are not automatically converted to m²/m³. Mesh output does not retain editable storeys or host relationships.

Current building limits include 10,000 total elements and bounded openings per wall. Curved/compound walls, arbitrary slab outlines, families, rooms/schedules beyond these quantities, structural/MEP systems, multi-model coordination, engineering analysis and RVT/RFA/IFC exchange remain unfinished.

[Advanced workflows](Advanced-workflows) · [Feature status](Feature-status)
