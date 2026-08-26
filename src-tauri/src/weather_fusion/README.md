# Weather Fusion Layer — Triple-Source Offline Meteorology

Path: `src-tauri/src/weather_fusion/`
Tests: 14 (bds_extractor 5 · spatial_interp 4 · fusion engine 5)

Fuses three physically independent atmospheric sources into one 3D matrix with
zero cloud dependency:

| Layer | Source | Ingest entry point |
|---|---|---|
| Surface | AWOS mast via Python sidecar (RS-485) | `ingest_surface(AwosObservation)` |
| Terminal sector | ACARS D-ATIS / METAR text | `ingest_datis_text` · `ingest_metar_text` |
| En-route | Mode S BDS 4,4 / 4,5 registers | `ingest_upper_air(WeatherNode)` |

---

## `bds_extractor.rs` — Comm-B Register Codecs

Encoder **and** decoder for both registers over the 7-byte BCS field, so bench
fixtures round-trip against production code:

**BDS 4,4 (Meteorological Routine Air Report)**

```
WS  [0..12]   wind speed, kt          (0 = invalid, >550 rejected)
WD  [12..23]  direction ×360/2048 deg
ST  [23]      direction-validity flag
SAT [24..40]  static air temp, two's-complement ×0.25 °C
```

**BDS 4,5 (Meteorological Hazard Report)**

```
TURB [0..2]   0 none .. 3 severe
SHR  [2..4]   windshear report scale
MB   [4..6]   microburst report scale
ICE  [6..9]   icing state (7 = all-invalid sentinel)
ΔT   [9..25]  ISA deviation, signed ×0.1 °C
```

Bit layouts are documented at each function per DO-260B field naming; when a
real Comm-B tap lands (roadmap #4) the extraction side needs zero changes.

## `spatial_interp.rs` — Atmosphere & Interpolation

* **ISA reference** — troposphere lapse 6.5 °C/km with the standard exponential
  pressure relation; isothermal 216.65 K above the tropopause. Validated against
  textbook values at sea level and FL350.
* **IDW sampling** (`sample_atmosphere`) — nodes older than 15 min are dropped;
  survivors ranked by horizontal distance with an altitude-mismatch penalty;
  top-8 blended with weight `1/((d_km+0.25)² · (1+penalty))`. **Wind is averaged
  as vectors**, never by averaging degrees (the 350°/010° averaging trap).
  Pressure blends toward ISA proportionally to altitude mismatch; density from
  the ideal-gas relation on the fused pair. Confidence = source diversity ×
  inverse range.
* Out-of-range or fully stale queries return pure ISA with `confidence == 0` so
  downstream consumers can flag degraded corrections honestly.
* **Dust layer heuristic** — Harmattan ceiling estimate: visibility-driven base
  (kicks in below 9 km visibility) plus a convective thermal bump above 28 °C
  surface temperature.

## `mod.rs` — Fusion Engine

Sliding 512-node deque for airborne samples; surface fields follow a strict
priority chain — **first writer wins a gap, AWOS always overwrites**: D-ATIS may
seed QNH/wind before the first mast frame arrives, but never displaces live AWOS
values. Text products (D-ATIS body, latest METAR/SPECI) are retained verbatim
for the HUD feed. QNH extraction accepts both `Q1013` (METAR form) and
`QNH 1014` (D-ATIS prose); wind-group scanning is anchor-verified
(`dddffKT` shape) after a sliding-window matcher once produced mid-number false
hits like "181 kt".

Point queries pass straight through to `spatial_interp`; `matrix()` renders the
full Display-2 snapshot including provenance bitmasks (AWOS=1, ACARS=2, BDS=4)
shown in the upper-air table's SRC column.
