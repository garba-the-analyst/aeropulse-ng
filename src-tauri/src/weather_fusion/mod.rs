//! Triple-Fusion offline weather engine.
//!
//! Fuses three independent atmospheric sources into one matrix:
//! 1. Surface layer — RS-485/RS-232 AWOS telemetry relayed by the sidecar,
//! 2. Terminal sector — ACARS D-ATIS / METAR text decoded off VHF airband,
//! 3. En-route — Mode S BDS 4,4 / 4,5 airborne register downlinks.
//!
//! No cloud APIs: every input originates from RF or serial links.

pub mod bds_extractor;
pub mod spatial_interp;

use std::collections::VecDeque;

use crate::models::{AwosObservation, WeatherMatrix, WeatherNode};
use spatial_interp::{dust_layer_top_ft, sample_atmosphere, AtmosphericSample};

/// Sliding retention window for airborne samples.
const NODE_HISTORY: usize = 512;

#[derive(Debug, Clone)]
pub struct FusionConfig {
    /// Lateral gate for point queries.
    pub max_query_range_nm: f64,
}

impl Default for FusionConfig {
    fn default() -> Self {
        Self {
            max_query_range_nm: 250.0,
        }
    }
}

pub struct WeatherFusion {
    cfg: FusionConfig,
    nodes: VecDeque<WeatherNode>,
    surface_qnh_hpa: Option<f32>,
    surface_wind_dir_deg: Option<f32>,
    surface_wind_speed_kt: Option<f32>,
    surface_temperature_c: Option<f32>,
    surface_dewpoint_c: Option<f32>,
    visibility_m: Option<f32>,
    metar_text: Option<String>,
    datis_text: Option<String>,
    updated_ms: u64,
}

impl Default for WeatherFusion {
    fn default() -> Self {
        Self::new(FusionConfig::default())
    }
}

impl WeatherFusion {
    pub fn new(cfg: FusionConfig) -> Self {
        Self {
            cfg,
            nodes: VecDeque::with_capacity(NODE_HISTORY),
            surface_qnh_hpa: None,
            surface_wind_dir_deg: None,
            surface_wind_speed_kt: None,
            surface_temperature_c: None,
            surface_dewpoint_c: None,
            visibility_m: None,
            metar_text: None,
            datis_text: None,
            updated_ms: 0,
        }
    }

    /// Ingests an AWOS observation. Surface values always take priority in
    /// the fused matrix; stale readings are overwritten on arrival.
    pub fn ingest_surface(&mut self, obs: &AwosObservation) {
        self.surface_qnh_hpa = Some(obs.qnh_hpa);
        self.surface_wind_dir_deg = Some(obs.wind_dir_deg);
        self.surface_wind_speed_kt = Some(obs.wind_speed_kt);
        self.surface_temperature_c = Some(obs.temperature_c);
        self.surface_dewpoint_c = Some(obs.dewpoint_c);
        self.visibility_m = Some(obs.visibility_m);
        self.updated_ms = obs.observed_ms;
    }

    /// Registers a decoded METAR/SPECI report.
    pub fn ingest_metar_text(&mut self, text: &str, now_ms: u64) {
        self.metar_text = Some(text.to_string());
        // Extract QNH when present ("Q1013") as terminal-layer corroboration;
        // AWOS still wins for the displayed value if both exist.
        if let Some(q) = extract_qnh(text) {
            if self.surface_qnh_hpa.is_none() {
                self.surface_qnh_hpa = Some(q);
                self.updated_ms = now_ms;
            }
        }
    }

    /// Registers a decoded D-ATIS broadcast body.
    pub fn ingest_datis_text(&mut self, text: &str, now_ms: u64) {
        self.datis_text = Some(text.to_string());
        if let Some(w) = extract_wind_group(text) {
            if self.surface_wind_dir_deg.is_none() {
                self.surface_wind_dir_deg = Some(w.0);
                self.surface_wind_speed_kt = Some(w.1);
                self.updated_ms = now_ms;
            }
        }
        if let Some(q) = extract_qnh(text) {
            if self.surface_qnh_hpa.is_none() {
                self.surface_qnh_hpa = Some(q);
                self.updated_ms = now_ms;
            }
        }
    }

    /// Pushes an airborne BDS-derived sample into the en-route layer.
    pub fn ingest_upper_air(&mut self, node: WeatherNode) {
        self.nodes.push_back(node);
        while self.nodes.len() > NODE_HISTORY {
            self.nodes.pop_front();
        }
        self.updated_ms = self.updated_ms.max(node.observed_ms);
    }

    #[inline]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Point query through the spatial interpolation stack.
    pub fn sample_at(
        &self,
        latitude: f64,
        longitude: f64,
        altitude_ft: f64,
        now_ms: u64,
    ) -> AtmosphericSample {
        let nodes: Vec<WeatherNode> = self.nodes.iter().copied().collect();
        sample_atmosphere(
            &nodes,
            latitude,
            longitude,
            altitude_ft,
            self.cfg.max_query_range_nm,
            now_ms,
        )
    }

    /// Full Display-2 snapshot.
    pub fn matrix(&self) -> WeatherMatrix {
        WeatherMatrix {
            nodes: self.nodes.iter().copied().collect(),
            surface_qnh_hpa: self.surface_qnh_hpa,
            surface_wind_dir_deg: self.surface_wind_dir_deg,
            surface_wind_speed_kt: self.surface_wind_speed_kt,
            surface_temperature_c: self.surface_temperature_c,
            surface_dewpoint_c: self.surface_dewpoint_c,
            visibility_m: self.visibility_m,
            dust_layer_top_ft: dust_layer_top_ft(self.visibility_m, self.surface_temperature_c),
            datis_text: self.datis_text.clone(),
            metar_text: self.metar_text.clone(),
            updated_ms: self.updated_ms,
        }
    }
}

/// Pulls "Q1013" style QNH groups out of raw text.
///
/// Accepts both the compact METAR form (`Q1013`) and the D-ATIS prose form
/// (`QNH 1014`), scanning all candidate Q positions.
fn extract_qnh(text: &str) -> Option<f32> {
    let upper = text.to_uppercase();
    let bytes = upper.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] != b'Q' {
            continue;
        }
        // Skip an optional NH designator plus separators.
        let mut j = i + 1;
        while j < bytes.len() && matches!(bytes[j], b'N' | b'H' | b' ') {
            j += 1;
        }
        let digits: String = upper[j..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if digits.len() == 4 {
            if let Ok(v) = digits.parse::<f32>() {
                return Some(v);
            }
        }
    }
    None
}

/// Pulls "18012KT"-style wind groups (degrees + knots).
///
/// Anchored scan: exactly three direction digits followed by a speed digit
/// run terminated by "KT", preventing mid-number false matches.
fn extract_wind_group(text: &str) -> Option<(f32, f32)> {
    let upper = text.to_uppercase();
    let bytes = upper.as_bytes();
    if bytes.len() < 7 {
        return None;
    }
    let is_digits =
        |slice: &[u8]| slice.iter().all(|c| c.is_ascii_digit());

    let mut i = 0usize;
    while i + 3 <= bytes.len() {
        if !is_digits(&bytes[i..i + 3]) {
            i += 1;
            continue;
        }
        // Collect speed digits immediately after the 3-digit direction.
        let mut j = i + 3;
        while j < bytes.len() && bytes[j].is_ascii_digit() && j - i < 8 {
            j += 1;
        }
        if j + 2 <= bytes.len() && &upper[j..j + 2] == "KT" && j > i + 3 {
            let dir: f32 = upper[i..i + 3].parse().ok()?;
            let spd: f32 = upper[i + 3..j].parse().ok()?;
            return Some((dir % 360.0, spd));
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(qnh: f32, vis: f32) -> AwosObservation {
        AwosObservation {
            qnh_hpa: qnh,
            wind_dir_deg: 180.0,
            wind_speed_kt: 12.0,
            wind_gust_kt: 16.0,
            temperature_c: 33.0,
            dewpoint_c: 24.0,
            visibility_m: vis,
            observed_ms: 1_000,
        }
    }

    #[test]
    fn awos_ingestion_populates_matrix() {
        let mut fusion = WeatherFusion::default();
        fusion.ingest_surface(&obs(1013.2, 8_000.0));
        let m = fusion.matrix();
        assert_eq!(m.surface_qnh_hpa, Some(1013.2));
        assert_eq!(m.surface_wind_dir_deg, Some(180.0));
        assert!(m.dust_layer_top_ft.is_some(), "haze implies dust layer");
    }

    #[test]
    fn awos_overrides_datis_but_not_vice_versa() {
        let mut fusion = WeatherFusion::default();
        fusion.ingest_datis_text("DNKN INFO C WIND 19014KT QNH 1014", 2_000);
        let m = fusion.matrix();
        assert_eq!(m.surface_wind_dir_deg, Some(190.0), "DATIS fills gap");
        assert_eq!(m.surface_qnh_hpa, Some(1014.0));

        fusion.ingest_surface(&obs(1012.7, 9_000.0));
        let m2 = fusion.matrix();
        assert_eq!(m2.surface_qnh_hpa, Some(1012.7), "AWOS priority");
        assert_eq!(m2.surface_wind_dir_deg, Some(180.0));
    }

    #[test]
    fn metar_extraction_fills_missing_surface_only() {
        let mut fusion = WeatherFusion::default();
        fusion.ingest_metar_text("METAR DNMO 261200Z 21008KT CAVOK Q1011=", 5_000);
        assert_eq!(fusion.matrix().surface_qnh_hpa, Some(1011.0));
    }

    #[test]
    fn upper_air_history_is_bounded_and_queriable() {
        let mut fusion = WeatherFusion::default();
        for i in 0..600u64 {
            fusion.ingest_upper_air(WeatherNode {
                latitude: 12.0 + (i as f64) * 0.0001,
                longitude: 8.5,
                altitude_ft: 30_000.0 + (i as f32),
                wind_dir_deg: 225.0,
                wind_speed_kt: 35.0,
                temperature_c: -30.0,
                pressure_hpa: 700.0,
                fusion_sources: 0b100,
                observed_ms: 10_000 + i * 1_000,
            });
        }
        assert!(fusion.node_count() <= NODE_HISTORY);

        let s = fusion.sample_at(12.02, 8.5, 30_200.0, 20_000);
        assert!(s.wind_speed_kt > 20.0 && s.wind_speed_kt < 50.0);
        assert!(s.confidence > 0.3);
    }

    #[test]
    fn qnh_and_wind_parsers() {
        assert_eq!(extract_qnh("... Q1013 NOSIG"), Some(1013.0));
        assert_eq!(extract_qnh("DNKN INFO C QNH 1014 TEMPO"), Some(1014.0));
        assert_eq!(extract_qnh("no pressure here"), None);
        assert_eq!(
            extract_wind_group("WIND 18012KT"),
            Some((180.0, 12.0))
        );
        assert_eq!(extract_wind_group("36005KT TEMP"), Some((0.0, 5.0)));
        assert_eq!(extract_wind_group("CAVOK"), None);
        assert_eq!(extract_wind_group("RUNWAY 18 CLOSED"), None);
    }
}
