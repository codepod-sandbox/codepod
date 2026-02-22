#!/bin/bash
set -euo pipefail

rustup target add wasm32-wasip1 2>/dev/null || true

cargo build \
  --release \
  --target wasm32-wasip1 \
  -p wasmsand-python

cp target/wasm32-wasip1/release/python3.wasm \
   packages/orchestrator/src/platform/__tests__/fixtures/python3.wasm

echo "Built python3.wasm ($(du -h packages/orchestrator/src/platform/__tests__/fixtures/python3.wasm | cut -f1))"
