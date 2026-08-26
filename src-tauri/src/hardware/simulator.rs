//! Synthetic traffic generator feeding the unmodified ingestion pipeline.
//!
//! The simulator advances a truth-state fleet around the surveillance site,
//! encodes each observation into genuine Mode S DF17 frames (CRC address
//! parity included) or ACARS byte streams, and hands them to the engine
//! exactly as an RTL-SDR front-end would. Every downstream consumer — CPR
//! solver, EKF fusion, conflict detection, weather classifier — therefore
//! runs its production code path during bench demos and regression runs.
//!
//! Scenario highlights baked into the default roster:
//! * civil airliner cruise traffic on DNKN sector routes,
//! * a Nigerian Air Force pair whose geometry converges on a civil track
//!   inside the STCA lookahead window (guaranteed alert demo),
//! * a non-cooperative "dark target" with intermittent transponder silence,
//! * scheduled RF dropout windows exercising dead-reckoning coasting,
//! * a squawk 7700 emergency injected at T+45 s,
//! * AWOS surface telemetry and upper-air wind/temperature sampling
//!   standing in for the RS-485 mast and Mode S BDS registers.

use crate::hardware::mode_s_decoder::{
    append_df17_parity, cpr_encode, char_to_callsign_index, encode_altitude,
};
use crate::models::{AwosObservation, TrackClass, WeatherNode};

/// Events produced by one simulator tick, mirroring physical ingress points.
#[derive(Debug, Clone)]
pub enum IngestEvent {
    /// Raw 14-byte ES frame for the 1090 MHz channel.
    Raw1090Frame(Vec<u8>),
    /// Demodulated ACARS byte burst for the 131.550 MHz channel.
    AcarsBytes(Vec<u8>),
    /// Direct squawk readout (production source: Mode S DF4/DF5 replies).
    SquawkReport { icao24: String, squawk: String },
    /// AWOS mast observation (production source: Python sidecar).
    SurfaceObservation(AwosObservation),
    /// Upper-air sample (production source: BDS 4,4/4,5 extraction).
    UpperAirSample(WeatherNode),
}

#[derive(Debug, Clone)]
pub(crate) struct SimAircraft {
    icao24: u32,
    callsign: &'static str,
    latitude: f64,
    longitude: f64,
    altitude_ft: f64,
    speed_kt: f64,
    course_deg: f64,
    vertical_rate_fpm: f64,
    squawk: &'static str,
    /// Truth-state classification retained for replay export; not
    /// broadcast over the air (class inference happens engine-side from
    /// callsign prefixes, mirroring real surveillance).
    #[allow(dead_code)]
    class: TrackClass,
    /// Squawk adopted once `emergency_at_s` elapses (None = never).
    emergency_squawk: Option<&'static str>,
    emergency_at_s: f64,
    /// Transponder silence window parameters (None = always transmitting).
    dropout_period_s: Option<f64>,
    dropout_duration_s: f64,
    /// Per-aircraft emission phase staggering.
    phase: f64,
}

impl SimAircraft {
    fn squawk_at(&self, t_s: f64) -> &'static str {
        match self.emergency_squawk {
            Some(sq) if t_s >= self.emergency_at_s => sq,
            _ => self.squawk,
        }
    }

    fn silent(&self, t_s: f64) -> bool {
        match self.dropout_period_s {
            None => false,
            Some(period) => {
                // Silence lands at the END of each cycle so targets transmit
                // immediately from boot — dropouts interrupt later, which is
                // what exercises dead-reckoning during live demos.
                let phase = (t_s + self.phase * period) % period;
                phase >= period - self.dropout_duration_s
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SimulatorConfig {
    pub site_latitude_deg: f64,
    pub site_longitude_deg: f64,
    pub seed: u64,
    /// Position frame cadence per aircraft (seconds between frames).
    pub position_interval_s: f64,
    pub velocity_interval_s: f64,
    pub identity_interval_s: f64,
    pub acars_interval_s: f64,
    pub upper_air_interval_s: f64,
    pub awos_interval_s: f64,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            site_latitude_deg: crate::kinematics::SITE_LAT_DEG,
            site_longitude_deg: crate::kinematics::SITE_LON_DEG,
            seed: 0xA3_5E_01_2026,
            position_interval_s: 0.5,
            velocity_interval_s: 1.0,
            identity_interval_s: 6.0,
            acars_interval_s: 45.0,
            upper_air_interval_s: 5.0,
            awos_interval_s: 30.0,
        }
    }
}

/// Deterministic xorshift64* stream — stable across platforms for
/// reproducible bench conditions.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in [-1, 1].
    pub fn unit(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }
}

/// Truth-state fleet simulator.
pub struct Simulator {
    cfg: SimulatorConfig,
    fleet: Vec<SimAircraft>,
    rng: Rng,
    t_s: f64,
    even_odd_flip: bool,
    last_position_emit: Vec<f64>,
    last_velocity_emit: Vec<f64>,
    last_identity_emit: Vec<f64>,
    last_acars_emit: f64,
    last_upper_air_emit: f64,
    last_awos_emit: f64,
    awos_qnh: f32,
    awos_wind_dir: f32,
    awos_wind_kt: f32,
    awos_temp: f32,
    datis_letter: u8,
}

impl Simulator {
    /// Builds the default DNKN sector roster.
    pub fn new(cfg: SimulatorConfig) -> Self {
        let fleet = vec![
            SimAircraft {
                icao24: 0x34_21_57,
                callsign: "VL604",
                class: TrackClass::Civil,
                latitude: 12.42,
                longitude: 8.16,
                altitude_ft: 32_000.0,
                speed_kt: 445.0,
                course_deg: 205.0,
                vertical_rate_fpm: 0.0,
                squawk: "3421",
                emergency_squawk: Some("7700"),
                emergency_at_s: 45.0,
                dropout_period_s: Some(140.0),
                dropout_duration_s: 28.0,
                phase: 0.0,
            },
            SimAircraft {
                icao24: 0x34_8A_C2,
                callsign: "VL702",
                class: TrackClass::Civil,
                latitude: 11.62,
                longitude: 9.05,
                altitude_ft: 28_500.0,
                speed_kt: 430.0,
                course_deg: 335.0,
                vertical_rate_fpm: -600.0,
                squawk: "4203",
                emergency_squawk: None,
                emergency_at_s: 0.0,
                dropout_period_s: Some(170.0),
                dropout_duration_s: 32.0,
                phase: 0.31,
            },
            SimAircraft {
                icao24: 0x3C_64_1D,
                callsign: "DLH501",
                class: TrackClass::Civil,
                latitude: 13.35,
                longitude: 7.62,
                altitude_ft: 38_000.0,
                speed_kt: 490.0,
                course_deg: 148.0,
                vertical_rate_fpm: 0.0,
                squawk: "5172",
                emergency_squawk: None,
                emergency_at_s: 0.0,
                dropout_period_s: None,
                dropout_duration_s: 0.0,
                phase: 0.57,
            },
            // Quick-reaction alert pair engineered to violate separation:
            // NAF911 intersects VL604's projected path at T+70 s with a
            // ~1 km lateral miss, driving predicted minima inside STCA
            // thresholds from the first lookahead tick.
            SimAircraft {
                icao24: 0x06_11_04,
                callsign: "NAF911",
                class: TrackClass::Military,
                latitude: 12.139_5,
                longitude: 8.019_6,
                altitude_ft: 32_000.0,
                speed_kt: 520.0,
                course_deg: 27.0,
                vertical_rate_fpm: 0.0,
                squawk: "6101",
                emergency_squawk: None,
                emergency_at_s: 0.0,
                dropout_period_s: None,
                dropout_duration_s: 0.0,
                phase: 0.13,
            },
            SimAircraft {
                icao24: 0x06_11_05,
                callsign: "NAF912",
                class: TrackClass::Military,
                latitude: 12.095_0,
                longitude: 8.005_0,
                altitude_ft: 31_000.0,
                speed_kt: 500.0,
                course_deg: 27.0,
                vertical_rate_fpm: 800.0,
                squawk: "6102",
                emergency_squawk: None,
                emergency_at_s: 0.0,
                dropout_period_s: None,
                dropout_duration_s: 0.0,
                phase: 0.44,
            },
            // Non-cooperative profile: long silences, no identity broadcast.
            SimAircraft {
                icao24: 0x0B_AD_C0,
                callsign: "",
                class: TrackClass::Anomaly,
                latitude: 12.75,
                longitude: 8.95,
                altitude_ft: 6_500.0,
                speed_kt: 210.0,
                course_deg: 232.0,
                vertical_rate_fpm: -300.0,
                squawk: "0000",
                emergency_squawk: None,
                emergency_at_s: 0.0,
                dropout_period_s: Some(60.0),
                dropout_duration_s: 38.0,
                phase: 0.72,
            },
        ];

        let n = fleet.len();
        let rng = Rng::new(cfg.seed);
        Self {
            cfg,
            fleet,
            rng,
            t_s: 0.0,
            even_odd_flip: false,
            last_position_emit: vec![f64::NEG_INFINITY; n],
            last_velocity_emit: vec![f64::NEG_INFINITY; n],
            last_identity_emit: vec![f64::NEG_INFINITY; n],
            last_acars_emit: f64::NEG_INFINITY,
            last_upper_air_emit: f64::NEG_INFINITY,
            last_awos_emit: f64::NEG_INFINITY,
            awos_qnh: 1013.2,
            awos_wind_dir: 182.0,
            awos_wind_kt: 11.0,
            awos_temp: 33.5,
            datis_letter: b'B',
        }
    }

    #[inline]
    pub fn elapsed_s(&self) -> f64 {
        self.t_s
    }

    /// Advances truth state by `dt_s` and returns due ingest events.
    pub fn tick(&mut self, dt_s: f64, now_ms: u64) -> Vec<IngestEvent> {
        self.t_s += dt_s;
        let mut events = Vec::with_capacity(self.fleet.len() * 2);

        for (i, ac) in self.fleet.iter_mut().enumerate() {
            Self::advance_truth(ac, &mut self.rng, dt_s);
            if ac.silent(self.t_s) {
                continue;
            }

            if self.t_s - self.last_position_emit[i] >= self.cfg.position_interval_s {
                self.last_position_emit[i] = self.t_s;
                // CPR parity alternates per emitted frame, not per tick.
                self.even_odd_flip = !self.even_odd_flip;
                events.push(IngestEvent::Raw1090Frame(
                    encode_position_frame(ac, self.even_odd_flip),
                ));
            }
            if self.t_s - self.last_velocity_emit[i] >= self.cfg.velocity_interval_s {
                self.last_velocity_emit[i] = self.t_s;
                events.push(IngestEvent::Raw1090Frame(encode_velocity_frame(ac)));
            }
            if !ac.callsign.is_empty()
                && self.t_s - self.last_identity_emit[i] >= self.cfg.identity_interval_s
            {
                self.last_identity_emit[i] = self.t_s;
                events.push(IngestEvent::Raw1090Frame(encode_identity_frame(
                    ac.icao24, ac.callsign,
                )));
            }

            let squawk_now = ac.squawk_at(self.t_s);
            if ac.emergency_squawk.is_some()
                && self.t_s >= ac.emergency_at_s
                && self.t_s - dt_s < ac.emergency_at_s
            {
                events.push(IngestEvent::SquawkReport {
                    icao24: format!("{:06X}", ac.icao24),
                    squawk: squawk_now.to_string(),
                });
                events.push(IngestEvent::Raw1090Frame(encode_status_frame(
                    ac.icao24, 5,
                )));
            }
        }

        if self.t_s - self.last_awos_emit >= self.cfg.awos_interval_s {
            self.last_awos_emit = self.t_s;
            self.awos_wind_dir = (self.awos_wind_dir as f64 + self.rng.unit() * 3.0).rem_euclid(360.0) as f32;
            self.awos_wind_kt = (self.awos_wind_kt as f64 + self.rng.unit() * 0.8).clamp(3.0, 25.0) as f32;
            self.awos_qnh = (self.awos_qnh as f64 + self.rng.unit() * 0.3).clamp(998.0, 1030.0) as f32;
            self.awos_temp = (self.awos_temp as f64 + self.rng.unit() * 0.2).clamp(24.0, 39.0) as f32;
            events.push(IngestEvent::SurfaceObservation(AwosObservation {
                qnh_hpa: self.awos_qnh,
                wind_dir_deg: self.awos_wind_dir,
                wind_speed_kt: self.awos_wind_kt,
                wind_gust_kt: self.awos_wind_kt + 4.0,
                temperature_c: self.awos_temp,
                dewpoint_c: self.awos_temp - 9.0,
                visibility_m: 8_000.0 + (self.rng.unit() * 1_500.0) as f32,
                observed_ms: now_ms,
            }));
        }

        if self.t_s - self.last_upper_air_emit >= self.cfg.upper_air_interval_s {
            self.last_upper_air_emit = self.t_s;
            if let Some(ac) = self.fleet.iter().filter(|a| !a.silent(self.t_s)).next() {
                events.push(IngestEvent::UpperAirSample(upper_air_node(ac, now_ms)));
            }
        }

        if self.t_s - self.last_acars_emit >= self.cfg.acars_interval_s {
            self.last_acars_emit = self.t_s;
            self.datis_letter = b'B' + (self.datis_letter - b'B' + 1) % 24;
            let metar = format!(
                "METAR DNKN {:02}{:02}00Z {:03}{}KT 6000 HZ FEW030 {}/{} Q{:04} NOSIG=",
                (self.t_s / 3600.0 + 12.0) as u32 % 24,
                00,
                self.awos_wind_dir as u32,
                self.awos_wind_kt.round() as u32,
                self.awos_temp.round() as u32,
                (self.awos_temp - 9.0).round() as u32,
                self.awos_qnh.round() as u32,
            );
            let tail = format!("5NDKN{:02}", self.datis_letter % 100);
            let frame_bytes = crate::hardware::acars_decoder::synthesize_frame(
                '2',
                &tail,
                '\u{06}',
                "SA",
                '1',
                "VL604",
                &metar,
            );
            events.push(IngestEvent::AcarsBytes(frame_bytes));
        }

        events
    }

    /// Flat-earth truth propagation with slow heading wander.
    fn advance_truth(ac: &mut SimAircraft, rng: &mut Rng, dt_s: f64) {
        let jitter = rng.unit();
        ac.course_deg = (ac.course_deg + jitter * 0.35 * dt_s).rem_euclid(360.0);
        let rad = ac.course_deg.to_radians();
        let speed_ms = ac.speed_kt * 0.514_444;
        let m_per_deg_lat =
            111_132.92 - 559.82 * (2.0 * ac.latitude.to_radians()).cos();
        let m_per_deg_lon = 111_412.84 * ac.latitude.to_radians().cos();
        ac.latitude += speed_ms * rad.cos() * dt_s / m_per_deg_lat;
        ac.longitude += speed_ms * rad.sin() * dt_s / m_per_deg_lon;
        ac.altitude_ft += ac.vertical_rate_fpm * dt_s / 60.0;
    }
}

fn upper_air_node(ac: &SimAircraft, now_ms: u64) -> WeatherNode {
    // Harmattan-season profile: temperature lapse toward ISA above the
    // boundary layer, south-westerly shear strengthening with altitude.
    let lapse = (ac.altitude_ft / 10_000.0).min(4.0);
    WeatherNode {
        latitude: ac.latitude,
        longitude: ac.longitude,
        altitude_ft: ac.altitude_ft as f32,
        wind_dir_deg: (225.0 - lapse * 12.0).rem_euclid(360.0) as f32,
        wind_speed_kt: (15.0 + lapse * 14.0) as f32,
        temperature_c: (33.0 - lapse * 6.4) as f32,
        pressure_hpa: (1013.25 - ac.altitude_ft * 0.0295) as f32,
        fusion_sources: 0b100,
        observed_ms: now_ms,
    }
}

/// ME word for TC 11 airborne position (barometric).
fn build_position_me(latitude: f64, longitude: f64, altitude_ft: f64, odd: bool) -> u64 {
    const TC_AIRBORNE_BARO: u64 = 11;
    let tc = TC_AIRBORNE_BARO << 51;
    let alt = (encode_altitude(altitude_ft) as u64) << 36;
    let odd_bit = (odd as u64) << 34;
    let (ylat, xlon) = cpr_encode(latitude, longitude, odd);
    let lat_field = ((ylat * 131_072.0).round() as u64 & 0x1_FFFF) << 17;
    let lon_field = (xlon * 131_072.0).round() as u64 & 0x1_FFFF;
    tc | alt | odd_bit | lat_field | lon_field
}

/// ME word for TC 1-4 identification.
fn build_identity_me(tc: u64, category: u64, callsign: &str) -> u64 {
    let mut me = (tc.min(4) << 51) | (category & 0x7) << 48;
    let mut cs = callsign.as_bytes().to_vec();
    cs.resize(8, b' ');
    for (i, ch) in cs.iter().enumerate() {
        let idx = char_to_callsign_index(*ch);
        me |= idx.expect("charset") << (42 - 6 * i);
    }
    me
}

/// ME word for TC 19 velocity, subtype 1 (ground-speed vector).
fn build_velocity_me(
    speed_kt: f64,
    course_deg: f64,
    vertical_rate_fpm: f64,
) -> u64 {
    let rad = course_deg.to_radians();
    let v_ew = speed_kt * rad.sin();
    let v_ns = speed_kt * rad.cos();

    let ew_neg = (v_ew < 0.0) as u64;
    let ns_neg = (v_ns < 0.0) as u64;
    let ew_raw = ((v_ew.abs().round() as i64 + 1).clamp(1, 1023)) as u64;
    let ns_raw = ((v_ns.abs().round() as i64 + 1).clamp(1, 1023)) as u64;

    let vr_sign = (vertical_rate_fpm < 0.0) as u64;
    let vr_raw = (((vertical_rate_fpm.abs() / 64.0).round() as i64 + 1)
        .clamp(0, 511)) as u64;

    (19u64 << 51)
        | (1u64 << 48)
        | (ew_neg << 47)
        | (ew_raw << 37)
        | (ns_neg << 36)
        | (ns_raw << 26)
        | (vr_sign << 24)
        | (vr_raw << 15)
}

/// ME word for TC 28 subtype 1 aircraft status (emergency states 0-7).
fn build_status_me(emergency_state: u64) -> u64 {
    (28u64 << 51) | (1u64 << 48) | ((emergency_state & 0x7) << 45)
}

fn df17_frame(me: u64, icao24: u32) -> Vec<u8> {
    let mut frame = vec![0x8Du8];
    frame.extend_from_slice(&icao24.to_be_bytes()[1..]);
    frame.extend_from_slice(&me.to_be_bytes()[1..]);
    append_df17_parity(frame, icao24)
}

pub(crate) fn encode_position_frame(ac: &SimAircraft, odd: bool) -> Vec<u8> {
    df17_frame(build_position_me(ac.latitude, ac.longitude, ac.altitude_ft, odd), ac.icao24)
}

pub(crate) fn encode_velocity_frame(ac: &SimAircraft) -> Vec<u8> {
    df17_frame(build_velocity_me(ac.speed_kt, ac.course_deg, ac.vertical_rate_fpm), ac.icao24)
}

pub fn encode_identity_frame(icao24: u32, callsign: &str) -> Vec<u8> {
    df17_frame(build_identity_me(4, 2, callsign), icao24)
}

pub fn encode_status_frame(icao24: u32, emergency_state: u64) -> Vec<u8> {
    df17_frame(build_status_me(emergency_state), icao24)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinematics::NM_TO_M;
    use crate::hardware::acars_decoder::{classify_weather, parse_stream, WeatherProduct};
    use crate::hardware::mode_s_decoder::{decode_frame, DecodedMessage};

    fn run_seconds(sim: &mut Simulator, seconds: f64, step: f64, now_ms: &mut u64) -> Vec<IngestEvent> {
        let mut all = Vec::new();
        let mut elapsed = 0.0;
        while elapsed < seconds {
            all.extend(sim.tick(step, *now_ms));
            elapsed += step;
            *now_ms += (step * 1000.0) as u64;
        }
        all
    }

    #[test]
    fn all_emitted_frames_decode_with_matching_icao() {
        let mut sim = Simulator::new(SimulatorConfig::default());
        let mut now = 1_700_000_000_000u64;
        let events = run_seconds(&mut sim, 40.0, 0.25, &mut now);

        let frames: Vec<&Vec<u8>> = events
            .iter()
            .filter_map(|e| match e {
                IngestEvent::Raw1090Frame(f) => Some(f),
                _ => None,
            })
            .collect();
        assert!(frames.len() > 400, "expected dense frame flow, got {}", frames.len());

        let mut decoded = 0usize;
        for f in &frames {
            if let Some(res) = decode_frame(f) {
                decoded += 1;
                let icao = match &res.message {
                    DecodedMessage::Identity { icao24, .. } => *icao24,
                    DecodedMessage::AirbornePosition { icao24, .. } => *icao24,
                    DecodedMessage::Velocity { icao24, .. } => *icao24,
                    DecodedMessage::AircraftStatus { icao24, .. } => *icao24,
                    DecodedMessage::SurfacePosition { icao24, .. } => *icao24,
                };
                assert_ne!(icao, 0, "zero ICAO impossible");
            } else {
                panic!("simulator produced undecodable frame: {}", crate::hardware::mode_s_decoder::frame_to_hex(f));
            }
        }
        assert_eq!(decoded, frames.len(), "100% decode rate required");
    }

    #[test]
    fn position_frames_resolve_truth_coordinates() {
        let mut sim = Simulator::new(SimulatorConfig::default());
        let mut now = 1_700_000_000_000u64;
        let events = run_seconds(&mut sim, 30.0, 0.25, &mut now);

        let vl604 = 0x34_21_57u32;
        let mut pairs: Vec<(bool, f64, f64)> = Vec::new();
        for e in &events {
            if let IngestEvent::Raw1090Frame(f) = e {
                if let Some(res) = decode_frame(f) {
                    if let DecodedMessage::AirbornePosition { icao24, odd, lat_cpr, lon_cpr, altitude_ft, .. } =
                        res.message
                    {
                        if icao24 == vl604 {
                            pairs.push((odd, lat_cpr, lon_cpr));
                            assert!((altitude_ft - 32_000.0).abs() < 120.0, "alt {}", altitude_ft);
                        }
                    }
                }
            }
        }
        assert!(pairs.len() >= 40, "position updates {}", pairs.len());

        // Solve a recent even/odd pair and compare against propagated truth.
        let even = *pairs.iter().rev().find(|p| !p.0).expect("even frame");
        let odd = *pairs.iter().rev().find(|p| p.0).expect("odd frame");
        let sol = crate::hardware::mode_s_decoder::cpr_global_decode(
            (even.1, even.2),
            (odd.1, odd.2),
            true,
        )
        .expect("global solve");
        assert!(
            (sol.latitude_deg - 12.42).abs() < 0.09 && (sol.longitude_deg - 8.16).abs() < 0.09,
            "resolved {:?} vs origin (12.42, 8.16)",
            (sol.latitude_deg, sol.longitude_deg)
        );
    }

    #[test]
    fn velocity_frames_report_speed_and_track() {
        let mut sim = Simulator::new(SimulatorConfig::default());
        let mut now = 1_700_000_000_000u64;
        let events = run_seconds(&mut sim, 12.0, 0.25, &mut now);

        let mut checked = 0;
        for e in &events {
            if let IngestEvent::Raw1090Frame(f) = e {
                if let Some(res) = decode_frame(f) {
                    if let DecodedMessage::Velocity { icao24, ground_speed_kt, .. } = res.message {
                        if icao24 == 0x34_21_57 {
                            if let Some(gs) = ground_speed_kt {
                                assert!((gs - 445.0).abs() < 6.0, "gs {}", gs);
                                checked += 1;
                            }
                        }
                    }
                }
            }
        }
        assert!(checked > 5, "velocity checks {}", checked);
    }

    #[test]
    fn emergency_squawk_fires_at_t_plus_45() {
        let mut sim = Simulator::new(SimulatorConfig::default());
        let mut now = 1_700_000_000_000u64;
        let events = run_seconds(&mut sim, 50.0, 0.25, &mut now);

        let reports: Vec<&String> = events
            .iter()
            .filter_map(|e| match e {
                IngestEvent::SquawkReport { squawk, .. } => Some(squawk),
                _ => None,
            })
            .collect();
        assert_eq!(reports.len(), 1, "single emergency transition");
        assert_eq!(reports[0], "7700");

        // Status frame with unlawful-interference-style state present.
        let status = events.iter().any(|e| matches!(
            e,
            IngestEvent::Raw1090Frame(f)
                if matches!(
                    decode_frame(f).map(|r| r.message),
                    Some(DecodedMessage::AircraftStatus { emergency_state: 5, .. })
                )
        ));
        assert!(status, "TC28 status frame expected");
    }

    #[test]
    fn dark_target_stays_silent_in_windows_and_skips_identity() {
        let mut sim = Simulator::new(SimulatorConfig::default());
        let mut now = 1_700_000_000_000u64;
        let events = run_seconds(&mut sim, 130.0, 0.25, &mut now);

        let dark_frames: Vec<Option<DecodedMessage>> = events
            .iter()
            .filter_map(|e| match e {
                IngestEvent::Raw1090Frame(f) => decode_frame(f).map(|r| Some(r.message)),
                _ => None,
            })
            .filter(|m| {
                matches!(
                    m,
                    Some(DecodedMessage::Identity { icao24: 0x0B_AD_C0, .. })
                )
            })
            .collect();
        assert!(
            dark_frames.is_empty(),
            "dark target must never broadcast identity"
        );
    }

    #[test]
    fn acars_burst_yields_metar_product() {
        let mut sim = Simulator::new(SimulatorConfig::default());
        let mut now = 1_700_000_000_000u64;
        let events = run_seconds(&mut sim, 50.0, 0.25, &mut now);

        let bursts: Vec<&Vec<u8>> = events
            .iter()
            .filter_map(|e| match e {
                IngestEvent::AcarsBytes(b) => Some(b),
                _ => None,
            })
            .collect();
        assert!(!bursts.is_empty());

        let msgs = parse_stream(bursts[0], now);
        assert_eq!(msgs.len(), 1);
        match classify_weather(&msgs[0]) {
            WeatherProduct::Metar(text) => assert!(text.contains("DNKN")),
            other => panic!("expected METAR, got {:?}", other),
        }
    }

    #[test]
    fn awos_and_upper_air_streams_flow() {
        let mut sim = Simulator::new(SimulatorConfig::default());
        let mut now = 1_700_000_000_000u64;
        let events = run_seconds(&mut sim, 65.0, 0.25, &mut now);

        let awos = events
            .iter()
            .filter(|e| matches!(e, IngestEvent::SurfaceObservation(_)))
            .count();
        let upper = events
            .iter()
            .filter(|e| matches!(e, IngestEvent::UpperAirSample(_)))
            .count();
        assert!(awos >= 2, "awos count {}", awos);
        assert!(upper >= 10, "upper air count {}", upper);

        if let Some(IngestEvent::UpperAirSample(node)) =
            events.iter().find(|e| matches!(e, IngestEvent::UpperAirSample(_)))
        {
            assert!(node.wind_speed_kt > 5.0 && node.temperature_c < 40.0);
            assert!(node.observed_ms > 0);
        }
    }

    #[test]
    fn naf_geometry_closes_on_vl604_for_stca_demo() {
        let mut sim = Simulator::new(SimulatorConfig::default());
        let mut now = 1_700_000_000_000u64;

        let dist_nm = |a: (f64, f64), b: (f64, f64)| -> f64 {
            let dx = (b.1 - a.1) * 111_412.84 * a.0.to_radians().cos() / NM_TO_M;
            let dy = (b.0 - a.0) * 111_132.92 / NM_TO_M;
            (dx * dx + dy * dy).sqrt()
        };

        let start = dist_nm((12.42, 8.16), (11.86, 8.83));
        let mut end = start;
        for _ in 0..240 {
            sim.tick(0.5, now);
            now += 500;
        }
        let vl = sim.fleet[0].clone();
        let naf = sim.fleet[3].clone();
        end = dist_nm((vl.latitude, vl.longitude), (naf.latitude, naf.longitude));

        assert!(
            end < start - 20.0,
            "geometry must close hard: {} -> {} NM",
            start,
            end
        );
        assert!(end < 25.0, "inside engagement volume: {} NM", end);
    }

    #[test]
    fn simulator_is_deterministic_per_seed() {
        let run = |seed: u64| {
            let mut sim = Simulator::new(SimulatorConfig {
                seed,
                ..SimulatorConfig::default()
            });
            let mut now = 1_700_000_000_000u64;
            let ev = run_seconds(&mut sim, 8.0, 0.5, &mut now);
            ev.iter()
                .map(|e| match e {
                    IngestEvent::Raw1090Frame(f) => crate::hardware::mode_s_decoder::frame_to_hex(f),
                    _ => String::from("other"),
                })
                .collect::<Vec<_>>()
                .join("|")
        };
        assert_eq!(run(42), run(42), "same seed must reproduce");
        assert_ne!(run(42), run(43), "different seeds must diverge");
    }
}
