#!/usr/bin/env bash
# AeroPulse-NG — full verification battery.
# Usage:  ./scripts/verify.sh          (all suites)
#         ./scripts/verify.sh quick    (skip frontend build)
set -euo pipefail
cd "$(dirname "$0")/.."

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

pass=0; fail=0
rust_count=0; py_count=0
banner() { printf "\n\033[1;36m== %s ==\033[0m\n" "$1"; }
ok()     { printf "  \033[1;32mPASS\033[0m %s\n" "$1"; pass=$((pass+1)); }
bad()    { printf "  \033[1;31mFAIL\033[0m %s\n" "$1"; fail=$((fail+1)); }

banner "1/4 RUST ENGINE CORE"
if cargo test --manifest-path src-tauri/Cargo.toml --no-default-features > "$TMPDIR/ap-rust.log" 2>&1; then
  # Parse actual passed count: sum of "X passed" across crates
  rust_count=$(grep -oE '[0-9]+ passed' "$TMPDIR/ap-rust.log" | awk '{s+=$1} END{print s+0}')
  if grep -q "test result: ok" "$TMPDIR/ap-rust.log"; then
    ok "cargo test --no-default-features (${rust_count} passed)"
  else
    bad "rust engine — no ok result (see $TMPDIR/ap-rust.log)"
  fi
else
  cat "$TMPDIR/ap-rust.log"
  bad "rust engine — cargo test failed (see $TMPDIR/ap-rust.log)"
fi

banner "2/4 PYTHON SIDECAR"
if python-sidecar/venv/bin/python -m pytest python-sidecar/tests -v > "$TMPDIR/ap-py.log" 2>&1; then
  py_count=$(grep -oE '[0-9]+ passed' "$TMPDIR/ap-py.log" | tail -1 | grep -oE '[0-9]+' | head -1)
  py_count=${py_count:-0}
  if grep -q "passed" "$TMPDIR/ap-py.log"; then
    ok "pytest tests (${py_count} passed)"
  else
    bad "python sidecar — unexpected output (see $TMPDIR/ap-py.log)"
  fi
else
  cat "$TMPDIR/ap-py.log"
  bad "python sidecar — pytest failed (see $TMPDIR/ap-py.log)"
fi

banner "3/4 FRONTEND TYPECHECK"
if npx tsc --noEmit -p src > "$TMPDIR/ap-tsc.log" 2>&1; then
  ok "tsc --noEmit"
else
  cat "$TMPDIR/ap-tsc.log"
  bad "typescript — see $TMPDIR/ap-tsc.log"
fi

if [[ "${1:-}" != "quick" ]]; then
  banner "4/4 FRONTEND PRODUCTION BUILD"
  if npm run build > "$TMPDIR/ap-build.log" 2>&1; then
    ok "vite build (dist/ refreshed)"
  else
    cat "$TMPDIR/ap-build.log"
    bad "vite build — see $TMPDIR/ap-build.log"
  fi
fi

printf "\n──────────────────────────────\n"
printf " RESULT: \033[1;32m%d passed\033[0m / \033[1;31m%d failed\033[0m\n" "$pass" "$fail"
printf " Counts: Rust %s · Python %s · total %s checks\n" "$rust_count" "$py_count" "$((rust_count + py_count))"
if [[ $fail -ne 0 ]]; then
  echo "Verification failed — logs in $TMPDIR"
  exit 1
fi
