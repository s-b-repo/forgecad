"""Exercise real pointer/keyboard editing on the smoke runner's owned display."""
import ctypes
import json
from pathlib import Path
import subprocess
import time


def exercise(root: Path, env: dict):
    x11 = ctypes.CDLL("libX11.so.6")
    xtest = ctypes.CDLL("libXtst.so.6")
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
    x11.XFlush.argtypes = [ctypes.c_void_p]
    x11.XSync.argtypes = [ctypes.c_void_p,ctypes.c_int]
    x11.XStringToKeysym.argtypes = [ctypes.c_char_p]
    x11.XStringToKeysym.restype = ctypes.c_ulong
    x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
    x11.XKeysymToKeycode.restype = ctypes.c_uint
    x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
    x11.XDefaultRootWindow.restype = ctypes.c_ulong
    x11.XQueryTree.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)), ctypes.POINTER(ctypes.c_uint)]
    x11.XFetchName.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_char_p)]
    x11.XSetInputFocus.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
    x11.XFree.argtypes = [ctypes.c_void_p]
    xtest.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
    xtest.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
    xtest.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
    display = x11.XOpenDisplay(env["DISPLAY"].encode())
    if not display:
        raise RuntimeError("Could not connect to owned test display")

    def flush():
        x11.XFlush(display)
        x11.XSync(display,0)
        time.sleep(0.07)

    def motion(x, y):
        xtest.XTestFakeMotionEvent(display, -1, x, y, 0)
        flush()

    def button(down):
        xtest.XTestFakeButtonEvent(display, 1, int(down), 0)
        flush()

    def click(x, y):
        motion(x, y)
        button(True)
        button(False)

    def key(name, down):
        code = x11.XKeysymToKeycode(display, x11.XStringToKeysym(name.encode()))
        if not code:
            raise RuntimeError(f"No key code for {name}")
        xtest.XTestFakeKeyEvent(display, code, int(down), 0)
        flush()

    def chord(name, control=False, shift=False):
        if control:
            key("Control_L", True)
        if shift:
            key("Shift_L", True)
        key(name, True)
        key(name, False)
        if shift:
            key("Shift_L", False)
        if control:
            key("Control_L", False)
        # Tool changes replace controls before the canvas. Let the native UI
        # process that frame before injecting the first construction click.
        if not control and name in {"o","t","p","a","d"}:
            time.sleep(0.2)

    def type_text(text):
        names = {" ": "space", "-": "minus", ".": "period"}
        for character in text:
            if character in names:
                name = names[character]
            elif "a" <= character <= "z" or "0" <= character <= "9":
                name = character
            else:
                raise ValueError(f"Unsupported native test character: {character!r}")
            key(name, True)
            key(name, False)

    def command(text):
        type_text(text)
        chord("Return")

    folder = root / "target" / "native-interaction"
    folder.mkdir(parents=True, exist_ok=True)
    project = folder / "drawing.forge.json"
    project.write_text(json.dumps({"format_version": 2, "document": {"name": "Native interaction fixture", "entities": [], "solids": [], "meshes": [], "layers": [{"id": 1, "name": "0", "color": [215, 222, 234], "visible": True}], "active_layer_id": 1, "units": 0, "next_id": 2}}))
    checks = []

    def save_and_read(count):
        chord("s", control=True)
        for _ in range(30):
            doc = json.loads(project.read_text())["document"]
            if len(doc["entities"]) == count:
                return doc
            time.sleep(0.05)
        raise AssertionError(f"Expected {count} entities after native Save; got {len(doc['entities'])}")

    process = None
    try:
        with (root / "validation" / "results" / "native-interaction.log").open("w") as log:
            process = subprocess.Popen([str(root / "target/desktop/debug/forge-craft"), "--product", "autocad", "--open", str(project)], env=env, cwd=root, stdout=log, stderr=subprocess.STDOUT)
            time.sleep(2)
            assert process.poll() is None, "Application exited before interaction"
            root_window = x11.XDefaultRootWindow(display)
            root_return, parent_return = ctypes.c_ulong(), ctypes.c_ulong()
            children = ctypes.POINTER(ctypes.c_ulong)()
            count = ctypes.c_uint()
            x11.XQueryTree(display, root_window, ctypes.byref(root_return), ctypes.byref(parent_return), ctypes.byref(children), ctypes.byref(count))
            focused = False
            try:
                for index in range(count.value):
                    name = ctypes.c_char_p()
                    if x11.XFetchName(display, children[index], ctypes.byref(name)) and name.value:
                        title = name.value.decode(errors="replace")
                        x11.XFree(name)
                        if "Forge Craft" in title:
                            x11.XSetInputFocus(display, children[index], 2, 0)
                            flush()
                            focused = True
                            break
            finally:
                x11.XFree(children)
            assert focused, "Could not focus the owned application window"
            click(140, 207)  # Line tool, measured in the 1480x920 native layout.
            motion(500, 400)
            button(True)
            for step in range(1, 11):
                motion(500 + step * 25, 400 + step * 10)
            button(False)
            line = save_and_read(1)["entities"][0]["geometry"]
            assert line["type"] == "line"
            assert abs(line["end"]["x"] - line["start"]["x"] - 250) < 1e-8, line
            assert abs(line["end"]["y"] - line["start"]["y"] + 100) < 1e-8, line
            checks.append("pointer drag preserves the mouse-down anchor and saves the line")
            chord("z", control=True)
            save_and_read(0)
            chord("z", control=True, shift=True)
            save_and_read(1)
            checks.append("keyboard undo/redo and native save preserve history")
            chord("o")
            click(600, 300)
            assert save_and_read(2)["entities"][-1]["geometry"]["type"] == "point"
            chord("t")
            click(780, 350)
            assert save_and_read(3)["entities"][-1]["geometry"]["type"] == "text"
            chord("p")
            for point in [(900, 250), (1000, 250), (1000, 350)]:
                click(*point)
            chord("Return")
            polyline = save_and_read(4)["entities"][-1]["geometry"]
            assert polyline["type"] == "polyline" and len(polyline["vertices"]) == 3
            chord("a")
            for point in [(950, 600), (1000, 600), (950, 550)]:
                click(*point)
            assert save_and_read(5)["entities"][-1]["geometry"]["type"] == "arc"
            checks.append("point, text, polyline and arc tools author persistent native geometry")
            chord("d")
            for point in [(500, 200), (700, 200), (500, 260)]:
                click(*point)
            dimension = save_and_read(6)["entities"][-1]["geometry"]
            assert dimension["type"] == "dimension", dimension
            assert dimension["dimension"]["dimension_type"] == "linear", dimension
            assert dimension["dimension"]["kind"] == "horizontal", dimension
            checks.append("the dimension tool authors a semantic dimension that native save preserves")
            chord("a", control=True)
            chord("g", control=True)
            grouped = save_and_read(1)
            assert grouped["entities"][0]["geometry"]["type"] == "insert", grouped["entities"]
            assert len(grouped["blocks"]) == 1, grouped["blocks"]
            assert len(grouped["blocks"][0]["entities"]) == 6, grouped["blocks"]
            checks.append("grouping the drawing keeps every object inside one reusable block reference")
            click(300, 847)  # Numeric command field, above its hint at y866.
            command("rectangle 0 0 120 80")
            rectangle = save_and_read(2)["entities"][-1]["geometry"]
            assert rectangle == {"type": "rectangle", "a": {"x": 0.0, "y": 0.0}, "b": {"x": 120.0, "y": 80.0}}, rectangle
            command("offset -5")
            offset = save_and_read(3)
            inset = offset["entities"][-1]["geometry"]
            assert inset == {"type": "rectangle", "a": {"x": 5.0, "y": 5.0}, "b": {"x": 115.0, "y": 75.0}}, inset
            command("arrayrect 3 2 150 100")
            array = save_and_read(8)
            assert array["blocks"] == grouped["blocks"], "Numeric drafting changed the reusable block"
            rectangles = [entity["geometry"] for entity in array["entities"] if entity["geometry"]["type"] == "rectangle"]
            assert len(rectangles) == 7, rectangles
            bounds = {(item["a"]["x"], item["a"]["y"], item["b"]["x"], item["b"]["y"]) for item in rectangles}
            expected = {(0.0, 0.0, 120.0, 80.0)} | {
                (5.0 + column * 150.0, 5.0 + row * 100.0, 115.0 + column * 150.0, 75.0 + row * 100.0)
                for column in range(3) for row in range(2)
            }
            assert bounds == expected, bounds
            checks.append("the focused numeric command bar authors exact rectangles, signed offsets and rectangular arrays")
            command("undo")
            assert save_and_read(3) == offset, "Array undo changed earlier drawing content or IDs"
            command("redo")
            assert save_and_read(8) == array, "Array redo changed persistent drawing content or IDs"
            checks.append("UNDO and REDO commands reverse and restore the entire array in one history step")
            # History restoration clears selection. Select the inset explicitly
            # so this exercises collapsed geometry rather than missing selection.
            command(f"select {offset['entities'][-1]['id']}")
            command("offset -100")
            assert save_and_read(8) == array, "Invalid offset altered drawing content or IDs"
            command("undo")
            assert save_and_read(3) == offset, "Invalid command added an undo history entry"
            command("redo")
            assert save_and_read(8) == array, "Redo after a rejected command changed the drawing"
            checks.append("a collapsing offset is refused without changing document IDs, geometry or undo history")
            assert process.poll() is None, "Application exited during editing"
            process.terminate()
            process.wait(timeout=5)
            reopened = subprocess.run([str(root / "target/desktop/debug/forge-craft"), "--product", "autocad", "--open", str(project)], env=dict(env, FORGE_SMOKE_FRAMES="8"), cwd=root, stdout=log, stderr=subprocess.STDOUT, timeout=30)
            assert reopened.returncode == 0, "Saved drawing failed native reopen/render"
            checks.append("saved edited drawing reopens and renders in the native application")
    except Exception:
        from PIL import ImageGrab
        ImageGrab.grab(xdisplay=env["DISPLAY"]).save(root / "validation/results/native-interaction-failure.png")
        raise
    finally:
        x11.XCloseDisplay(display)
        if process and process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
        (root / "validation/results/native-interaction.json").write_text(json.dumps({"checks_passed": checks, "scope": "real pointer/keyboard and numeric command authoring, transactional edits, undo/redo and save on owned X11 display"}, indent=2) + "\n")
    assert len(checks) == 9
