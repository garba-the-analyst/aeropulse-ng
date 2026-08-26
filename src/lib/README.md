# lib/ — Shared Frontend Utilities

| Module | Purpose |
|---|---|
| `units.ts` | **Single source of truth** for the NCAA / ICAO Annex-5 SI presentation policy: conversion factors, display formatters, metric range-ring plan, Doc-4444 separation minima in SI. Full policy table documented in its header and in [`../types/README.md`](../types/README.md). |
| `ipc.ts` | `invokeSafe<T>(cmd, args)` Tauri command accessor (dynamic import, graceful null outside the desktop host) and `openOpsHud()` second-window management. |

Rule of the house: operator-facing values format **only** through
`units.ts`; wire contracts stay native (ft/kt per DO-260B) until the
presentation boundary.
