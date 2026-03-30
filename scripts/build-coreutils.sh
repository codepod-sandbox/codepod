#!/bin/bash
set -euo pipefail

# Build all coreutils and shell to wasm32-wasip1
# Usage: ./scripts/build-coreutils.sh [--copy-fixtures]

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="$REPO_ROOT/target/wasm32-wasip1/release"
FIXTURES_DIR="$REPO_ROOT/packages/orchestrator/src/platform/__tests__/fixtures"

echo "Building coreutils + shell + shell-exec to wasm32-wasip1..."
cargo build \
  -p codepod-coreutils \
  -p codepod-shell-exec \
  -p true-cmd-wasm \
  -p false-cmd-wasm \
  --target wasm32-wasip1 \
  --release

echo ""
echo "Built binaries:"
ls -lh "$TARGET_DIR"/*.wasm 2>/dev/null | while read line; do
  size=$(echo "$line" | awk '{print $5}')
  name=$(echo "$line" | awk '{print $NF}' | xargs basename)
  printf "  %-30s %s\n" "$name" "$size"
done

# Copy to test fixtures if requested
if [[ "${1:-}" == "--copy-fixtures" ]]; then
  echo ""
  echo "Copying to test fixtures..."

  TOOLS=(cat echo head tail wc sort uniq grep ls mkdir rm cp mv touch tee tr cut basename dirname env printf find sed awk jq du df gzip tar bc dc hostname base64 sha256sum sha1sum sha224sum sha384sum sha512sum md5sum stat xxd rev nproc fmt fold nl expand unexpand paste comm join split strings od cksum truncate tree patch file column cmp timeout numfmt csplit zip unzip arch factor shuf sum link unlink base32 dd tsort nice nohup hostid uptime chown chgrp sudo groups logname users who)
  for tool in "${TOOLS[@]}"; do
    cp "$TARGET_DIR/$tool.wasm" "$FIXTURES_DIR/$tool.wasm"
  done

  cp "$TARGET_DIR/true-cmd-wasm.wasm" "$FIXTURES_DIR/true-cmd.wasm"
  cp "$TARGET_DIR/false-cmd-wasm.wasm" "$FIXTURES_DIR/false-cmd.wasm"
  cp "$TARGET_DIR/codepod-shell-exec.wasm" "$REPO_ROOT/packages/orchestrator/src/shell/__tests__/fixtures/codepod-shell-exec.wasm"

  # Build asyncify variant for non-JSPI environments (Safari, Bun, older browsers).
  # Requires wasm-opt (Binaryen) — skipped if not available.
  if command -v wasm-opt &>/dev/null; then
    echo ""
    echo "Building codepod-shell-exec-asyncify.wasm via wasm-opt --asyncify..."
    wasm-opt "$TARGET_DIR/codepod-shell-exec.wasm" \
      --asyncify \
      --enable-bulk-memory \
      --enable-nontrapping-float-to-int \
      --enable-sign-ext \
      --enable-mutable-globals \
      --pass-arg=asyncify-imports@codepod.host_waitpid,codepod.host_yield,codepod.host_network_fetch,codepod.host_register_tool,codepod.host_run_command,wasi_snapshot_preview1.fd_read,wasi_snapshot_preview1.poll_oneoff \
      -O1 \
      -o "$FIXTURES_DIR/codepod-shell-exec-asyncify.wasm"
    cp "$FIXTURES_DIR/codepod-shell-exec-asyncify.wasm" \
       "$REPO_ROOT/packages/orchestrator/src/shell/__tests__/fixtures/codepod-shell-exec-asyncify.wasm"
    echo "  codepod-shell-exec-asyncify.wasm built."
  else
    echo "WARNING: wasm-opt not found — skipping asyncify build."
    echo "         Install Binaryen (brew install binaryen) and re-run to build the asyncify variant."
  fi

  echo "Done."
fi
