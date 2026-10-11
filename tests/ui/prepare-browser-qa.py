#!/usr/bin/env python3
"""Prepare local-only visual comparison pages after a Trunk build.

These pages live only in ignored dist. A clean Trunk build removes them.
Never publish a dist directory containing these diagnostic pages.
"""

from pathlib import Path
import shutil


ROOT = Path(__file__).resolve().parents[2]
DIST = ROOT / "apps/web-shell/dist"
REFERENCES = ROOT / "docs/design/action-first/references"
WIDTH, HEIGHT = 1672, 941
DISPLAY_WIDTH = 660


def page(name, region, focus):
    x, y, width, height = region
    scale = DISPLAY_WIDTH / width
    display_height = height * scale
    transform = f"scale({scale}) translate({-x}px, {-y}px)"
    fx, fy, fw, fh = focus
    focus_scale = DISPLAY_WIDTH / fw
    focus_transform = f"scale({focus_scale}) translate({-fx}px, {-fy}px)"
    return f"""<!doctype html>
<html lang="en">
<meta charset="utf-8">
<title>Wonderland {name} fidelity check</title>
<style>
* {{ box-sizing: border-box; }}
body {{ margin: 0; padding: 10px; background: #182837; color: white; font: 16px system-ui; }}
h1 {{ margin: 0 0 8px; font-size: 18px; }}
button {{ margin-bottom: 8px; padding: 6px 12px; }}
.row {{ display: flex; gap: 16px; align-items: flex-start; }}
figure {{ margin: 0; width: {DISPLAY_WIDTH}px; }}
figcaption {{ height: 28px; }}
.crop {{ width: {DISPLAY_WIDTH}px; height: {display_height}px; overflow: hidden; }}
img, iframe {{ display: block; border: 0; width: {WIDTH}px; height: {HEIGHT}px;
  max-width: none; transform: {transform}; transform-origin: top left; }}
.focused .crop {{ height: {fh * focus_scale}px; }}
.focused img, .focused iframe {{ transform: {focus_transform}; }}
</style>
<h1>Reference and live viewport: {WIDTH} × {HEIGHT}, DPR 1. Region: {x}, {y}, {width}, {height}.</h1>
<button type="button" onclick="document.body.classList.toggle('focused')">Toggle detailed crop ({fx}, {fy}, {fw}, {fh})</button>
<div class="row">
  <figure><figcaption>Approved {name} reference</figcaption><div class="crop">
    <img src="/__verify-{name}-reference.png" alt="Approved {name} reference">
  </div></figure>
  <figure><figcaption>Live Rust/WASM implementation</figcaption><div class="crop">
    <iframe src="/" title="Live Wonderland {name}"></iframe>
  </div></figure>
</div>
</html>
"""


def main():
    if not (DIST / "index.html").is_file():
        raise SystemExit("Build apps/web-shell with Trunk first.")
    focused_regions = {
        "characters": (45, 105, 880, 685),
        "city": (590, 440, 575, 380),
        "lot": (795, 260, 500, 390),
    }
    for name, focus in focused_regions.items():
        shutil.copyfile(REFERENCES / f"{name}.png", DIST / f"__verify-{name}-reference.png")
        for kind, region in (("comparison", (0, 0, WIDTH, HEIGHT)), ("focus", focus)):
            (DIST / f"__verify-{name}-{kind}.html").write_text(page(name, region, focus))
    shutil.copyfile(ROOT / "tests/ui/mobile-viewport.html", DIST / "__verify-mobile.html")
    print("Prepared local-only comparison and 390×844 viewport pages in ignored dist.")


if __name__ == "__main__":
    main()
