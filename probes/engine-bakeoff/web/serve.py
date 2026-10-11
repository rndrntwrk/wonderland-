#!/usr/bin/env python3
"""Local static host. Intentionally sends no COOP/COEP isolation headers."""
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

parser = argparse.ArgumentParser()
parser.add_argument("directory", nargs="?", default=".")
parser.add_argument("--port", type=int, default=8080)
args = parser.parse_args()

class Host(SimpleHTTPRequestHandler):
    extensions_map = {**SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm", ".mjs": "text/javascript"}
    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

with ThreadingHTTPServer(("127.0.0.1", args.port), partial(Host, directory=args.directory)) as server:
    print(f"Engine probe: http://127.0.0.1:{args.port}/", flush=True)
    server.serve_forever()
