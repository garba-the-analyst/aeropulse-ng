//! Central surveillance fusion engine.
//!
//! Owns every per-ICAO filter state and stitches the processing chain into
//! one deterministic pass:
//!
//! ```text
//! IngestEvent ──► decode ──► CPR solve ──► EKF update ─┐
//! ACARS bytes ──► classify ──► weather fusion ─────────┤
//! AWOS / upper-air ────────────────────────────────────┤
//!                                                      ▼
//!        tick(): predict ► tracks ► STCA ► anomalies ► geofence ► snapshot
//! ```
//!
//! The engine is deliberately pure with respect to time: callers supply the
//! wall clock, which keeps the whole pipeline unit-testable and makes
//! replays bit-reproducible for incident review.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::broadcast;

use crate::hardware::mode_s_decoder::{cpr_global_decode, decode_frame, DecodedMessage};
use crate::hardware::sdr_registry::UsbProbe;
use crate::hardware::simulator::IngestEvent;
use crate::kinematics::dead_reckoning::{project_leader_line, SiteOrigin};
use crate::kinematics::ekf::{Ekf, EkfConfig};
use crate::kinematics::stca_math::{StcaDetector, TrackSample};
use crate::models::{
    AlertState, AnomalyNote, EngineSnapshot, EngineStatus, FlightDataBlock, HardwareStatus,
    STCAAlert, Track, TrackClass, VerticalTrend, WeatherMatrix,
};
use crate::defense::dark_target::{AnomalyDetector, TrackObservation};
use crate::defense::geofence::{FenceFix, GeofenceMonitor};
use crate::models::AwosObservation;
use crate::weather_fusion::WeatherFusion;

/// Seconds after which a track without fresh fixes is rendered coasting.
const COAST_THRESHOLD_S: f64 = 8.0;
/// Tracks removed entirely after this much silence.
const DROP_TRACK_AFTER_S: f64 = 180.0;
/// CPR even/odd pairing validity window (DO-260B position epoch).
const CPR_PAIR_WINDOW_MS: u64 = 10_000;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub site: SiteOrigin,
    pub tick_hz: f64,
    /// STCA passes executed per second (sub-sampled from tick rate).
    pub stca_hz: f64,
    pub anomaly_period_s: f64,
    pub geofence_period_s: f64,
    /// Leader-line lookahead for display vectors.
    pub leader_lookahead_s: f64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            site: SiteOrigin::new(
                crate::kinematics::SITE_LAT_DEG,
                crate::kinematics::SITE_LON_DEG,
                crate::kinematics::SITE_ALT_FT,
            ),
            tick_hz: 60.0,
            stca_hz: 15.0,
            anomaly_period_s: 1.0,
            geofence_period_s: 0.5,
            leader_lookahead_s: 120.0,
        }
    }
}

struct CprSlot {
    lat_cpr: f64,
    lon_cpr: f64,
    received_ms: u64,
}

struct TrackState {
    ekf: Ekf,
    callsign: String,
    class: TrackClass,
    alert: AlertState,
    squawk: String,
    cpr_even: Option<CprSlot>,
    cpr_odd: Option<CprSlot>,
    last_position_ms: u64,
    last_velocity_ms: Option<u64>,
    messages_seen: u64,
    breach_until_ms: u64,
}

impl TrackState {
    fn new(now_ms: u64) -> Self {
        Self {
            ekf: Ekf::new(EkfConfig::default()),
            callsign: String::new(),
            class: TrackClass::Unidentified,
            alert: AlertState::None,
            squawk: String::new(),
            cpr_even: None,
            cpr_odd: None,
            last_position_ms: now_ms,
            last_velocity_ms: None,
            messages_seen: 0,
            breach_until_ms: 0,
        }
    }
}

pub struct SurveillanceEngine {
    cfg: EngineConfig,
    tracks: HashMap<String, TrackState>,
    weather: WeatherFusion,
    anomalies: AnomalyDetector,
    fences: GeofenceMonitor,
    stca: StcaDetector,
    tx: broadcast::Sender<Arc<EngineSnapshot>>,
    last_snapshot: Arc<EngineSnapshot>,

    pending: Vec<(IngestEvent, u64)>,
    tick_count: u64,
    last_tick_ms: u64,
    ekf_latency_us: f64,
    duckdb_writes_per_sec: f64,
    sidecar_online: bool,
    emergency_override_until_ms: u64,
    hardware: Vec<HardwareStatus>,
}

impl SurveillanceEngine {
    pub fn new(cfg: EngineConfig) -> Self {
        let (tx, _) = broadcast::channel(16);
        let empty = Arc::new(EngineSnapshot {
            tracks: vec![],
            flight_data_blocks: vec![],
            stca_alerts: vec![],
            geofence_breaches: vec![],
            anomalies: vec![],
            weather: WeatherMatrix {
                nodes: vec![],
                surface_qnh_hpa: None,
                surface_wind_dir_deg: None,
                surface_wind_speed_kt: None,
                surface_temperature_c: None,
                surface_dewpoint_c: None,
                visibility_m: None,
                dust_layer_top_ft: None,
                datis_text: None,
                metar_text: None,
                updated_ms: 0,
            },
            status: EngineStatus {
                hardware: vec![],
                ekf_active: true,
                ekf_latency_us: 0.0,
                stca_pairs_active: 0,
                tracks_total: 0,
                duckdb_writes_per_sec: 0.0,
                sidecar_online: false,
                updated_ms: 0,
            },
        });
        Self {
            cfg,
            tracks: HashMap::new(),
            weather: WeatherFusion::default(),
            anomalies: AnomalyDetector::default(),
            fences: GeofenceMonitor::default(),
            stca: StcaDetector::default(),
            tx,
            last_snapshot: empty,
            pending: Vec::with_capacity(64),
            tick_count: 0,
            last_tick_ms: 0,
            ekf_latency_us: 0.0,
            duckdb_writes_per_sec: 0.0,
            sidecar_online: false,
            emergency_override_until_ms: 0,
            hardware: {
                let probe = crate::hardware::sdr_registry::SimulatedProbe;
                crate::hardware::sdr_registry::bind_channels(
                    &crate::hardware::sdr_registry::ChannelPlan::default(),
                    &probe.scan(),
                )
                .into_iter()
                .map(|b| b.status)
                .collect()
            },
        }
    }

    #[inline]
    pub fn set_sidecar_online(&mut self, online: bool) {
        self.sidecar_online = online;
    }

    /// Manual emergency override from the master command bar: every active
    /// track renders with the emergency flag until `until_ms`.
    pub fn arm_emergency_override(&mut self, until_ms: u64) {
        self.emergency_override_until_ms = until_ms;
    }

    #[inline]
    pub fn site_origin(&self) -> SiteOrigin {
        self.cfg.site
    }

    pub fn add_geofence(&mut self, fence: crate::models::Geofence) {
        self.fences.add_fence(fence);
    }

    pub fn list_geofences(&self) -> Vec<crate::models::Geofence> {
        self.fences.fences().to_vec()
    }

    pub fn set_fence_active(&mut self, id: &str, active: bool) -> bool {
        self.fences.set_active(id, active)
    }

    #[inline]
    pub fn report_duckdb_rate(&mut self, rows_per_sec: f64) {
        self.duckdb_writes_per_sec = rows_per_sec;
    }

    #[inline]
    pub fn replace_fences(&mut self, fences: GeofenceMonitor) {
        self.fences = fences;
    }

    /// Subscribe to the 60 Hz snapshot bus.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<EngineSnapshot>> {
        self.tx.subscribe()
    }

    #[inline]
    pub fn latest(&self) -> Arc<EngineSnapshot> {
        Arc::clone(&self.last_snapshot)
    }

    #[inline]
    pub fn weather_matrix(&self) -> WeatherMatrix {
        self.weather.matrix()
    }

    /// Atmospheric point query through the fused matrix.
    pub fn sample_at(
        &self,
        latitude: f64,
        longitude: f64,
        altitude_ft: f64,
        now_ms: u64,
    ) -> crate::weather_fusion::spatial_interp::AtmosphericSample {
        self.weather.sample_at(latitude, longitude, altitude_ft, now_ms)
    }

    // ------------------------------------------------------------------
    // Ingestion
    //
    // Events are queued and drained inside tick() AFTER the filter
    // prediction pass, guaranteeing each measurement fuses against a state
    // propagated exactly once to its own epoch.
    // ------------------------------------------------------------------

    pub fn ingest(&mut self, events: &[IngestEvent], now_ms: u64) {
        for ev in events {
            self.pending.push((ev.clone(), now_ms));
        }
    }

    fn drain_pending(&mut self, now_ms: u64) {
        for (ev, ev_ms) in std::mem::take(&mut self.pending) {
            match ev {
                IngestEvent::Raw1090Frame(bytes) => self.ingest_frame(&bytes, ev_ms),
                IngestEvent::AcarsBytes(raw) => self.ingest_acars(&raw, ev_ms),
                IngestEvent::SquawkReport { icao24, squawk } => {
                    if let Some(ts) = self.tracks.get_mut(icao24.as_str()) {
                        ts.squawk = squawk;
                    }
                }
                IngestEvent::SurfaceObservation(obs) => self.weather.ingest_surface(&obs),
                IngestEvent::UpperAirSample(node) => self.weather.ingest_upper_air(node),
            }
        }
        let _ = now_ms;
    }

    fn ingest_frame(&mut self, bytes: &[u8], now_ms: u64) {
        let Some(result) = decode_frame(bytes) else {
            return;
        };
        let msg = result.message;

        let icao_key = format!(
            "{:06X}",
            match &msg {
                DecodedMessage::Identity { icao24, .. } => *icao24,
                DecodedMessage::AirbornePosition { icao24, .. } => *icao24,
                DecodedMessage::Velocity { icao24, .. } => *icao24,
                DecodedMessage::AircraftStatus { icao24, .. } => *icao24,
                DecodedMessage::SurfacePosition { icao24, .. } => *icao24,
            }
        );

        let ts = self.tracks.entry(icao_key.clone()).or_insert_with(|| TrackState::new(now_ms));
        ts.messages_seen += 1;

        match msg {
            DecodedMessage::Identity { callsign, .. } => {
                if !callsign.is_empty() {
                    ts.callsign = callsign.clone();
                    ts.class = infer_class(&callsign);
                }
            }
            DecodedMessage::AirbornePosition {
                odd,
                lat_cpr,
                lon_cpr,
                altitude_ft,
                ..
            } => {
                // Take the opposite-parity slot for pairing attempts.
                let opposite = if odd {
                    ts.cpr_even.take()
                } else {
                    ts.cpr_odd.take()
                };

                let current = CprSlot {
                    lat_cpr,
                    lon_cpr,
                    received_ms: now_ms,
                };

                let mut solved = false;
                if let Some(prev) =
                    opposite.filter(|p| now_ms.saturating_sub(p.received_ms) <= CPR_PAIR_WINDOW_MS)
                {
                    let (even_pair, odd_pair) = if odd {
                        ((prev.lat_cpr, prev.lon_cpr), (lat_cpr, lon_cpr))
                    } else {
                        ((lat_cpr, lon_cpr), (prev.lat_cpr, prev.lon_cpr))
                    };
                    if let Some(pos) = cpr_global_decode(even_pair, odd_pair, odd) {
                        let (x_m, y_m) =
                            self.cfg.site.to_local(pos.latitude_deg, pos.longitude_deg);
                        ts.ekf.fuse_position(x_m, y_m, altitude_ft, now_ms as f64 / 1000.0);
                        ts.last_position_ms = now_ms;
                        solved = true;
                    }
                }

                // Whichever frame arrived becomes its parity's latest slot.
                if odd {
                    ts.cpr_odd = Some(current);
                } else {
                    ts.cpr_even = Some(current);
                }
                let _ = solved;
            }
            DecodedMessage::Velocity {
                ground_speed_kt,
                track_deg,
                vertical_rate_fpm,
                ..
            } => {
                if let (Some(gs), Some(trk)) = (ground_speed_kt, track_deg) {
                    ts.ekf.fuse_velocity(gs, trk, vertical_rate_fpm.unwrap_or(0.0), now_ms as f64 / 1000.0);
                    ts.last_velocity_ms = Some(now_ms);
                }
            }
            DecodedMessage::AircraftStatus { emergency_state, .. } => {
                if (1..=6).contains(&emergency_state) && ts.squawk.is_empty() {
                    ts.squawk = "7700".into();
                }
            }
            DecodedMessage::SurfacePosition { .. } => {}
        }
    }

    fn ingest_acars(&mut self, raw: &[u8], now_ms: u64) {
        use crate::hardware::acars_decoder::{classify_weather, parse_stream, WeatherProduct};
        for msg in parse_stream(raw, now_ms) {
            match classify_weather(&msg) {
                WeatherProduct::Metar(text) | WeatherProduct::Speci(text) => {
                    self.weather.ingest_metar_text(&text, now_ms)
                }
                WeatherProduct::Datis { body, .. } => self.weather.ingest_datis_text(&body, now_ms),
                WeatherProduct::Other => {}
            }
        }
    }

    // ------------------------------------------------------------------
    // Tick
    // ------------------------------------------------------------------

    /// Advances the world to `now_ms`, publishing a fresh snapshot.
    pub fn tick(&mut self, now_ms: u64) -> Arc<EngineSnapshot> {
        let started = std::time::Instant::now();
        let dt_s = if self.last_tick_ms == 0 {
            0.0
        } else {
            (now_ms.saturating_sub(self.last_tick_ms)) as f64 / 1000.0
        };
        self.last_tick_ms = now_ms;
        self.tick_count += 1;

        // 1. Filter propagation (also performs dead-reckoning coasting).
        for ts in self.tracks.values_mut() {
            ts.ekf.predict(dt_s);
        }

        // 2. Fuse queued measurements against the freshly propagated state.
        self.drain_pending(now_ms);

        // 2. Periodic subsystem scans.
        let stca_due =
            dt_s > 0.0 && (self.tick_count as f64 * (1.0 / self.cfg.tick_hz)) % (1.0 / self.cfg.stca_hz)
                < dt_s.max(0.001);
        let mut active_alerts: Vec<STCAAlert> = Vec::new();
        if stca_due || self.tick_count == 1 {
            let samples = self.collect_samples();
            active_alerts = self.stca.detect(&samples, now_ms);
        }

        let anomaly_due = self.tick_count == 1
            || dt_s > 0.0
                && (self.tick_count as f64 / self.cfg.tick_hz) % self.cfg.anomaly_period_s < dt_s.max(0.001);
        let mut anomaly_notes: Vec<AnomalyNote> = Vec::new();
        if anomaly_due {
            let observations: Vec<TrackObservation> = self
                .tracks
                .iter()
                .map(|(icao, ts)| TrackObservation {
                    icao24: icao.clone(),
                    callsign: ts.callsign.clone(),
                    squawk: ts.squawk.clone(),
                    age_s: (now_ms.saturating_sub(ts.last_position_ms)) as f64 / 1000.0,
                })
                .collect();
            for verdict in self.anomalies.scan(&observations, now_ms) {
                if let Some(ts) = self.tracks.get_mut(&verdict.icao24) {
                    ts.alert = verdict.alert.unwrap_or(AlertState::None);
                }
                if verdict.alert.is_some() {
                    anomaly_notes.push(AnomalyNote {
                        icao24: verdict.icao24,
                        reason: verdict.reason,
                    });
                }
            }
        }

        let fence_due = self.tick_count == 1
            || dt_s > 0.0
                && (self.tick_count as f64 / self.cfg.tick_hz) % self.cfg.geofence_period_s < dt_s.max(0.001);
        let mut breaches = Vec::new();
        if fence_due {
            let fixes: Vec<FenceFix> = self
                .tracks
                .iter()
                .filter(|(_, ts)| ts.ekf.is_initialised())
                .map(|(icao, ts)| {
                    let s = ts.ekf.state();
                    let (lat, lon) = self.cfg.site.from_local(s[0], s[1]);
                    FenceFix {
                        icao24: icao.clone(),
                        callsign: ts.callsign.clone(),
                        latitude: lat,
                        longitude: lon,
                        altitude_ft: s[2] / 0.3048,
                        now_ms,
                    }
                })
                .collect();
            for b in self.fences.scan(&fixes) {
                eprintln!("[fence] BREACH {} in {}", b.icao24, b.fence_id);
                breaches.push(b.clone());
                if let Some(ts) = self.tracks.get_mut(&b.icao24) {
                    ts.breach_until_ms = now_ms + 30_000;
                }
            }
        }

        // 3. Materialise display tracks.
        let mut tracks_out: Vec<Track> = Vec::with_capacity(self.tracks.len());
        let mut fdbs: Vec<FlightDataBlock> = Vec::with_capacity(self.tracks.len());
        let drop_cutoff_s = DROP_TRACK_AFTER_S;

        self.tracks.retain(|_, ts| {
            let age_s = (now_ms.saturating_sub(ts.last_position_ms)) as f64 / 1000.0;
            age_s < drop_cutoff_s
        });

        for (icao, ts) in self.tracks.iter() {
            if !ts.ekf.is_initialised() {
                continue;
            }
            let s = ts.ekf.state();
            let (latitude, longitude) = self.cfg.site.from_local(s[0], s[1]);
            let alt_ft = s[2] / 0.3048;
            let vx = s[3];
            let vy = s[4];
            let vz_fps = s[5] / 0.3048;
            let gs_kt = vx.hypot(vy) / 0.514_444;
            let course = vx.atan2(vy).to_degrees().rem_euclid(360.0);
            let vr_fpm = vz_fps * 60.0;
            let trend = if vr_fpm > 250.0 {
                VerticalTrend::Climb
            } else if vr_fpm < -250.0 {
                VerticalTrend::Descent
            } else {
                VerticalTrend::Level
            };
            let age_s = (now_ms.saturating_sub(ts.last_position_ms)) as f64 / 1000.0;
            let coasting = age_s > COAST_THRESHOLD_S;

            let leader_line = project_leader_line(
                &self.cfg.site,
                s[0],
                s[1],
                vx,
                vy,
                self.cfg.leader_lookahead_s,
                10.0,
            );

            let mut alert = ts.alert;
            if alert == AlertState::None && now_ms < ts.breach_until_ms {
                alert = AlertState::GeofenceBreach;
            }
            if alert == AlertState::None && now_ms < self.emergency_override_until_ms {
                alert = AlertState::Emergency;
            }

            let track = Track {
                icao24: icao.clone(),
                callsign: if ts.callsign.is_empty() {
                    format!("UNK-{icao}")
                } else {
                    ts.callsign.clone()
                },
                class: ts.class,
                alert,
                latitude,
                longitude,
                altitude_ft: alt_ft,
                ground_speed_kt: gs_kt,
                course_deg: course,
                vertical_rate_fpm: vr_fpm,
                vertical_trend: trend,
                squawk: if ts.squawk.is_empty() {
                    "----".into()
                } else {
                    ts.squawk.clone()
                },
                on_ground: false,
                last_update_ms: ts.last_position_ms,
                age_s,
                coasting,
                position_sigma_m: ts.ekf.horizontal_sigma_m(),
                leader_line,
            };
            fdbs.push(FlightDataBlock::from_track(&track));
            tracks_out.push(track);
        }
        tracks_out.sort_by(|a, b| a.icao24.cmp(&b.icao24));

        self.ekf_latency_us = started.elapsed().as_secs_f64() * 1e6;

        let snapshot = Arc::new(EngineSnapshot {
            tracks: tracks_out,
            flight_data_blocks: fdbs,
            stca_alerts: active_alerts,
            geofence_breaches: breaches,
            anomalies: anomaly_notes,
            weather: self.weather.matrix(),
            status: EngineStatus {
                hardware: self.hardware.clone(),
                ekf_active: true,
                ekf_latency_us: self.ekf_latency_us,
                stca_pairs_active: self.last_snapshot.stca_alerts.len(),
                tracks_total: self.last_snapshot.tracks.len(),
                duckdb_writes_per_sec: self.duckdb_writes_per_sec,
                sidecar_online: self.sidecar_online,
                updated_ms: now_ms,
            },
        });
        let _ = self.tx.send(Arc::clone(&snapshot));
        self.last_snapshot = Arc::clone(&snapshot);
        Arc::clone(&snapshot)
    }

    fn collect_samples(&mut self) -> Vec<TrackSample> {
        self.tracks
            .iter()
            .filter(|(_, ts)| ts.ekf.is_initialised())
            .enumerate()
            .map(|(i, (icao, ts))| {
                let s = ts.ekf.state();
                TrackSample::from_components(
                    i,
                    icao,
                    if ts.callsign.is_empty() { icao.as_str() } else { ts.callsign.as_str() },
                    s[0],
                    s[1],
                    s[2] / 0.3048,
                    s[3],
                    s[4],
                    s[5] / 0.3048,
                )
            })
            .collect()
    }

    /// Feeds an AWOS observation directly (sidecar path).
    pub fn ingest_awos(&mut self, obs: &AwosObservation) {
        self.weather.ingest_surface(obs);
    }
}

fn infer_class(callsign: &str) -> TrackClass {
    let upper = callsign.to_uppercase();
    if upper.starts_with("NAF") || upper.starts_with("NGA") || upper.starts_with("MIL") {
        TrackClass::Military
    } else {
        TrackClass::Civil
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::simulator::{Simulator, SimulatorConfig};

    struct Harness {
        engine: SurveillanceEngine,
        sim: Simulator,
        sim_ms: u64,
        /// Peak breach-event count seen on any single snapshot.
        max_breaches: usize,
    }

    impl Harness {
        fn run_for(&mut self, seconds: f64) {
            let steps = (seconds / 0.25).ceil() as usize;
            for _ in 0..steps {
                let events = self.sim.tick(0.25, self.sim_ms);
                self.engine.ingest(&events, self.sim_ms);
                let snap = self.engine.tick(self.sim_ms);
                self.max_breaches = self.max_breaches.max(snap.geofence_breaches.len());
                self.sim_ms += 250;
            }
        }
    }

    #[test]
    fn simulator_feed_builds_tracks_and_resolves_identity() {
        let mut h = Harness {
            engine: SurveillanceEngine::new(EngineConfig::default()),
            sim: Simulator::new(SimulatorConfig::default()),
            sim_ms: 1_700_000_000_000,
            max_breaches: 0,
        };
        h.run_for(35.0);

        let snap = h.engine.latest();
        assert!(snap.tracks.len() >= 4, "tracks {}", snap.tracks.len());

        let vl604 = snap
            .tracks
            .iter()
            .find(|t| t.callsign.contains("VL604"))
            .expect("VL604 identified");
        assert_eq!(vl604.class, TrackClass::Civil);
        assert!((12.0..=13.0).contains(&vl604.latitude));
        assert!((8.0..=9.5).contains(&vl604.longitude));
        assert!(vl604.ground_speed_kt > 350.0 && vl604.ground_speed_kt < 550.0);
        assert!(!vl604.leader_line.is_empty());
        assert_eq!(vl604.leader_line.len(), 13, "2-minute line at 10 s steps");

        let naf = snap
            .tracks
            .iter()
            .find(|t| t.callsign.starts_with("NAF"))
            .expect("NAF pair");
        assert_eq!(naf.class, TrackClass::Military);

        // FDB contract: three lines populated.
        assert!(snap.flight_data_blocks.len() >= 4);
        let fdb = snap.flight_data_blocks.first().expect("fdb");
        assert!(!fdb.line1.is_empty() && !fdb.line2.is_empty());
    }

    #[test]
    fn emergency_and_stca_scenarios_fire() {
        let mut h = Harness {
            engine: SurveillanceEngine::new(EngineConfig::default()),
            sim: Simulator::new(SimulatorConfig::default()),
            sim_ms: 1_700_000_000_000,
            max_breaches: 0,
        };
        // Run past the T+45 squawk event and deep into the NAF closure.
        h.run_for(75.0);

        let snap = h.engine.latest();
        let vl604 = snap.tracks.iter().find(|t| t.icao24 == "342157");
        assert!(
            vl604.map(|t| t.squawk == "7700" && t.alert == AlertState::Emergency).unwrap_or(false),
            "emergency propagation failed: {:?}",
            vl604.map(|t| (&t.squawk, t.alert))
        );

        // STCA: NAF911 converges on VL604 — must be flagged by now.
        let conflict = snap
            .stca_alerts
            .iter()
            .any(|a| a.icao_a.starts_with("0611") || a.icao_b.starts_with("0611"));
        assert!(conflict, "expected NAF/civil conflict pair");
    }

    #[test]
    fn weather_layers_reach_the_matrix() {
        let mut h = Harness {
            engine: SurveillanceEngine::new(EngineConfig::default()),
            sim: Simulator::new(SimulatorConfig::default()),
            sim_ms: 1_700_000_000_000,
            max_breaches: 0,
        };
        h.run_for(65.0);
        let w = h.engine.weather_matrix();
        assert!(w.surface_qnh_hpa.is_some(), "AWOS qnh missing");
        assert!(w.surface_wind_dir_deg.is_some());
        assert!(w.metar_text.is_some(), "ACARS METAR missing");
        assert!(w.nodes.len() > 5, "upper air nodes {}", w.nodes.len());

        let sample = h.engine.sample_at(12.05, 8.52, 31_500.0, h.sim_ms);
        assert!(sample.wind_speed_kt > 10.0);
        assert!(sample.confidence > 0.2);
    }

    #[test]
    fn dark_target_gets_flagged_end_to_end() {
        let cfg = EngineConfig::default();
        let mut h = Harness {
            engine: SurveillanceEngine::new(cfg),
            sim: Simulator::new(SimulatorConfig::default()),
            sim_ms: 1_700_000_000_000,
            max_breaches: 0,
        };
        // Intruder only starts transmitting around T+39 s due to its
        // silence-phase offset; arming needs 30 s of observed span, which
        // lands inside its second transmit window (~T+99 s).
        h.run_for(110.0);
        let snap = h.engine.latest();
        let dark = snap.tracks.iter().find(|t| t.icao24 == "0BADC0");
        assert!(
            dark.map(|t| t.alert == AlertState::DarkTarget || t.class == TrackClass::Anomaly)
                .unwrap_or(false),
            "dark target classification failed"
        );
    }

    #[test]
    fn coasting_during_dropout_windows() {
        let mut h = Harness {
            engine: SurveillanceEngine::new(EngineConfig::default()),
            sim: Simulator::new(SimulatorConfig::default()),
            sim_ms: 1_700_000_000_000,
            max_breaches: 0,
        };
        // VL604 goes silent in [112,140)s of each 140 s cycle.
        h.run_for(125.0);
        let snap = h.engine.latest();
        let vl604 = snap.tracks.iter().find(|t| t.icao24 == "342157").expect("track alive");
        assert!(vl604.coasting, "must be dead-reckoning during dropout");

        // Snapshot bus delivers to subscribers.
        let mut rx = h.engine.subscribe();
        h.run_for(1.0);
        assert!(rx.try_recv().is_ok(), "bus publish expected");
    }

    #[test]
    fn geofence_breach_flags_track() {
        use crate::models::Geofence;
        let mut h = Harness {
            engine: SurveillanceEngine::new(EngineConfig::default()),
            sim: Simulator::new(SimulatorConfig::default()),
            sim_ms: 1_700_000_000_000,
            max_breaches: 0,
        };

        // Box straddling VL604's southbound corridor between T+20 s and
        // T+55 s of simulated flight.
        h.engine.replace_fences(GeofenceMonitor::with_fences(vec![Geofence {
            id: "TST".into(),
            name: "TEST BOX".into(),
            vertices_deg: vec![
                [12.29, 8.09],
                [12.29, 8.17],
                [12.41, 8.17],
                [12.41, 8.09],
            ],
            floor_ft: 0.0,
            ceiling_ft: 45_000.0,
            active: true,
        }]));

        // Entry occurs near T+5.4 s when VL604 dips below the box's north
        // edge; the one-shot event lives on that tick's snapshot only.
        h.run_for(20.0);
        assert!(h.max_breaches >= 1, "breach event must fire");
        let snap = h.engine.latest();
        let vl = snap.tracks.iter().find(|t| t.icao24 == "342157").expect("track");
        assert_eq!(
            vl.alert,
            AlertState::GeofenceBreach,
            "breach flag must persist through the hold-down window"
        );
        let _ = Geofence {
            id: String::new(),
            name: String::new(),
            vertices_deg: vec![],
            floor_ft: 0.0,
            ceiling_ft: 0.0,
            active: false,
        };
    }
}
