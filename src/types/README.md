# Types & Units — Wire Contracts and SI Policy

Path: `src/types/` · `src/lib/`

---

## `types/telemetry.ts`

Mirror of `src-tauri/src/models.rs`. Field names are the serde wire format
exactly (**snake_case**) — do not "prettify" them; the event payload deserialises
straight into these interfaces.

Key exports: `Track`, `TrackClass`, `AlertState`, `VerticalTrend`,
`FlightDataBlock`, `STCAAlert`, `HardwareStatus`, `EngineStatus`, `AnomalyNote`,
`EngineSnapshot`, plus the `SYMBOLOGY` palette constants and `trackColor()`
helper encoding the alert-over-class colour rule shared by canvas and DOM.

Wire-unit note: DO-260B values arrive in feet/knots by standard. They stay that
way on the wire; conversion to SI happens **only** in presentation code.

## `types/weather.ts`

`WeatherNode`, `WeatherMatrix`, `AtmosphericSample` + `fusionSources(mask)`
decoding the provenance bitmask (bit0 AWOS · bit1 ACARS · bit2 Mode-S BDS).

## `types/defense.ts`

`Geofence`, `GeofenceBreach`, `InterceptSolution`.

## `lib/units.ts` — NCAA / ICAO Annex 5 SI policy (single source of truth)

| Export | Value/purpose |
|---|---|
| `NM_TO_KM / FT_TO_M / KT_TO_KMH / FPM_TO_MS` | exact conversion factors |
| `SEPARATION_MIN` | Doc 4444 minima rendered in SI (9.26 km / 305 m) |
| `nmToKm · ftToM · ktToKmh · fpmToMs` | converters |
| `fmtAltM · fmtSpdKmh · fmtKm · fmtVertM` | display formatters |
| `RANGE_RINGS_KM = [100,200,300]` | metric ring plan |
| `RANGE_OPTIONS_KM = [25…450]` | operator scale selector |

The header table documents every quantity's display unit and regulatory
invariant (squawk octal, degrees true, QNH hPa unchanged). **Rule:** any new
operator-facing value must format through this module — raw ft/kt/km/h literals
in components are a review rejection.

## `lib/ipc.ts`

`invokeSafe<T>()` (null-fallback outside the desktop host) and `openOpsHud()`
window management. Dynamic imports keep the browser dev bundle free of Tauri
internals until runtime.
