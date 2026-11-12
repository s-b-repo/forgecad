# Code audit

The user-supplied `audit-bad-patterns.sh` is retained unchanged at the repository root. Its default paths assume a script below `scripts/` and a single `src/` tree. Forge Craft is a Cargo workspace, so the wrapper supplies an explicit absolute list of every Rust file under `apps`, `crates` and `validation`, excluding build output and dependency source.

```sh
python3 scripts/run_code_audit.py
python3 scripts/run_code_audit.py --strict
```

Both commands execute the original script in strict mode and save its exact output to `validation/results/code-audit.log`. The JSON companion records the script/source hashes, scope, section counts, raw gate result and reviewed fixes. The wrapper's default mode records findings; `--strict` also propagates the original nonzero gate. A successful informational wrapper run does not mean the raw strict audit passed.

## Findings and remediation

The raw gate is **not clean**. The scan includes test code and reports overlapping patterns, so its hit count is not a count of distinct vulnerabilities. It flags `assert!`, `assert_eq!`, test `unwrap()` calls, intentional optional defaults, fixed-size numeric indexing and bounded mesh conversions. Those signals need contextual review alongside actual production error paths. Legacy production invariants and fallbacks remain audit debt; the repository does not claim all findings are remediated.

This round fixes production issues in the new workflows:

- Extrusion creation allocates IDs through a fallible path. Existing extrusion IDs now participate in the global allocator and numeric drafting's capacity preflight, so these paths reject exhausted ID space without mutating the project.
- The confirmed object-ID exhaustion panic is **fixed**. Authoring is fallible: `add_geometry`, `add_solid`, `add_layer`, `add_mesh`, `add_block` and `duplicate_entities` return an authoring error instead of panicking at the ID limit, and the allocator never reissues an existing ID. The private `try_allocate_id` helper now backs every authoring call, so the old panicking `expect` in `Document::allocate_id` is gone.
- Reachable panic paths were hardened at the remaining call sites: `edit.rs` duplicate/transform paths, `blocks.rs` block definition, `dxf.rs` `read_layer`/`write_entity`/`read_entity`, `command.rs` `Args::take`, `scene.rs` `pose_at`/`validate`, `building.rs` `schedule_csv`, `cam.rs` preflight arms, `plot.rs` and `drafting.rs`.
- Unsupported extrusion geometry and model-command fallthrough return explicit errors instead of using `unreachable!`.
- Civil endpoint/projection failures return errors, and CSV formatting propagates failure instead of unwrapping.
- An unavailable extrusion cache returns an error. The inspector displays mesh/volume evaluation errors instead of swallowing them or presenting a false zero volume.

Tests cover invalid/exhausted extrusion authoring without mutation, authoring APIs that refuse to panic at the object ID limit while leaving the saved bytes identical, native migration, mesh closure/winding/volume, analytic stationing, CLI overwrite protection and reversible desktop changes.

## Resolved finding

**Previously medium priority: legacy authoring could panic at the object ID limit.** [`Document::try_allocate_id`](../crates/forge-core/src/lib.rs) now backs every authoring API with `try_allocate_id()?`, so a native project whose highest ID leaves no capacity is still loadable and rejects further authoring with `Object ID space exhausted` instead of panicking. This required a specially constructed or exhausted project. Numeric drafting commands already preflighted ID capacity and returned a normal error.

The direct API reproduction is re-recorded in [code-audit-id-exhaustion.json](../validation/results/code-audit-id-exhaustion.json): the same loaded project, the same `add_solid` call and the same document snapshot now report an authoring error, no panic and identical saved bytes. The committed unit test `authoring_apis_refuse_to_panic_at_the_object_id_limit` covers `add_geometry`, `add_solid`, `add_layer` and `add_mesh` at the limit and the two-remaining-IDs case. The other raw matches have not all received individual manual review; only the fixes and reproductions described here are confirmed findings.

## Context for remaining pattern groups

| Group | Review context |
| --- | --- |
| A: panic handling | Many matches are test assertions or validated internal invariants. Legacy infallible authoring allocation has been converted to a fallible path, so remaining matches concern test assertions and reviewed invariants. |
| B: swallowed errors | Error conversion, cleanup in test helpers and optional display paths all match. Review production fallbacks individually; do not blanket-suppress them. |
| C: lint attributes | Existing attributes require justification; no audit suppression comments or new lint blanket allowances were added. |
| D/E: indexing, casts, numeric operations | Geometry uses fixed arrays, validated indices and bounded sizes. Resource/finite-value checks and independent geometry cases are required in addition to regex review. |
| F–J, L/M/O/P | Consult the current raw report; a zero match count does not establish that all possible issues in that category are absent. |
| N: concurrency/UB heuristics | The script also flags `Result<..., String>` parser helper signatures; those signatures alone are not undefined behavior. |

## Follow-up plan

Prioritize explicit production fallback/error handling, typed parser errors, and checked conversions where bounds are not already established. Preserve meaningful test assertions. Keep the raw strict result visible, review changes against the same file scope, and require a separate contextual policy before treating these regex counts as an unconditional merge gate.

This is a Rust pattern audit, not a comprehensive security review of Python tooling, native dependencies, proprietary formats or machine programs.
