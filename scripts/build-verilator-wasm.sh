#!/usr/bin/env bash
# Placeholder build pipeline for the Verilator-compiled NTT RTL blob.
#
# The output goes to worker/src/backend/ntt_rtl.wasm.placeholder, which
# the v0.1 backend module loads at startup.  v0 ships only the
# placeholder marker; replace this script with the real toolchain
# (yosys + verilator + wasm32-wasi cross) when ready.

set -euo pipefail

OUT=worker/src/backend
mkdir -p "${OUT}"
echo "// placeholder: replace with verilator-compiled WASM blob" > "${OUT}/ntt_rtl.wasm.placeholder"
echo "wrote ${OUT}/ntt_rtl.wasm.placeholder"
