#!/usr/bin/env bash
# Build the WebAssembly package and serve the web frontend locally.
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "wasm-pack not found. Install it from https://rustwasm.github.io/wasm-pack/" >&2
  exit 1
fi

echo "Building WebAssembly (web target)…"
wasm-pack build crates/fast-text-wasm --target web --out-dir ../../web/pkg

port="${1:-8080}"
echo "Serving http://localhost:${port}/ (Ctrl+C to stop)"
cd web
exec python3 -m http.server "${port}"
