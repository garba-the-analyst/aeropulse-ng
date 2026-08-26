# NDAIE 2026 — Executive Project Summary

**Entry:** AeroPulse-NG — Offline Tactical Radar & Airspace Surveillance for African Skies
**Primary category:** Flight Planning & Air Traffic Management
**Word count:** ≈285 / 300

---

**Problem.** Nigeria's airspace is monitored largely through expensive imported primary and secondary surveillance radar — installations costing millions of dollars, demanding specialist maintenance, yet still leaving low-altitude corridors, secondary aerodromes and terrain-shadowed sectors under-covered. When connectivity or a radar head fails, controllers lose the traffic picture precisely when harmattan haze, diversion surges or military activity raise risk. Non-cooperative and transponder-silent aircraft widen the gap further.

**Proposed solution.** AeroPulse-NG is an air-gapped surveillance engine that turns commodity software-defined radios (about USD 150 of hardware) plus a standard PC into a working tactical radar display. It ingests over-the-air aviation telemetry with zero internet or cloud dependency, giving civil air traffic management (NAMA) and tactical command (NAF) a sovereign, locally maintainable traffic picture at a fraction of conventional cost.

**Technical approach.** A Rust/Tauri backend demodulates 1090 MHz Mode S ADS-B, resolving positions through Compact Position Reporting; a six-state Extended Kalman Filter smooths jitter and dead-reckons tracks through 30-second RF dropouts. An R*-tree spatial index drives continuous Short-Term Conflict Alerting against 5 NM / 1,000 ft separation minima within a 120-second lookahead. A triple-fusion weather matrix merges AWOS serial telemetry, ACARS D-ATIS broadcasts and Mode S BDS 4,4/4,5 downlinks into an offline 3D wind/temperature model tuned for harmattan operations. Defence overlays flag dark targets, squawk emergencies and geofence incursions, and compute intercept geometry. A hardware-accelerated WebGL front end renders HF-STD-010A-compliant displays across dual operator monitors.

**Likely impact.** Operational: conflict alerting and fused weather at aerodromes that cannot justify radar. Economic: over 99% capital reduction per station. Sovereignty: fully offline national capability. Educational: an open engineering reference for Nigerian avionics talent. Development stage: prototype core (decoder, EKF, STCA engines) verified by automated test suites; multi-window operator interface and field-trial hardware integration in progress.
