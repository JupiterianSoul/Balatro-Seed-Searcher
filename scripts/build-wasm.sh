#!/usr/bin/env bash
# Build both WASM bundles (scalar + SIMD) and publish them to web/public/.
# Run from anywhere — the script cd's to the repo root.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root/engine"

echo "→ scalar WASM"
wasm-pack build --target web --release --features wasm > /dev/null

echo "→ SIMD WASM (+simd128)"
RUSTFLAGS='-C target-feature=+simd128' \
  wasm-pack build --target web --release --out-dir pkg-simd --features wasm > /dev/null

echo "→ publishing to web/public/"
mkdir -p "$repo_root/web/public/engine" "$repo_root/web/public/engine-simd"
cp pkg/* "$repo_root/web/public/engine/"
cp pkg-simd/* "$repo_root/web/public/engine-simd/"

scalar_size=$(stat -c%s "$repo_root/web/public/engine/balatro_seed_engine_bg.wasm")
simd_size=$(stat -c%s "$repo_root/web/public/engine-simd/balatro_seed_engine_bg.wasm")
printf "  scalar: %5d KB\n  simd:   %5d KB\n" $((scalar_size/1024)) $((simd_size/1024))
echo "✓ done"
