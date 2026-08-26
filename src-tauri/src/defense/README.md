# Defence Layer — Anomaly Detection, Geofencing, Intercept Geometry

Path: `src-tauri/src/defense/`
Tests: 16 (dark_target 5 · geofence 6 · intercept_math 5)

Military/tactical overlays that consume engine tracks and emit alerts without
ever mutating surveillance state themselves — verdicts flow back through the
engine for composition.

---

## `dark_target.rs` — Transponder Anomaly Detection

`AnomalyDetector.scan(observations, now_ms)` evaluates every live track and ages
absentees. Alert priority within a scan:

1. **Emergency squawk family** — 7700 → `Emergency`, 7600 → `RadioFailure`,
   7500 → `Hijack` (ICAO Annex 10 meanings).
2. **Identity-less emitter** — no callsign ever observed AND observation span
   ≥ `min_track_age_s` (default 30 s) → `DarkTarget`, reason includes observed
   span. Squawk churn is *counted* (windowed deque, default 60 s) for diagnostics
   but deliberately does not alert alone — code changes are routine around
   handoffs.
3. **Absentee silence** — a previously cooperative track missing from the current
   scan whose last sighting exceeds `silence_threshold_s` (default 45 s) after an
   arming span → `DarkTarget("TRANSPONDER SILENCE…")`.

Verdicts are **transition-only**: recovery emits a clearing verdict that resets
symbology. Tests pin arming boundaries, grace periods, recovery transitions and
churn non-alerting.

## `geofence.rs` — Restricted-Airspace Monitoring

* `point_in_polygon()` — ray-cast on the local tangent plane (metres-per-degree
  series consistent with the kinematics layer), horizontal-edge guarded.
* Fences carry `[lat,lon]` vertices plus floor/ceiling band; fixes inside the
  polygon **and** vertical band count as occupied.
* `GeofenceMonitor.scan()` emits **entry events only**, tracked per
  `(fence_id, icao)` occupancy sets: continuous presence never re-triggers, exit
  re-arms. The engine additionally holds a 30 s display window per breach so the
  alert survives later snapshots.
* Fences can be toggled active/inactive at runtime via IPC; independent fences
  track independently (tested).

## `intercept_math.rs` — Tactical Intercept Solver

Constant-speed interceptor vs constant-velocity target, solved as a pursuit-curve
fixed point:

```
heading_n = atan2(E_pred(t), N_pred(t))        # bearing FROM NORTH (engine convention)
t_{k+1}   = t_k + 0.65 · (|pred(t_k)|/v_i − t_k)   # damped iteration
```

Convergence tolerance 15 m of miss distance; ≤48 iterations. The solution is then
**forward-simulated** and must reproduce contact within 250 m to be declared
feasible (guards against pathological oscillation). `minimum_feasible_speed_kt`
bisects the feasibility predicate (~22 probes).

⚠ Bearing convention note: this module measures headings from **north** using
`(sin θ, cos θ)` on (east, north) — identical to the EKF/course convention. A
mid-development version mixed `atan2(y,x)` from-east maths with from-north unit
vectors and flew the interceptor north when ordered east; the test suite now pins
cardinal cases explicitly (`head_on_intercept_is_straight_line` asserts 90.0°T ±0.5).

Tests: head-on closure timing, crossing-target lead angle (solution heading must
exceed instantaneous bearing), receding-target infeasibility, moving-interceptor
shortening TTI, bisection envelope sanity, plus an end-to-end dark-intruder
scenario in `mod.rs`.
