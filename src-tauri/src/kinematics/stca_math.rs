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
        let r = self.cfg.horizontal_nm * NM_TO_M;
        let v = self.cfg.vertical_ft;
        let l = self.cfg.lookahead_s;

        // Relative motion at t=0
        let dx0 = b.x_m - a.x_m;
        let dy0 = b.y_m - a.y_m;
        let dz0 = b.z_ft - a.z_ft;
        let dvx = b.vx_ms - a.vx_ms;
        let dvy = b.vy_ms - a.vy_ms;
        let dvz = b.vz_fps - a.vz_fps;

        // Horizontal interval: |dp + dv*t|^2 < R^2  => a t^2 + b t + c < 0
        let a_h = dvx * dvx + dvy * dvy;
        let b_h = 2.0 * (dx0 * dvx + dy0 * dvy);
        let c_h = dx0 * dx0 + dy0 * dy0 - r * r;

        let h_interval = if a_h.abs() < 1e-9 {
            // dv ~ 0
            if c_h < 0.0 {
                Some((0.0, l))
            } else {
                None
            }
        } else {
            let disc = b_h * b_h - 4.0 * a_h * c_h;
            if disc < 0.0 {
                None
            } else {
                let sq = disc.sqrt();
                let t1 = (-b_h - sq) / (2.0 * a_h);
                let t2 = (-b_h + sq) / (2.0 * a_h);
                let (lo, hi) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
                let clo = lo.max(0.0).min(l);
                let chi = hi.max(0.0).min(l);
                if chi > clo && hi >= 0.0 && lo <= l {
                    Some((clo, chi))
                } else {
                    None
                }
            }
        };

        // Vertical interval: |dz0 + dvz*t| < V
        let v_interval = if dvz.abs() < 1e-9 {
            if dz0.abs() < v {
                Some((0.0, l))
            } else {
                None
            }
        } else {
            // -V < dz0 + dvz*t < V  => (-V - dz0)/dvz < t < (V - dz0)/dvz
            let t1 = (-v - dz0) / dvz;
            let t2 = (v - dz0) / dvz;
            let (lo, hi) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
            let clo = lo.max(0.0).min(l);
            let chi = hi.max(0.0).min(l);
            if chi > clo && hi >= 0.0 && lo <= l {
                Some((clo, chi))
            } else {
                None
            }
        };

        let (hl, hh) = h_interval?;
        let (vl, vh) = v_interval?;
        let ol = hl.max(vl);
        let oh = hh.min(vh);
        if oh <= ol {
            return None;
        }

        // Overlap exists → conflict
        // Compute true minima within overlap
        // Horizontal minimum: closest approach of relative motion
        let t_hmin = if a_h.abs() < 1e-9 {
            0.0
        } else {
            // t at min horizontal distance: -b/(2a)
            let t0 = -b_h / (2.0 * a_h);
            t0.clamp(ol, oh)
        };
        let (ax, ay) = a.pos(t_hmin);
        let (bx, by) = b.pos(t_hmin);
        let min_hz = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();

        // Vertical minimum within overlap: closest to dz=0 if 0 is in interval, else at endpoint nearer to 0
        let mut min_vt = f64::INFINITY;
        for &t in &[ol, oh, t_hmin] {
            let dv = (b.alt_ft(t) - a.alt_ft(t)).abs();
            if dv < min_vt {
                min_vt = dv;
            }
        }
        // Also check if dz crosses 0 within overlap
        if dz0.signum() != (dz0 + dvz * oh).signum() {
            min_vt = min_vt.min(0.0);
        }

        // Time to closest is time of minimum horizontal separation within overlap
        // For reporting, use t_hmin clamped to overlap (already)
        let t_closest = t_hmin;

        Some(STCAAlert {
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

    #[test]
    fn stca_matches_bruteforce_ground_truth() {
        struct Rng(u64);
        impl Rng {
            fn next_u64(&mut self) -> u64 {
                let mut x = self.0;
                x ^= x >> 12;
                x ^= x << 25;
                x ^= x >> 27;
                self.0 = x;
                x.wrapping_mul(0x2545_F491_4F6C_DD1D)
            }
            fn unit_pos(&mut self) -> f64 {
                // uniform [-1, 1]
                ((self.next_u64() >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
            }
        }
        let mut rng = Rng(0x9E3779B97F4A7C15);
        let mut det = StcaDetector::default();
        let r_m = HORIZONTAL_MIN_NM * NM_TO_M;
        let v_ft = VERTICAL_MIN_FT;
        let look = LOOKAHEAD_S;
        let dt_brute = 0.05;
        let steps = (look / dt_brute).ceil() as usize;
        let mut missed = 0usize;
        let mut false_alarms = 0usize;
        // boundary exclusion: ignore cases where minima within 1m/1ft of threshold
        let hz_eps_m = 1.0;
        let vt_eps_ft = 1.0;
        for _ in 0..20000 {
            let a = TrackSample::from_components(0, "AAA", "AAA", 0.0, 0.0, 30_000.0, 0.0, 0.0, 0.0);
            let bx = rng.unit_pos() * 60_000.0;
            let by = rng.unit_pos() * 60_000.0;
            let bvx = rng.unit_pos() * 250.0;
            let bvy = rng.unit_pos() * 250.0;
            let alt_diff = rng.unit_pos() * 2000.0;
            let vz = rng.unit_pos() * 40.0;
            let b = TrackSample::from_components(1, "BBB", "BBB", bx, by, 30_000.0 + alt_diff, bvx, bvy, vz);
            // analytic
            let analytic_alert = det.detect(&[a.clone(), b.clone()], 0).len() > 0;
            // brute force sampling at 0.05s
            let mut brute = false;
            let mut min_hz = f64::INFINITY;
            let mut min_vt = f64::INFINITY;
            for i in 0..=steps {
                let t = i as f64 * dt_brute;
                let (ax, ay) = (a.x_m + a.vx_ms * t, a.y_m + a.vy_ms * t);
                let (bxp, byp) = (b.x_m + b.vx_ms * t, b.y_m + b.vy_ms * t);
                let hz = ((bxp - ax).powi(2) + (byp - ay).powi(2)).sqrt();
                let vt = (b.z_ft + b.vz_fps * t - (a.z_ft + a.vz_fps * t)).abs();
                if hz < min_hz {
                    min_hz = hz;
                }
                if vt < min_vt {
                    min_vt = vt;
                }
                if hz < r_m && vt < v_ft {
                    brute = true;
                    break;
                }
            }
            // boundary exclusion
            let near_boundary = (min_hz - r_m).abs() < hz_eps_m || (min_vt - v_ft).abs() < vt_eps_ft;
            if near_boundary {
                continue;
            }
            if brute && !analytic_alert {
                missed += 1;
            }
            if !brute && analytic_alert {
                false_alarms += 1;
            }
        }
        assert_eq!(missed, 0, "analytic missed {} brute true cases", missed);
        assert_eq!(false_alarms, 0, "analytic false alarms {} where brute false", false_alarms);
    }

    #[test]
    fn fast_crossing_pair_that_5s_grid_would_miss() {
        // Analytic must detect a pair that is only inside cylinder between 5s samples.
        // We search deterministically for a configuration where 5s-sampled brute misses but analytic hits.
        let r_m = HORIZONTAL_MIN_NM * NM_TO_M;
        let v_ft = VERTICAL_MIN_FT;
        let mut det_analytic = StcaDetector::default();
        // Try to find a fast perpendicular grazing pass with ~3.5s inside window
        // Use brute search over offset and timing
        let mut found_pair: Option<(TrackSample, TrackSample)> = None;
        // deterministic search using small rng
        struct Rng(u64);
        impl Rng {
            fn next_u64(&mut self) -> u64 {
                let mut x = self.0;
                x ^= x >> 12;
                x ^= x << 25;
                x ^= x >> 27;
                self.0 = x;
                x.wrapping_mul(0x2545_F491_4F6C_DD1D)
            }
            fn unit(&mut self) -> f64 {
                ((self.next_u64() >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
            }
        }
        let mut rng = Rng(0xC0FFEE12345678);
        for _ in 0..5000 {
            // A stationary at origin level 30000
            let a = TrackSample::from_components(0, "AAA", "AAA", 0.0, 0.0, 30_000.0, 0.0, 0.0, 0.0);
            // B high speed perpendicular: start at (-D, b_offset) moving east
            // D large enough to start outside at t=0
            let speed = 250.0 + rng.unit().abs() * 100.0; // 250..350
            let b_offset = r_m - 5.0 - rng.unit().abs() * 15.0; //  R-20 .. R-5 => short chord 2-4s
            let start_x = -1500.0 - rng.unit().abs() * 1000.0; // -1500..-2500
            let b = TrackSample::from_components(
                1,
                "BBB",
                "BBB",
                start_x,
                b_offset,
                30_000.0,
                speed,
                0.0,
                0.0,
            );
            // analytic detect
            let analytic = !det_analytic.detect(&[a.clone(), b.clone()], 0).is_empty();
            // naive 5s grid: sample at 0,5,10,... up to 120
            let mut naive = false;
            let mut t = 0.0;
            while t <= LOOKAHEAD_S + 1e-9 {
                let (ax, ay) = (a.x_m + a.vx_ms * t, a.y_m + a.vy_ms * t);
                let (bx, by) = (b.x_m + b.vx_ms * t, b.y_m + b.vy_ms * t);
                let hz = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
                let vt = (b.z_ft - a.z_ft).abs();
                if hz < r_m && vt < v_ft {
                    naive = true;
                    break;
                }
                t += 5.0;
            }
            if analytic && !naive {
                found_pair = Some((a, b));
                break;
            }
        }
        // Fallback hardcoded pair if search fails (should not)
        let (a, b) = found_pair.unwrap_or_else(|| {
            // Hardcoded grazing pass: chord ~3.4s between samples 0 and 5
            // b passes tangentially at y = r -10
            let a = TrackSample::from_components(0, "AAA", "AAA", 0.0, 0.0, 30_000.0, 0.0, 0.0, 0.0);
            let b = TrackSample::from_components(1, "BBB", "BBB", -1200.0, r_m - 10.0, 30_000.0, 250.0, 0.0, 0.0);
            (a, b)
        });
        // Verify analytic detects
        let mut det = StcaDetector::default();
        let alerts = det.detect(&[a.clone(), b.clone()], 0);
        assert_eq!(alerts.len(), 1, "fast crossing between 5s samples must be detected by analytic");
        // Verify naive 5s grid indeed misses this pair
        let mut naive = false;
        let mut t = 0.0;
        while t <= LOOKAHEAD_S + 1e-9 {
            let (ax, ay) = (a.x_m + a.vx_ms * t, a.y_m + a.vy_ms * t);
            let (bx, by) = (b.x_m + b.vx_ms * t, b.y_m + b.vy_ms * t);
            let hz = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
            let vt = (b.z_ft - a.z_ft).abs();
            if hz < r_m && vt < v_ft {
                naive = true;
                break;
            }
            t += 5.0;
        }
        assert!(!naive, "naive 5s grid must miss this fast crossing");
    }
}
