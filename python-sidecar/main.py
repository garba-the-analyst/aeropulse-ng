"""AeroPulse-NG sidecar host.

NDJSON process bridge serving two responsibilities:
  1. DuckDB persistence for track history and alert records.
  2. RS-485/RS-232 AWOS surface telemetry ingestion.

Protocol (one JSON object per line, ``\\n`` terminated):

Inbound (from the Rust engine):
    {"cmd": "init", "db_path": "..."}
    {"cmd": "log_tracks", "tracks": [...]}
    {"cmd": "log_alerts", "alerts": [...]}
    {"cmd": "log_weather", "source": "...", "observed_ms": ..., ...}
    {"cmd": "shutdown"}

Outbound (to the Rust engine):
    {"ev": "ready"}
    {"ev": "awos", "qnh_hpa": ..., ...}
    {"ev": "db_ack", "rows": N}
    {"ev": "error", "cmd": "...", "message": "..."}

The sidecar must never crash the surveillance stack: every command handler
is wrapped so failures degrade to a structured error event
``{"ev":"error","cmd":...,"message":...}`` on stdout and a log line on
stderr. ``init`` and ``log_tracks`` are explicitly wrapped; ``log_alerts``
and ``log_weather`` follow the same contract.
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


def _opt_float_msg(value) -> float | None:
    if value is None:
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def start_awos_thread(interval_s: float = 10.0, device: str | None = None) -> threading.Event:
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
        device=device,
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
                emit({"ev": "error", "cmd": "unknown", "message": f"unparseable frame: {exc}"})
                continue

            cmd = msg.get("cmd")

            if cmd == "init":
                try:
                    # Honor per-message db_path (NDAiE fix: was ignoring init.db_path
                    # and always using CLI --db). Fall back to CLI default.
                    db_path = str(msg.get("db_path") or args.db)
                    engine = DuckDbEngine(db_path)
                    engine.initialise_schema()
                    if awos_stop is None:
                        awos_stop = start_awos_thread(device=args.serial)
                    emit({"ev": "ready"})
                except Exception as exc:
                    log_error(f"init failed: {exc}")
                    # Persistence is optional; AWOS still runs.
                    if awos_stop is None:
                        awos_stop = start_awos_thread(device=args.serial)
                    emit({"ev": "ready"})

            elif cmd == "log_tracks":
                try:
                    if engine is None:
                        emit({"ev": "db_ack", "rows": 0})
                        continue
                    tracks = msg.get("tracks", [])
                    rows = engine.insert_tracks(tracks)
                    emit({"ev": "db_ack", "rows": rows})
                except Exception as exc:
                    log_error(f"log_tracks failed: {exc}")
                    emit({"ev": "error", "cmd": "log_tracks", "message": str(exc)})

            elif cmd == "log_alerts":
                try:
                    if engine is None:
                        emit({"ev": "db_ack", "rows": 0})
                        continue
                    alerts = msg.get("alerts", [])
                    rows = engine.insert_stca_alerts(alerts)
                    emit({"ev": "db_ack", "rows": rows})
                except Exception as exc:
                    log_error(f"log_alerts failed: {exc}")
                    emit({"ev": "error", "cmd": "log_alerts", "message": str(exc)})

            elif cmd == "log_weather":
                try:
                    if engine is None:
                        emit({"ev": "db_ack", "rows": 0})
                        continue
                    # record_weather signature: source, observed_ms, qnh_hpa, wind..., raw_text
                    rows = engine.record_weather(
                        source=str(msg.get("source", "UNKNOWN")),
                        observed_ms=int(msg.get("observed_ms", int(time.time() * 1000))),
                        qnh_hpa=_opt_float_msg(msg.get("qnh_hpa")),
                        wind_dir_deg=_opt_float_msg(msg.get("wind_dir_deg")),
                        wind_speed_kt=_opt_float_msg(msg.get("wind_speed_kt")),
                        wind_gust_kt=_opt_float_msg(msg.get("wind_gust_kt")),
                        temperature_c=_opt_float_msg(msg.get("temperature_c")),
                        dewpoint_c=_opt_float_msg(msg.get("dewpoint_c")),
                        visibility_m=_opt_float_msg(msg.get("visibility_m")),
                        raw_text=msg.get("raw_text"),
                    )
                    emit({"ev": "db_ack", "rows": rows})
                except Exception as exc:
                    log_error(f"log_weather failed: {exc}")
                    emit({"ev": "error", "cmd": "log_weather", "message": str(exc)})

            elif cmd == "shutdown":
                try:
                    break
                except Exception as exc:
                    log_error(f"shutdown failed: {exc}")
                    break

            else:
                try:
                    log_error(f"unknown cmd: {cmd!r}")
                    emit({"ev": "error", "cmd": cmd, "message": f"unknown cmd {cmd!r}"})
                except Exception as exc:
                    log_error(f"unknown cmd handler failed: {exc}")
    except KeyboardInterrupt:  # pragma: no cover
        pass
    finally:
        if awos_stop is not None:
            awos_stop.set()
        if engine is not None:
            engine.close()


if __name__ == "__main__":
    main()
