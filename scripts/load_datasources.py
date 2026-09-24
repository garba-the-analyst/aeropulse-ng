#!/usr/bin/env python3
"""
AeroPulse-NG — datasources inspector / replay helper
Handles what you actually downloaded to datasources/:
  - acas*.csv.gz (DF, bytes, lat, lon, alt — raw ACAS frames + decoded pos)
  - operations*.csv.gz (time, icao, operation, airport, flight, squawk — events)
  - ax_arrivals_*.csv (AirLabs arrivals: hex, callsign, altitude, speed)

Also handles the two JSON trace formats the fetch script pulls:
  - readsb-hist: {"now": ts, "aircraft": [{"hex","lat","lon","alt_baro","gs","track","squawk","flight"}]}
  - traces: per-aircraft {"icao": "a42dfb", "trace": [[ts,lat,lon,alt,gs,track,squawk], ...]}

Usage:
  python3 scripts/load_datasources.py --inspect
  python3 scripts/load_datasources.py --acas datasources/acas.csv.gz --limit 5
  python3 scripts/load_datasources.py --operations datasources/operations.csv.gz --limit 5
  zcat datasources/acas*.csv.gz | python3 scripts/replay_diff.py --raw-stdin --pyModes
"""
import argparse
import csv
import gzip
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
DS = ROOT / "datasources"

def open_maybe_gzip(p: pathlib.Path):
    # files are gzip even without .gz in samples tree; here they do have .gz
    try:
        with gzip.open(p, "rt", encoding="utf-8", errors="ignore") as f:
            f.read(1)
            f.seek(0)
            return gzip.open(p, "rt", encoding="utf-8", errors="ignore")
    except Exception:
        return open(p, "r", encoding="utf-8", errors="ignore")

def inspect():
    print(f"datasources: {DS}")
    for p in sorted(DS.iterdir())[:30]:
        print(f"  {p.name:35} {p.stat().st_size/1024:.1f} KB")
    print("\nSampling acas (DF frames):")
    for p in sorted(DS.glob("acas*.csv.gz"))[:1]:
        try:
            with gzip.open(p, "rt") as f:
                print(f"  {p.name}: {next(f).strip()[:120]}")
                print(f"           {next(f).strip()[:180]}")
        except Exception as e:
            print(f"  {p.name}: {e}")
    print("\nSampling operations:")
    for p in sorted(DS.glob("operations*.csv.gz"))[:1]:
        try:
            with gzip.open(p, "rt") as f:
                print(f"  {p.name}: {next(f).strip()[:120]}")
                print(f"           {next(f).strip()[:180]}")
        except Exception as e:
            print(f"  {p.name}: {e}")
    print("\nSampling ax_arrivals:")
    for p in sorted(DS.glob("ax_arrivals*.csv"))[:1]:
        try:
            with open(p, "r") as f:
                print(f"  {p.name}: {next(f).strip()[:120]}")
                print(f"           {next(f).strip()[:180]}")
        except Exception as e:
            print(f"  {p.name}: {e}")

def load_acas(path: pathlib.Path, limit=5):
    with gzip.open(path, "rt") as f:
        rdr = csv.DictReader(f)
        print(f"acas {path.name} fields: {rdr.fieldnames}")
        for i, row in enumerate(rdr):
            if i >= limit:
                break
            print(f"  {row.get('icao','?'):6} DF:{row.get('DF:','?')} bytes:{row.get('bytes:','?')[:16]} lat:{row.get('lat',row.get('',''))} lon:{row.get('lon','')} alt:{row.get('alt',row.get('',''))}")

def load_operations(path: pathlib.Path, limit=5):
    with gzip.open(path, "rt") as f:
        rdr = csv.DictReader(f)
        print(f"operations {path.name} fields: {rdr.fieldnames[:8]} ...")
        for i, row in enumerate(rdr):
            if i >= limit:
                break
            print(f"  {row.get('icao','?'):6} {row.get('operation','?'):8} {row.get('airport','?'):4} {row.get('flight','?'):8} squawk:{row.get('squawk','?')}")

if __name__ == "__main__":
    ap = argparse.ArgumentParser(description="datasources inspector")
    ap.add_argument("--inspect", action="store_true", help="list datasources and sample")
    ap.add_argument("--acas", type=pathlib.Path, help="inspect acas file")
    ap.add_argument("--operations", type=pathlib.Path, help="inspect operations file")
    ap.add_argument("--limit", type=int, default=5)
    args = ap.parse_args()
    if args.inspect or not (args.acas or args.operations):
        inspect()
    if args.acas:
        load_acas(args.acas, args.limit)
    if args.operations:
        load_operations(args.operations, args.limit)
