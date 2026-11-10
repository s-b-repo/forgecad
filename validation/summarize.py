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
    assert len(startup["checks"]) == 7 and all(check["exit_code"] == 0 for check in startup["checks"])
    assert {check["product"] for check in startup["checks"]} == {"autocad", "inventor", "revit", "maya", "civil-3d", "inventor-cam", "autodesk-docs"}
    for check in startup["checks"]:
        assert digest(ROOT / check["input"]) == check["source_sha256"]
        preview = check["preview"]
        assert preview["width"] > 0 and preview["height"] > 0
        assert digest(ROOT / preview["path"]) == preview["sha256"]
    assert len(interaction["checks_passed"]) == 9
    advanced = json.loads((RESULTS / "advanced-workflows.json").read_text())
    assert len(advanced["checks_passed"]) == 4
    for name, artifact in advanced["artifacts"].items():
        assert digest(ROOT / name) == artifact["sha256"]
    lint = (RESULTS / "remaining-clippy.log").read_text()
    assert "Finished" in lint and "error:" not in lint
    source_files = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", ROOT / "README.md"]
    for folder in ["apps", "crates", "validation", "examples", "docs"]:
        source_files += [path for path in (ROOT / folder).rglob("*") if path.is_file() and path.suffix in [".rs", ".toml", ".py", ".json", ".md", ".forgecmd"] and not {"results", "target", "__pycache__"}.intersection(path.relative_to(ROOT).parts)]
    artifacts = {}
    for name in ["forge-craft", "forge"]:
        path = ROOT / "target/desktop/debug" / name
        artifacts[str(path.relative_to(ROOT))] = {"sha256": digest(path), "bytes": path.stat().st_size}
    for check in startup["checks"]:
        preview = check["preview"]
        image = ROOT / preview["path"]
        artifacts[preview["path"]] = {"sha256": preview["sha256"], "bytes": image.stat().st_size, "width": preview["width"], "height": preview["height"]}
    artifacts.update(advanced["artifacts"])
    report = {
        "generated_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "objective_complete": False,
        "product_targets": len(json.loads((ROOT / "crates/forge-core/data/products.json").read_text())),
        "workspace_tests_passed": sum(int(passed) for passed, _ in suites),
        "workspace_tests_failed": sum(int(failed) for _, failed in suites),
        "native_project_format_version": 5,
        "clippy_status": "passed with style warnings" if "warning:" in lint else "passed",
        "advanced_workflow_cases": advanced["checks_passed"],
        "native_startup_cases": startup["checks"],
        "native_interaction_cases": interaction["checks_passed"],
        "verification_limits": ["Linux build/startup tested; Windows/macOS installers not verified", "Building workflows cover storeys, walls, rectangular slabs, door/window openings and quantity schedules; families, coordination, structural/energy analysis and full BIM remain incomplete", "Civil workflows cover bounded surveyed TIN surfaces, elevation interpolation, contours, profiles, polyline alignments/stationing and exact level-datum cut/fill; curved alignments, roads, utility networks, drainage and geospatial/project standards remain incomplete", "Motion workflows cover colored primitive objects, checked parent hierarchies and evaluated transform keyframes; rigs, materials/shaders, dynamics, production rendering and general scene dependency graphs remain incomplete", "Manufacturing covers bounded 2.5D profiles, cutter compensation, rectangular/circular/polygon pockets with optional uncut islands, peck drilling, stepdowns and generic G-code; adaptive/ramped paths, machine-specific postprocessors, stock/collision simulation, assemblies, FEA and CAM product parity remain incomplete", "Connect covers locally saved review issues, descriptions, status/priority and CSV schedules; it has no cloud accounts, messaging, synchronization or vendor service integration", "DWG read/write uses an engine-supported subset with R2000/R2018 output; full native fidelity has not been verified in AutoCAD", "Checked STEP exchange supports one closed manifold solid with one shell, without assemblies, multiple bodies or external references; it remains separate from Forge project documents", "RVT/RFA, IPT/IAM, F3D/F3Z, MAX, MA/MB and FBX formats remain unimplemented", "SVG plotting covers model-space drawings and saved sheets with clipped scaled/rotated viewports and paper notes; DXF paper-space exchange, vendor fonts and full plot styling remain incomplete", "Native screenshots prove populated startup/rendering; domain unit and CLI tests cover the stated semantic workflows, while native interactive verification is limited to drafting", "Current tests cover documented subsets; they do not establish full product parity"],
        "artifacts": artifacts,
        "source_sha256": {str(path.relative_to(ROOT)): digest(path) for path in sorted(set(source_files))},
    }
    (RESULTS / "verification.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"{report['workspace_tests_passed']} tests passed; {len(startup['checks'])} populated native startup cases; {len(interaction['checks_passed'])} native drafting/editing/save/reopen checks; {report['product_targets']} product targets; full goal remains open")


if __name__ == "__main__":
    main()
