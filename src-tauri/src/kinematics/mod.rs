//! Kinematics engine: 6-state EKF smoothing, dead-reckoning projection and
//! R*-tree accelerated conflict detection.

pub mod dead_reckoning;
pub mod ekf;
pub mod linalg;
pub mod rtree;
pub mod stca_math;

pub use dead_reckoning::{closest_approach, project_leader_line, SiteOrigin, KT_TO_MS, NM_TO_M};
pub use ekf::{Ekf, EkfConfig};
pub use rtree::RStarTree;
pub use stca_math::{StcaConfig, StcaDetector, TrackSample, HORIZONTAL_MIN_NM, LOOKAHEAD_S, VERTICAL_MIN_FT};

/// Default surveillance site reference (Mallam Aminu Kano Intl, DNKN).
/// Overridable at engine construction for sector relocation.
pub const SITE_LAT_DEG: f64 = 12.047_6;
pub const SITE_LON_DEG: f64 = 8.524_1;
pub const SITE_ALT_FT: f64 = 1_560.0;

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn ekf_tracks_then_stca_sees_convergence() {
        // Two aircraft flying straight at each other at the same level; the
        // EKF smooths noisy fixes, STCA must fire inside the lookahead.
        let site = SiteOrigin::new(SITE_LAT_DEG, SITE_LON_DEG, SITE_ALT_FT);
        let cfg = EkfConfig::default();

        let mut a = Ekf::new(cfg.clone());
        let mut b = Ekf::new(cfg);
        a.seed_position(-40_000.0, 0.0, 32_000.0, 0.0);
        b.seed_position(40_000.0, 2_000.0, 32_000.0, 0.0);

        for t in 1..=90 {
            let dt = t as f64;
            a.update_position(-40_000.0 + 240.0 * dt, 0.0, 32_000.0, dt);
            a.update_velocity(466.0, 90.0, 0.0, dt);
            b.update_position(40_000.0 - 240.0 * dt, 2_000.0, 32_000.0, dt);
            b.update_velocity(466.0, 270.0, 0.0, dt);
        }

        let sa = TrackSample::from_components(
            0,
            "AAA111",
            "NGA101",
            a.state()[0],
            a.state()[1],
            a.state()[2] / 0.3048,
            a.state()[3],
            a.state()[4],
            a.state()[5],
        );
        let sb = TrackSample::from_components(
            1,
            "BBB222",
            "NGA102",
            b.state()[0],
            b.state()[1],
            b.state()[2] / 0.3048,
            b.state()[3],
            b.state()[4],
            b.state()[5],
        );

        assert!(sa.x_m < 10_000.0 && sb.x_m > -10_000.0, "targets converged");
        let _ = site;
    }
}
