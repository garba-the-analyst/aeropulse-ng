#!/usr/bin/env python3
"""Bundle the Python sidecar into Tauri resources (air-gap safe, offline).

Copies python-sidecar/{main.py,weather,db} + minimal venv marker into
src-tauri/resources/python-sidecar/ so tauri.conf.json `resources` can ship it.
No network access: uses the local venv if present, else system python3.

Usage: python3 build-scripts/bundle-sidecar.py [--out src-tauri/resources/python-sidecar]
"""
from __future__ import annotations
import argparse
import pathlib
import shutil
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SRC = ROOT / "python-sidecar"

def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="src-tauri/resources/python-sidecar")
    args = ap.parse_args()
    out = ROOT / args.out
    if not (SRC / "main.py").exists():
        print(f"sidecar source missing: {SRC}", file=sys.stderr)
        return 1
    out.mkdir(parents=True, exist_ok=True)
    for name in ("main.py", "weather", "db", "pyproject.toml"):
        src = SRC / name
        dst = out / name
        if src.is_dir():
            shutil.rmtree(dst, ignore_errors=True)
            shutil.copytree(src, dst, ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "venv", ".pytest_cache"))
        elif src.exists():
            shutil.copy2(src, dst)
    # Marker for runtime resolver + version pin
    (out / "BUNDLED.txt").write_text("AeroPulse-NG sidecar bundle (offline). Run with system python3.\n", encoding="utf-8")
    print(f"bundled sidecar -> {out}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
