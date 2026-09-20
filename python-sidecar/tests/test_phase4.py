"""Phase 4 sidecar protocol + migration tests."""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import time

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

TRACK_BAD = {
    "icao24": "",  # empty icao -> skipped
    "callsign": "BAD",
    "latitude": "not-a-number",
    "longitude": None,
    "altitude_ft": "bad",
    "last_update_ms": 1_700_000_000_000,
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
        engine = DuckDbEngine(str(tmp_path / "partial.duckdb"))
        engine.initialise_schema()
        rows = engine.insert_tracks([TRACK_GOOD, TRACK_BAD, TRACK_GOOD])
        assert rows == 2
        assert engine.track_count() == 2
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
        # Create old schema without new columns
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
        con.close()
        # Now initialise via engine — should migrate
        engine = DuckDbEngine(str(db_path))
        engine.initialise_schema()
        cols = {
            r[1] for r in engine._con.execute("PRAGMA table_info('track_positions')").fetchall()
        }
        assert "mode_s_capable" in cols
        assert "last_baro_altitude_ft" in cols
        # Insertion should now succeed
        rows = engine.insert_tracks([TRACK_GOOD])
        assert rows == 1
        engine.close()


class TestMalformedDoesNotCrash:
    def _read_frames_until(self, proc, want_evs, timeout_s=5.0):
        """Collect stdout JSON frames until wanted evs seen or timeout."""
        frames = []
        start = time.time()
        # proc.stdout is text mode buffered; read line by line with timeout via polling
        import select

        # fallback: use iterative readline with non-blocking via timeout
        # We'll read using proc.stdout.readline in a loop with timeout
        # Use a simple polling loop; set proc.stdout to non-blocking via select if available
        while time.time() - start < timeout_s:
            # Check if proc terminated
            if proc.poll() is not None:
                break
            # Try to read with timeout 0.2s using select on unix
            try:
                # select works on pipes on linux
                import select as sel
                rlist, _, _ = sel.select([proc.stdout], [], [], 0.2)
                if not rlist:
                    continue
                line = proc.stdout.readline()
                if not line:
                    break
                line = line.strip()
                if not line:
                    continue
                try:
                    obj = json.loads(line)
                except Exception:
                    continue
                frames.append(obj)
                # check if we have all wanted
                evs = [f.get("ev") for f in frames]
                if all(e in evs for e in want_evs):
                    break
                # also check db_ack rows accumulation
                if len([f for f in frames if f.get("ev") == "db_ack"]) >= 3:
                    break
            except Exception:
                time.sleep(0.1)
        return frames

    def test_malformed_track_does_not_crash_process(self, tmp_path):
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
            # Send init
            proc.stdin.write(json.dumps({"cmd": "init", "db_path": str(db_path)}) + "\n")
            proc.stdin.flush()
            time.sleep(0.5)
            # Send good
            proc.stdin.write(json.dumps({"cmd": "log_tracks", "tracks": [TRACK_GOOD]}) + "\n")
            proc.stdin.flush()
            time.sleep(0.3)
            # Send malformed batch (bad row inside)
            proc.stdin.write(json.dumps({"cmd": "log_tracks", "tracks": [TRACK_BAD]}) + "\n")
            proc.stdin.flush()
            time.sleep(0.3)
            # Send good again
            proc.stdin.write(json.dumps({"cmd": "log_tracks", "tracks": [TRACK_GOOD]}) + "\n")
            proc.stdin.flush()
            time.sleep(0.3)
            proc.stdin.write(json.dumps({"cmd": "shutdown"}) + "\n")
            proc.stdin.flush()

            # Wait for process to exit gracefully
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                pytest.fail("sidecar did not exit after shutdown — likely hung/crashed")

            # Process should have exited cleanly (returncode 0 or None killed)
            # After shutdown, poll should be not None (exited)
            assert proc.poll() is not None, "process should have terminated"
            # Verify DB has both good rows (2) and bad batch inserted 0
            # need to read stdout frames that were buffered; use communicate approach alternative
            # We already used poll; try to get remaining stdout via proc.stdout.read if not closed
            # For robustness, reopen DB and check count
            engine = DuckDbEngine(str(db_path))
            # don't re-initialise schema to avoid altering count; just count
            count = engine.track_count()
            engine.close()
            assert count == 2, f"expected 2 good rows persisted, got {count}"

            # Also verify process didn't crash before handling second good batch:
            # count ==2 implies second good was processed after malformed
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
