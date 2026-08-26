# Commands Layer — Tauri IPC Surface

Path: `src-tauri/src/commands/` (compiled under the `tauri-host` feature only)
Sibling: `src-tauri/src/runtime.rs` — the 60 Hz host loop this layer serves.

---

## State Model

```rust
AppShared { engine: Arc<Mutex<SurveillanceEngine>> }
```

One shared handle between the runtime task and every command. Lock discipline:
**sub-millisecond critical sections only** — commands either read the latest
`Arc<EngineSnapshot>` (lock-free after the pointer copy) or perform bounded
mutations (fence add/toggle, override arming). No lock is ever held across an
await; all commands are synchronous fns returning `Result<T, String>`.

## Command Catalogue

| Command | Module | Returns |
|---|---|---|
| `get_latest_snapshot` | telemetry | full `EngineSnapshot` (pull fallback to the event bus) |
| `get_engine_status` | telemetry | `EngineStatus` for Screen 0 chips |
| `get_hardware_status` | telemetry | `Vec<HardwareStatus>` diagnostics rows |
| `trigger_emergency_override { duration_s }` | telemetry | arms fleet-wide emergency hold-down |
| `cancel_emergency_override` | telemetry | clears ahead of schedule |
| `tracks_by_alert { alert }` | telemetry | icao24 list matching a state |
| `get_weather_matrix` | weather | Display-2 matrix |
| `sample_atmosphere { lat, lon, alt_ft }` | weather | interpolated SI sample + confidence |
| `list_geofences` | defense | current fence set |
| `create_geofence { name, vertices_deg, floor_ft, ceiling_ft }` | defense | server-assigned id (`GF-nnnn`) |
| `toggle_geofence { id, active }` | defense | success flag |
| `compute_intercept { target_icao24, interceptor_icao24, interceptor_speed_kt }` | defense | `InterceptSolution` |

`compute_intercept` resolves both tracks from the live snapshot, projects them
into the site ENU frame through the engine's `SiteOrigin`, derives velocity
vectors from filtered ground speed/course, and delegates to
`defense::solve_intercept`. The frontend passes **km/h**, which converts to the
solver's knots at the boundary (SI presentation policy).

## Event Stream

The runtime emits one global event consumed by both windows:

```
telemetry://snapshot  →  EngineSnapshot (60 Hz)
```

Commands are the *pull* path; the event bus is the *push* path. Frontend hooks
subscribe via `useTelemetry()` and never poll IPC in steady state.

## Runtime Loop (`runtime.rs`)

1. Spawns the simulator + sidecar process (graceful `None` on failure).
2. `tokio::select!` over sidecar frames (`Ready→sidecar_online`,
   `Awos→ingest_awos`, `DbAck→rate gauge`) and a 60 Hz tick interval with
   `MissedTickBehavior::Skip`.
3. Each tick: sim events → engine ingest → tick → `app.emit(EVT_SNAPSHOT)`;
   every 2 s pushes `log_tracks` batches to the recorder.
4. Sim clock anchored at boot wall-clock; advances 1/60 s per tick.

## Verification Status

This layer compiles under `tauri-host` (verified: full desktop build links and
runs — windows appear, bus streams). It is feature-gated because the tauri crate
requires webkit system libraries at build time; engine-side logic it delegates to
is fully covered by the 90-test ungated suite. Frontend fallbacks (`invokeSafe`)
keep browser-mode development functional without the host present.
