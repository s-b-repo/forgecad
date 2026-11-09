# File compatibility ledger

The requested end state includes Autodesk native formats and complete editable content. The currently implemented formats cover a subset. Native feature histories, assemblies, BIM relationships, scene controllers, plug-in data, and external references require their own models and verification.

| Format | Current implementation | Explicit limits / remaining evidence |
| --- | --- | --- |
| Forge JSON | Versioned document serialization, validation, atomic save, v1 migration, v2 meshes, v3 semantic dimensions | Project-specific format; does not establish Autodesk native compatibility |
| ASCII DXF | LINE, CIRCLE, ARC, POINT, LWPOLYLINE, planar POLYLINE, basic TEXT, INSERT and BLOCK definitions with base points and nesting; layers, visibility, RGB colors, units, bulges; AC1021 output | No binary DXF, hatches, layouts, xrefs, splines, proxy/custom objects, or full styling; semantic DIMENSION entities are retained natively but refused on DXF export instead of being flattened silently. Unsupported input is reported and strict import refuses loss |
| STL | ASCII/binary triangle geometry, primitive tessellation, validation and round trips | Mesh geometry only; no feature history, assemblies, units, materials, color attributes or editable analytic surfaces |
| OBJ | Vertex positions and polygon triangulation, including negative indices | Triangle geometry only; texture/material/normal/group semantics are not retained; freeform records refused |
| DWG / DWT / DWS | Native read and write through the unmodified acadrust 0.6.8 engine, exposed by the `forge native` CLI commands (inspect, convert, transform); R2000 (AC1015) and R2018 (AC1032) write targets; every save re-reads the output and compares the entity, layer and block inventory | Engine-supported entity subset only; saves are refused when the engine reports unsupported or unrecovered content; version conversion is refused for version-locked data; handles are the engine's identifiers; no acceptance corpus checked in AutoCAD itself. The desktop editor keeps its documented DXF subset and does not open DWG directly |
| RVT / RFA / RTE / RFT | Unimplemented | Requires semantic BIM models, families, relationships, parameters, views and versioned native-file verification |
| IPT / IAM / IDW / IPN / IDE | Unimplemented | Requires parametric parts, assembly references/features, drawings, presentations and iFeatures |
| F3D / F3Z / F2D | Unimplemented | Requires feature timelines, components, drawings, configurations and external references |
| MAX | Unimplemented | Complete scene semantics, object stacks, controllers, materials and references |
| MA / MB | Unimplemented | Maya scene/node/dependency semantics, animation, dynamics, reference and plug-in preservation |
| FBX | Unimplemented | Geometry plus scene hierarchy, materials, skins, rigs, animation and format-version coverage |
| NWD / NWC / NWF, RCS / RCP, WIRE, DWF / DWFx | Unimplemented | Product-specific models and native acceptance fixtures required |
| STEP / IGES / SAT, IFC, USD / Alembic | Unimplemented | Interchange coverage is also required; it does not replace the native formats above |

The core tests include hand-authored exchange fixtures and round trips. Passing these tests does not prove interoperability with every Autodesk release or preserve unsupported content.

## Why native data remains a separate requirement

Autodesk identifies `.ipt`, `.iam`, `.idw`, `.ipn`, and `.ide` as related files carrying different Inventor data, including dependency updates; a flattened mesh cannot retain those relationships. [Autodesk Inventor file types](https://help.autodesk.com/cloudhelp/2014/ENU/Inventor/files/GUID-AB438E7A-ADD2-4B42-96E1-2B22953385F4.htm)

Fusion's native archive retains timeline information and assemblies/external references have distinct archive handling. The open format exporters also have their own limitations. [Fusion export options](https://help.autodesk.com/view/fusion360/ENU/?caas=caas%2Fsfdcarticles%2Fsfdcarticles%2FExport-format-options-for-Fusion-360.html), [Fusion export design documentation](https://help.autodesk.com/cloudhelp/ENU/Fusion-Designs/files/ASM-EXPORT-DESIGN.htm)

The MAX scene format includes objects, modifiers, materials and controllers. Maya documents MA/MB as the formats that preserve all scene information. Geometry-only OBJ/STL exchange therefore leaves native scene compatibility unfinished. [MAX scene SDK documentation](https://help.autodesk.com/cloudhelp/2025/ENU/MAXDEV-Developer/files/3ds_max_sdk_features/scenes_and_nodes/scene_file_max_file.html), [Maya supported export formats](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-ManagingScenes/files/GUID-864BD203-C437-4481-8BFC-3A6C1D2C824C.htm)

Autodesk's RealDWG SDK provides native DWG/DXF database access. No RealDWG is integrated here; DWG support instead routes through the unmodified, independently licensed acadrust engine (MPL-2.0) with the limits recorded in the table above. RVT/RFA, IPT/IAM, F3D/F3Z, MAX, MA/MB and FBX still have no integrated engine at all. [Autodesk RealDWG API](https://aps.autodesk.com/developer/overview/realdwg-api)

DXF implementation follows Autodesk's documented entity and group-code reference, with limitations reported explicitly. [DXF entities](https://help.autodesk.com/cloudhelp/2020/ENU/AutoCAD-DXF/files/GUID-748FC305-F3F2-4F74-825A-61F04D757A50.htm), [DXF polyline bulges](https://help.autodesk.com/cloudhelp/2015/ENU/AutoCAD-DXF/files/GUID-D94802B0-8BE8-4AC9-8054-17197688AFDB.htm)
