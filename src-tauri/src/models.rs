//! AeroPulse-NG shared data models.
//!
//! Every structure in this module is the canonical wire contract between the
//! Rust engine, the Python sidecar, and the TypeScript frontends. All types
//! are `Clone + Serialize` so snapshots can be broadcast at 60 Hz without
//! locking contention on the telemetry bus.

use serde::{Deserialize, Serialize};

pub const MODEL_VERSION: u32 = 4;

/// Transponder cooperation classification driving symbology selection
/// (FAA HF-STD-010A target classes mapped onto MIL-STD-2525D frames).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackClass {
    Civil,
    Military,
    Unidentified,
    Anomaly,
}

/// Operational alert state for a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertState {
    None,
    Emergency,
    RadioFailure,
    Hijack,
    GeofenceBreach,
    DarkTarget,
}

/// Vertical trend extracted from Mode S barometric rate or EKF z-velocity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalTrend {
    Level,
    Climb,
    Descent,
}

/// A fused surveillance track. Positions are stored in WGS-84 geodetic form
/// plus a local tangent-plane (ENU) projection in metres for filter math.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub icao24: String,
    pub callsign: String,
    pub class: TrackClass,
    pub alert: AlertState,

    pub latitude: f64,
    pub longitude: f64,
    /// Barometric altitude, feet.
    pub altitude_ft: f64,
    /// Ground speed, knots.
    pub ground_speed_kt: f64,
    /// True track angle, degrees.
    pub course_deg: f64,
    pub vertical_rate_fpm: f64,
    pub vertical_trend: VerticalTrend,

    pub squawk: String,
    pub on_ground: bool,
    /// Milliseconds since UNIX epoch of last position update.
    pub last_update_ms: u64,
    /// Seconds since last transponder reception (grows during dropouts).
    pub age_s: f64,
    /// True while position is being dead-reckoned rather than measured.
    pub coasting: bool,
    /// 1-sigma horizontal position uncertainty from the EKF covariance, metres.
    pub position_sigma_m: f64,

    /// Projected 2-minute trajectory polyline in geodetic coordinates.
    pub leader_line: Vec<[f64; 2]>,
}

/// Three-line Flight Data Block rendered beside each target symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightDataBlock {
    pub icao24: String,
    /// Line 1: callsign, altitude in hundreds of feet, vertical arrow.
    pub line1: String,
    /// Line 2: transponder source, ground speed in knots, squawk.
    pub line2: String,
    /// Line 3: system quality / conflict annotation.
    pub line3: String,
    pub color: String,
}

/// A pairwise separation infringement detected inside the lookahead window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct STCAAlert {
    pub id: String,
    pub icao_a: String,
    pub callsign_a: String,
    pub icao_b: String,
    pub callsign_b: String,
    /// Predicted minimum horizontal separation over the lookahead, NM.
    pub min_horizontal_nm: f64,
    /// Predicted minimum vertical separation over the lookahead, feet.
    pub min_vertical_ft: f64,
    /// Seconds until predicted closest approach.
    pub time_to_closest_s: f64,
    pub triggered_ms: u64,
}

/// One atmospheric sample node of the triple-fusion matrix.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WeatherNode {
    pub latitude: f64,
    pub longitude: f64,
    /// Pressure altitude of this node, feet.
    pub altitude_ft: f32,
    /// Wind direction from-true, degrees.
    pub wind_dir_deg: f32,
    pub wind_speed_kt: f32,
    pub temperature_c: f32,
    /// QNH-referenced station pressure at node altitude, hPa.
    pub pressure_hpa: f32,
    /// Source bitmask: bit0 AWOS, bit1 ACARS D-ATIS, bit2 Mode-S BDS.
    pub fusion_sources: u8,
    pub observed_ms: u64,
}

/// The complete offline weather state handed to Display 2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherMatrix {
    pub nodes: Vec<WeatherNode>,
    /// Fused surface observation block (AWOS priority).
    pub surface_qnh_hpa: Option<f32>,
    pub surface_wind_dir_deg: Option<f32>,
    pub surface_wind_speed_kt: Option<f32>,
    pub surface_temperature_c: Option<f32>,
    pub surface_dewpoint_c: Option<f32>,
    /// Meteorological optical range in metres (Harmattan haze metric).
    pub visibility_m: Option<f32>,
    /// Estimated Harmattan dust layer ceiling, feet AGL.
    pub dust_layer_top_ft: Option<f32>,
    pub datis_text: Option<String>,
    pub metar_text: Option<String>,
    pub updated_ms: u64,
}

/// Live health of one SDR receiver channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareStatus {
    pub channel_id: String,
    pub role: String,
    pub serial_lock: String,
    pub frequency_hz: u32,
    pub sample_rate_sps: u32,
    pub gain_db: f32,
    pub messages_per_second: f64,
    pub ppm_error: f32,
    pub online: bool,
    pub usb_port: Option<String>,
}

/// Engine-wide diagnostics snapshot for Screen 0 status cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStatus {
    pub hardware: Vec<HardwareStatus>,
    pub ekf_active: bool,
    pub ekf_latency_us: f64,
    pub stca_pairs_active: usize,
    pub tracks_total: usize,
    pub duckdb_writes_per_sec: f64,
    pub sidecar_online: bool,
    pub updated_ms: u64,
}

/// FFT magnitude bins for the live spectrum analyzer plot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectrumFrame {
    pub center_hz: u32,
    pub span_hz: u32,
    /// Normalised dBFS bins, length fixed by engine configuration.
    pub power_dbfs: Vec<f32>,
    pub noise_floor_dbfs: f32,
    pub peak_hz: u32,
    pub updated_ms: u64,
}

/// Restricted airspace polygon with vertical extent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Geofence {
    pub id: String,
    pub name: String,
    pub vertices_deg: Vec<[f64; 2]>,
    pub floor_ft: f64,
    pub ceiling_ft: f64,
    pub active: bool,
}

/// Geofence violation event emitted once per entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeofenceBreach {
    pub fence_id: String,
    pub fence_name: String,
    pub icao24: String,
    pub callsign: String,
    pub entered_ms: u64,
}

/// Result of the tactical intercept solver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterceptSolution {
    pub target_icao24: String,
    pub interceptor_icao24: String,
    /// Required true heading for the interceptor, degrees.
    pub intercept_heading_deg: f64,
    /// Minimum ground speed to close the geometry within max_time_s, knots.
    pub required_speed_kt: f64,
    pub time_to_intercept_s: f64,
    pub initial_bearing_deg: f64,
    pub range_nm: f64,
    pub feasible: bool,
}

/// Complete Display-layer snapshot published on the telemetry bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSnapshot {
    pub tracks: Vec<Track>,
    pub flight_data_blocks: Vec<FlightDataBlock>,
    pub stca_alerts: Vec<STCAAlert>,
    pub geofence_breaches: Vec<GeofenceBreach>,
    pub anomalies: Vec<AnomalyNote>,
    pub weather: WeatherMatrix,
    pub status: EngineStatus,
}

/// Lightweight anomaly annotation for the threat-matrix panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyNote {
    pub icao24: String,
    pub reason: String,
}

/// AWOS surface observation relayed from the Python sidecar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwosObservation {
    pub qnh_hpa: f32,
    pub wind_dir_deg: f32,
    pub wind_speed_kt: f32,
    pub wind_gust_kt: f32,
    pub temperature_c: f32,
    pub dewpoint_c: f32,
    pub visibility_m: f32,
    pub observed_ms: u64,
}

/// Decoded ADS-B/Mode-S message consumed by the kinematics stack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdsbPositionUpdate {
    pub icao24: String,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_ft: f64,
    pub on_ground: bool,
    pub received_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdsbVelocityUpdate {
    pub icao24: String,
    pub ground_speed_kt: f64,
    pub course_deg: f64,
    pub vertical_rate_fpm: f64,
    pub received_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdsbIdentityUpdate {
    pub icao24: String,
    pub callsign: String,
    pub category: u8,
    pub received_ms: u64,
}

impl TrackClass {
    #[inline]
    pub fn css_color(self) -> &'static str {
        match self {
            TrackClass::Civil => "#00FF66",
            TrackClass::Military => "#7C4DFF",
            TrackClass::Unidentified | TrackClass::Anomaly => "#FFB300",
        }
    }

    #[inline]
    pub fn symbol_char(self) -> &'static str {
        match self {
            TrackClass::Civil => "\u{25C7}",
            TrackClass::Military => "\u{2227}",
            TrackClass::Unidentified | TrackClass::Anomaly => "\u{25A1}",
        }
    }
}

impl FlightDataBlock {
    /// Builds the standard three-line tactical tag from a track snapshot.
    pub fn from_track(track: &Track) -> Self {
        let alt_hundreds = (track.altitude_ft / 100.0).round() as i64;
        let arrow = match track.vertical_trend {
            VerticalTrend::Level => "",
            VerticalTrend::Climb => "\u{2191}",
            VerticalTrend::Descent => "\u{2193}",
        };
        let source = if track.coasting { "DR" } else { "1090" };
        let line3 = match track.alert {
            AlertState::None => format!("S-{:02}", track.squawk.len()),
            AlertState::Emergency => "EMRG".to_string(),
            AlertState::RadioFailure => "RADO".to_string(),
            AlertState::Hijack => "HIJK".to_string(),
            AlertState::GeofenceBreach => "GEO!".to_string(),
            AlertState::DarkTarget => "DARK".to_string(),
        };
        FlightDataBlock {
            icao24: track.icao24.clone(),
            line1: format!("{:<8} {:03} {}", track.callsign, alt_hundreds, arrow),
            line2: format!("{} {}K {}", source, track.ground_speed_kt.round(), track.squawk),
            line3,
            color: track.class.css_color().to_string(),
        }
    }
}
