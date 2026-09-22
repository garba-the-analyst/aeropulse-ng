#!/usr/bin/env python3
"""
AeroPulse-NG — replay ingest stub
- Decoded traces (ADS-B Exchange / adsb.lol, readsb JSON, gzip without .gz) -> Track
- Raw hex (OpenSky rollcall_replies_data4, dump1090 --raw) -> decode_frame_with_cache diff vs pyModeS

Keeps simulator parallel: this is a separate ingest path, not a replacement.
You handle RTL-SDR capture; this stub is for offline file replay.

Usage:
  # Decoded traces (already JS, just validate geometry for STCA):
  python3 scripts/replay_diff.py --decoded data/real/adsbex-hist/2026-09-01/somefile --limit 100
  zcat data/real/adsbex-hist/2026-09-01/* | python3 scripts/replay_diff.py --decoded-stdin

  # Raw hex diff vs pyModeS (requires pip install pyModeS):
  python3 scripts/replay_diff.py --raw-hex "A000149800000000000000E65891" --pyModes
  python3 scripts/replay_diff.py --raw-file dump1090_raw.hex --pyModes

Real RTL-SDR path (you handle):
  dump1090 --raw --net | python3 scripts/replay_diff.py --raw-stdin --pyModes
  # or: rtl_sdr -> dump1090 --raw -> file -> this script
"""
from __future__ import annotations
import argparse
import gzip
import json
import pathlib
import sys

def _open_maybe_gzip(path: pathlib.Path):
    # files are gzip without .gz — try gzip first, fallback raw
    try:
        with gzip.open(path, "rt", encoding="utf-8") as f:
            # peek
            f.read(1)
            f.seek(0)
            return gzip.open(path, "rt", encoding="utf-8")
    except Exception:
        return open(path, "r", encoding="utf-8", errors="ignore")

def replay_decoded(path: pathlib.Path, limit: int = 20):
    """Decoded readsb JSON: just show how it maps to Track (engine STCA sanity)."""
    try:
        f = _open_maybe_gzip(path) if path.is_file() else None
        if f is None:
            print(f"not a file: {path}", file=sys.stderr); return 1
        with f as fh:
            for i, line in enumerate(fh):
                if i >= limit:
                    break
                try:
                    obj = json.loads(line)
                except Exception:
                    continue
                # readsb-hist: one JSON per line or array; traces: per-aircraft files
                # Show minimal mapping
                icao = obj.get("hex") or obj.get("icao24") or obj.get("addr") or "?"
                lat = obj.get("lat") or obj.get("latitude")
                lon = obj.get("lon") or obj.get("longitude")
                alt = obj.get("alt_baro") or obj.get("altitude") or obj.get("altitude_ft")
                print(f"{icao:6} {lat} {lon} {alt}")
        return 0
    except FileNotFoundError:
        print(f"missing {path}", file=sys.stderr); return 2

def replay_raw_hex(hex: str, use_pymodes: bool = False):
    """Single raw hex via Rust decoder would be ideal; here we just show pyModeS diff if available."""
    hex = hex.strip().upper()
    print(f"hex: {hex}")
    if use_pymodes:
        try:
            import pyModeS as pms
            # pyModeS 3.x: pms.decode(hex) returns dict-like Decoded
            dec = pms.decode(hex)
            print(f"pyModeS: {dict(dec) if dec else None}")
            # Rust side: cargo test --no-default-features already verifies these 5 vectors:
            # DF20 A000149800000000000000E65891 -> 4840D6 32000
            # For live diff, pipe file through Rust binary or call via PyO3 (future)
        except ImportError:
            print("pyModeS not installed: pip install pyModeS", file=sys.stderr)
        except Exception as e:
            print(f"pyModeS decode failed: {e}", file=sys.stderr)
    # Note: Rust decode_frame_with_cache diff requires building a small Rust binary
    # or using the existing cargo test vectors. This stub documents the intended flow:
    print("Rust: cargo test --manifest-path src-tauri/Cargo.toml --no-default-features -- ground_truth --nocapture")
    return 0

def replay_raw_file(path: pathlib.Path, use_pymodes: bool = False):
    # File may be AVR hex lines or BEAST; handle plain hex per line
    try:
        with open(path, "r", encoding="utf-8", errors="ignore") as f:
            for line in f:
                line=line.strip()
                if not line or line.startswith("#"):
                    continue
                # AVR: *8D406B90...; or just hex
                hexpart = line.strip().lstrip("*").rstrip(";").strip()
                if len(hexpart) < 14:
                    continue
                replay_raw_hex(hexpart, use_pymodes=use_pymodes)
    except FileNotFoundError:
        print(f"missing {path}", file=sys.stderr); return 2
    return 0

def main():
    ap = argparse.ArgumentParser(description="AeroPulse-NG replay ingest stub")
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--decoded", type=pathlib.Path, help="decoded JSON file (maybe gzip without .gz)")
    g.add_argument("--decoded-stdin", action="store_true", help="read decoded JSON from stdin (zcat | this)")
    g.add_argument("--raw-hex", type=str, help="single raw hex (e.g. A000149800000000000000E65891)")
    g.add_argument("--raw-file", type=pathlib.Path, help="file of hex lines (dump1090 --raw)")
    g.add_argument("--raw-stdin", action="store_true", help="read hex lines from stdin")
    ap.add_argument("--limit", type=int, default=20)
    ap.add_argument("--pyModes", action="store_true", help="also decode via pyModeS for diff")
    args = ap.parse_args()

    if args.decoded:
        return replay_decoded(args.decoded, args.limit)
    if args.decoded_stdin:
        # zcat ... | python3 scripts/replay_diff.py --decoded-stdin
        for i, line in enumerate(sys.stdin):
            if i >= args.limit: break
            try:
                obj = json.loads(line)
                print(f"{obj.get('hex', '?'):6} {obj.get('lat')} {obj.get('lon')} {obj.get('alt_baro')}")
            except Exception:
                continue
        return 0
    if args.raw_hex:
        return replay_raw_hex(args.raw_hex, use_pymodes=args.pyModes)
    if args.raw_file:
        return replay_raw_file(args.raw_file, use_pymodes=args.pyModes)
    if args.raw_stdin:
        for line in sys.stdin:
            hexpart = line.strip().lstrip("*").rstrip(";").strip()
            if len(hexpart) >= 14:
                replay_raw_hex(hexpart, use_pymodes=args.pyModes)
        return 0
    ap.print_help(); return 1

if __name__ == "__main__":
    raise SystemExit(main())
