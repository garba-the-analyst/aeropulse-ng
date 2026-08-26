# Kinematics Layer — Filtering, Projection, Conflict Geometry

Path: `src-tauri/src/kinematics/`
Tests: 27 across five modules (linalg 3 · ekf 4 · dead_reckoning 4 · rtree 4 ·
stca_math 5 · integration 1 · shared with engine suite)

All surveillance math lives here in one dependency-light package: fixed-size
arrays, no external matrix crate, bit-exact reproducibility on air-gapped hosts.

---

## `linalg.rs` — Fixed-Size Linear Algebra

Row-major `[f64;36]` kernels for the 6-state filter: multiply, multiply-by-
transpose, block extraction, **Cholesky factorisation** (`A = LLᵀ`, SPD only —
returns `None` on non-positive pivot) and Cholesky-based solving with multiple
right-hand-side columns. Covariance symmetrisation enforces symmetry after every
update. The solver is the workhorse behind Kalman gains *and* Mahalanobis
distances; its correctness is pinned by a Gaussian-elimination cross-check.

## `ekf.rs` — 6-State Extended Kalman Filter

State `X = [x, y, z, vx, vy, vz]ᵀ` on the local ENU tangent plane (metres,
m·s⁻¹). Process model is piecewise-constant white-noise acceleration with
σ_a = 3.5 m/s² default (tunable per config; transport tests use 1.5).

* **Predict** — standard `x=Fx`, `P=FPFᵀ+Q` with Q built from σ_a² blocks
  (`dt⁴/4`, `dt³/2`, `dt²`).
* **Fuse** (`fuse_position` / `fuse_velocity`) — caller-predicts contract for the
  engine loop; innovation covariance `S = P_sel + R`, gain via Cholesky solve,
  **Joseph-form** update `(I−KH)P(I−KH)ᵀ + KRKᵀ` for numerical stability.
* **Gate** — Mahalanobis distance vs χ²(3) at 16.27, engaged only after 12
  accepted fixes (acquisition warm-up). Rationale: before velocity converges the
  CV model's prediction error legitimately exceeds steady-state bounds; without
  warm-up the gate rejected reality forever.
* Legacy `update_position/update_velocity` wrap fuse with internal prediction for
  standalone users.

Convergence test: 200 noisy fixes on a 220 m/s target → velocity within 15 m/s,
position within 250 m; gross-outlier rejection asserted after warm-up.

## `dead_reckoning.rs` — Geodesy & Projection

`SiteOrigin` anchors an ENU frame with series-expanded metres-per-degree
(centimetre-grade across a 250 NM volume). Provides `to_local/from_local`,
bearing and range helpers. `project_leader_line()` samples constant-velocity
motion into geodetic vertices (display vectors + STCA corridors);
`closest_approach()` solves the analytic CPA between two CV tracks.

## `rtree.rs` — R*-Tree Spatial Index

Full Beckmann et al. implementation over AABBs with `u32` payloads:

* **ChooseSubtree** — overlap-enlargement minimisation at the leaf-parent band,
  area-enlargement above it; ties by area.
* **Split** — minimum-margin axis selection, then overlap-then-area optimal
  distribution along that axis.
* **ForcedReinsert** — 30% farthest-centre entries reinserted once per level.

Arena-backed (`Vec<Node>`), no `unsafe`. Integrity tests: 500-point exact window
queries, 2 000 randomised inserts remain query-consistent, clear/reset semantics.

## `stca_math.rs` — Conflict Detection

Per pass: every track's predicted corridor (constant-velocity polyline over the
lookahead, padded by the horizontal threshold) is bulk-loaded into the tree;
candidate pairs from range queries are refined by 5 s time-stepping checking
**simultaneous** infringement of:

```
horizontal < 5 NM (9.26 km)   AND   vertical < 1 000 ft (305 m)
```

within `LOOKAHEAD_S = 120 s`. Reports minima and time-to-closest approach.
Dedup via ordered index pairs; single-track input short-circuits. Tests cover
head-on convergence, vertical-separation suppression, diverging quietness and a
climb-through-level geometry.

Thresholds live in `StcaConfig` — regulatory constants by default, tunable for
trial environments.

## Integration Contract

`engine.rs` builds `TrackSample`s from filter states each pass; the detector owns
no mutable world state beyond its tree, so passes are trivially parallelisable
later if track counts grow past single-thread headroom.
