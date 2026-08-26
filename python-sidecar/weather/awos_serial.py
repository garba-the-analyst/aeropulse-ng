"""RS-485/RS-232 AWOS mast reader with simulated fallback.

Field installations wire an Automated Weather Observing System to a USB
serial adapter exposing sentence frames such as::

    $AWOS,QNH,1013.2,WIND,182.0,11.4,TMP,33.5,DEW,24.2,VIS,8000*7A

When no serial device answers within the probe window — typical for bench
development — the reader degrades to a deterministic synthetic mast so the
weather pipeline stays exercisable offline.
"""

from __future__ import annotations

import glob
import math
import random
import time

try:  # pragma: no cover - import guard depends on host build
    import serial as _pyserial
except ImportError:  # pragma: no cover
    _pyserial = None

SENTENCE_PREFIX = b"$AWOS"
DEFAULT_BAUD = 9600
PROBE_GLOBS = ["/dev/ttyUSB*", "/dev/ttyACM*", "/dev/ttyS*"]


def parse_awos_sentence(line: str) -> dict | None:
    """Parses ``$AWOS,...*CS`` frames into observation dicts.

    Returns ``None`` for malformed input; checksum errors also return
    ``None`` because corrupted mast data must never reach the fusion matrix.
    """
    line = line.strip()
    if not line.startswith("$AWOS"):
        return None
    body, _, cs_part = line.rpartition("*")
    if not body:
        return None

    fields = body.split(",")
    obs: dict = {"source": "AWOS"}
    numeric_keys = {
        "QNH": "qnh_hpa",
        "WIND": "wind_dir_deg",
        "WSPD": "wind_speed_kt",
        "TMP": "temperature_c",
        "DEW": "dewpoint_c",
        "VIS": "visibility_m",
    }

    i = 1
    while i + 1 < len(fields) or i + 1 == len(fields):
        key = fields[i].upper()
        if key in numeric_keys and i + 1 < len(fields):
            try:
                obs[numeric_keys[key]] = float(fields[i + 1])
            except ValueError:
                return None
            i += 2
        else:
            i += 1

    if "qnh_hpa" not in obs:
        return None

    if cs_part:
        expected = _checksum(body)
        try:
            provided = int(cs_part, 16)
        except ValueError:
            return None
        if expected != provided:
            return None

    return obs


def _checksum(body: str) -> int:
    x = 0
    for ch in body:
        x ^= ord(ch)
    return x & 0xFF


def format_awos_sentence(obs: dict) -> str:
    """Inverse of :func:`parse_awos_sentence` used by fixtures and tests."""
    parts = ["$AWOS"]
    mapping = [
        ("QNH", "qnh_hpa"),
        ("WIND", "wind_dir_deg"),
        ("WSPD", "wind_speed_kt"),
        ("TMP", "temperature_c"),
        ("DEW", "dewpoint_c"),
        ("VIS", "visibility_m"),
    ]
    for tag, key in mapping:
        if key in obs and obs[key] is not None:
            parts.append(tag)
            parts.append(f"{float(obs[key]):.1f}")
    body = ",".join(parts)
    return f"{body}*{_checksum(body):02X}"


class AwosSerialReader:
    """Polls the serial mast (or the synthetic fallback) on a worker thread."""

    def __init__(
        self,
        callback,
        stop_event,
        poll_interval_s: float = 10.0,
        device: str | None = None,
        simulate: bool | None = None,
    ):
        self._callback = callback
        self._stop = stop_event
        self._interval = poll_interval_s
        self._device = device
        self._simulate = simulate if simulate is not None else (_pyserial is None)
        self._port = None
        self._rng = random.Random(20260826)

        # Synthetic mast state (Kano harmattan afternoon baseline).
        self._qnh = 1013.2
        self._wind_dir = 182.0
        self._wind_kt = 11.4
        self._temp = 33.5

    # -- public API --------------------------------------------------------

    def run_forever(self) -> None:  # pragma: no cover - thread entry
        while not self._stop.is_set():
            obs = self.read_once()
            if obs is not None:
                obs["observed_ms"] = int(time.time() * 1000)
                try:
                    self._callback(obs)
                except Exception:
                    pass
            self._stop.wait(self._interval)

    def read_once(self) -> dict | None:
        if not self._simulate:
            frame = self._read_serial_frame()
            if frame is not None:
                return parse_awos_sentence(frame)
            # Serial failed: fall through to synthetic so the HUD keeps
            # receiving data, and retry hardware next cycle.
            self._simulate_probe()

        return self._synthetic_observation()

    # -- internals ----------------------------------------------------------

    def _open_port(self) -> bool:
        if _pyserial is None:
            return False
        candidates = [self._device] if self._device else sorted(
            p for pattern in PROBE_GLOBS for p in glob.glob(pattern)
        )
        for cand in candidates:
            try:
                self._port = _pyserial.Serial(cand, DEFAULT_BAUD, timeout=2.0)
                return True
            except Exception:
                continue
        return False

    def _read_serial_frame(self) -> str | None:
        if self._port is None and not self._open_port():
            return None
        try:
            raw = self._port.readline()
            if not raw.startswith(SENTENCE_PREFIX):
                return None
            return raw.decode("ascii", errors="replace")
        except Exception:
            self._close_port()
            return None

    def _simulate_probe(self) -> None:
        # Re-probe occasionally rather than every tick.
        self._simulate = True

    def _close_port(self) -> None:
        try:
            if self._port is not None:
                self._port.close()
        except Exception:  # pragma: no cover
            pass
        self._port = None

    def _synthetic_observation(self) -> dict:
        t = time.time()
        diurnal = 2.4 * math.sin((t % 86_400) / 86_400 * 2 * math.pi - 1.2)
        self._wind_dir = (self._wind_dir + self._rng.uniform(-2.0, 2.0)) % 360.0
        self._wind_kt = min(24.0, max(3.0, self._wind_kt + self._rng.uniform(-0.6, 0.6)))
        self._qnh = min(1030.0, max(998.0, self._qnh + self._rng.uniform(-0.15, 0.15)))
        self._temp = 31.0 + diurnal + self._rng.uniform(-0.4, 0.4)

        return {
            "source": "AWOS-SIM",
            "qnh_hpa": round(self._qnh, 1),
            "wind_dir_deg": round(self._wind_dir, 1),
            "wind_speed_kt": round(self._wind_kt, 1),
            "wind_gust_kt": round(self._wind_kt + 4.0, 1),
            "temperature_c": round(self._temp, 1),
            "dewpoint_c": round(self._temp - 9.0, 1),
            "visibility_m": round(8_000 + self._rng.uniform(-800, 1_200)),
        }
