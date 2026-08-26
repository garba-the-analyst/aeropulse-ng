//! Short-Term Conflict Alert (STCA) detection engine.
//!
//! Projects every active track through the 120-second lookahead window,
//! indexes the predicted corridors inside the R*-tree, and refines candidate
//! pairs with time-stepped separation checks against ICAO Doc 4444 radar
//! separation minima (5.0 NM horizontal / 1 000 ft vertical).

use std::collections::HashSet;

use super::dead_reckoning::NM_TO_M;
use super::rtree::{Rect, RStarTree};
use crate::models::STCAAlert;

pub const HORIZONTAL_MIN_NM: f64 = 5.0;
pub const VERTICAL_MIN_FT: f64 = 1000.0;
pub const LOOKAHEAD_S: f64 = 120.0;
/// Refinement sampling interval across the lookahead window.
pub const REFINE_STEP_S: f64 = 5.0;

#[derive(Debug, Clone, Copy)]
pub struct StcaConfig {
    pub horizontal_nm: f64,
    pub vertical_ft: f64,
    pub lookahead_s: f64,
    pub refine_step_s: f64,
}

impl Default for StcaConfig {
    fn default() -> Self {
        Self {
            horizontal_nm: HORIZONTAL_MIN_NM,
            vertical_ft: VERTICAL_MIN_FT,
            lookahead_s: LOOKAHEAD_S,
            refine_step_s: REFINE_STEP_S,
        }
    }
}

/// Immutable kinematic snapshot consumed by the detector.
#[derive(Debug, Clone)]
pub struct TrackSample {
    pub index: usize,
    pub icao24: String,
    pub callsign: String,
    pub x_m: f64,
    pub y_m: f64,
    pub z_ft: f64,
    pub vx_ms: f64,
    pub vy_ms: f64,
    pub vz_fps: f64,
}

impl TrackSample {
    /// Builds a sample from ENU position/velocity components.
    pub fn from_components(
        index: usize,
        icao24: &str,
        callsign: &str,
        x_m: f64,
        y_m: f64,
        z_ft: f64,
        vx_ms: f64,
        vy_ms: f64,
        vz_fps: f64,
    ) -> Self {
        Self {
            index,
            icao24: icao24.to_string(),
            callsign: callsign.to_string(),
            x_m,
            y_m,
            z_ft,
            vx_ms,
            vy_ms,
            vz_fps,
        }
    }

    #[inline]
    fn pos(&self, dt: f64) -> (f64, f64) {
        (self.x_m + self.vx_ms * dt, self.y_m + self.vy_ms * dt)
    }

    #[inline]
    fn alt_ft(&self, dt: f64) -> f64 {
        self.z_ft + self.vz_fps * dt
    }
}

pub struct StcaDetector {
    cfg: StcaConfig,
    tree: RStarTree,
}

impl Default for StcaDetector {
    fn default() -> Self {
        Self::new(StcaConfig::default())
    }
}

impl StcaDetector {
    pub fn new(cfg: StcaConfig) -> Self {
        Self {
            cfg,
            tree: RStarTree::new(),
        }
    }

    #[inline]
    pub fn config(&self) -> &StcaConfig {
        &self.cfg
    }

    /// Executes one detection pass over all supplied samples.
    ///
    /// The spatial index is bulk-cleared and rebuilt each pass: at the
    /// operational track population (<2 000) a packed rebuild costs tens of
    /// microseconds while keeping memory flat and allocation churn minimal.
    pub fn detect(&mut self, tracks: &[TrackSample], now_ms: u64) -> Vec<STCAAlert> {
        if tracks.len() < 2 {
            return Vec::new();
        }
        let hz_margin = self.cfg.horizontal_nm * NM_TO_M;
        self.tree.clear();

        // Phase 1: bulk-load predicted-corridor bounding boxes.
        for t in tracks {
            let steps = (self.cfg.lookahead_s / self.cfg.refine_step_s).ceil();
            let mut min_x = t.x_m;
            let mut min_y = t.y_m;
            let mut max_x = t.x_m;
            let mut max_y = t.y_m;
            for i in 0..=steps as usize {
                let dt = i as f64 * self.cfg.refine_step_s;
                let (px, py) = t.pos(dt);
                min_x = min_x.min(px);
                max_x = max_x.max(px);
                min_y = min_y.min(py);
                max_y = max_y.max(py);
            }
            self.tree.insert(
                t.index as u32,
                Rect::new(
                    min_x - hz_margin,
                    min_y - hz_margin,
                    max_x + hz_margin,
                    max_y + hz_margin,
                ),
            );
        }

        // Phase 2: candidate retrieval then exact refinement.
        let mut alerts = Vec::new();
        let mut seen: HashSet<(u32, u32)> = HashSet::with_capacity(tracks.len());
        let mut candidates: Vec<u32> = Vec::with_capacity(64);

        for t in tracks {
            candidates.clear();
            self.tree.query(&self.corridor_rect(t), &mut candidates);

            for cand in candidates.iter().copied() {
                let j = cand as usize;
                if j <= t.index || !seen.insert((t.index as u32, cand)) {
                    continue;
                }
                if let Some(alert) = self.refine_pair(t, &tracks[j], now_ms) {
                    alerts.push(alert);
                }
            }
        }
        alerts
    }

    fn corridor_rect(&self, t: &TrackSample) -> Rect {
        let m = self.cfg.horizontal_nm * NM_TO_M;
        let ex = t.x_m + t.vx_ms * self.cfg.lookahead_s;
        let ey = t.y_m + t.vy_ms * self.cfg.lookahead_s;
        Rect::new(
            t.x_m.min(ex) - m,
            t.y_m.min(ey) - m,
            t.x_m.max(ex) + m,
            t.y_m.max(ey) + m,
        )
    }

    fn refine_pair(&self, a: &TrackSample, b: &TrackSample, now_ms: u64) -> Option<STCAAlert> {
        let hz_limit = self.cfg.horizontal_nm * NM_TO_M;
        let vt_limit = self.cfg.vertical_ft;

        let steps = (self.cfg.lookahead_s / self.cfg.refine_step_s).ceil() as usize;
        let mut min_hz = f64::INFINITY;
        let mut min_vt = f64::INFINITY;
        let mut t_closest = 0.0f64;
        let mut conflicted = false;

        for i in 0..=steps {
            let dt = i as f64 * self.cfg.refine_step_s;
            let (ax, ay) = a.pos(dt);
            let (bx, by) = b.pos(dt);
            let dh = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
            let dv = (b.alt_ft(dt) - a.alt_ft(dt)).abs();

            if dh < min_hz {
                min_hz = dh;
                t_closest = dt;
            }
            min_vt = min_vt.min(dv);

            if dh < hz_limit && dv < vt_limit {
                conflicted = true;
            }
        }

        conflicted.then(|| STCAAlert {
            id: format!("{}-{}", a.icao24, b.icao24),
            icao_a: a.icao24.clone(),
            callsign_a: a.callsign.clone(),
            icao_b: b.icao24.clone(),
            callsign_b: b.callsign.clone(),
            min_horizontal_nm: min_hz / NM_TO_M,
            min_vertical_ft: min_vt,
            time_to_closest_s: t_closest,
            triggered_ms: now_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(idx: usize, icao: &str, x: f64, y: f64, vx: f64, vy: f64, alt: f64) -> TrackSample {
        TrackSample::from_components(idx, icao, icao, x, y, alt, vx, vy, 0.0)
    }

    #[test]
    fn detects_head_on_convergence() {
        let mut det = StcaDetector::default();
        let sa = sample(0, "A1", 0.0, 0.0, 0.0, 0.0, 35_000.0);
        let sb = sample(1, "B2", 20_000.0, 0.0, -250.0, 0.0, 35_000.0);
        let alerts = det.detect(&[sa, sb], 0);
        assert_eq!(alerts.len(), 1);
        assert!(alerts[0].min_horizontal_nm < HORIZONTAL_MIN_NM);
    }

    #[test]
    fn vertical_separation_prevents_alert() {
        let mut det = StcaDetector::default();
        let sa = sample(0, "A1", 0.0, 0.0, 0.0, 0.0, 35_000.0);
        let sb = sample(1, "B2", 3_000.0, 0.0, -250.0, 0.0, 36_500.0);
        assert!(det.detect(&[sa, sb], 0).is_empty());
    }

    #[test]
    fn diverging_traffic_is_quiet() {
        let mut det = StcaDetector::default();
        let sa = sample(0, "A1", 0.0, 0.0, 250.0, 0.0, 35_000.0);
        let sb = sample(1, "B2", 10_000.0, 0.0, 250.0, 0.0, 35_000.0);
        assert!(det.detect(&[sa, sb], 0).is_empty());
    }

    #[test]
    fn climbing_through_level_reports_conflict() {
        // BBB crosses head-on from the east while climbing through AAA's
        // level: horizontal closure ~430 m/s from 12 km, vertical gap
        // 1400 ft closing at 40 ft/s -> infringement around t ~= 11 s.
        let mut det = StcaDetector::default();
        let level = sample(0, "AAA", 0.0, 0.0, 200.0, 0.0, 30_000.0);
        let climber = TrackSample {
            vz_fps: 40.0,
            ..sample(1, "BBB", 12_000.0, 300.0, -230.0, 0.0, 29_000.0)
        };
        let alerts = det.detect(&[level, climber], 0);
        assert_eq!(alerts.len(), 1, "climb-through must alert");
        assert!(alerts[0].min_vertical_ft < VERTICAL_MIN_FT);
        assert!(alerts[0].time_to_closest_s > 0.0);
    }

    #[test]
    fn single_track_short_circuit() {
        let mut det = StcaDetector::default();
        let sa = sample(0, "SOLO", 0.0, 0.0, 250.0, 250.0, 20_000.0);
        assert!(det.detect(&[sa], 0).is_empty());
    }
}
