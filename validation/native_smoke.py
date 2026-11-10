#!/usr/bin/env python3
"""Launch representative product targets on an owned Xvfb display.

All logs, screenshots and build artifacts live in the workspace. This checks
startup, import and rendering paths; it is not a full interaction/parity test.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
RESULTS = ROOT / "validation" / "results"
RUNTIME = ROOT / "target" / "x11-runtime"
BINARY = ROOT / "target" / "desktop" / "debug" / "forge-craft"
CLI = ROOT / "target" / "desktop" / "debug" / "forge"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def prepare_cases():
    """Create editable, populated workbench fixtures with the built CLI."""
    fixtures = ROOT / "target" / "workbench-smoke"
    fixtures.mkdir(parents=True, exist_ok=True)
    draft = fixtures / "draft.forge.json"
    for command in [
        ["convert", "examples/office-plan.dxf", str(draft), "--overwrite"],
        ["layout", "create", str(draft), str(draft), "--name", "Office plan", "--overwrite"],
    ]:
        generated = subprocess.run([str(CLI), *command], cwd=ROOT, capture_output=True, text=True, timeout=30)
        if generated.returncode != 0:
            raise RuntimeError(f"Could not create layout fixture: {generated.stderr}")
    cases = [
        {"product": "autocad", "input": str(draft.relative_to(ROOT)), "area": "draft"},
        {"product": "inventor", "input": "examples/mesh-box.obj", "area": "model"},
    ]
    expected_counts = {"build": "walls", "terrain": "terrain_triangles", "motion": "scene_objects", "make": "cam_profiles", "connect": "review_items"}
    for product, area in [("revit", "build"), ("maya", "motion"), ("civil-3d", "terrain"), ("inventor-cam", "make"), ("autodesk-docs", "connect")]:
        fixture = fixtures / f"{area}.forge.json"
        generated = subprocess.run([str(CLI), "workbench", "create", area, str(fixture), "--overwrite"], cwd=ROOT, capture_output=True, text=True, timeout=30)
        if generated.returncode != 0:
            raise RuntimeError(f"Could not create {area} native fixture: {generated.stderr}")
        scripts = {"terrain": "examples/civil-alignment.json", "motion": "examples/scene-parenting.json", "make": "examples/machining-operations.json"}
        if area in scripts:
            edited = subprocess.run([str(CLI), "workbench", "apply", str(fixture), str(fixture), "--script", scripts[area], "--overwrite"], cwd=ROOT, capture_output=True, text=True, timeout=30)
            if edited.returncode != 0:
                raise RuntimeError(f"Could not author {area} fixture: {edited.stderr}")
        cases.append({"product": product, "input": str(fixture.relative_to(ROOT)), "area": area})
    for case in cases:
        source = ROOT / case["input"]
        inspected = subprocess.run([str(CLI), "inspect", str(source), "--json"], cwd=ROOT, capture_output=True, text=True, timeout=30)
        if inspected.returncode != 0:
            raise RuntimeError(f"Could not inspect {case['input']}: {inspected.stderr}")
        info = json.loads(inspected.stdout)
        case["content"] = {key: info[key] for key in ["format", "entities", "solids", "meshes", "vertices", "triangles", "units", "workbenches"]}
        case["source_sha256"] = digest(source)
        area = case["area"]
        if area in expected_counts:
            assert info["workbenches"][expected_counts[area]] > 0, f"{area} fixture has no semantic content"
            case["format_version"] = json.loads(source.read_text())["format_version"]
            assert case["format_version"] == 5
        elif area == "draft":
            assert info["entities"] > 0
            assert info["layouts"] == 1
        else:
            assert info["meshes"] > 0 and info["triangles"] > 0
        if area == "terrain":
            assert info["workbenches"]["terrain_alignments"] == 1
        if area == "motion":
            assert info["workbenches"]["parented_objects"] == 1
        if area == "make":
            assert info["workbenches"]["cam_pockets"] == 3
            assert info["workbenches"]["cam_drills"] == 1
            assert info["workbenches"]["cam_islands"] == 1
    return cases


def main():
    from PIL import ImageGrab

    RESULTS.mkdir(parents=True, exist_ok=True)
    RUNTIME.mkdir(parents=True, exist_ok=True, mode=0o700)
    RUNTIME.chmod(0o700)
    display = next(f":{n}" for n in range(161, 190) if not Path(f"/tmp/.X11-unix/X{n}").exists())
    env = dict(os.environ, DISPLAY=display, XDG_RUNTIME_DIR=str(RUNTIME), TMPDIR=str(RUNTIME), LIBGL_ALWAYS_SOFTWARE="1")
    env.pop("WAYLAND_DISPLAY", None)
    env.pop("WAYLAND_SOCKET", None)
    # Fixture creation and inspection need no display and must succeed before
    # graphical startup can count as evidence for a populated workbench.
    cases = prepare_cases()
    records = []
    with (RESULTS / "xvfb.log").open("w") as xvfb_log:
        server = subprocess.Popen(["Xvfb", display, "-screen", "0", "1600x1000x24", "-nolisten", "tcp", "-ac"], stdout=xvfb_log, stderr=subprocess.STDOUT, env=env, cwd=ROOT)
        try:
            socket = Path(f"/tmp/.X11-unix/X{display[1:]}")
            for _ in range(100):
                if server.poll() is not None:
                    raise RuntimeError("Xvfb exited; see validation/results/xvfb.log")
                if socket.exists():
                    break
                time.sleep(0.05)
            else:
                raise RuntimeError("Xvfb did not become ready")
            for case in cases:
                product, source = case["product"], case["input"]
                args = [str(BINARY), "--product", product]
                if source:
                    args += ["--open", source]
                with (RESULTS / f"{product}-startup.log").open("w") as log:
                    result = subprocess.run(args, env=dict(env, FORGE_SMOKE_FRAMES="8"), cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, timeout=30)
                records.append(dict(case, exit_code=result.returncode))
                print(f"{product}: exit {result.returncode}", flush=True)
                if result.returncode != 0:
                    raise RuntimeError(f"{product} startup failed")
            from native_interaction import exercise
            exercise(ROOT, env)
            print("native pointer/keyboard editing: passed", flush=True)
            for record in records:
                product, source = record["product"], record["input"]
                args = [str(BINARY), "--product", product, "--open", source]
                with (RESULTS / f"{product}-preview.log").open("w") as log:
                    process = subprocess.Popen(args, env=env, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
                    try:
                        time.sleep(2.5)
                        if process.poll() is not None:
                            raise RuntimeError(f"{product} exited before capture")
                        preview = RESULTS / f"{product}-preview.png"
                        capture = ImageGrab.grab(xdisplay=display)
                        capture.save(preview)
                        record["preview"] = {"path": str(preview.relative_to(ROOT)), "width": capture.width, "height": capture.height, "sha256": digest(preview)}
                        print(f"{product}: populated {record['area']} preview captured", flush=True)
                    finally:
                        if process.poll() is None:
                            process.terminate()
                            try:
                                process.wait(timeout=5)
                            except subprocess.TimeoutExpired:
                                process.kill()
                                process.wait()
        finally:
            server.terminate()
            server.wait(timeout=5)
            (RESULTS / "native-smoke.json").write_text(json.dumps({"checks": records, "scope": "seven populated native startup/import/render cases with source inventories and screenshot hashes; drafting interaction is recorded separately; no claim of full product parity"}, indent=2) + "\n")
    assert len(records) == len(cases) == 7 and all(record["exit_code"] == 0 and "preview" in record for record in records)


if __name__ == "__main__":
    main()
