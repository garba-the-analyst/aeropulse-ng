"""METAR / SPECI decoder for terminal-sector weather corroboration.

Extracts the canonical observation groups the fusion matrix consumes:
wind (dddffKT with gust), visibility, temperature/dewpoint and QNH.
Deliberately regex-light and dependency-free so it runs on air-gapped
hosts without the ``metar`` package installed.
"""

from __future__ import annotations

import re

_WIND_RE = re.compile(
    r"\b(?P<dir>\d{3}|VRB)(?P<spd>\d{2,3})(?:G(?P<gust>\d{2,3}))?KT\b"
)
_VIS_RE = re.compile(r"\b(?P<vis>\d{4})\b(?:SM)?")
_TEMP_RE = re.compile(r"\b(?P<temp>M?\d{2})/(?P<dew>M?\d{2})\b")
_QNH_RE = re.compile(r"\bQ(?P<qnh>\d{4})\b")
_STATION_RE = re.compile(r"^(METAR|SPECI)\s+(?P<station>[A-Z]{4})")


def _signed_temp(token: str) -> float:
    if token.startswith("M"):
        return -float(token[1:])
    return float(token)


def parse_metar(text: str) -> dict:
    """Parses a raw METAR/SPECI string into fusion-matrix fields.

    Missing groups are simply absent from the result; malformed input
    returns whatever could be decoded rather than raising, mirroring how
    partial RF text frames arrive in practice.
    """
    result: dict = {"raw": text.strip()}
    head = _STATION_RE.match(text.strip().upper())
    if head:
        result["report_type"] = head.group(1)
        result["station"] = head.group(2)

    wind = _WIND_RE.search(text.upper())
    if wind:
        if wind.group("dir") == "VRB":
            result["wind_dir_deg"] = None
            result["wind_variable"] = True
        else:
            result["wind_dir_deg"] = float(wind.group("dir"))
        result["wind_speed_kt"] = float(wind.group("spd"))
        if wind.group("gust"):
            result["wind_gust_kt"] = float(wind.group("gust"))

    vis = _VIS_RE.search(text.replace("CAVOK", ""))
    if vis:
        result["visibility_m"] = float(vis.group("vis"))
    if " CAVOK" in f" {text.upper()} ":
        result["visibility_m"] = 10_000.0
        result["cavok"] = True

    temps = _TEMP_RE.search(text.upper())
    if temps:
        result["temperature_c"] = _signed_temp(temps.group("temp"))
        result["dewpoint_c"] = _signed_temp(temps.group("dew"))

    qnh = _QNH_RE.search(text.upper())
    if qnh:
        result["qnh_hpa"] = float(qnh.group("qnh"))

    return result


def is_harmattan_profile(parsed: dict) -> bool:
    """Heuristic haze classifier used to annotate dust-layer estimates."""
    vis = parsed.get("visibility_m")
    temp = parsed.get("temperature_c")
    if vis is None:
        return False
    hazy_band = 1_500.0 <= vis < 9_000.0
    warm = temp is None or temp > 25.0
    return bool(hazy_band and warm)
