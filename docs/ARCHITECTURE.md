# AeroPulse-NG — Architecture

Design rationale for the surveillance pipeline, the trade-offs behind it, and the
roadmap. Module-level detail lives in each directory's README; this document is
the connective tissue.

---

## 1. Guiding Constraints

1. **Air-gapped by contract.** No feature may require internet at build *or*
   runtime. Consequence: no map tiles (vector-drawn PPI instead), no NTP
   (operator clock is authoritative), no package downloads post-clone.
2. **One decode path.** Synthetic traffic is encoded into genuine DF17 frames —
   CRC parity included — and pushed through the production decoder. Bench demos,
   unit tests and (future) real RF all execute identical code below the antenna.
3. **Deterministic core, async shell.** `SurveillanceEngine` is a pure state
   machine driven by explicit `now_ms` stamps: replays are bit-reproducible and
   every subsystem is unit-testable without tokio. The Tauri runtime owns clocks
   and I/O.
4. **Presentation converts once.** Engine internals stay native (EKF metres/m·s⁻¹;
   DO-260B wire values in ft/kt). SI-metric formatting happens exclusively in
   `src/lib/units.ts` + the headless formatter, per the NCAA/Annex-5 policy.

## 2. Data Flow

```text
IngestEvent{Raw1090Frame|AcarsBytes|SquawkReport|SurfaceObservation|UpperAirSample}
        │  (queued; drained AFTER the prediction pass)
        ▼
┌─ tick(now_ms) ────────────────────────────────────────────────────────┐
│ 1 predict(dt) for every TrackState          ← dead-reckoning happens │
│ 2 drain queue → decode_frame → CPR pair → EKF fuse_*             here │
│ 3 periodic scans (sub-sampled from 60 Hz):                           │
│     STCA @15 Hz · anomaly @1 Hz · geofence @2 Hz                     │
│ 4 materialise Track[] (+FDB text, leader lines, alert composition)    │
│ 5 publish Arc<EngineSnapshot> on broadcast bus                       │
└───────────────────────────────────────────────────────────────────────┘
```

**Why queue-then-drain?** An earlier revision fused directly inside `ingest()`
while `tick()` also predicted — double propagation ran filters at 2× truth speed
and the acquisition gate then locked out every correction (tracks froze mid-air).
The fix is structural: measurements must fuse against a state propagated exactly
once to their epoch.

## 3. Key Decisions

| Decision | Rationale | Alternative rejected |
|---|---|---|
| Hand-rolled 6×6 linalg (`kinematics/linalg.rs`) | zero transitive native deps on air-gapped hosts; bit-exact reproducibility | nalgebra (excellent, but adds ~200 crates' worth of surface to audit) |
| Bulk-rebuild R\*-tree per STCA pass | ≤2 000 tracks rebuild in tens of µs — cheaper than incremental rebalancing bookkeeping; queries stay log-n | k-d tree / uniform grid (worse with corridor AABBs) |
| Joseph-form covariance update | numerical stability under the wide dynamic range of σ² during acquisition | simple `(I−KH)P` form (drifted negative in long sims) |
| Acquisition gate warm-up (12 fixes) | constant-velocity assumption is wrong before velocity converges; without warm-up the gate rejected reality and adopted it never | sequential initialisation via two-fix differencing (more state, marginal gain) |
| Squawk via side-channel event | squawk is not carried in DF17 ES; modelling DF4/5 interrogation replies was out of scope | synthesising full surveillance-reply frames |
| JSON IPC at 60 Hz rather than shared memory | webview boundary simplicity; payload ≤ ~40 KB/frame measured with demo fleet; zero-copy migration path documented below | SharedArrayBuffer ring (premature) |
| Canvas2D renderer | 60 fps sustained with headroom at demo scale; WebGL/Pixi adds dependency weight for no present benefit | Pixi/WebGL now (revisit >2 000 tracks or GPU particle effects) |

### Zero-copy IPC migration path

The snapshot already separates flat numeric arrays (leader lines, positions)
from strings. Moving to true zero-copy = serialise numeric planes into an
`ArrayBuffer` ring exposed via Tauri's `tauri::ipc::InvokeResponseBody::Raw`,
with the metadata frame travelling as today's JSON. Frontend consumes through
`Float32Array` views. Estimated effort: 2–3 days; trigger when track counts
regularly exceed ~1 500.

## 4. Threat / Alert Composition

Final displayed alert is a priority fold:

```
squawk emergency family  >  dark/silence verdicts  >  geofence hold-down  >  manual override  >  none
```

Geofence entries are one-shot events with a 30 s hold-down so a single crossing
is visible on subsequent snapshots without re-trigger spam. Anomaly verdicts are
transition-only; recovery emits a clearing verdict that resets symbology.

## 5. Weather Fusion Chain

Priority for surface fields: **AWOS (sidecar) > D-ATIS extraction > METAR
extraction** — first writer wins a `None`, AWOS always overwrites. Upper-air
nodes accumulate in a 512-slot deque (15 min TTL) feeding IDW sampling:
weight `1/((d/1000+0.25)² · (1+alt_penalty))`, wind blended as vectors (never
averaging degrees), pressure blended toward ISA by altitude mismatch, confidence
from source diversity × inverse range. Dust-layer estimate: visibility-driven
base + convective thermal bump above 28 °C surface temperature.

## 6. Testing Philosophy

Every mathematical claim in this repository is executable:

* CRC implementations validated against externally published frames.
* CPR solver proven by synthesis round-trips across zone boundaries **and**
  against moving-target pairs (the case static fixtures miss).
* EKF convergence asserted under seeded noise; gating behaviour asserted both
  directions (accepts truth, rejects gross outliers).
* Engine integration runs 110 simulated seconds end-to-end asserting identity
  resolution, STCA engagement timing, weather arrival and coasting semantics.

Run everything: `./scripts/verify.sh`.

## 7. Roadmap

| # | Item | Notes |
|---|---|---|
| 1 | RTL-SDR I/Q front-end | libusb transfers → DC/PPM correction → Manchester sync → `decode_frame()`; registry & decoder interfaces already shaped for it |
| 2 | Live FFT tap | replace synthesised spectrum with magnitudes from (1); plot component accepts any `[f32]` bins |
| 3 | Physical AWOS bring-up | sentence parser + checksum ready; needs field characterisation |
| 4 | DF20/21 Comm-B uplink scheduling | BDS codecs complete; interrogator cadence TBD |
| 5 | Alert/weather persistence | tables exist; writers pending |
| 6 | Zero-copy IPC | §3 migration path |
| 7 | Multi-site handover, RBAC views, offline vector tiles | operational-scale features |
