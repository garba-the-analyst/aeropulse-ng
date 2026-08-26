# AeroPulse-NG — User Manual

**Version 2.4.0 · air-gapped tactical radar & airspace surveillance workstation**

This manual takes you from a clean machine to a running surveillance desk, then
walks through every screen, control and operational scenario. For design depth see
`docs/ARCHITECTURE.md`; for module internals follow the README links in the root
`README.md`.

---

## 1. System Requirements

| Component | Minimum | Notes |
|---|---|---|
| OS | Ubuntu 22.04/24.04 (X11) | Windows/macOS build via Tauri toolchain untested here |
| CPU / RAM | 4 cores · 8 GB | 60 fps canvas + EKF comfortably inside budget |
| GPU | any OpenGL 2 class | Canvas2D renderer; no discrete GPU required |
| Rust | ≥ 1.77 | `rustup` default stable is fine |
| Node.js | ≥ 20 | ships Vite 8 + TypeScript 7 |
| Python | 3.10+ with `duckdb pyserial pytest` | sidecar venv (setup below) |

Optional hardware: up to three RTL-SDR dongles on a powered USB hub, an RS-485
AWOS mast. **Without them the system runs its deterministic synthetic feed** —
every feature stays operable.

---

## 2. Installation

### 2.1 One-time setup

```bash
# Frontend dependencies
npm install

# Python sidecar environment
cd python-sidecar
python3 -m venv venv
venv/bin/pip install duckdb pyserial pytest
cd ..

# Verify everything compiles and passes
./scripts/verify.sh
```

### 2.2 Webkit headers without sudo (Linux desktop shell)

The desktop shell needs `webkit2gtk-4.1` **development headers** at compile time;
the runtime library is already present on stock Ubuntu. If you lack sudo:

```bash
scripts/webkit-env.sh   # no-op if ~/ap-deps already populated
```

If you are rebuilding the environment from scratch, the recipe is: download
`libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev
libsysprof-capture-4-dev libpsl-dev libnghttp2-dev` via `apt-get download`,
extract with `dpkg -x` into `~/ap-deps/merged`, symlink the system `.so`
runtimes beside them, then source the env file (see script contents).

With sudo it is simply:

```bash
sudo apt install -y libwebkit2gtk-4.1-dev libsoup-3.0-dev build-essential pkg-config
```

### 2.3 Launch modes

| Mode | Command | Use when |
|---|---|---|
| **Desktop application** | `scripts/desktop.sh` | Full operations: both displays, Rust runtime, sidecar, recorder |
| **Headless console** | `cd src-tauri && cargo run --no-default-features --bin aeropulse-headless` | Terminal/SSH, servers, regression observation |
| **Browser UI** | `npm run dev` → http://localhost:1420 (+ `/hud.html`) | UI iteration; synthetic feed auto-engages |
| **Sidecar standalone** | `cd python-sidecar && venv/bin/python main.py --db test.duckdb` | Protocol/persistence testing |

First desktop launch compiles ~700 crates once (`target/debug` cache makes later
starts ≈15 s).

---

## 3. Screen Reference

Both windows share the **Master Command Bar** (Screen 0):

* **Left** — product identity.
* **Centre** — channel health chips (`SDR-1/2/3`, `AWOS`, `EKF ENGINE`). Green dot =
  nominal, flashing red = fault/simulated-absent.
* **Right** — range-scale selector (**km**, SI policy), ZULU + WAT clocks,
  `EMERG 7700` override button (arms every track emergency-red for 30 s).

### Display 1 — Tactical Radar

| Element | Operation |
|---|---|
| Range rings | Drawn at 100 / 200 / 300 km when inside the selected scale; outer boundary = selector value |
| Sweep | 4 s rotation, phosphor-fade wedge |
| Target symbols | ◇ civil (green) · ∧ military (violet) · □ unidentified/anomaly (amber); alert overrides to flashing red ring |
| Coasting indicator | Dashed halo — position is dead-reckoned, not measured |
| Flight Data Block | Three lines right of symbol: `CALLSIGN altM↕`, `src spdKM/H squawk`, annotation (`S-nn`, `EMRG`, `DARK`, …) |
| STCA connector | Dashed red line between conflicting pair, flashing 2 Hz, label `x.x KM / xxx M / T-ss` |
| R&B ruler | Toolbar toggle → click sets anchor, move to measure; readout `R&B bbb° / xx.x KM`; double-click clears |
| Intercept calculator | Right panel: pick target & interceptor, set QRA speed (km/h) → heading °T, range, time-to-intercept, feasibility |

### Display 2 — Operations HUD

| Panel | Contents |
|---|---|
| Flight Data Strip Bay | Arrivals/departures sorted by callsign; edge colour green standard · amber priority/approach (< 1 100 m) · red emergency |
| Triple-Fusion Weather | AWOS surface block (QNH hPa, wind °/km·h⁻¹, temp/dew), visibility km, Harmattan dust-layer top (m), upper-air table from fused nodes with provenance column, raw D-ATIS/METAR feed |
| Threat Matrix | Live anomalies: squawk emergencies, dark targets, silence events, breaches |
| Diagnostics | Per-channel message rates, FFT plot around 1090 MHz / 131.55 MHz, EKF latency gauge against the 2 ms ceiling, DuckDB write rate |

---

## 4. Operational Walkthrough (what you will see)

With the simulator engaged, a standard session unfolds as:

1. **Boot** — six tracks appear already filtered (EKF converges in ~12 fixes);
   weather fills within the first ACARS/AWOS cadence (~30–45 s).
2. **T+45 s** — VL604 squawks 7700: symbol turns red, FDB shows `EMRG`,
   strip edge goes red, threat matrix logs `GENERAL EMERGENCY`.
3. **T+60–80 s** — NAF911's path crosses VL604's with sub-kilometre predicted
   miss → red dashed STCA connector flashes at 2 Hz with minima label;
   HUD strip bay flags both.
4. **T+112 s** — VL604 drops off the (synthetic) RF: dashed coasting halo,
   FDB source flips to `DR`, leader line keeps projecting from filter state.
5. **Rolling** — `0BADC0` transmits positions without identity → after the arming
   window the track is flagged `DARK TARGET` in the matrix.

Press `EMERG 7700` anytime to exercise the override path.

---

## 5. Data & Files

| Path | Produced by | Contents |
|---|---|---|
| `aeropulse.duckdb` (cwd of sidecar) | DuckDB recorder | `track_positions`, `stca_alerts`, `weather_observations` |
| `python-sidecar/aeropulse.duckdb` | standalone sidecar runs | same schema |

Query example:

```bash
python-sidecar/venv/bin/python - <<'PY'
import duckdb
c = duckdb.connect("python-sidecar/aeropulse.duckdb", read_only=True)
print(c.execute("""
  SELECT callsign, count(*) AS fixes, round(min(altitude_ft)*0.3048) AS min_m,
         round(max(altitude_ft)*0.3048) AS max_m
  FROM track_positions GROUP BY callsign ORDER BY fixes DESC LIMIT 10
""").fetchall())
PY
```

---

## 6. Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `pkg-config webkit2gtk-4.1 not found` during build | dev headers absent | §2.2 (sudo-free) or apt install |
| `cargo run could not determine which binary` | two bins in manifest | fixed via `default-run`; update your checkout |
| Vite warns about config loader | older Node/config style | harmless; eliminated by `"type": "module"` in root `package.json` |
| Windows show but tiles black | compositor/GPU quirk | run under X11 session (`XDG_SESSION_TYPE=x11` verified) |
| No weather data | sidecar not spawned | check `[sidecar]` lines in runtime log; persistence degrades gracefully by design |
| STCA never fires in browser tab | demo feed window timing | conflict window is t≈20–140 s after load; reload to replay |
| Clocks wrong | system timezone | ZULU is UTC-derived; WAT assumes UTC+1 always (no DST) |

Runtime warnings you may still see in the CLI are benign: the build is currently
warning-free under `cargo build`; anything new is a regression — run
`./scripts/verify.sh` and attach the logs under `/tmp/opencode/ap-*.log`.

---

## 7. Keyboard / Mouse Summary

| Action | Binding |
|---|---|
| Measure range & bearing | toolbar `R&B RULER` → click anchor → move → double-click clears |
| Change radar scale | command-bar selector (25–450 km) |
| Arm emergency overlay | command-bar `EMERG 7700` |
| Open second display | toolbar `OPS HUD` (auto-opens at boot) |
| Compute intercept | right panel selects + `COMPUTE INTERCEPT VECTOR` |

---

## 8. Shutdown & Persistence

Close either window or `Ctrl-C` the launcher terminal. The sidecar flushes and
exits on stdin closure; DuckDB commits are transactional per batch, so kill -9
loses at most the last 2 s batch. Delete `aeropulse.duckdb` to start recording
fresh.
