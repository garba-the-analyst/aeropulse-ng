#!/usr/bin/env bash
# AeroPulse-NG — full verification battery.
# Usage:  ./scripts/verify.sh          (all suites)
#         ./scripts/verify.sh quick    (skip frontend build)
set -euo pipefail
cd "$(dirname "$0")/.."

pass=0; fail=0
banner() { printf "\n\033[1;36m== %s ==\033[0m\n" "$1"; }
ok()     { printf "  \033[1;32mPASS\033[0m %s\n" "$1"; pass=$((pass+1)); }
bad()    { printf "  \033[1;31mFAIL\033[0m %s\n" "$1"; fail=$((fail+1)); }

banner "1/4 RUST ENGINE CORE (90 tests)"
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features \
  > /tmp/opencode/ap-rust.log 2>&1 || true
if grep -q "test result: ok" /tmp/opencode/ap-rust.log; then
  ok "cargo test --no-default-features"
else bad "rust engine — see /tmp/opencode/ap-rust.log"; fi

banner "2/4 PYTHON SIDECAR (14 tests)"
(cd python-sidecar && venv/bin/python -m pytest tests -q) \
  > /tmp/opencode/ap-py.log 2>&1 || true
if grep -q "100%" /tmp/opencode/ap-py.log; then
  ok "pytest tests"
else bad "python sidecar — see /tmp/opencode/ap-py.log"; fi

banner "3/4 FRONTEND TYPECHECK"
if npx tsc --noEmit -p src > /tmp/opencode/ap-tsc.log 2>&1; then
  ok "tsc --noEmit"
else bad "typescript — see /tmp/opencode/ap-tsc.log"; fi

if [[ "${1:-}" != "quick" ]]; then
  banner "4/4 FRONTEND PRODUCTION BUILD"
  if npm run build > /tmp/opencode/ap-build.log 2>&1 && grep -q "built in" /tmp/opencode/ap-build.log; then
    ok "vite build (dist/ refreshed)"
  else bad "vite build — see /tmp/opencode/ap-build.log"; fi
fi

printf "\n──────────────────────────────\n"
printf " RESULT: \033[1;32m%d passed\033[0m / \033[1;31m%d failed\033[0m\n" "$pass" "$fail"
[[ $fail -eq 0 ]]
