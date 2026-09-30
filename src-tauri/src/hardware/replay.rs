//! Replay ingest — reads real-data archives into the engine.
//!
//! Handles two decoded JSON trace formats (both *not* raw hex):
//! - `readsb-hist`: global snapshots every 5s → `{"now": 1788220800, "aircraft": [{"hex":"a42dfb","lat":..., "lon":..., "alt_baro":..., "gs":..., "track":..., "squawk":"1200", "flight":"CAP2164 "}] }`
//! - `traces`: per-aircraft per-day → `{"icao":"a42dfb","trace": [[ts, lat, lon, alt, ...], ...]}` or readsb `aircraft` array
//!
//! Also handles the CSVs you actually downloaded to `datasources/`:
//! - `ax_arrivals_*.csv` (AirLabs style): `hex, callsign, altitude, speed, ...`
//! - `operations.csv.gz` / `acas.csv.gz` (ADS-B Exchange ops/acas dumps)
//!
//! Decoded traces cannot go through `decode_frame_with_cache` (no raw bytes) —
//! they are injected as synthetic `AdsbPositionUpdate`/`AdsbVelocityUpdate` that
//! bypass the decoder and feed the EKF directly. Raw ACAS `acas.csv.gz` *does*
//! contain `bytes:E20EBA2AC13AD0` hex which *can* go through the production decoder
//! for a true `decode_frame` diff vs `pyModeS`.
//!
//! All paths keep the simulator parallel — this is an additional ingest source.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::models::{Track, TrackClass, VerticalTrend};

/// Normalised replay sample — minimal fields the engine needs for a Track.
#[derive(Debug, Clone)]
pub struct ReplaySample {
    pub icao24: String,
    pub callsign: String,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_ft: f64,
    pub ground_speed_kt: f64,
    pub course_deg: f64,
    pub squawk: String,
    pub timestamp_ms: u64,
}

/// Reads a file that may be plain or gzip-compressed *without* a `.gz` extension
/// (samples.adsbexchange.com convention). Caller must handle the `Result`.
pub fn open_maybe_gzip(path: &Path) -> std::io::Result<Box<dyn std::io::Read>> {
    let f = std::fs::File::open(path)?;
    // Try to read gzip header
    let mut buf = [0u8; 3];
    use std::io::Read;
    let mut reader = std::io::BufReader::new(f);
    let n = reader.read(&mut buf)?;
    if n >= 2 && buf[0] == 0x1f && buf[1] == 0x8b {
        // Gzip
        let f2 = std::fs::File::open(path)?;
        let gz = flate2::read::GzDecoder::new(f2);
        Ok(Box::new(gz))
    } else {
        let f2 = std::fs::File::open(path)?;
        Ok(Box::new(f2))
    }
}

// ---- readsb-hist ------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ReadsbHistSnapshot {
    now: Option<f64>,
    aircraft: Option<Vec<ReadsbAircraft>>,
}

#[derive(Debug, Deserialize)]
struct ReadsbAircraft {
    hex: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    alt_baro: Option<serde_json::Value>, // can be "ground" or number
    gs: Option<f64>,
    track: Option<f64>,
    squawk: Option<String>,
    flight: Option<String>,
}

/// Parse a single `readsb-hist` file (maybe gzip without extension) into `ReplaySample`s.
pub fn read_readsb_hist(path: &Path) -> std::io::Result<Vec<ReplaySample>> {
    let reader = open_maybe_gzip(path)?;
    let snapshot: ReadsbHistSnapshot = serde_json::from_reader(reader)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let now_ms = snapshot.now.unwrap_or(0.0) as u64 * 1000;
    let mut out = Vec::new();
    if let Some(aircraft) = snapshot.aircraft {
        for ac in aircraft {
            if let (Some(hex), Some(lat), Some(lon)) = (ac.hex, ac.lat, ac.lon) {
                let alt_val = ac.alt_baro.and_then(|v| match v {
                    serde_json::Value::Number(n) => n.as_f64(),
                    serde_json::Value::String(s) if s == "ground" => Some(0.0),
                    _ => None,
                });
                out.push(ReplaySample {
                    icao24: hex.to_lowercase(),
                    callsign: ac.flight.unwrap_or_default().trim().to_string(),
                    latitude: lat,
                    longitude: lon,
                    altitude_ft: alt_val.unwrap_or(0.0),
                    ground_speed_kt: ac.gs.unwrap_or(0.0),
                    course_deg: ac.track.unwrap_or(0.0),
                    squawk: ac.squawk.unwrap_or_else(|| "----".to_string()),
                    timestamp_ms: now_ms,
                });
            }
        }
    }
    Ok(out)
}

// ---- traces (per-aircraft) --------------------------------------------------

#[derive(Debug, Deserialize)]
struct TracesFile {
    icao: Option<String>,
    trace: Option<Vec<Vec<serde_json::Value>>>,
    // Some traces are just an array of aircraft objects
    aircraft: Option<Vec<ReadsbAircraft>>,
}

/// Parse a `traces` file (per-aircraft, per-day). Handles both
/// `{"icao":"a42dfb","trace":[[ts,lat,lon,alt,...]]}` and `{"aircraft":[...]}` forms.
pub fn read_traces(path: &Path) -> std::io::Result<Vec<ReplaySample>> {
    let reader = open_maybe_gzip(path)?;
    let value: serde_json::Value = serde_json::from_reader(reader)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    // Form 1: {"icao":"...","trace":[[ts,lat,lon,alt,gs,track,squawk,flight],...]}
    if let Some(trace) = value.get("trace").and_then(|v| v.as_array()) {
        let icao = value.get("icao").and_then(|v| v.as_str()).unwrap_or("unknown").to_lowercase();
        let mut out = Vec::new();
        for entry in trace {
            if let Some(arr) = entry.as_array() {
                if arr.len() >= 4 {
                    let ts = arr[0].as_f64().unwrap_or(0.0) as u64 * 1000;
                    let lat = arr[1].as_f64();
                    let lon = arr[2].as_f64();
                    let alt = arr[3].as_f64();
                    if let (Some(lat), Some(lon)) = (lat, lon) {
                        out.push(ReplaySample {
                            icao24: icao.clone(),
                            callsign: String::new(),
                            latitude: lat,
                            longitude: lon,
                            altitude_ft: alt.unwrap_or(0.0),
                            ground_speed_kt: arr.get(4).and_then(|v| v.as_f64()).unwrap_or(0.0),
                            course_deg: arr.get(5).and_then(|v| v.as_f64()).unwrap_or(0.0),
                            squawk: arr.get(6).and_then(|v| v.as_str()).unwrap_or("----").to_string(),
                            timestamp_ms: ts,
                        });
                    }
                }
            }
        }
        return Ok(out);
    }

    // Form 2: {"aircraft":[...]} — same as readsb-hist
    if let Ok(snapshot) = serde_json::from_value::<ReadsbHistSnapshot>(value.clone()) {
        if let Some(aircraft) = snapshot.aircraft {
            let now_ms = snapshot.now.unwrap_or(0.0) as u64 * 1000;
            let mut out = Vec::new();
            for ac in aircraft {
                if let (Some(hex), Some(lat), Some(lon)) = (ac.hex, ac.lat, ac.lon) {
                    out.push(ReplaySample {
                        icao24: hex.to_lowercase(),
                        callsign: ac.flight.unwrap_or_default().trim().to_string(),
                        latitude: lat,
                        longitude: lon,
                        altitude_ft: ac.alt_baro.and_then(|v| match v {
                            serde_json::Value::Number(n) => n.as_f64(),
                            _ => None,
                        }).unwrap_or(0.0),
                        ground_speed_kt: ac.gs.unwrap_or(0.0),
                        course_deg: ac.track.unwrap_or(0.0),
                        squawk: ac.squawk.unwrap_or_else(|| "----".to_string()),
                        timestamp_ms: now_ms,
                    });
                }
            }
            return Ok(out);
        }
    }

    Ok(vec![])
}

// ---- CSV loaders for your actual datasources/ --------------------------------

/// `ax_arrivals_*.csv` — AirLabs-style arrivals: hex, callsign, altitude, speed
pub fn read_ax_arrivals_csv(path: &Path) -> std::io::Result<Vec<ReplaySample>> {
    let reader = open_maybe_gzip(path)?;
    let mut rdr = csv::Reader::from_reader(reader);
    let mut out = Vec::new();
    for result in rdr.deserialize::<HashMap<String, String>>() {
        let record = result.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if let (Some(hex), Some(lat), Some(lon)) = (
            record.get("hex").or_else(|| record.get("icao24")),
            record.get("altitude").and_then(|v| v.parse::<f64>().ok()),
            // ax_arrivals has no lat/lon — skip unless you have another CSV with it
            // For demo, treat altitude as lat placeholder is wrong, so we skip unless lat/lon present
            None::<f64>,
        ) {
            let _ = (hex, lat, lon);
        }
        // Real ax_arrivals has no position — useful only for callsign/hex inventory, not replay.
        // We log it for inventory but don't emit a ReplaySample without lat/lon.
    }
    Ok(out)
}

/// `operations.csv.gz` — header: time,icao,operation,airport,registration,flight,ac_type,runway,flight_link,squawk,...
/// No lat/lon — inventory only, not directly replayable as Track.
pub fn read_operations_csv(path: &Path) -> std::io::Result<Vec<ReplaySample>> {
    // Operations is event-based (landing/takeoff at airport), not position traces.
    // Return empty but allow caller to see file was handled.
    let _ = open_maybe_gzip(path)?;
    Ok(vec![])
}

/// `acas.csv.gz` — header: time,icao,DF:,bytes:,lat,lon,alt, ... — this *is* replayable
/// Example: 2026-01-01,00:07:53.8, ab9345,DF:,17,bytes:,E2E000069F380C,  32.948364, -96.788947, 1775,ft, ...
pub fn read_acas_csv(path: &Path) -> std::io::Result<Vec<ReplaySample>> {
    let reader = open_maybe_gzip(path)?;
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(reader);
    let mut out = Vec::new();
    for result in rdr.records() {
        let record = result.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        // acas.csv.gz has no header row in some files — handle flexibly
        // For files with header, csv crate will map; for those without, we parse by index
        // Simplified: look for lat/lon/alt in cols 6,7,8 if DF:17
        if record.len() < 10 {
            continue;
        }
        // Try to find icao (col 2), DF (col 4), bytes (col 6), lat (col 7), lon (col 8), alt (col 9)
        let icao = record.get(2).unwrap_or("").trim();
        let df = record.get(4).unwrap_or("").trim();
        let bytes = record.get(6).unwrap_or("").trim();
        let lat_str = record.get(7).unwrap_or("").trim();
        let lon_str = record.get(8).unwrap_or("").trim();
        let alt_str = record.get(9).unwrap_or("").trim();
        if icao.is_empty() || bytes.is_empty() || lat_str.is_empty() {
            continue;
        }
        // Only replay DF17 ACAS that already has decoded lat/lon (for engine Track)
        if df != "17" {
            continue;
        }
        if let (Ok(lat), Ok(lon), Ok(alt)) = (
            lat_str.parse::<f64>(),
            lon_str.parse::<f64>(),
            alt_str.parse::<f64>(),
        ) {
            out.push(ReplaySample {
                icao24: icao.to_lowercase(),
                callsign: String::new(),
                latitude: lat,
                longitude: lon,
                altitude_ft: alt,
                ground_speed_kt: 0.0,
                course_deg: 0.0,
                squawk: String::new(),
                timestamp_ms: 0,
            });
        }
    }
    Ok(out)
}

/// Unified dispatcher — tries each format in order.
pub fn read_any(path: &Path) -> std::io::Result<Vec<ReplaySample>> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    // Try JSON first
    if name.ends_with(".json") || name.contains("readsb") || name.contains("trace") {
        if let Ok(samples) = read_readsb_hist(path) {
            if !samples.is_empty() {
                return Ok(samples);
            }
        }
        if let Ok(samples) = read_traces(path) {
            if !samples.is_empty() {
                return Ok(samples);
            }
        }
    }
    // Try CSV
    if name.contains("ax_arrivals") {
        return read_ax_arrivals_csv(path);
    }
    if name.contains("operations") {
        return read_operations_csv(path);
    }
    if name.contains("acas") {
        return read_acas_csv(path);
    }
    // Fallback: try JSON then CSV
    if let Ok(samples) = read_readsb_hist(path) {
        if !samples.is_empty() {
            return Ok(samples);
        }
    }
    read_acas_csv(path)
}

/// Convert `ReplaySample` to `Track` for direct engine injection (bypasses decoder).
pub fn sample_to_track(s: &ReplaySample) -> Track {
    Track {
        icao24: s.icao24.clone(),
        callsign: if s.callsign.is_empty() {
            format!("UNK-{}", s.icao24)
        } else {
            s.callsign.clone()
        },
        class: if s.callsign.starts_with("NAF") || s.callsign.starts_with("NGA") {
            TrackClass::Military
        } else {
            TrackClass::Civil
        },
        alert: crate::models::AlertState::None,
        latitude: s.latitude,
        longitude: s.longitude,
        altitude_ft: s.altitude_ft,
        ground_speed_kt: s.ground_speed_kt,
        course_deg: s.course_deg,
        vertical_rate_fpm: 0.0,
        vertical_trend: VerticalTrend::Level,
        squawk: s.squawk.clone(),
        on_ground: s.altitude_ft == 0.0,
        last_update_ms: s.timestamp_ms,
        age_s: 0.0,
        coasting: false,
        position_sigma_m: 41.0,
        leader_line: vec![],
        mode_s_capable: true,
        last_baro_altitude_ft: Some(s.altitude_ft),
    }
}

// ---- Real-time playback controller ------------------------------------------

use std::sync::{Arc, Mutex};

/// Controls real-time replay of a pre-recorded file. Sorts by timestamp and
/// emits at engine wall-clock `now_ms` speed (1.0 = real-time, 2.0 = 2×, 0.5 = half).
/// This keeps STCA 120s lookahead and EKF `now_ms` deterministic.
pub struct ReplayController {
    samples: Vec<ReplaySample>,
    index: usize,
    start_engine_ms: Option<u64>,
    start_timestamp_ms: u64,
    speed: f64,
    paused: bool,
}

impl ReplayController {
    pub fn new(samples: Vec<ReplaySample>, speed: f64) -> Self {
        let mut sorted = samples;
        sorted.sort_by_key(|s| s.timestamp_ms);
        let start_ts = sorted.first().map(|s| s.timestamp_ms).unwrap_or(0);
        Self {
            samples: sorted,
            index: 0,
            start_engine_ms: None,
            start_timestamp_ms: start_ts,
            speed: speed.max(0.1),
            paused: false,
        }
    }

    pub fn set_speed(&mut self, speed: f64) {
        self.speed = speed.max(0.1);
        self.start_engine_ms = None; // resync on next tick
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if !paused {
            self.start_engine_ms = None;
        }
    }

    /// Tick wired to engine `now_ms` (60 Hz `sim_ms`), not `Instant::now()`.
    pub fn tick_at(&mut self, now_ms: u64) -> Vec<ReplaySample> {
        if self.paused || self.samples.is_empty() {
            return vec![];
        }
        if self.start_engine_ms.is_none() {
            self.start_engine_ms = Some(now_ms);
            if let Some(first) = self.samples.get(self.index) {
                self.start_timestamp_ms = first.timestamp_ms;
            }
        }
        let start_ms = match self.start_engine_ms {
            Some(v) => v,
            None => return vec![],
        };
        let elapsed = now_ms.saturating_sub(start_ms);
        let scaled_elapsed = (elapsed as f64 * self.speed) as u64;
        let target_ts = self.start_timestamp_ms + scaled_elapsed;

        let mut out = Vec::new();
        while self.index < self.samples.len() && self.samples[self.index].timestamp_ms <= target_ts {
            out.push(self.samples[self.index].clone());
            self.index += 1;
        }
        out
    }

    /// Backward-compat wall-clock tick (uses `Instant::now()` → `now_ms`).
    pub fn tick(&mut self, now: std::time::Instant) -> Vec<ReplaySample> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let _ = now;
        self.tick_at(now_ms)
    }

    pub fn is_finished(&self) -> bool {
        self.index >= self.samples.len()
    }

    pub fn progress(&self) -> f64 {
        if self.samples.is_empty() {
            1.0
        } else {
            self.index as f64 / self.samples.len() as f64
        }
    }
}

/// Global replay controller (Tauri state).
pub type SharedReplay = Arc<Mutex<Option<ReplayController>>>;

// ---- OSINT (online, no hardware) --------------------------------------------

/// OSINT source — fetches decoded state vectors from open APIs when online.
/// This is *online* and *not* air-gapped; it is an alternative to RTL-SDR hardware.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OsintConfig {
    pub enabled: bool,
    pub source: String, // "opensky" | "adsblol" | "adsbx"
    pub bbox: [f64; 4], // [lamin, lomin, lamax, lomax] — e.g. Nigeria FIR
    pub poll_interval_s: u64,
}

impl Default for OsintConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            source: "opensky".to_string(),
            bbox: [4.0, 2.5, 14.0, 15.0], // Nigeria approx
            poll_interval_s: 10,
        }
    }
}

/// Fetch from OpenSky Network REST API (no key, anonymous, rate-limited).
/// Returns `ReplaySample`s for direct Track injection. Call from a `tokio::spawn` loop.
///
/// ONLINE-ONLY: compiled only with `osint-live` feature. Default air-gap
/// builds return an explanatory error instead of touching the network.
#[cfg(feature = "osint-live")]
pub async fn fetch_opensky(bbox: [f64; 4]) -> Result<Vec<ReplaySample>, String> {
    let url = format!(
        "https://opensky-network.org/api/states/all?lamin={}&lomin={}&lamax={}&lomax={}",
        bbox[0], bbox[1], bbox[2], bbox[3]
    );
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", "AeroPulse-NG/2.4")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("OpenSky HTTP {}", resp.status()));
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let states = v.get("states").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut out = Vec::new();
    for s in states {
        if let Some(arr) = s.as_array() {
            // OpenSky state vector: [icao24, callsign, origin_country, time_pos, last_contact, lon, lat, baro_alt, on_ground, velocity, true_track, vertical_rate, sensors, geo_alt, squawk, spi, pos_src]
            let icao = arr.get(0).and_then(|v| v.as_str()).unwrap_or("").trim().to_lowercase();
            let callsign = arr.get(1).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let lon = arr.get(5).and_then(|v| v.as_f64());
            let lat = arr.get(6).and_then(|v| v.as_f64());
            let alt = arr.get(7).and_then(|v| v.as_f64());
            let (lat, lon) = match (lat, lon) {
                (Some(la), Some(lo)) => (la, lo),
                _ => continue,
            };
            if icao.is_empty() {
                continue;
            }
            out.push(ReplaySample {
                icao24: icao,
                callsign,
                latitude: lat,
                longitude: lon,
                altitude_ft: alt.unwrap_or(0.0) * 3.28084,
                ground_speed_kt: arr.get(9).and_then(|v| v.as_f64()).unwrap_or(0.0) * 1.94384,
                course_deg: arr.get(10).and_then(|v| v.as_f64()).unwrap_or(0.0),
                squawk: arr.get(14).and_then(|v| v.as_str()).unwrap_or("----").to_string(),
                timestamp_ms: now,
            });
        }
    }
    Ok(out)
}

/// Offline stub when `osint-live` is disabled (default air-gap build).
/// Preserves the type signature so `osint_cmd` compiles without network deps.
#[cfg(not(feature = "osint-live"))]
pub async fn fetch_opensky(_bbox: [f64; 4]) -> Result<Vec<ReplaySample>, String> {
    Err("OSINT live fetch disabled in air-gap build (enable feature `osint-live`)".into())
}
