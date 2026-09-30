"""Phase 4 sidecar protocol + migration tests."""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import time
import select

import duckdb
import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

from db.duckdb_engine import DuckDbEngine  # noqa: E402


TRACK_GOOD = {
    "icao24": "342157",
    "callsign": "VL604",
    "class": "civil",
    "latitude": 12.35,
    "longitude": 8.13,
    "altitude_ft": 32_000,
    "ground_speed_kt": 445,
    "course_deg": 205,
    "vertical_rate_fpm": 0,
    "squawk": "3421",
    "coasting": False,
    "position_sigma_m": 41.2,
    "last_update_ms": 1_700_000_000_000,
    "mode_s_capable": True,
    "last_baro_altitude_ft": 32000,
}

# bad with empty icao for unit-level skip test
TRACK_BAD = {
    "icao24": "",  # empty icao -> skipped
    "callsign": "BAD",
    "latitude": "not-a-number",
    "longitude": None,
    "altitude_ft": "bad",
    "last_update_ms": 1_700_000_000_000,
}

# bad with NaN-ish latitude as required by DoD spec
TRACK_BAD_NAN = {
    "icao24": "BAD01",
    "callsign": "BAD",
    "class": "civil",
    "latitude": "NaN-ish",
    "longitude": 8.13,
    "altitude_ft": 30000,
    "ground_speed_kt": 400,
    "course_deg": 180,
    "last_update_ms": 1_700_000_000_001,
    "squawk": "0000",
    "coasting": False,
    "position_sigma_m": 40.0,
}

# Also rows lacking new keys (old payload)
TRACK_OLD_KEYS = {
    "icao24": "ABCD12",
    "callsign": "OLD01",
    "class": "civil",
    "latitude": 12.0,
    "longitude": 8.0,
    "altitude_ft": 30000,
    "ground_speed_kt": 400,
    "course_deg": 180,
    "last_update_ms": 1_700_000_000_001,
    # no mode_s_capable, no last_baro_altitude_ft
}


class TestPartialBatch:
    def test_partial_batch_inserts_good_rows(self, tmp_path):
        """Spec: batch of [good, bad (latitude NaN-ish), good] -> 2 rows, 1 rejected."""
        engine = DuckDbEngine(str(tmp_path / "partial.duckdb"))
        engine.initialise_schema()
        # Use NaN-ish bad as per spec
        bad_nan = dict(TRACK_GOOD)
        bad_nan["icao24"] = "BAD01"
        bad_nan["latitude"] = "NaN-ish"
        rows = engine.insert_tracks([TRACK_GOOD, bad_nan, TRACK_GOOD])
        assert rows == 2
        assert engine.track_count() == 2
        assert engine.total_rows_written == 2
        engine.close()

    def test_rows_lacking_new_keys_insert(self, tmp_path):
        engine = DuckDbEngine(str(tmp_path / "oldkeys.duckdb"))
        engine.initialise_schema()
        rows = engine.insert_tracks([TRACK_OLD_KEYS, TRACK_BAD, TRACK_GOOD])
        assert rows == 2
        # verify defaults written as false/null for old row
        row = engine._con.execute(
            "SELECT mode_s_capable, last_baro_altitude_ft FROM track_positions WHERE icao24='ABCD12'"
        ).fetchone()
        assert row is not None
        assert row[0] is False or row[0] == 0
        assert row[1] is None
        engine.close()


class TestSchemaMigration:
    def test_fresh_db_has_new_columns(self, tmp_path):
        db_path = tmp_path / "fresh.duckdb"
        engine = DuckDbEngine(str(db_path))
        engine.initialise_schema()
        cols = {
            r[1] for r in engine._con.execute("PRAGMA table_info('track_positions')").fetchall()
        }
        assert "mode_s_capable" in cols
        assert "last_baro_altitude_ft" in cols
        engine.close()

    def test_schema_migration_adds_columns(self, tmp_path):
        db_path = tmp_path / "old.duckdb"
        # Create old schema without new columns, insert one row, close
        con = duckdb.connect(str(db_path))
        con.execute(
            """
            CREATE TABLE track_positions (
                ts_ms BIGINT NOT NULL,
                icao24 VARCHAR NOT NULL,
                callsign VARCHAR,
                class VARCHAR,
                latitude DOUBLE NOT NULL,
                longitude DOUBLE NOT NULL,
                altitude_ft DOUBLE,
                ground_speed_kt DOUBLE,
                course_deg DOUBLE,
                vertical_rate_fpm DOUBLE,
                squawk VARCHAR,
                coasting BOOLEAN,
                sigma_m DOUBLE
            )
            """
        )
        # also create other tables so initialise_schema doesn't fail? schema.sql uses IF NOT EXISTS
        con.execute(
            """
            CREATE TABLE IF NOT EXISTS stca_alerts (
                triggered_ms BIGINT NOT NULL,
                alert_id VARCHAR NOT NULL,
                icao_a VARCHAR NOT NULL,
                callsign_a VARCHAR,
                icao_b VARCHAR NOT NULL,
                callsign_b VARCHAR,
                min_horizontal_nm DOUBLE,
                min_vertical_ft DOUBLE,
                time_to_closest_s DOUBLE
            )
            """
        )
        con.execute(
            """
            CREATE TABLE IF NOT EXISTS weather_observations (
                observed_ms BIGINT NOT NULL,
                source VARCHAR NOT NULL,
                qnh_hpa DOUBLE,
                wind_dir_deg DOUBLE,
                wind_speed_kt DOUBLE,
                temperature_c DOUBLE,
                dewpoint_c DOUBLE,
                visibility_m DOUBLE,
                raw_text VARCHAR
            )
            """
        )
        # insert one old row
        con.execute(
            """
            INSERT INTO track_positions (
                ts_ms, icao24, callsign, class, latitude, longitude,
                altitude_ft, ground_speed_kt, course_deg, vertical_rate_fpm,
                squawk, coasting, sigma_m
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                1_700_000_000_000,
                "OLD001",
                "OLD01",
                "civil",
                12.0,
                8.0,
                30000,
                400,
                180,
                0,
                "1200",
                False,
                41.0,
            ),
        )
        con.close()
        # Now initialise via engine — should migrate via ALTER TABLE IF NOT EXISTS
        engine = DuckDbEngine(str(db_path))
        engine.initialise_schema()
        cols = {
            r[1] for r in engine._con.execute("PRAGMA table_info('track_positions')").fetchall()
        }
        assert "mode_s_capable" in cols
        assert "last_baro_altitude_ft" in cols
        # Check that SELECT new columns works and old row has NULL/False
        row = engine._con.execute(
            "SELECT mode_s_capable, last_baro_altitude_ft FROM track_positions WHERE icao24='OLD001'"
        ).fetchone()
        assert row is not None
        # old row should have NULL or False for new columns after migration
        assert row[0] is None or row[0] is False or row[0] == 0
        assert row[1] is None
        # New insert with new fields should succeed
        rows = engine.insert_tracks([TRACK_GOOD])
        assert rows == 1
        # now 2 rows total
        assert engine.track_count() == 2
        engine.close()


class TestMalformedDoesNotCrash:
    def _read_frame(self, proc, timeout_s=2.0):
        """Read single NDJSON frame from stdout with timeout via select."""
        end = time.time() + timeout_s
        while time.time() < end:
            if proc.poll() is not None:
                return None
            rlist, _, _ = select.select([proc.stdout], [], [], 0.2)
            if not rlist:
                continue
            line = proc.stdout.readline()
            if not line:
                return None
            line = line.strip()
            if not line:
                continue
            try:
                return json.loads(line)
            except Exception:
                continue
        return None

    def _read_until(self, proc, wanted_evs, timeout_s=3.0):
        """Read NDJSON frames until one of wanted_evs appears, skipping awos."""
        end = time.time() + timeout_s
        while time.time() < end:
            f = self._read_frame(proc, timeout_s=0.5)
            if f is None:
                if proc.poll() is not None:
                    return None
                continue
            ev = f.get("ev")
            if ev in wanted_evs:
                return f
            # skip awos and other unsolicited frames, keep waiting
            if ev == "awos":
                continue
            # if we wanted db_ack/error but got something else, keep looping? return it for assertion
            # but for this test, we want to ignore awos only
            # so return unexpected for caller to fail
            if ev not in ("awos",):
                # if looking for ready, we skip awos only; other ev is candidate
                # For log_tracks we look for db_ack/error, so any other ev counts as result
                return f
        return None

    def _expect_frames(self, proc, count, timeout_s=5.0):
        frames = []
        deadline = time.time() + timeout_s
        while len(frames) < count and time.time() < deadline:
            f = self._read_frame(proc, timeout_s=0.5)
            if f is not None:
                frames.append(f)
            elif proc.poll() is not None:
                break
        return frames

    def test_malformed_track_does_not_crash_process(self, tmp_path):
        """Spec: init -> good -> malformed (latitude NaN-ish) -> good -> 4th good; check acks and alive."""
        db_path = tmp_path / "proc.duckdb"
        script = pathlib.Path(__file__).resolve().parents[1] / "main.py"
        proc = subprocess.Popen(
            [sys.executable, str(script), "--db", str(db_path)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        try:
            def send(cmd):
                proc.stdin.write(json.dumps(cmd) + "\n")
                proc.stdin.flush()

            # init - skip awos frames that may appear immediately after init
            send({"cmd": "init", "db_path": str(db_path)})
            f = self._read_until(proc, {"ready"}, timeout_s=4.0)
            assert f is not None and f.get("ev") == "ready", f"expected ready, got {f}"

            # good batch 1 : TRACK_GOOD
            good = dict(TRACK_GOOD)
            send({"cmd": "log_tracks", "tracks": [good]})
            f1 = self._read_until(proc, {"db_ack", "error"}, timeout_s=4.0)
            assert proc.poll() is None, "process died after first good batch"
            assert f1 is not None, "no ack after good batch 1"
            # good should ack rows==1
            assert f1.get("ev") == "db_ack" and f1.get("rows") == 1, f"good1 ack mismatch: {f1}"

            # malformed batch: latitude NaN-ish
            bad = dict(TRACK_GOOD)
            bad["latitude"] = "NaN-ish"
            bad["icao24"] = "BAD99"
            send({"cmd": "log_tracks", "tracks": [bad]})
            f2 = self._read_until(proc, {"db_ack", "error"}, timeout_s=4.0)
            assert proc.poll() is None, "process died after malformed batch"
            assert f2 is not None, "no response after malformed batch"
            # bad batch should be error or db_ack rows 0/1 but not kill process
            if f2.get("ev") == "error":
                assert f2.get("cmd") == "log_tracks"
            elif f2.get("ev") == "db_ack":
                assert f2.get("rows") in (0, 1), f"bad batch rows unexpected: {f2}"
            else:
                pytest.fail(f"malformed batch unexpected ev: {f2}")

            # good batch 2
            send({"cmd": "log_tracks", "tracks": [good]})
            f3 = self._read_until(proc, {"db_ack", "error"}, timeout_s=4.0)
            assert proc.poll() is None, "process died after second good batch"
            assert f3 is not None and f3.get("ev") == "db_ack" and f3.get("rows") == 1, f"good2 ack mismatch: {f3}"

            # 4th good batch still acks
            send({"cmd": "log_tracks", "tracks": [good]})
            f4 = self._read_until(proc, {"db_ack", "error"}, timeout_s=4.0)
            assert proc.poll() is None, "process died before 4th good batch"
            assert f4 is not None and f4.get("ev") == "db_ack" and f4.get("rows") == 1, f"good4 ack mismatch: {f4}"

            # shutdown
            send({"cmd": "shutdown"})
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                pytest.fail("sidecar did not exit after shutdown — likely hung/crashed")
            assert proc.poll() is not None, "process should have terminated"
            # Verify DB persisted 3 good rows (bad contributed 0)
            engine = DuckDbEngine(str(db_path))
            count = engine.track_count()
            engine.close()
            assert count == 3, f"expected 3 good rows persisted, got {count}"
        finally:
            if proc.poll() is None:
                try:
                    proc.kill()
                except Exception:
                    pass
            try:
                proc.stdin.close()
            except Exception:
                pass


class TestAlertWeatherWriters:
    def test_insert_stca_alerts(self, tmp_path):
        engine = DuckDbEngine(str(tmp_path / "alert.duckdb"))
        engine.initialise_schema()
        alerts = [
            {
                "triggered_ms": 1700000000000,
                "alert_id": "A1",
                "icao_a": "342157",
                "callsign_a": "VL604",
                "icao_b": "061104",
                "callsign_b": "NAF911",
                "min_horizontal_nm": 2.8,
                "min_vertical_ft": 120,
                "time_to_closest_s": 42,
            },
            {"icao_a": "", "icao_b": "XYZ"},  # bad -> skipped
            {
                "triggered_ms": 1700000001000,
                "alert_id": "A2",
                "icao_a": "ABCD01",
                "icao_b": "ABCD02",
                "min_horizontal_nm": 5.0,
            },
        ]
        rows = engine.insert_stca_alerts(alerts)
        assert rows == 2
        cnt = engine._con.execute("SELECT COUNT(*) FROM stca_alerts").fetchone()[0]
        assert cnt == 2
        engine.close()

    def test_log_alerts_and_weather_via_main(self, tmp_path):
        """Smoke test for log_alerts/log_weather NDJSON paths."""
        db_path = tmp_path / "aw.duckdb"
        script = pathlib.Path(__file__).resolve().parents[1] / "main.py"
        proc = subprocess.Popen(
            [sys.executable, str(script), "--db", str(db_path)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        try:
            proc.stdin.write(json.dumps({"cmd": "init", "db_path": str(db_path)}) + "\n")
            proc.stdin.flush()
            time.sleep(0.4)
            alert = {
                "triggered_ms": 1700000000000,
                "alert_id": "A1",
                "icao_a": "342157",
                "icao_b": "061104",
                "min_horizontal_nm": 2.0,
                "min_vertical_ft": 100,
                "time_to_closest_s": 30,
            }
            proc.stdin.write(json.dumps({"cmd": "log_alerts", "alerts": [alert]}) + "\n")
            proc.stdin.flush()
            time.sleep(0.3)
            proc.stdin.write(
                json.dumps(
                    {
                        "cmd": "log_weather",
                        "source": "AWOS",
                        "observed_ms": 1700000000000,
                        "qnh_hpa": 1013.2,
                        "wind_dir_deg": 180,
                    }
                )
                + "\n"
            )
            proc.stdin.flush()
            time.sleep(0.3)
            proc.stdin.write(json.dumps({"cmd": "shutdown"}) + "\n")
            proc.stdin.flush()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                pytest.fail("log_alerts/log_weather path hung")
            engine = DuckDbEngine(str(db_path))
            alert_cnt = engine._con.execute("SELECT COUNT(*) FROM stca_alerts").fetchone()[0]
            weather_cnt = engine._con.execute("SELECT COUNT(*) FROM weather_observations").fetchone()[0]
            engine.close()
            assert alert_cnt == 1
            assert weather_cnt == 1
        finally:
            if proc.poll() is None:
                try:
                    proc.kill()
                except Exception:
                    pass
