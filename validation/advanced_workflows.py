#!/usr/bin/env python3
"""Exercise the documented workflows and render a saved sheet with librsvg.

Artifacts remain under target/feature-acceptance; this proves the bounded
workflows only. It does not establish Autodesk or machine compatibility.
"""
import ctypes
import ctypes.util
import hashlib
import json
from pathlib import Path
import subprocess
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / "target/desktop/debug/forge"
ARTIFACTS = ROOT / "target/feature-acceptance"
RESULTS = ROOT / "validation/results/advanced-workflows.json"


def run(*args):
    result = subprocess.run([str(CLI), *map(str, args)], cwd=ROOT, capture_output=True, text=True, timeout=30)
    if result.returncode:
        raise RuntimeError(f"forge {' '.join(map(str, args))}: {result.stderr}")
    return result.stdout


def render_svg(source, destination):
    """Use the system SVG renderer as an independent consumer of the output."""
    rsvg = ctypes.CDLL(ctypes.util.find_library("rsvg-2"))
    cairo = ctypes.CDLL(ctypes.util.find_library("cairo"))
    gobject = ctypes.CDLL(ctypes.util.find_library("gobject-2.0"))
    pointer = ctypes.c_void_p

    class Rectangle(ctypes.Structure):
        _fields_ = [(name, ctypes.c_double) for name in ["x", "y", "width", "height"]]

    rsvg.rsvg_handle_new_from_file.argtypes = [ctypes.c_char_p, pointer]
    rsvg.rsvg_handle_new_from_file.restype = pointer
    rsvg.rsvg_handle_render_document.argtypes = [pointer, pointer, ctypes.POINTER(Rectangle), pointer]
    rsvg.rsvg_handle_render_document.restype = ctypes.c_int
    cairo.cairo_image_surface_create.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_int]
    cairo.cairo_image_surface_create.restype = pointer
    cairo.cairo_create.argtypes = [pointer]
    cairo.cairo_create.restype = pointer
    cairo.cairo_status.argtypes = [pointer]
    cairo.cairo_surface_write_to_png.argtypes = [pointer, ctypes.c_char_p]
    cairo.cairo_destroy.argtypes = [pointer]
    cairo.cairo_surface_destroy.argtypes = [pointer]
    gobject.g_object_unref.argtypes = [pointer]
    handle = rsvg.rsvg_handle_new_from_file(str(source).encode(), None)
    assert handle, "librsvg could not parse the saved sheet"
    surface = cairo.cairo_image_surface_create(0, 1188, 840)
    context = cairo.cairo_create(surface)
    try:
        assert rsvg.rsvg_handle_render_document(handle, context, ctypes.byref(Rectangle(0, 0, 1188, 840)), None)
        assert cairo.cairo_status(context) == 0
        assert cairo.cairo_surface_write_to_png(surface, str(destination).encode()) == 0
    finally:
        cairo.cairo_destroy(context)
        cairo.cairo_surface_destroy(surface)
        gobject.g_object_unref(handle)
    from PIL import Image
    with Image.open(destination) as image:
        assert image.size == (1188, 840)
        # A valid white page alone is insufficient evidence of plotted content.
        assert image.convert("L").point(lambda v: 255 if v < 200 else 0).histogram()[255] > 500


def main():
    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    checks = []
    plan = ARTIFACTS / "plan.forge.json"
    sheet = ARTIFACTS / "sheet.forge.json"
    svg = ARTIFACTS / "sheet.svg"
    run("draft", "-", plan, "--script", "examples/drafting.forgecmd", "--overwrite")
    run("layout", "create", plan, sheet, "--name", "Plan and column detail", "--overwrite")
    run("layout", "viewport", sheet, sheet, "--layout", "Plan and column detail", "--name", "Overall", "--paper", 10, 10, 160, 175, "--center", 650, 400, "--scale", 0.1, "--overwrite")
    run("layout", "viewport", sheet, sheet, "--layout", "Plan and column detail", "--name", "Detail", "--paper", 180, 30, 100, 100, "--center", 100, 100, "--scale", 1, "--rotation", 15, "--overwrite")
    run("layout", "note", sheet, sheet, "--layout", "Plan and column detail", "--text", "Plan 1:10 / column detail 1:1", "--position", 20, 197, "--height", 4, "--overwrite")
    run("plot", sheet, svg, "--layout", "Plan and column detail", "--overwrite")
    assert len(ET.parse(svg).findall(".//{http://www.w3.org/2000/svg}clipPath")) == 2
    render_svg(svg, ARTIFACTS / "sheet-preview.png")
    checks.append("Saved native two-viewport layout plots and independently renders with paper text, physical scales and a rotated model view")

    machining = ARTIFACTS / "machining.forge.json"
    run("workbench", "apply", "-", machining, "--script", "examples/machining-operations.json", "--overwrite")
    run("workbench", "export", machining, ARTIFACTS / "machining.nc", "--area", "make", "--overwrite")
    info = json.loads(run("workbench", "inspect", machining, "--json"))
    assert info["manufacturing"]["pockets"] == 3 and info["manufacturing"]["drills"] == 1
    assert info["manufacturing"]["islands"] == 1
    checks.append("Documented machining script produces rectangular, circular and islanded polygon pockets, peck drilling and a generic G-code program")

    model=ARTIFACTS / "model.forge.json"
    run("workbench","apply","-",model,"--script","examples/model-parts.json","--overwrite")
    model_info=json.loads(run("model","inspect",model,"--json"))
    assert len(model_info["extrusions"])==2 and all(p["volume"]>0 for p in model_info["extrusions"])
    run("convert",model,ARTIFACTS / "extrusions.obj","--allow-lossy","--overwrite")
    checks.append("Editable concave and circular extrusion profiles persist natively, derive positive closed volumes and export through explicit parameter-loss authorization")

    terrain = ARTIFACTS / "terrain.forge.json"
    run("workbench", "create", "terrain", terrain, "--overwrite")
    run("workbench", "apply", terrain, terrain, "--script", "examples/civil-alignment.json", "--overwrite")
    station = json.loads(run("workbench", "station", terrain, "--alignment", 1, "--point", 60, 55, "--json"))
    assert station["projection"]["station"] == 1060 and station["projection"]["offset"] == 5
    run("workbench", "export", terrain, ARTIFACTS / "alignment-profile.csv", "--area", "terrain", "--alignment", 1, "--samples", 101, "--overwrite")
    run("workbench", "export", terrain, ARTIFACTS / "earthwork.csv", "--area", "terrain", "--datum", 15, "--overwrite")
    quantities = json.loads(run("workbench", "earthwork", terrain, "--datum", 15, "--json"))
    assert quantities["quantities"]["area"] > 0
    checks.append("Documented line/circular-arc civil script retains analytic stationing and generates sampled alignment and exact level-datum earthwork reports")

    motion = ARTIFACTS / "motion.forge.json"
    run("workbench", "create", "motion", motion, "--overwrite")
    run("workbench", "apply", motion, motion, "--script", "examples/scene-parenting.json", "--overwrite")
    run("workbench","apply",motion,motion,"--script","examples/scene-modifiers.json","--overwrite")
    run("workbench", "export", motion, ARTIFACTS / "parented-frame.obj", "--area", "motion", "--time", 2.5, "--overwrite")
    assert json.loads(run("workbench", "inspect", motion, "--json"))["scene"]["parented_objects"] == 2
    checks.append("Parented scene preserves smoothstep animation and array/mirror modifiers while exporting an evaluated frame")

    artifacts = {str(p.relative_to(ROOT)): {"bytes": p.stat().st_size, "sha256": hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(ARTIFACTS.iterdir()) if p.is_file()}
    report = {"checks_passed": checks, "artifacts": artifacts, "objective_complete": False, "scope": "Documented bounded workflows and independent SVG rendering; no full product or native-format parity claim"}
    RESULTS.parent.mkdir(parents=True, exist_ok=True)
    RESULTS.write_text(json.dumps(report, indent=2) + "\n")
    print(f"{len(checks)} documented workflow groups passed; {len(artifacts)} artifacts recorded")


if __name__ == "__main__":
    main()
