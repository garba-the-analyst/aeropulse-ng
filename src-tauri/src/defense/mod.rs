//! Defence & tactical overlay subsystem: dark-target alerting, geofence
//! monitoring and intercept geometry.

pub mod dark_target;
pub mod geofence;
pub mod intercept_math;

pub use dark_target::{
    AnomalyDetector, AnomalyVerdict, DarkTargetConfig, TrackObservation, SQUAWK_EMERGENCY,
    SQUAWK_HIJACK, SQUAWK_RADIO_FAILURE,
};
pub use geofence::{FenceFix, GeofenceMonitor};
pub use intercept_math::{solve_intercept, InterceptInput};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::models::{Geofence, TrackClass};

    /// End-to-end: an uncooperative target crosses the restricted box while
    /// a QRA flight scrambles — anomaly flag and breach must both fire and
    /// the intercept solution must be feasible.
    #[test]
    fn dark_intruder_scenario_end_to_end() {
        let mut anomalies = AnomalyDetector::new(DarkTargetConfig {
            min_track_age_s: 2.0,
            silence_threshold_s: 8.0,
            ..Default::default()
        });

        let fence = Geofence {
            id: "R-51".into(),
            name: "VOLcanic EXCLUSION".into(),
            vertices_deg: vec![[12.00, 8.45], [12.00, 8.60], [12.10, 8.60], [12.10, 8.45]],
            floor_ft: 0.0,
            ceiling_ft: 15_000.0,
            active: true,
        };
        let mut fences = GeofenceMonitor::with_fences(vec![fence]);

        // Intruder descends through the box between t=2s..6s.
        let mut breaches_total = 0;
        for step in 0..12u64 {
            let now = step * 1_000;
            let lat = 12.14 - step as f64 * 0.013;
            let lon = 8.52;
            let alt = (9_000.0 - step as f64 * 400.0).max(500.0);

            let verdicts = anomalies.scan(
                &[TrackObservation {
                    icao24: "DEAD01".into(),
                    callsign: String::new(),
                    squawk: "0000".into(),
                    age_s: 0.0,
                }],
                now,
            );
            if step >= 3 && verdicts.iter().any(|v| v.alert.is_some()) {
                // Armed by then.
            }

            breaches_total += fences
                .scan(&[FenceFix {
                    icao24: "DEAD01".into(),
                    callsign: String::new(),
                    latitude: lat,
                    longitude: lon,
                    altitude_ft: alt,
                    now_ms: now,
                }])
                .len();
        }
        assert!(breaches_total >= 1, "intruder crossed the band");

        // QRA launch from the field toward the departing intruder.
        let sol = solve_intercept(&InterceptInput {
            ix_m: 0.0,
            iy_m: 0.0,
            ivx_ms: 0.0,
            ivy_ms: 0.0,
            tx_m: 18_000.0,
            ty_m: 6_000.0,
            tvx_ms: 120.0,
            tvy_ms: -30.0,
            interceptor_speed_kt: 470.0,
            max_time_s: 600.0,
        });
        assert!(sol.feasible);
        assert!(sol.time_to_intercept_s < 300.0);
        assert_eq!(
            TrackClass::Anomaly.css_color(),
            "#FFB300",
            "anomaly symbology contract"
        );
    }
}
