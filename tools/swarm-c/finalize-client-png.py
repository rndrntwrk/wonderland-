from pathlib import Path
# The parent patch is SHA256-verified before this packaging/type correction.
gpu = Path('apps/web-shell/public/world-gpu.mjs')
s = gpu.read_text()
old = "import {WorldPngCapture} from './world-capture.mjs';\nexport {WorldPngCapture} from './world-capture.mjs';\n"
assert s.startswith(old)
capture = Path('apps/web-shell/public/world-capture.mjs')
c = capture.read_text()
marker = "const abort = message => new DOMException(message, 'AbortError');\n"
assert c.count(marker) == 1
c = c.replace(marker, '')
gpu.write_text(s[len(old):] + '\n// Keep this implementation in the wasm-bindgen snippet: no unbundled imports.\n' + c)
capture.write_text("// Compatibility import for isolated consumers; the shipping snippet is self-contained.\nexport {WorldPngCapture} from './world-gpu.mjs';\n")
view = Path('apps/web-shell/src/world_renderer.rs')
s = view.read_text()
old = 'on:webglcontextlost=move |_| {'
assert s.count(old) == 1
view.write_text(s.replace(old, 'on:webglcontextlost=move |_: web_sys::Event| {'))
