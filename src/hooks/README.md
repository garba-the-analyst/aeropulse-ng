# Hooks & State — Telemetry Binding Layer

Path: `src/hooks/`

Four modules connect the React tree to the Rust engine without per-frame
re-renders.

---

## `useFlightTracks.ts` — telemetry store (the core)

A module-level singleton store, not a context:

```text
tauri event "telemetry://snapshot" (60 Hz) ──► publish(snapshot)
                                                ├─ latest ref  ← canvas rAF reads
                                                └─ listeners   ← panel throttles
```

* **`getSnapshot()`** — synchronous latest read; radar canvas calls this inside
  its own requestAnimationFrame loop, so display refresh never touches React.
* **`useTelemetry(hz = 8)`** — React binding that samples the store on an
  interval (default 8 Hz) for human-rate panels. Panels pick their own cadence:
  strips 4 Hz, weather 2 Hz, threat matrix 3 Hz, command bar 4 Hz, metrics 4 Hz.
* **`startTelemetry()`** — idempotent boot: subscribes to the Tauri event when
  running inside the desktop host; otherwise starts the **demo feed** (20 Hz
  synthetic snapshots mirroring the Rust simulator roster — VL604/NAF911 STCA
  geometry, 7700 at T+45 s, dark target, weather matrix) so the UI is fully
  operable in a plain browser and usable for offline screen capture.

## `useTauriEvent.ts` — environment-safe bindings

* `isTauri()` — detects the desktop host via `__TAURI_INTERNALS__`.
* `listenSafe(event, handler)` — dynamic-imports `@tauri-apps/api/event` only
  when present; returns a no-op unlisten in browser mode. Every consumer uses
  this wrapper rather than importing Tauri directly, which is what keeps
  `npm run dev` viable without the Rust runtime.

## `useDualDisplay.ts` — workspace pairing

Polls for the `ops_hud` webview every 5 s and exposes `{ hudAvailable, launch }`;
`launch()` delegates to `lib/ipc.openOpsHud()` which creates or focuses the
second window (`hud.html`, 1280×860). Display 1 auto-launches the HUD on boot
(`App.tsx` effect).

## Related

* `src/lib/ipc.ts` — `invokeSafe<T>(cmd, args)` command accessor with graceful
  null fallback + `openOpsHud()`.
* Event name contract lives in both worlds: `EVT_SNAPSHOT = "telemetry://snapshot"`
  (TS) matches `runtime.rs EVT_SNAPSHOT` (Rust).
