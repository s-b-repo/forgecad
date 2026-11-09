# Mesh fixtures

These small fixtures are hand authored and contain no Autodesk or third-party model data.

- `tetrahedron-ascii.stl`: closed, outward-facing tetrahedron with volume 1/6.
- `triangle-binary.stl`: one triangle, 80-byte header, little-endian triangle count, 12 float32 values, and a zero attribute field. Built directly from the binary layout, independently of the Forge writer.

Reference layouts: [Library of Congress STL binary description](https://www.loc.gov/preservation/digital/formats/fdd/fdd000505.shtml) and [original Wavefront Advanced Visualizer OBJ appendix](https://paulbourke.org/dataformats/obj/obj_spec.pdf).
