#!/usr/bin/env python3
"""Run the supplied audit-bad-patterns.sh over every workspace Rust file.

The original script is unchanged. Its raw strict result includes test assertions,
intentional defaults and legacy invariants. Default mode records the findings;
--strict propagates that original gate rather than claiming a clean audit.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--strict", action="store_true")
    options = parser.parse_args()
    script = ROOT / "audit-bad-patterns.sh"
    if not script.is_file():
        raise SystemExit("The supplied audit-bad-patterns.sh is missing")
    paths = subprocess.check_output(["rg", "--files", "apps", "crates", "validation", "-g", "*.rs", "-g", "!**/target/**"], cwd=ROOT, text=True).splitlines()
    paths = sorted(set(paths))
    if not paths:
        raise SystemExit("No workspace Rust files found; refusing an empty audit")
    file_list = ROOT / "target/audit-rust-files.txt"
    file_list.parent.mkdir(parents=True, exist_ok=True)
    file_list.write_text("\n".join(str(ROOT / p) for p in paths) + "\n")
    result = subprocess.run(["bash", str(script), "--strict", "--files", str(file_list)], cwd=ROOT, capture_output=True, text=True, timeout=120)
    if result.returncode not in (0, 1) or "TOTALS" not in result.stdout:
        raise SystemExit(f"Audit execution failed: {result.stderr or result.stdout}")
    sections = {}
    for label, description, hits, hit_patterns, patterns in re.findall(r"^  ([A-P])\s+(.+?)\s+:\s+(\d+) hits across\s+(\d+)/(\d+)\s+patterns", result.stdout, re.M):
        sections[label] = {"description": description.strip(), "hits": int(hits), "patterns_hit": int(hit_patterns), "patterns": int(patterns)}
    strict = sum(sections[s]["hits"] for s in ["A", "B", "C", "L", "M", "N", "O"])
    report = {
        "script": "audit-bad-patterns.sh", "script_sha256": hashlib.sha256(script.read_bytes()).hexdigest(),
        "scope": "All workspace Rust source and tests, including embedded test modules; no generated build output or dependency source",
        "files_scanned": len(paths), "raw_strict_passed": result.returncode == 0, "strict_hits": strict,
        "sections": sections,
        "interpretation": "Regex hits are overlapping pattern matches, not distinct confirmed defects. Raw strict findings remain and require contextual review; no suppression markers were added.",
        "source_sha256": {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest() for p in paths},
        "fixes_this_round": [
            "Extrusion creation uses fallible ID allocation; the allocator and numeric command capacity checks include existing extrusion identities",
            "Unsupported extrusion profiles and model command fallthrough return errors instead of unreachable panics",
            "Civil station/projection failure paths return explicit errors and propagate CSV formatting errors",
            "Extrusion cache lookup returns an error and inspector geometry/volume failures are displayed instead of hidden or reported as zero"
        ]
    }
    results = ROOT / "validation/results"
    results.mkdir(parents=True, exist_ok=True)
    (results / "code-audit.log").write_text(result.stdout + result.stderr)
    (results / "code-audit.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"Audited {len(paths)} Rust files: {strict} raw strict pattern hits; raw gate {'passed' if result.returncode == 0 else 'not clean'}. See docs/code-audit.md and validation/results/code-audit.json")
    if options.strict:
        raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
