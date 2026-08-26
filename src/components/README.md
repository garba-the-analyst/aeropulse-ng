# Frontend Components — Dual-Display Workspace

Path: `src/components/`
Stack: React 19 + TypeScript (strict) + Canvas2D, styled by `src/styles.css`
design tokens. All operator-facing values render through `src/lib/units.ts`
(NCAA/ICAO Annex-5 SI policy).

---

## Layout

```text
components/
├── shared/MasterCommandBar.tsx     Screen 0 — persistent top bar
├── radar/                          Display 1 (tactical canvas)
│   ├── RadarCanvas.tsx             60 fps rAF renderer (the centrepiece)
│   ├── FlightDataBlock.tsx         3-line tag formatter (pure)
│   └── STCABoundBox.tsx            conflict-pair geometry + label (pure)
├── hud/                            Display 2 panels
│   ├── FlightStripTable.tsx        arrivals/departures strip bay
│   ├── WeatherGridHUD.tsx          triple-fusion weather panel
│   └── SystemMetrics.tsx           FFT plot + latency/write gauges
└── defense/
    ├── ThreatMatrix.tsx            live anomaly list (+ exports InterceptPanel)
    └── InterceptPanel.tsx          re-export shim for App import path
```

Window roots: `src/App.tsx` (radar) · `src/HudApp.tsx` (HUD), mounted by
`main.tsx` / `hud-main.tsx` from `index.html` / `hud.html` (Vite multi-page).

## MasterCommandBar

Product id · channel health chips (`SDR-1/2/3`, AWOS, EKF) · range-scale
selector (**km**, options from `RANGE_OPTIONS_KM`) · ZULU + WAT clocks
(WAT = UTC+1 fixed) · `EMERG 7700` override → `invokeSafe('trigger_emergency_override')`
with a 30 s local armed state.

## RadarCanvas — rendering model

**Zero React churn per frame**: the rAF loop reads the telemetry store ref
directly (`getSnapshot()`); React state only drives props (range scale, tool
mode). Per frame:

1. DPR-aware resize; SI projection: site DNKN anchored centre,
   `kmPx = min(w,h)/2 / rangeKm`, degrees-per-km from series-expanded
   metres-per-degree.
2. Range rings **100/200/300 km** (`RANGE_RINGS_KM`) when inside scale, outer
   boundary ring, crosshair grid + 30° spokes at 40 % opacity charcoal.
3. Conic-gradient sweep (4 s period).
4. Geofences (dashed amber polygons, fetched via IPC every 15 s).
5. Leader lines beneath symbols — geodetic polylines straight from the engine's
   dead-reckoning projection.
6. Symbols per MIL-STD-2525D mapping: ◇ civil `#00FF66`, ∧ military `#7C4DFF`,
   □ unknown/anomaly `#FFB300`; alert → `#FF1744`; coasting halo dashed;
   emergency extra ring.
7. FDB three-line tags via `formatFDB()` — metric line1 (`alt M ↕`) and line2
   (`1090 nnnKM/H squawk`).
8. STCA connectors flashing 2 Hz (`Date.now() % 500 < 250`) with SI minima label.
9. R&B ruler overlay in white with `R&B bbb° / xx.x KM` readout.
10. Site marker `DNKN`.

Intercept solutions arrive as `ap-intercept-solution` DOM CustomEvents and draw
on the next frames.

## HUD Panels

* **FlightStripTable** — arrivals (< 1 650 m or descending) vs departures bays;
  edge classes `standard/priority/emergency`; ETA derived from ground speed to
  the 222 km volume boundary.
* **WeatherGridHUD** — surface KV block (QNH hPa, wind °/km·h⁻¹, temp/dew,
  visibility km, dust-layer top m, node count) + upper-air table (altitude m,
  wind °, speed km/h, °C, provenance bitmask decode) + raw D-ATIS/METAR feed.
* **SystemMetrics** — per-channel message rates; spectrum plot synthesised from
  live message-rate amplitude around the two centre spikes (documented stand-in
  for the RF tap); gauges: EKF latency vs 2 ms budget (warn/crit bands), DuckDB
  writes/s, track/STCA counts.

## Defense Panels

* **ThreatMatrix** — folds track alerts + engine anomaly notes into one ranked
  list with human reasons (`GENERAL EMERGENCY (7700)`, `DARK TARGET…`).
* **InterceptPanel** — target/interceptor selects from live tracks, QRA speed in
  km/h (converted to kt for the backend solver); renders heading °T, speed,
  range km, TTI minutes and FEASIBLE/NO-SOLUTION status; browser-dev fallback
  computes a local approximation when IPC is absent.

## Styling Contract

All colours/typography come from CSS custom properties in `src/styles.css`
(`--ap-civil #00FF66`, `--ap-military #7C4DFF`, `--ap-unknown #FFB300`,
`--ap-alert #FF1744`, background `#0B0E14`, JetBrains Mono stack). The alert
blink animation is 1 Hz steps; STCA canvas flashing is deliberately independent
at 2 Hz per spec.
