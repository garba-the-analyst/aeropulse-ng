"""Sidecar test suite: AWOS sentence codec, METAR parsing, DuckDB engine."""

from __future__ import annotations

import sys
import time
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from weather.awos_serial import (  # noqa: E402
    AwosSerialReader,
    format_awos_sentence,
    parse_awos_sentence,
)
from weather.metar_parser import is_harmattan_profile, parse_metar  # noqa: E402
from db.duckdb_engine import DuckDbEngine  # noqa: E402


# ---------------------------------------------------------------------------
# AWOS serial sentence codec
# ---------------------------------------------------------------------------


class TestAwosSentences:
    def test_roundtrip_full_observation(self):
        obs = {
            "qnh_hpa": 1013.2,
            "wind_dir_deg": 182.0,
            "wind_speed_kt": 11.4,
            "temperature_c": 33.5,
            "dewpoint_c": 24.2,
            "visibility_m": 8000.0,
        }
        line = format_awos_sentence(obs)
        parsed = parse_awos_sentence(line)
        assert parsed is not None
        assert parsed["qnh_hpa"] == pytest.approx(1013.2, abs=0.05)
        assert parsed["wind_dir_deg"] == pytest.approx(182.0, abs=0.05)
        assert parsed["temperature_c"] == pytest.approx(33.5, abs=0.05)

    def test_checksum_corruption_rejected(self):
        line = format_awos_sentence({"qnh_hpa": 1013.2})
        body, _, cs = line.rpartition("*")
        bad_cs = format(int(cs, 16) ^ 0x01, "02X")
        assert parse_awos_sentence(f"{body}*{bad_cs}") is None

    def test_garbage_lines_return_none(self):
        assert parse_awos_sentence("") is None
        assert parse_awos_sentence("NMEA GPGGA junk") is None
        assert parse_awos_sentence("$AWOS,QNH,abc*00") is None
        # Missing QNH group -> invalid frame by contract.
        assert parse_awos_sentence("$AWOS,TMP,30.0*FF") is None

    def test_partial_frames_still_parse(self):
        line = format_awos_sentence({"qnh_hpa": 1009.0, "wind_dir_deg": 45.0})
        parsed = parse_awos_sentence(line)
        assert parsed is not None
        assert "temperature_c" not in parsed


class TestSyntheticMast:
    def test_simulated_reader_produces_plausible_values(self):
        stop = __import__("threading").Event()
        reader = AwosSerialReader(callback=lambda o: None, stop_event=stop, simulate=True)
        for _ in range(20):
            obs = reader.read_once()
            assert 990.0 <= obs["qnh_hpa"] <= 1035.0
            assert 0.0 <= obs["wind_dir_deg"] < 360.0
            assert 2.0 <= obs["wind_speed_kt"] <= 26.0
            assert obs["visibility_m"] > 3_000

    def test_reader_callback_receives_timestamped_frames(self):
        received = []
        stop = __import__("threading").Event()
        reader = AwosSerialReader(
            callback=received.append,
            stop_event=stop,
            poll_interval_s=0.01,
            simulate=True,
        )
        import threading

        thread = threading.Thread(target=reader.run_forever, daemon=True)
        thread.start()
        deadline = time.time() + 2.0
        while len(received) < 3 and time.time() < deadline:
            time.sleep(0.01)
        stop.set()
        thread.join(timeout=1.0)
        assert len(received) >= 3
        assert all("observed_ms" in o for o in received)


# ---------------------------------------------------------------------------
# METAR parser
# ---------------------------------------------------------------------------


class TestMetarParser:
    SAMPLE = "METAR DNKN 261200Z 18012KT 6000 HZ FEW030 33/24 Q1013 NOSIG="

    def test_kano_harmattan_report(self):
        parsed = parse_metar(self.SAMPLE)
        assert parsed["station"] == "DNKN"
        assert parsed["report_type"] == "METAR"
        assert parsed["wind_dir_deg"] == 180.0
        assert parsed["wind_speed_kt"] == 12.0
        assert parsed["visibility_m"] == 6_000.0
        assert parsed["temperature_c"] == 33.0
        assert parsed["dewpoint_c"] == 24.0
        assert parsed["qnh_hpa"] == 1013.0
        assert is_harmattan_profile(parsed) is True

    def test_gust_and_variable_wind(self):
        parsed = parse_metar("SPECI DNMM 261205Z VRB08G18KT CAVOK 29/22 Q1012=")
        assert parsed["station"] == "DNMM"
        assert parsed.get("wind_variable") is True
        assert parsed["wind_speed_kt"] == 8.0
        assert parsed["wind_gust_kt"] == 18.0
        assert parsed["visibility_m"] == 10_000.0
        assert parsed.get("cavok") is True

    def test_negative_temperatures_use_m_prefix(self):
        parsed = parse_metar("METAR DIAP 261200Z 34005KT 9999 FEW010 M03/M12 Q1024=")
        assert parsed["temperature_c"] == -3.0
        assert parsed["dewpoint_c"] == -12.0
        assert is_harmattan_profile(parsed) is False

    def test_degraded_text_never_raises(self):
        parsed = parse_metar("~~corrupted rf frame~~")
        assert "raw" in parsed
        assert "qnh_hpa" not in parsed


# ---------------------------------------------------------------------------
# DuckDB engine
# ---------------------------------------------------------------------------


TRACK_FIXTURE = {
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
}


class TestDuckDbEngine:
    def test_track_batch_persists_and_counts(self, tmp_path):
        db_path = tmp_path / "test.duckdb"
        engine = DuckDbEngine(str(db_path))
        engine.initialise_schema()

        rows = engine.insert_tracks([TRACK_FIXTURE] * 5)
        assert rows == 5
        assert engine.track_count() == 5
        assert engine.total_rows_written == 5
        engine.close()

        # Reopen: durability across process restarts.
        engine2 = DuckDbEngine(str(db_path))
        engine2.initialise_schema()
        assert engine2.track_count() == 5
        engine2.close()

    def test_empty_batch_is_noop(self, tmp_path):
        engine = DuckDbEngine(str(tmp_path / "e.duckdb"))
        engine.initialise_schema()
        assert engine.insert_tracks([]) == 0
        assert engine.track_count() == 0
        engine.close()

    def test_weather_recording(self, tmp_path):
        engine = DuckDbEngine(str(tmp_path / "w.duckdb"))
        engine.initialise_schema()
        assert (
            engine.record_weather(
                source="AWOS",
                observed_ms=1_700_000_000_000,
                qnh_hpa=1013.2,
                wind_dir_deg=182.0,
                wind_speed_kt=11.4,
                raw_text=None,
            )
            == 1
        )
        row = engine._con.execute(
            "SELECT source, qnh_hpa FROM weather_observations"
        ).fetchone()
        assert row == ("AWOS", 1013.2)
        engine.close()

    def test_bad_numeric_fields_coerced_to_null(self, tmp_path):
        bad = dict(TRACK_FIXTURE)
        bad["altitude_ft"] = "not-a-number"
        engine = DuckDbEngine(str(tmp_path / "b.duckdb"))
        engine.initialise_schema()
        engine.insert_tracks([bad])
        row = engine._con.execute(
            "SELECT altitude_ft FROM track_positions WHERE icao24='342157'"
        ).fetchone()
        assert row is not None and row[0] is None
        engine.close()
