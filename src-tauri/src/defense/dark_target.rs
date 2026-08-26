//! Transponder anomaly & dark-target alerting heuristics.
//!
//! Flags non-cooperative flight profiles for Display-1 symbology:
//! * emergency / radio-failure / unlawful-interference squawk codes,
//! * abrupt transponder silence on previously active tracks,
//! * identity-less emitters (no TC 1-4 broadcast observed).

use std::collections::{HashMap, VecDeque};

use crate::models::AlertState;

pub const SQUAWK_HIJACK: &str = "7500";
pub const SQUAWK_RADIO_FAILURE: &str = "7600";
pub const SQUAWK_EMERGENCY: &str = "7700";

#[derive(Debug, Clone)]
pub struct DarkTargetConfig {
    /// Seconds of silence before a previously-active track is flagged.
    pub silence_threshold_s: f64,
    /// Minimum observation span before silence/identity alerting arms.
    pub min_track_age_s: f64,
    /// Window for counting suspicious squawk-code churn.
    pub churn_window_s: f64,
}

impl Default for DarkTargetConfig {
    fn default() -> Self {
        Self {
            silence_threshold_s: 45.0,
            min_track_age_s: 30.0,
            churn_window_s: 60.0,
        }
    }
}

#[derive(Debug, Clone)]
struct Record {
    first_seen_ms: u64,
    last_seen_ms: u64,
    ever_had_callsign: bool,
    last_squawk: Option<String>,
    squawk_change_times_ms: VecDeque<u64>,
    churn_count: usize,
    current_alert: Option<AlertState>,
}

impl Record {
    fn new(now_ms: u64) -> Self {
        Self {
            first_seen_ms: now_ms,
            last_seen_ms: now_ms,
            ever_had_callsign: false,
            last_squawk: None,
            squawk_change_times_ms: VecDeque::with_capacity(8),
            churn_count: 0,
            current_alert: None,
        }
    }

    #[inline]
    fn span_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.first_seen_ms)
    }
}

/// Snapshot input consumed each scan.
#[derive(Debug, Clone)]
pub struct TrackObservation {
    pub icao24: String,
    pub callsign: String,
    pub squawk: String,
    pub age_s: f64,
}

/// One anomaly outcome; emitted only on alert-state transitions.
#[derive(Debug, Clone, PartialEq)]
pub struct AnomalyVerdict {
    pub icao24: String,
    pub alert: Option<AlertState>,
    pub reason: String,
}

pub struct AnomalyDetector {
    cfg: DarkTargetConfig,
    records: HashMap<String, Record>,
}

impl Default for AnomalyDetector {
    fn default() -> Self {
        Self::new(DarkTargetConfig::default())
    }
}

impl AnomalyDetector {
    pub fn new(cfg: DarkTargetConfig) -> Self {
        Self {
            cfg,
            records: HashMap::new(),
        }
    }

    #[inline]
    pub fn tracked_count(&self) -> usize {
        self.records.len()
    }

    /// Processes one scan at engine time `now_ms`.
    ///
    /// `observations` covers every track currently alive in the fusion
    /// pipeline; anything known-but-absent ages toward the silence verdict.
    pub fn scan(&mut self, observations: &[TrackObservation], now_ms: u64) -> Vec<AnomalyVerdict> {
        let mut changed = Vec::new();
        let seen: std::collections::HashSet<&str> =
            observations.iter().map(|o| o.icao24.as_str()).collect();
        let min_age_ms = (self.cfg.min_track_age_s * 1000.0) as u64;

        for obs in observations {
            let rec = self
                .records
                .entry(obs.icao24.clone())
                .or_insert_with(|| Record::new(now_ms));
            rec.last_seen_ms = now_ms;
            if !obs.callsign.is_empty() {
                rec.ever_had_callsign = true;
            }

            let mut next: Option<AlertState> = None;
            let mut reason = String::new();

            match obs.squawk.as_str() {
                SQUAWK_EMERGENCY => {
                    next = Some(AlertState::Emergency);
                    reason = "SQUAWK 7700 GENERAL EMERGENCY".into();
                }
                SQUAWK_RADIO_FAILURE => {
                    next = Some(AlertState::RadioFailure);
                    reason = "SQUAWK 7600 RADIO FAILURE".into();
                }
                SQUAWK_HIJACK => {
                    next = Some(AlertState::Hijack);
                    reason = "SQUAWK 7500 UNLAWFUL INTERFERENCE".into();
                }
                _ => {}
            }

            if next.is_none() && !obs.squawk.is_empty() {
                match &rec.last_squawk {
                    None => rec.last_squawk = Some(obs.squawk.clone()),
                    Some(prev) if prev != &obs.squawk => {
                        rec.last_squawk = Some(obs.squawk.clone());
                        rec.squawk_change_times_ms.push_back(now_ms);
                        rec.churn_count += 1;
                        let window_ms = (self.cfg.churn_window_s * 1000.0) as u64;
                        while let Some(front) = rec.squawk_change_times_ms.front() {
                            if now_ms.saturating_sub(*front) > window_ms {
                                rec.squawk_change_times_ms.pop_front();
                            } else {
                                break;
                            }
                        }
                    }
                    Some(_) => {}
                }
            }

            // Identity-less emitter past the arming age.
            if next.is_none() && !rec.ever_had_callsign && rec.span_ms(now_ms) >= min_age_ms {
                next = Some(AlertState::DarkTarget);
                reason = format!(
                    "NO IDENTITY BROADCAST ({:.0}s OBSERVED)",
                    rec.span_ms(now_ms) as f64 / 1000.0
                );
            }

            if next != rec.current_alert {
                rec.current_alert = next;
                changed.push(AnomalyVerdict {
                    icao24: obs.icao24.clone(),
                    alert: next,
                    reason: if next.is_some() { reason } else { String::new() },
                });
            }
        }

        // Absentee ageing: previously cooperative tracks gone quiet.
        let silence_ms = (self.cfg.silence_threshold_s * 1000.0) as u64;
        let silent_records: Vec<(String, bool)> = self
            .records
            .iter()
            .filter(|(icao, rec)| {
                !seen.contains(icao.as_str())
                    && rec.ever_had_callsign
                    && rec.span_ms(rec.last_seen_ms) >= min_age_ms
                    && now_ms.saturating_sub(rec.last_seen_ms) >= silence_ms
            })
            .map(|(k, _)| (k.clone(), true))
            .collect();

        for (icao, _) in silent_records {
            let rec = self.records.get_mut(&icao).expect("just filtered");
            if rec.current_alert != Some(AlertState::DarkTarget) {
                rec.current_alert = Some(AlertState::DarkTarget);
                changed.push(AnomalyVerdict {
                    icao24: icao,
                    alert: Some(AlertState::DarkTarget),
                    reason: "TRANSPONDER SILENCE AFTER ACTIVE TRACK".into(),
                });
            }
        }

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(icao: &str, callsign: &str, squawk: &str) -> TrackObservation {
        TrackObservation {
            icao24: icao.into(),
            callsign: callsign.into(),
            squawk: squawk.into(),
            age_s: 0.0,
        }
    }

    #[test]
    fn emergency_squawks_map_to_alert_states() {
        let mut det = AnomalyDetector::default();
        assert_eq!(
            det.scan(&[obs("A1", "VL604", "7700")], 1_000)[0].alert,
            Some(AlertState::Emergency)
        );
        assert_eq!(
            det.scan(&[obs("B2", "NAF911", "7500")], 1_000)[0].alert,
            Some(AlertState::Hijack)
        );
        assert_eq!(
            det.scan(&[obs("C3", "NGA212", "7600")], 1_000)[0].alert,
            Some(AlertState::RadioFailure)
        );
    }

    #[test]
    fn identity_less_emitter_flags_after_arming() {
        let cfg = DarkTargetConfig { min_track_age_s: 5.0, ..Default::default() };
        let mut det = AnomalyDetector::new(cfg);

        let early = det.scan(&[obs("DARK", "", "0000")], 1_000);
        assert!(early.is_empty(), "must stay quiet while arming");

        let armed = det.scan(&[obs("DARK", "", "0000")], 6_000);
        assert_eq!(armed.len(), 1);
        assert_eq!(armed[0].alert, Some(AlertState::DarkTarget));
        assert!(armed[0].reason.contains("IDENTITY"));
    }

    #[test]
    fn transponder_silence_flags_absent_cooperative_tracks() {
        let cfg = DarkTargetConfig {
            min_track_age_s: 5.0,
            silence_threshold_s: 10.0,
            ..Default::default()
        };
        let mut det = AnomalyDetector::new(cfg);

        det.scan(&[obs("AAA", "VL604", "2000")], 1_000);
        det.scan(&[obs("AAA", "VL604", "2000")], 6_000);

        let quiet_mid = det.scan(&[], 12_000);
        assert!(quiet_mid.is_empty(), "inside grace period");

        let flagged = det.scan(&[], 16_000);
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].alert, Some(AlertState::DarkTarget));
        assert!(flagged[0].reason.contains("SILENCE"));
    }

    #[test]
    fn recovery_clears_alert_and_emits_transition() {
        let cfg = DarkTargetConfig { min_track_age_s: 1.0, ..Default::default() };
        let mut det = AnomalyDetector::new(cfg);

        det.scan(&[obs("X9", "", "0000")], 1_000); // seed record
        let armed = det.scan(&[obs("X9", "", "0000")], 3_000); // span >= 1 s -> flag
        assert_eq!(armed.len(), 1, "dark target armed");
        assert_eq!(armed[0].alert, Some(AlertState::DarkTarget));

        let recovered = det.scan(&[obs("X9", "NEWCS", "2000")], 4_000);
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].alert, None);
    }

    #[test]
    fn squawk_churn_is_counted_without_false_alerts() {
        let mut det = AnomalyDetector::default();
        det.scan(&[obs("CH", "TST01", "2000")], 1_000);
        det.scan(&[obs("CH", "TST01", "2345")], 2_000);
        det.scan(&[obs("CH", "TST01", "3456")], 3_000);
        let v = det.scan(&[obs("CH", "TST01", "4567")], 4_000);
        assert!(v.is_empty(), "churn alone must not alert");
        assert!(det.records["CH"].churn_count >= 3);
    }
}
