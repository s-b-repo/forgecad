# Progress plan

A workflow is verified only within its documented bounds. Product parity requires a release/platform-specific inventory and originating-application acceptance evidence.

The next implementation priorities are persistent sketch constraints, dependent solid features/booleans, and expanded BIM/civil systems. Milestones below describe acceptance gates; catalog entries or launcher routes do not satisfy them.

## M1 — Native editing foundation

**Status:** Verified within documented bounds

Author, undo, save/reopen and export supported content without silent loss.

Tracked features: `draft-authoring`, `draft-transforms`, `draft-snaps`, `draft-layers`, `draft-blocks`, `draft-dimensions`, `draft-commands`, `model-primitives`, `model-meshes`, `exchange-native`, `exchange-mesh`, `quality-history`, `quality-linux`.

## M2 — Semantic engineering workbenches

**Status:** Verified within documented bounds

Retain domain parameters and references; independently check geometry, quantities, interpolation and toolpaths.

Tracked features: `build-elements`, `build-quantities`, `terrain-tin`, `terrain-contours`, `terrain-earthwork`, `make-profiles`, `make-pockets`, `make-drilling`, `make-preview`, `motion-scene`, `connect-review`.

## M3 — Advanced authoring and maintained documentation

**Status:** Verified within documented bounds

Pass closed-mesh volume/winding, analytic arc stationing, interpolation/modifier, migration, CLI and reversible desktop cases; publish generated wiki, full feature inventory and progress records. Record raw code-audit findings and remediate new production failure paths without hiding legacy debt.

Tracked features: `draft-offset-array`, `draft-trim`, `draft-layouts`, `model-extrusions`, `terrain-alignments`, `motion-animation`, `motion-modifiers`, `quality-audit`.

## M4 — Constraint-driven CAD and solid modeling

**Status:** Planned

Implement persistent sketch constraints with convergence/conflict diagnostics, feature dependencies, robust solid operations and assembly/drawing workflows.

Tracked features: `draft-constraints`, `model-kernel`, `model-assemblies`.

## M5 — Building and infrastructure systems

**Status:** Planned

Add families/spaces, specialist systems, coordination, engineering analysis, corridors, grading, networks, assets and geospatial validation.

Tracked features: `build-families`, `build-analysis`, `build-coordination`, `terrain-grading`, `terrain-assets`, `draft-toolsets`.

## M6 — Production manufacturing

**Status:** Planned

Validate machine posts, tools, ramp/adaptive/multi-axis paths, stock/fixture collision models, additive/nesting and manufacturing analysis.

Tracked features: `make-production`, `make-simulation`, `make-additive`.

## M7 — Media production

**Status:** Planned

Implement rigs, skinning, constraints, shaders, cameras, lighting, production rendering, dynamics and compositing with independent fixtures.

Tracked features: `motion-rigs`, `motion-rendering`, `motion-dynamics`.

## M8 — Connected project services

**Status:** Planned

Implement account/project permissions, multi-user revisions/sync and independently exercised service workflows.

Tracked features: `connect-services`, `connect-vendor`.

## M9 — Interchange and packaged releases

**Status:** Planned

Expand neutral and native interchange; verify metadata/references/editability and clean Windows/macOS/Linux installs and recovery.

Tracked features: `model-step`, `exchange-dxf`, `exchange-dwg`, `exchange-proprietary`, `exchange-open`, `quality-release`.

## M10 — Full product acceptance

**Status:** Pending external acceptance data

Choose each product release and platform; obtain authoritative fixtures and originating applications, inventory every required workflow and record independent acceptance results.

Tracked features: `quality-native-acceptance`.

## Updating this plan

Add or update feature evidence and limits in `feature-status.json`. Move a milestone to verified only after its acceptance cases pass and the current source/build hashes are recorded. Keep incomplete capabilities and external verification requirements explicit. Generate this page and the wiki with `python3 scripts/generate_docs.py`.
