#!/usr/bin/env python3
"""Tie current verification logs to exact source and local binary artifacts."""
import datetime
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
RESULTS = ROOT / "validation/results"


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def main():
    tests = (RESULTS / "workspace-tests.log").read_text()
    build = (RESULTS / "workspace-build.log").read_text()
    suites = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed", tests)
    assert suites and "test result: FAILED" not in tests and "error:" not in tests
    assert "Finished" in build and "error:" not in build
    startup = json.loads((RESULTS / "native-smoke.json").read_text())
    interaction = json.loads((RESULTS / "native-interaction.json").read_text())
    assert len(startup["checks"]) == 5 and all(check["exit_code"] == 0 for check in startup["checks"])
    assert len(interaction["checks_passed"]) == 6
    source_files = [ROOT / "Cargo.toml", ROOT / "Cargo.lock"]
    for folder in ["apps", "crates", "validation"]:
        source_files += [path for path in (ROOT / folder).rglob("*") if path.is_file() and path.suffix in [".rs", ".toml", ".py", ".json"] and "results" not in path.parts and "__pycache__" not in path.parts]
    artifacts = {}
    for name in ["forge-studio", "forge"]:
        path = ROOT / "target/desktop/debug" / name
        artifacts[str(path.relative_to(ROOT))] = {"sha256": digest(path), "bytes": path.stat().st_size}
    report = {
        "generated_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "objective_complete": False,
        "product_targets": len(json.loads((ROOT / "crates/forge-core/data/products.json").read_text())),
        "workspace_tests_passed": sum(int(passed) for passed, _ in suites),
        "workspace_tests_failed": sum(int(failed) for _, failed in suites),
        "native_startup_cases": startup["checks"],
        "native_interaction_cases": interaction["checks_passed"],
        "verification_limits": ["Linux build/startup tested; Windows/macOS installers not verified", "BIM/civil/media/manufacturing/service-specific product workflows remain incomplete", "Autodesk native DWG/RVT/IPT/F3D/MAX/MA/MB/FBX compatibility is not implemented", "Current tests cover documented subsets; they do not establish full product parity"],
        "artifacts": artifacts,
        "source_sha256": {str(path.relative_to(ROOT)): digest(path) for path in sorted(set(source_files))},
    }
    (RESULTS / "verification.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"{report['workspace_tests_passed']} tests passed; 5 native startup cases; 6 native editing/save/reopen checks; {report['product_targets']} product targets; full goal remains open")


if __name__ == "__main__":
    main()
