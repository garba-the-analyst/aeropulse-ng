# AeroPulse-NG

**Tactical Radar & Airspace Surveillance Engine — air-gapped, offline-first, NCAA/ICAO-aligned.**

AeroPulse-NG turns commodity software-defined radios and a standard PC into a working
tactical air-traffic surveillance workstation for civil ATC (NAMA) and military command
operations (NAF). It ingests over-the-air aviation telemetry, filters it through a
6-state Extended Kalman Filter, detects separation conflicts with an R\*-tree-accelerated
STCA engine, fuses three independent weather sources into one 3D matrix — all with
**zero internet dependency**.

> **Status: feature-complete prototype (v2.4.0).** All five engine subsystems implemented
> and unit-tested; dual-window desktop application builds and runs on Linux.
> See [Status Ledger](#status-ledger) and [docs/USER_MANUAL.md](docs/USER_MANUAL.md).

---

## 1. Quick Start

```bash
# One-shot verification battery (Rust + Python + TypeScript + bundle)
./scripts/verify.sh

# Full desktop application (dual windows, Rust runtime, sidecar)
scripts/desktop.sh

# Headless tactical console (terminal-only, SSH-friendly)
cd src-tauri && cargo run --no-default-features --bin aeropulse-headless

# UI-only development in a browser (synthetic feed, no Rust host needed)
npm run dev            # → http://localhost:1420 and /hud.html
```

Full setup prerequisites, first-run walkthrough and per-screen operations:
**[docs/USER_MANUAL.md](docs/USER_MANUAL.md)**.

---

## 2. What It Does

| Capability | Implementation |
|---|---|
| ADS-B / Mode S reception (1090 MHz) | DF17 decode, CRC-24 single-bit repair, global CPR even/odd position solve, Gillham altitude codec, velocity & identity extraction |
| ACARS VHF (131.550 MHz) | Frame sync, CCITT-CRC integrity, METAR/SPECI/D-ATIS classification |
| Track filtering | 6-state EKF `[x,y,z,vx,vy,vz]` in local ENU metres, Joseph-form covariance update, Mahalanobis gating with acquisition warm-up |
| Dead reckoning | Constant-velocity coasting through RF dropouts; leader-line projection 120 s ahead |
| Conflict detection | R\*-tree spatial index (Beckmann split + forced reinsertion) → pairwise time-stepped refinement against ICAO Doc 4444 minima inside a 120 s lookahead |
| Weather fusion | AWOS serial (RS-485 via Python sidecar) + ACARS D-ATIS/METAR + Mode S BDS 4,4/4,5 registers → inverse-distance-weighted 3D matrix with ISA fallback and Harmattan dust-layer estimate |
| Defence overlays | Squawk 7500/7600/7700 alerting, dark-target & transponder-silence detection, polygon geofencing with vertical bands, lead-pursuit intercept solver |
| Displays | Display 1: 60 fps radar canvas (rings, sweep, MIL-STD-2525D symbols, FDB tags, flashing STCA connectors, R&B ruler). Display 2: flight-strip bay, weather HUD, FFT diagnostics, threat matrix, intercept calculator |

### Demo scenarios baked into the synthetic feed

| T+ | Event |
|---|---|
| boot | Six aircraft airborne in the DNKN sector; STCA geometry already inside lookahead |
| 45 s | VL604 squawks **7700** (general emergency) + TC-28 status frame |
| ~70 s | NAF911 intercepts VL604's path — predicted miss **< 1 km** → STCA flash |
| 112 s+ | VL604 enters a 28 s transponder dropout → track coasts on dead reckoning |
| rolling | Dark target `0BADC0` transmits without identity; flagged after arming window |
| every 45 s | ACARS METAR burst; every 30 s AWOS surface frame; every 5 s upper-air sample |

---

## 3. Architecture

```
┌──────────────────────────────────────────────────────────────────────────┐
│                          INGESTION (RF / SIMULATED)                      │
│  SDR-1 1090 MHz ──► Mode S decoder ──┐                                   │
│  SDR-2 131.55   ──► ACARS decoder ───┤                                   │
│  simulator.rs ────► synthesised DF17 frames through the SAME decoders  │
└──────────────────────────────┬─────────────────────────────────────────┘
                               ▼ IngestEvent stream
┌──────────────────────────────────────────────────────────────────────────┐
│                    SURVEILLANCE ENGINE (engine.rs, 60 Hz)                │
│   queue ► EKF predict ► fuse ► STCA scan ► anomaly scan ► geofence scan  │
│                 ▼                                                        │
│      EngineSnapshot { tracks · FDBs · STCA · weather · status }          │
└──────────────┬───────────────────────────────────────┬───────────────────┘
               ▼ tokio broadcast (60 Hz)              ▼ NDJSON stdio
┌──────────────────────────────┐      ┌──────────────────────────────────┐
│ DISPLAY 1 — Radar canvas     │      │ Python sidecar                   │
│ DISPLAY 2 — Operations HUD   │      │  ├ DuckDB flight recorder        │
│ (Tauri v2 webviews, React)   │      │  └ AWOS RS-485 mast reader       │
└──────────────────────────────┘      └──────────────────────────────────┘
```

Detailed subsystem documentation lives next to the code:

| Path | Contents |
|---|---|
| [`src-tauri/src/hardware/`](src-tauri/src/hardware/) | Signal decoding, SDR registry, synthetic feed |
| [`src-tauri/src/kinematics/`](src-tauri/src/kinematics/) | Linear algebra, EKF, R\*-tree, STCA math |
| [`src-tauri/src/weather_fusion/`](src-tauri/src/weather_fusion/) | BDS registers, ISA atmosphere, IDW interpolation |
| [`src-tauri/src/defense/`](src-tauri/src/defense/) | Anomaly detection, geofencing, intercept solver |
| [`src-tauri/src/commands/`](src-tauri/src/commands/) | Tauri IPC command surface |
| [`src-tauri/src/engine.rs`](src-tauri/src/engine.rs) | Fusion loop (module doc in header comment + ARCHITECTURE.md) |
| [`python-sidecar/`](python-sidecar/) | Persistence + AWOS bridge incl. wire protocol |
| [`src/components/`](src/components/) · [`src/hooks/`](src/hooks/) · [`src/types/`](src/types/) · [`src/lib/`](src/lib/) | Frontend modules |
| [`docs/USER_MANUAL.md`](docs/USER_MANUAL.md) | Setup, operations, troubleshooting |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Deep-dive design rationale & roadmap |

---

## 4. Units Policy — NCAA / ICAO Annex 5 (SI Metric)

Per project directive, every operator-facing value is presented in **SI metric**,
consistent with NCAA's adoption of ICAO Annex 5. The single source of truth is
[`src/lib/units.ts`](src/lib/units.ts), mirrored by the headless console formatter.

| Quantity | Displayed | Conversion from wire |
|---|---|---|
| Distance / range rings | km | 1 NM = 1.852 km |
| Altitude | m (whole metres) | 1 ft = 0.3048 m |
| Speed | km/h | 1 kt = 1.852 km/h |
| Vertical rate | m/s equivalent | 1 fpm = 0.00508 m/s |
| Visibility | km | WMO convention |
| Pressure | hPa | QNH invariant (no conversion) |

**Deliberate regulatory invariants** (not converted): 4-digit octal squawk codes,
degrees-true bearings, and the Doc 4444 separation *thresholds* — the safety logic
evaluates the legal constants (5 NM / 1 000 ft) internally and renders their SI
equivalents (**9.26 km / 305 m**) on screen. Engine internals never convert: the
EKF integrates metres/m·s⁻¹ natively; DO-260B wire values decode to feet/knots and
transform once at the presentation boundary.

---

## 5. Repository Layout

```text
aeropulse-ng/
├── scripts/                  verify.sh · desktop.sh · webkit-env.sh
├── docs/                     USER_MANUAL.md · ARCHITECTURE.md
├── src-tauri/                Rust core (Tauri v2 host)
│   ├── src/
│   │   ├── hardware/         decoders · registry · simulator     [+ README]
│   │   ├── kinematics/       linalg · ekf · rtree · stca_math    [+ README]
│   │   ├── weather_fusion/   bds_extractor · spatial_interp      [+ README]
│   │   ├── defense/          dark_target · geofence · intercept  [+ README]
│   │   ├── commands/         Tauri IPC handlers                  [+ README]
│   │   ├── engine.rs         60 Hz fusion loop
│   │   ├── sidecar.rs        NDJSON process bridge
│   │   ├── models.rs         shared wire contracts
│   │   └── bin/aeropulse-headless.rs   ANSI tactical console
│   └── icons/                generated RGBA icon set
├── python-sidecar/           DuckDB recorder · AWOS reader       [+ README]
├── src/                      TypeScript workspace
│   ├── components/           radar · hud · defense · shared      [+ README]
│   ├── hooks/                telemetry store · event bindings    [+ README]
│   ├── lib/                  units policy · IPC accessors        [+ README]
│   └── types/                mirrored wire contracts             [+ README]
└── .github/workflows/        CI verification
```

---

## 6. Verification

```bash
./scripts/verify.sh          # full battery — currently 108 checks green:
                             #   Rust 90 · Python 14 · tsc strict · vite build
npm run engine:test          # Rust only
npm run sidecar:test         # pytest only
npm run typecheck            # TypeScript only
```

Test coverage highlights: CRC-24 against published reference frames, CPR global
solve round-trips across zone boundaries, EKF convergence under noise with outlier
gating, R\*-tree integrity at 2 000 inserts, STCA conflict/divergence discrimination,
ACARS checksum corruption handling, geofence entry/re-arm cycles, intercept solver
feasibility envelope, end-to-end fusion pipeline over 110 simulated seconds.

---

## 7. Status Ledger

### Implemented ✅

- **Hardware layer** — DF17/CPR/Gillham decode with single-bit repair; ACARS framing,
  CRC-16, weather classification; USB serial-lock registry (sysfs probe); deterministic
  six-aircraft simulator feeding the *production* decode path.
- **Kinematics** — hand-rolled 6×6 Cholesky linear algebra; Joseph-form EKF with
  acquisition-phase gate warm-up; WGS-84↔ENU transforms; full R\*-tree;
  time-stepped STCA detector.
- **Weather fusion** — BDS 4,4/4,5 register codecs; ICAO standard atmosphere;
  IDW spatial interpolation with confidence metric; triple-source priority chain
  (AWOS > D-ATIS > METAR); dust-layer heuristic.
- **Defence** — emergency squawk mapping, identity-less emitter flagging, absentee
  silence detection; ray-cast geofences with altitude bands and stateful re-arm;
  damped fixed-point intercept solver + minimum-speed bisection.
- **Engine** — queued ingestion, single-predict-per-tick discipline, periodic scans,
  snapshot bus (`telemetry://snapshot`, 60 Hz), emergency override hold-down.
- **Host integration** — 12 Tauri IPC commands; sidecar process lifecycle with
  graceful degradation; DuckDB schema + transactional batch writer.
- **Frontend** — both displays fully implemented in SI-metric presentation;
  browser demo feed mirroring the simulator roster for offline UI work.
- **Tooling** — one-command verification battery; sudo-free webkit dev environment
  builder; generated icon set.

### Remaining ⬜ (roadmap)

1. **Real RTL-SDR I/Q tap** — libusb sampling → ppm-correction → bit synchroniser
   feeding `decode_frame()`; registry plumbing exists, DSP front-end is the gap.
2. **Live FFT spectrum** — plot currently synthesised from message-rate telemetry;
   replace with magnitudes from the real tap.
3. **Physical AWOS wiring** — sentence parser + simulator ready; needs field cable
   and port characterisation.
4. **Comm-B BDS extraction from live DF20/21 frames** — codecs exist and are
   round-trip tested; uplink interrogation scheduling pending.
5. **Persistence of alerts/weather to DuckDB** — tables exist; track writer is wired,
   alert writer is not.
6. **Multi-site handover, RBAC views, offline map tiles** — see ARCHITECTURE roadmap.

---

## 8. Standards & Integrity Notes (NDAIE submission context)

- **Standards referenced**: NCAA Nig.CARs (via ICAO Annexes), ICAO Annex 5 (units),
  Annex 10 Vol III (Mode S), Annex 2 (signals), Doc 4444 (separation minima),
  DO-260B (ADS-B MOPS), HF-STD-010A (ATC display human factors),
  MIL-STD-2525D (symbology).
- **Academic integrity**: this codebase was produced with significant generative-AI
  assistance as declared in the NDAIE application. All third-party dependencies are
  MIT/Apache licensed; no proprietary code was incorporated.
- Proprietary dual-use airspace surveillance platform. All rights reserved (LICENSE).
