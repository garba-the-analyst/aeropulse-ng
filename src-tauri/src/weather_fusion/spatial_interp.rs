//! ISA reference atmosphere and inverse-distance-weighted spatial
//! interpolation over the triple-fusion weather matrix.

use crate::models::WeatherNode;

/// Standard-atmosphere sample at a pressure altitude.
#[derive(Debug, Clone, Copy)]
pub struct IsaSample {
    pub temperature_c: f64,
    pub pressure_hpa: f64,
    pub density_kg_m3: f64,
}

/// ICAO standard atmosphere (troposphere + lower stratosphere).
pub fn isa_at(altitude_ft: f64) -> IsaSample {
    const T0_C: f64 = 15.0;
    const LAPSE_C_PER_KM: f64 = 6.5;
    const P0_HPA: f64 = 1013.25;

    let h_m = altitude_ft * 0.3048;
    if h_m <= 11_000.0 {
        let t_c = T0_C - LAPSE_C_PER_KM * (h_m / 1000.0);
        let p = P0_HPA * (1.0 - 2.25577e-5 * h_m).powf(5.255_88);
        IsaSample {
            temperature_c: t_c,
            pressure_hpa: p,
            density_kg_m3: p * 100.0 / (287.053 * (t_c + 273.15)),
        }
    } else {
        // Isothermal layer above the tropopause.
        let t_k = 216.65;
        let p_at_trop = 226.32_f64;
        let p = p_at_trop * (-((h_m - 11_000.0) / 6_341.62)).exp();
        IsaSample {
            temperature_c: t_k - 273.15,
            pressure_hpa: p,
            density_kg_m3: p * 100.0 / (287.053 * t_k),
        }
    }
}

/// Result of a fusion-matrix query.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct AtmosphericSample {
    /// Wind direction from-true, degrees.
    pub wind_dir_deg: f32,
    /// Wind speed, knots.
    pub wind_speed_kt: f32,
    pub temperature_c: f32,
    pub pressure_hpa: f32,
    /// Air density from fused temperature and ISA pressure profile.
    pub density_kg_m3: f32,
    /// 0..1 data confidence based on source count and horizontal distance.
    pub confidence: f32,
}

/// Great-circle distance approximation via local tangent plane, metres.
fn approx_distance_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let mlat = 111_132.92 - 559.82 * (2.0 * lat1.to_radians()).cos();
    let mlon = 111_412.84 * lat1.to_radians().cos();
    let dy = (lat2 - lat1) * mlat;
    let dx = wrap_longitude(lon2 - lon1) * mlon;
    (dx * dx + dy * dy).sqrt()
}

#[inline]
fn wrap_longitude(delta_deg: f64) -> f64 {
    if delta_deg > 180.0 {
        delta_deg - 360.0
    } else if delta_deg < -180.0 {
        delta_deg + 360.0
    } else {
        delta_deg
    }
}

/// Horizontal IDW interpolation with vertical bracketing between the two
/// nearest altitude layers at the same lateral anchor set.
///
/// `max_range_nm` gates lateral influence; beyond it the function returns
/// pure-ISA fallback (`confidence == 0`).
pub fn sample_atmosphere(
    nodes: &[WeatherNode],
    latitude: f64,
    longitude: f64,
    altitude_ft: f64,
    max_range_nm: f64,
    now_ms: u64,
) -> AtmosphericSample {
    const NODE_TTL_MS: u64 = 900_000; // 15 minutes
    let max_range_m = max_range_nm * 1852.0;

    // Rank nodes by combined metric: horizontal distance dominates, altitude
    // separation acts as a soft penalty.
    struct Ranked<'a> {
        node: &'a WeatherNode,
        dist_m: f64,
        alt_penalty: f64,
    }

    let mut ranked: Vec<Ranked> = nodes
        .iter()
        .filter(|n| now_ms.saturating_sub(n.observed_ms) < NODE_TTL_MS)
        .map(|n| {
            let d = approx_distance_m(latitude, longitude, n.latitude as f64, n.longitude as f64);
            Ranked {
                node: n,
                dist_m: d,
                alt_penalty: ((n.altitude_ft as f64 - altitude_ft).abs() / 4_000.0).min(3.0),
            }
        })
        .filter(|r| r.dist_m <= max_range_m)
        .collect();
    ranked.sort_by(|a, b| a.dist_m.total_cmp(&b.dist_m));

    if ranked.is_empty() {
        let isa = isa_at(altitude_ft);
        return AtmosphericSample {
            wind_dir_deg: 0.0,
            wind_speed_kt: 0.0,
            temperature_c: isa.temperature_c as f32,
            pressure_hpa: isa.pressure_hpa as f32,
            density_kg_m3: isa.density_kg_m3 as f32,
            confidence: 0.0,
        };
    }

    // Weighted blend of up to 8 nearest nodes.
    let _eps = 250.0_f64; // metres — flattens singularity at zero distance
    let mut w_sum = 0.0;
    let mut v_east = 0.0;
    let mut v_north = 0.0;
    let mut temp = 0.0;
    let mut pres = 0.0;
    let mut src_mask = 0u8;

    for r in ranked.iter().take(8) {
        let weight = 1.0 / ((r.dist_m / 1000.0 + 0.25).powi(2) * (1.0 + r.alt_penalty));
        let rad = (r.node.wind_dir_deg as f64).to_radians();
        v_east += weight * -(r.node.wind_speed_kt as f64) * rad.sin();
        v_north += weight * -(r.node.wind_speed_kt as f64) * rad.cos();
        temp += weight * r.node.temperature_c as f64;
        pres += weight * r.node.pressure_hpa as f64;
        src_mask |= r.node.fusion_sources;
        w_sum += weight;
    }

    let inv = 1.0 / w_sum;
    v_east *= inv;
    v_north *= inv;
    temp *= inv;
    pres *= inv;

    let speed = (v_east * v_east + v_north * v_north).sqrt();
    let mut dir = (-v_east).atan2(-v_north).to_degrees();
    if dir < 0.0 {
        dir += 360.0;
    }

    // Pressure follows the ISA profile better than sparse point samples at
    // distant altitudes; blend fused value toward ISA by altitude mismatch.
    let isa = isa_at(altitude_ft);
    let nearest_alt_gap = ranked[0].alt_penalty;
    let blended_p = pres * (1.0 - (nearest_alt_gap / 3.0).min(0.8)) + isa.pressure_hpa * (nearest_alt_gap / 3.0).min(0.8);

    let density = blended_p as f64 * 100.0 / (287.053 * (temp as f64 + 273.15));

    // Confidence: source diversity plus inverse range.
    let sources = src_mask.count_ones() as f32;
    let range_factor = 1.0 - (ranked[0].dist_m / max_range_m) as f32;
    AtmosphericSample {
        wind_dir_deg: dir as f32,
        wind_speed_kt: speed as f32,
        temperature_c: temp as f32,
        pressure_hpa: blended_p as f32,
        density_kg_m3: density as f32,
        confidence: ((sources / 3.0) * 0.6 + range_factor.max(0.0) * 0.4).clamp(0.0, 1.0),
    }
}

/// Estimated Harmattan dust-layer ceiling from surface visibility and
/// boundary-layer depth heuristics (feet AGL). None when visibility is good.
pub fn dust_layer_top_ft(surface_visibility_m: Option<f32>, surface_temp_c: Option<f32>) -> Option<f32> {
    let vis = surface_visibility_m?;
    if vis >= 9_000.0 {
        return None;
    }
    // Convective boundary-layer proxy: hotter surface mixes dust higher.
    let thermal_bump = surface_temp_c.map(|t| ((t - 28.0).max(0.0)) * 120.0).unwrap_or(0.0);
    Some(1_800.0 + (9_500.0 - vis.min(9_500.0)) * 1.4 + thermal_bump)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isa_reference_values() {
        let sea = isa_at(0.0);
        assert!((sea.temperature_c - 15.0).abs() < 0.01);
        assert!((sea.pressure_hpa - 1013.25).abs() < 0.1);
        assert!((sea.density_kg_m3 - 1.225).abs() < 0.005);

        let fl350 = isa_at(35_000.0);
        assert!((fl350.temperature_c - (-54.3)).abs() < 0.5);
        assert!((fl350.pressure_hpa - 238.4).abs() < 1.5);
        assert!(fl350.density_kg_m3 < 0.4);
    }

    #[test]
    fn idw_recovers_uniform_field_exactly() {
        let mk = |lat: f64, lon: f64, alt: f32| WeatherNode {
            latitude: lat,
            longitude: lon,
            altitude_ft: alt,
            wind_dir_deg: 90.0,
            wind_speed_kt: 40.0,
            temperature_c: 20.0,
            pressure_hpa: 1013.0,
            fusion_sources: 0b010,
            observed_ms: 1_000,
        };
        let nodes = vec![mk(12.10, 8.50, 30_000.0), mk(12.00, 8.60, 31_000.0), mk(11.95, 8.45, 29_000.0)];
        let s = sample_atmosphere(&nodes, 12.02, 8.55, 30_200.0, 120.0, 2_000);
        assert!((s.wind_speed_kt - 40.0).abs() < 0.5, "spd {}", s.wind_speed_kt);
        assert!((s.wind_dir_deg - 90.0).abs() < 1.0);
        assert!((s.temperature_c - 20.0).abs() < 0.3);
        assert!(s.confidence > 0.2);
    }

    #[test]
    fn out_of_range_returns_isa_fallback() {
        let node = WeatherNode {
            latitude: 12.0,
            longitude: 8.5,
            altitude_ft: 30_000.0,
            wind_dir_deg: 90.0,
            wind_speed_kt: 40.0,
            temperature_c: 20.0,
            pressure_hpa: 700.0,
            fusion_sources: 0b001,
            observed_ms: 1_000,
        };
        let s = sample_atmosphere(&[node], 13.5, 10.0, 30_000.0, 60.0, 2_000);
        assert_eq!(s.confidence, 0.0);
        let isa = isa_at(30_000.0);
        assert!((s.pressure_hpa - isa.pressure_hpa as f32).abs() < 0.5);
    }

    #[test]
    fn stale_nodes_are_ignored() {
        let node = WeatherNode {
            latitude: 12.0,
            longitude: 8.5,
            altitude_ft: 30_000.0,
            wind_dir_deg: 270.0,
            wind_speed_kt: 55.0,
            temperature_c: -40.0,
            pressure_hpa: 700.0,
            fusion_sources: 0b100,
            observed_ms: 1_000,
        };
        let s = sample_atmosphere(&[node], 12.0, 8.5, 30_000.0, 120.0, 1_000_000);
        assert_eq!(s.confidence, 0.0);
    }

    #[test]
    fn dust_layer_scales_with_haze_severity() {
        assert!(dust_layer_top_ft(Some(10_000.0), None).is_none());
        let mild = dust_layer_top_ft(Some(8_000.0), None).expect("layer");
        let severe = dust_layer_top_ft(Some(2_500.0), Some(36.0)).expect("layer");
        assert!(severe > mild);
        assert!(severe > 3_500.0);
    }
}
