"""AeroPulse-NG sidecar host.

NDJSON process bridge serving two responsibilities:
  1. DuckDB persistence for track history and alert records.
  2. RS-485/RS-232 AWOS surface telemetry ingestion.

Protocol (one JSON object per line, ``\\n`` terminated):

Inbound (from the Rust engine):
    {"cmd": "init", "db_path": "..."}
    {"cmd": "log_tracks", "tracks": [...]}
    {"cmd": "shutdown"}

Outbound (to the Rust engine):
    {"ev": "ready"}
    {"ev": "awos", "qnh_hpa": ..., ...}
    {"ev": "db_ack", "rows": N}

The sidecar must never crash the surveillance stack: every command handler
is wrapped so failures degrade to an error frame on stderr.
"""

from __future__ import annotations

import argparse
import json
import sys
import threading
import time

from db.duckdb_engine import DuckDbEngine
from weather.awos_serial import AwosSerialReader


def emit(payload: dict) -> None:
    """Writes one NDJSON frame to stdout and flushes immediately."""
    sys.stdout.write(json.dumps(payload, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def log_error(message: str) -> None:
    sys.stderr.write(f"[sidecar] {message}\n")
    sys.stderr.flush()


def start_awos_thread(interval_s: float = 10.0) -> threading.Event:
    """Spawns the AWOS reader; returns a stop event handle."""

    stop = threading.Event()

    def on_observation(obs: dict) -> None:
        obs.setdefault("observed_ms", int(time.time() * 1000))
        try:
            emit({"ev": "awos", **obs})
        except Exception as exc:  # pragma: no cover - defensive
            log_error(f"awos emit failed: {exc}")

    reader = AwosSerialReader(
        callback=on_observation,
        stop_event=stop,
        poll_interval_s=interval_s,
    )
    thread = threading.Thread(target=reader.run_forever, name="awos-reader", daemon=True)
    thread.start()
    return stop


def main() -> None:
    parser = argparse.ArgumentParser(description="AeroPulse-NG sidecar")
    parser.add_argument("--db", default="aeropulse.duckdb", help="DuckDB file path")
    parser.add_argument("--serial", default=None, help="AWOS serial device override")
    args = parser.parse_args()

    engine: DuckDbEngine | None = None
    awos_stop: threading.Event | None = None

    try:
        for raw_line in sys.stdin:
            line = raw_line.strip()
            if not line:
                continue

            try:
                msg = json.loads(line)
            except json.JSONDecodeError as exc:
                log_error(f"unparseable frame: {exc}")
                continue

            cmd = msg.get("cmd")

            if cmd == "init":
                try:
                    engine = DuckDbEngine(args.db)
                    engine.initialise_schema()
                    if awos_stop is None:
                        awos_stop = start_awos_thread()
                    emit({"ev": "ready"})
                except Exception as exc:
                    log_error(f"init failed: {exc}")
                    # Persistence is optional; AWOS still runs.
                    if awos_stop is None:
                        awos_stop = start_awos_thread()
                    emit({"ev": "ready"})

            elif cmd == "log_tracks":
                if engine is None:
                    emit({"ev": "db_ack", "rows": 0})
                    continue
                tracks = msg.get("tracks", [])
                rows = engine.insert_tracks(tracks)
                emit({"ev": "db_ack", "rows": rows})

            elif cmd == "shutdown":
                break

            else:
                log_error(f"unknown cmd: {cmd!r}")
    except KeyboardInterrupt:  # pragma: no cover
        pass
    finally:
        if awos_stop is not None:
            awos_stop.set()
        if engine is not None:
            engine.close()


if __name__ == "__main__":
    main()
