"""DuckDB persistence engine for the AeroPulse-NG sidecar.

Batches are inserted transactionally; a failed batch must never take the
sidecar process down since the surveillance loop treats persistence as
best-effort.
"""

from __future__ import annotations

import pathlib

import duckdb

_SCHEMA_PATH = pathlib.Path(__file__).with_name("schema.sql")


class DuckDbEngine:
    """Thin, dependency-light wrapper over duckdb for operational writes."""

    def __init__(self, db_path: str):
        self._db_path = db_path
        self._con = duckdb.connect(db_path)
        self._total_rows = 0

    # -- lifecycle ---------------------------------------------------------

    def initialise_schema(self) -> None:
        self._con.execute(_SCHEMA_PATH.read_text(encoding="utf-8"))

    def close(self) -> None:
        try:
            self._con.close()
        except Exception:  # pragma: no cover - defensive
            pass

    # -- writers -----------------------------------------------------------

    def insert_tracks(self, tracks: list[dict]) -> int:
        """Persists one snapshot batch; returns accepted row count."""
        if not tracks:
            return 0
        rows = [
            (
                int(t.get("last_update_ms", 0)),
                t.get("icao24", "")[:12],
                t.get("callsign"),
                str(t.get("class", "")),
                float(t.get("latitude", 0.0)),
                float(t.get("longitude", 0.0)),
                _opt_float(t.get("altitude_ft")),
                _opt_float(t.get("ground_speed_kt")),
                _opt_float(t.get("course_deg")),
                _opt_float(t.get("vertical_rate_fpm")),
                t.get("squawk"),
                bool(t.get("coasting", False)),
                _opt_float(t.get("position_sigma_m")),
            )
            for t in tracks
        ]
        self._con.execute("BEGIN")
        try:
            self._con.executemany(
                """
                INSERT INTO track_positions (
                    ts_ms, icao24, callsign, class, latitude, longitude,
                    altitude_ft, ground_speed_kt, course_deg,
                    vertical_rate_fpm, squawk, coasting, sigma_m
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                rows,
            )
            self._con.execute("COMMIT")
        except Exception:
            self._con.execute("ROLLBACK")
            raise
        self._total_rows += len(rows)
        return len(rows)

    def record_weather(
        self,
        source: str,
        observed_ms: int,
        qnh_hpa: float | None = None,
        wind_dir_deg: float | None = None,
        wind_speed_kt: float | None = None,
        temperature_c: float | None = None,
        dewpoint_c: float | None = None,
        visibility_m: float | None = None,
        raw_text: str | None = None,
    ) -> int:
        self._con.execute(
            """
            INSERT INTO weather_observations (
                observed_ms, source, qnh_hpa, wind_dir_deg, wind_speed_kt,
                temperature_c, dewpoint_c, visibility_m, raw_text
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                observed_ms,
                source,
                qnh_hpa,
                wind_dir_deg,
                wind_speed_kt,
                temperature_c,
                dewpoint_c,
                visibility_m,
                raw_text,
            ),
        )
        return 1

    # -- readers -----------------------------------------------------------

    def track_count(self) -> int:
        row = self._con.execute(
            "SELECT COUNT(*) FROM track_positions"
        ).fetchone()
        return int(row[0]) if row else 0

    @property
    def total_rows_written(self) -> int:
        return self._total_rows


def _opt_float(value) -> float | None:
    if value is None:
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None
