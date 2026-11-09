#!/usr/bin/env python3
"""Launch representative product targets on an owned Xvfb display.

All logs, screenshots and build artifacts live in the workspace. This checks
startup, import and rendering paths; it is not a full interaction/parity test.
"""
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
RESULTS = ROOT / "validation" / "results"
RUNTIME = ROOT / "target" / "x11-runtime"
BINARY = ROOT / "target" / "desktop" / "debug" / "forge-studio"


def main():
    from PIL import ImageGrab

    RESULTS.mkdir(parents=True, exist_ok=True)
    RUNTIME.mkdir(parents=True, exist_ok=True, mode=0o700)
    RUNTIME.chmod(0o700)
    display = next(f":{n}" for n in range(161, 190) if not Path(f"/tmp/.X11-unix/X{n}").exists())
    env = dict(os.environ, DISPLAY=display, XDG_RUNTIME_DIR=str(RUNTIME), TMPDIR=str(RUNTIME), LIBGL_ALWAYS_SOFTWARE="1")
    env.pop("WAYLAND_DISPLAY", None)
    env.pop("WAYLAND_SOCKET", None)
    cases = [
        ("autocad", "examples/office-plan.dxf"),
        ("inventor", "examples/mesh-box.obj"),
        ("revit", None),
        ("maya", None),
        ("civil-3d", None),
    ]
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
            for product, source in cases:
                args = [str(BINARY), "--product", product]
                if source:
                    args += ["--open", source]
                with (RESULTS / f"{product}-startup.log").open("w") as log:
                    result = subprocess.run(args, env=dict(env, FORGE_SMOKE_FRAMES="8"), cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, timeout=30)
                records.append({"product": product, "input": source, "exit_code": result.returncode})
                print(f"{product}: exit {result.returncode}", flush=True)
                if result.returncode != 0:
                    raise RuntimeError(f"{product} startup failed")
            from native_interaction import exercise
            exercise(ROOT, env)
            print("native pointer/keyboard editing: passed", flush=True)
            for product, source in cases[:2]:
                args = [str(BINARY), "--product", product, "--open", source]
                with (RESULTS / f"{product}-preview.log").open("w") as log:
                    process = subprocess.Popen(args, env=env, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
                    try:
                        time.sleep(2.5)
                        if process.poll() is not None:
                            raise RuntimeError(f"{product} exited before capture")
                        ImageGrab.grab(xdisplay=display).save(RESULTS / f"{product}-preview.png")
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
            (RESULTS / "native-smoke.json").write_text(json.dumps({"checks": records, "scope": "native startup/import/render only; no claim of full product parity"}, indent=2) + "\n")
    assert len(records) == len(cases) and all(record["exit_code"] == 0 for record in records)


if __name__ == "__main__":
    main()
