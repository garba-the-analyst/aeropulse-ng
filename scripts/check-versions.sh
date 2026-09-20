#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
pkg=$(node -e 'console.log(require("./package.json").version)')
cargo_ver=$(grep -E '^version =' src-tauri/Cargo.toml | head -1 | sed -E 's/.*"(.*)".*/\1/')
tauri_ver=$(node -e 'console.log(JSON.parse(require("fs").readFileSync("src-tauri/tauri.conf.json","utf8")).version)')
py_ver=$(grep -E '^version =' python-sidecar/pyproject.toml | head -1 | sed -E 's/.*"(.*)".*/\1/' | sed 's/"//g')

echo "package.json: $pkg"
echo "Cargo.toml: $cargo_ver"
echo "tauri.conf.json: $tauri_ver"
echo "pyproject.toml: $py_ver"

if [[ "$pkg" != "$cargo_ver" ]]; then echo "FAIL: package.json ($pkg) != Cargo.toml ($cargo_ver)"; fail=1; fi
if [[ "$pkg" != "$tauri_ver" ]]; then echo "FAIL: package.json ($pkg) != tauri.conf.json ($tauri_ver)"; fail=1; fi
if [[ "$pkg" != "$py_ver" ]]; then echo "FAIL: package.json ($pkg) != pyproject.toml ($py_ver)"; fail=1; fi

if [[ $fail -ne 0 ]]; then
  echo "Version drift detected"
  exit 1
fi
echo "All versions agree: $pkg"
