# Product family map

This is a clean-room roadmap based on the product families in the supplied Autodesk catalog. Product names below identify workflows the project aims to cover; Forge Craft does not contain Autodesk code, assets, or product branding.

| Forge workbench | Product families represented | Current bounded implementation |
| --- | --- | --- |
| Draft | AutoCAD, AutoCAD LT, AutoCAD Architecture/Electrical/Map 3D/Mechanical/MEP/Plant 3D/Raster Design, AutoCAD Web, DWG TrueView/TrueConvert, Design Review, DWF Writer, ObjectARX | 2D editing, numeric command bar/scripts, offsets/arrays/LINE trim-extend, layers, semantic dimensions/blocks, saved viewport sheets and SVG plotting, partial DXF exchange and separate DWG CLI |
| Model | Fusion, Inventor, Alias AutoStudio/Concept/Design/SpeedForm/Surface, ReCap, Meshmixer, VRED, Tinkercad, EAGLE | Early parametric primitives/mesh exchange in desktop; checked analytic STEP primitive and one-solid exchange CLI |
| Build | Architecture, Engineering & Construction Collection, Revit, Revit LT, Revit LT Suite, Revit Live, Forma, FormIt, Advance Steel, Assemble, Navisworks, BIM Collaborate/Pro, Autodesk Docs/Build/Takeoff, BuildingConnected, Fabrication CADmep/CAMduct/ESTmep, Fabrication ESTmep, Point Layout, Structural Bridge Design, Structural Analysis for Revit, Robot Structural Analysis, Vehicle Tracking, Plant Design Suite (discontinued) | Semantic storeys/walls/slabs/hosted openings, numeric authoring, opening-aware geometry, CSV quantities and derived OBJ. Families, coordination and engineering analysis remain planned |
| Terrain | Civil 3D, InfraWorks, InfoDrainage, InfoWorks ICM/WS Pro, InfoWater Pro, Info360 Asset/Insight, Vehicle Tracking, Recap, ReCap Pro | Editable survey points, checked TIN triangulation, contours, elevation queries/profiles, survey CSV, surface OBJ, contour DXF, polyline station/offset alignments and exact level-datum cut/fill. Curved alignments, corridors, drainage, networks and assets remain planned |
| Make | Fusion Additive Build/Generative Design/Machining/Nesting & Fabrication/Product Design/Simulation/Manage extensions, Inventor CAM/Nesting/Nastran, HSMWorks, FeatureCAM, PowerMill, PowerShape, PowerInspect, Factory Design Utilities, Moldflow Adviser/Communicator/Insight, Netfabb, Helius Composite/PFA, TruFiber/TruLaser/TruNest/TruPlan, Vault, CFD | Stock/tool setup, compensated profiles, rectangular/circular/polygon pockets with optional uncut islands, peck drilling, depth passes, preview and generic G-code. Adaptive/ramped paths, stock/collision simulation, machine posts and additive workflows remain planned |
| Motion | Maya, Maya LT, 3ds Max, MotionBuilder, Mudbox, Flame/Assist, Flare, Arnold, Lustre, VRED variants, Flow Capture/Production Tracking/Studio, FBX/Review, Media & Entertainment Collection | Colored primitive scenes, parent hierarchies, transform keyframes, playback/scrubbing and evaluated-frame OBJ. Rigs, modifiers, dynamics, shader materials, compositing and production rendering remain planned |
| Connect | Autodesk Platform Services, Autodesk Docs/Build, Fusion Team, BIM Collaborate/Pro, Vault Office/Professional, Flow Production Tracking, The Wild, Prospect, A360 | Local project review issues, priorities, status lifecycle, search and CSV; account/cloud collaboration services remain unfinished |

Other catalog entries include collections, extensions, platform utilities, cloud services, education tools, and discontinued products. They should map to the closest workbench and remain separately identified by format, platform, eligibility, and lifecycle status where that affects the workflow.

Full compatibility and feature parity are long-term goals, not current capabilities.

## Extended catalog register

The full supplied list is recorded here so later work can track distinct product, edition, extension, and lifecycle requirements. These are targets for clean-room alternatives, not current feature claims.

### Draft and viewing

AutoCAD; AutoCAD for Mac; AutoCAD LT; AutoCAD LT for Mac; AutoCAD Architecture; AutoCAD Design Suite (discontinued); AutoCAD Electrical; AutoCAD Map 3D; AutoCAD Mechanical; AutoCAD MEP; AutoCAD Plant 3D; AutoCAD Raster Design; AutoCAD Revit LT Suite; AutoCAD Web; DWF Writer; DWG TrueConvert; DWG TrueView; Design Review; FBX; FBX Review; ObjectARX.

### Building and infrastructure

Architecture, Engineering & Construction Collection; Advance Steel; Autodesk Build; Autodesk Docs; Autodesk Takeoff; Assemble; BIM Collaborate; BIM Collaborate Pro; BuildingConnected; Building Design Suite (discontinued); Civil 3D; Constructware (renewal only); Fabrication CADmep; Fabrication CAMduct; Fabrication ESTmep; Forma; FormIt; Info360 Asset; Info360 Insight; InfoDrainage; InfoWorks ICM; InfoWorks WS Pro; InfoWater Pro; Infrastructure Design Suite (discontinued); InfraWorks; Insight; Navisworks Freedom; Navisworks Manage; Navisworks Simulate; Plant Design Suite (discontinued); Point Layout; Revit; Revit Live; Revit LT; Robot Structural Analysis Professional; Simulation Robot Structural Analysis; Structural Analysis for Revit; Structural Bridge Design; Vehicle Tracking; The Wild; Prospect by IrisVR.

### Product design and manufacturing

Alias AutoStudio; Alias Concept; Alias Design; Alias SpeedForm; Alias Surface; EAGLE; Factory Design Utilities; Fusion; Fusion 360 Additive Build Extension; Fusion 360 Generative Design Extension; Fusion 360 Machining Extension; Fusion 360 Manage Extension; Fusion 360 Nesting & Fabrication Extension; Fusion 360 Product Design Extension; Fusion 360 Simulation Extension; Fusion 360 Manage with Upchain; Fusion Team; Fusion with PowerInspect; Fusion with PowerMill; Fusion with PowerShape; Helius Composite; Helius PFA; HSMWorks; Inventor; Inventor CAM; Inventor Engineer-to-Order; Inventor Nastran; Inventor Nesting; Moldflow Adviser; Moldflow Communicator; Moldflow Insight; Netfabb; Product Design & Manufacturing Collection; Product Design Suite (discontinued); TruFiber (renewal only); TruLaser (renewal only); TruNest; TruPlan (renewal only); Vault Office; Vault Professional; Within Medical.

### Media and visualization

3ds Max; Arnold; Autodesk Rendering; Character Generator (discontinued); Entertainment Creation Suite (discontinued); Flame; Flame Assist; Flare; Flow Capture; Flow Production Tracking; Flow Studio; Lustre; Maya; Maya LT; Media & Entertainment Collection; Motion FX; MotionBuilder; Mudbox; ReCap; ReCap Pro; ReMake (discontinued); Showcase; Simulation Mechanical; SketchBook for Enterprise (discontinued); SketchBook Pro (discontinued); Stingray; VRED; VRED Design; VRED Presenter; VRED Professional; VRED Server.

### Platform and other services

A360; Autodesk Platform Services (formerly Forge); Autodesk CFD; Dynamo Studio (discontinued); Instructables; Tinkercad.
