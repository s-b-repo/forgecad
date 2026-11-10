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
- Unsupported extrusion geometry and model-command fallthrough return explicit errors instead of using `unreachable!`.
- Civil endpoint/projection failures return errors, and CSV formatting propagates failure instead of unwrapping.
- An unavailable extrusion cache returns an error. The inspector displays mesh/volume evaluation errors instead of swallowing them or presenting a false zero volume.

Tests cover invalid/exhausted extrusion authoring without mutation, native migration, mesh closure/winding/volume, analytic stationing, CLI overwrite protection and reversible desktop changes.

## Confirmed remaining issue

**Medium priority: legacy authoring can panic at the object ID limit.** In [`Document::allocate_id`](https://github.com/s-b-repo/forgecad/blob/main/crates/forge-core/src/lib.rs), `try_allocate_id().expect(...)` still backs the older infallible geometry, primitive and layer APIs. A native project with a layer ID of `u64::MAX - 1` loads successfully, leaving no capacity for these authoring APIs. Direct primitive creation then panics instead of returning an authoring error. This requires a specially constructed or exhausted project. Numeric drafting commands already preflight ID capacity and return a normal error. The direct API reproduction is recorded in [code-audit-id-exhaustion.json](https://github.com/s-b-repo/forgecad/blob/main/validation/results/code-audit-id-exhaustion.json).

The new extrusion path handles this case through a fallible transaction. Extending that contract to the older public APIs and their desktop/CLI callers remains open. The other raw matches have not all received individual manual review; only the fixes and reproduction described here are confirmed findings.

## Context for remaining pattern groups

| Group | Review context |
| --- | --- |
| A: panic handling | Many matches are test assertions or validated internal invariants. Legacy infallible allocation and remaining production panics need further API work. |
| B: swallowed errors | Error conversion, cleanup in test helpers and optional display paths all match. Review production fallbacks individually; do not blanket-suppress them. |
| C: lint attributes | Existing attributes require justification; no audit suppression comments or new lint blanket allowances were added. |
| D/E: indexing, casts, numeric operations | Geometry uses fixed arrays, validated indices and bounded sizes. Resource/finite-value checks and independent geometry cases are required in addition to regex review. |
| F–J, L/M/O/P | Consult the current raw report; a zero match count does not establish that all possible issues in that category are absent. |
| N: concurrency/UB heuristics | The script also flags `Result<..., String>` parser helper signatures; those signatures alone are not undefined behavior. |

## Follow-up plan

Prioritize fallible public authoring APIs, explicit production fallback/error handling, typed parser errors, and checked conversions where bounds are not already established. Preserve meaningful test assertions. Keep the raw strict result visible, review changes against the same file scope, and require a separate contextual policy before treating these regex counts as an unconditional merge gate.

This is a Rust pattern audit, not a comprehensive security review of Python tooling, native dependencies, proprietary formats or machine programs.
