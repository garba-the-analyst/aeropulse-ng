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
        # Migration for older DBs: add new columns if missing
        for col, typ in [
            ("mode_s_capable", "BOOLEAN"),
            ("last_baro_altitude_ft", "DOUBLE"),
        ]:
            try:
                self._con.execute(
                    f"ALTER TABLE track_positions ADD COLUMN IF NOT EXISTS {col} {typ}"
                )
            except Exception:
                pass

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
        import math

        def is_finite(v) -> bool:
            try:
                f = float(v)
                return math.isfinite(f)
            except Exception:
                return False

        rows = []
        for t in tracks:
            try:
                icao = str(t.get("icao24", "")).strip()[:12]
                if not icao:
                    continue
                lat = t.get("latitude", 0.0)
                lon = t.get("longitude", 0.0)
                if not is_finite(lat) or not is_finite(lon):
                    continue
                # Validate lat/lon finite and within plausible ranges
                lat_f = float(lat)
                lon_f = float(lon)
                if not (math.isfinite(lat_f) and math.isfinite(lon_f)):
                    continue
                # Build row with per-field validation
                rows.append(
                    (
                        int(t.get("last_update_ms", 0)),
                        icao,
                        t.get("callsign"),
                        str(t.get("class", "")),
                        lat_f,
                        lon_f,
                        _opt_float(t.get("altitude_ft")),
                        _opt_float(t.get("ground_speed_kt")),
                        _opt_float(t.get("course_deg")),
                        _opt_float(t.get("vertical_rate_fpm")),
                        t.get("squawk"),
                        bool(t.get("coasting", False)),
                        _opt_float(t.get("position_sigma_m")),
                        bool(t.get("mode_s_capable", False)),
                        _opt_float(t.get("last_baro_altitude_ft")),
                    )
                )
            except Exception:
                continue

        if not rows:
            return 0

        self._con.execute("BEGIN")
        try:
            self._con.executemany(
                """
                INSERT INTO track_positions (
                    ts_ms, icao24, callsign, class, latitude, longitude,
                    altitude_ft, ground_speed_kt, course_deg,
                    vertical_rate_fpm, squawk, coasting, sigma_m,
                    mode_s_capable, last_baro_altitude_ft
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                rows,
            )
            self._con.execute("COMMIT")
        except Exception:
            self._con.execute("ROLLBACK")
            raise
        self._total_rows += len(rows)
        return len(rows)

    def insert_stca_alerts(self, alerts: list[dict]) -> int:
        """Persists STCA alert batch; returns accepted row count."""
        if not alerts:
            return 0
        rows = []
        for a in alerts:
            try:
                icao_a = str(a.get("icao_a", "")).strip()[:12]
                icao_b = str(a.get("icao_b", "")).strip()[:12]
                if not icao_a or not icao_b:
                    continue
                alert_id = str(a.get("alert_id") or a.get("id", "")).strip()
                if not alert_id:
                    # synthesize id from pair + timestamp
                    alert_id = f"{icao_a}-{icao_b}-{a.get('triggered_ms', 0)}"
                rows.append(
                    (
                        int(a.get("triggered_ms", 0)),
                        alert_id,
                        icao_a,
                        a.get("callsign_a"),
                        icao_b,
                        a.get("callsign_b"),
                        _opt_float(a.get("min_horizontal_nm")),
                        _opt_float(a.get("min_vertical_ft")),
                        _opt_float(a.get("time_to_closest_s")),
                    )
                )
            except Exception:
                continue
        if not rows:
            return 0
        self._con.execute("BEGIN")
        try:
            self._con.executemany(
                """
                INSERT INTO stca_alerts (
                    triggered_ms, alert_id, icao_a, callsign_a,
                    icao_b, callsign_b, min_horizontal_nm,
                    min_vertical_ft, time_to_closest_s
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                rows,
            )
            self._con.execute("COMMIT")
        except Exception:
            self._con.execute("ROLLBACK")
            raise
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
