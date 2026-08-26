//! Geodetic <-> tangent-plane transforms and kinematic trajectory projection.
//!
//! The surveillance engine works in a local East-North-Up frame anchored at
//! the radar site; this module provides the WGS-84 conversions with series
//! expansions accurate to centimetres across the 250 NM operating volume, plus
//! the dead-reckoning extrapolator used during transponder dropouts.

/// Anchor point of the local ENU frame (site reference).
#[derive(Debug, Clone, Copy)]
pub struct SiteOrigin {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub altitude_ft: f64,
    m_per_deg_lat: f64,
    m_per_deg_lon: f64,
}

impl SiteOrigin {
    pub fn new(latitude_deg: f64, longitude_deg: f64, altitude_ft: f64) -> Self {
        let lat = latitude_deg.to_radians();
        let m_per_deg_lat =
            111_132.92 - 559.82 * (2.0 * lat).cos() + 1.175 * (4.0 * lat).cos() - 0.0023 * (6.0 * lat).cos();
        let m_per_deg_lon =
            111_412.84 * lat.cos() - 93.5 * (3.0 * lat).cos() + 0.118 * (5.0 * lat).cos();
        Self {
            latitude_deg,
            longitude_deg,
            altitude_ft,
            m_per_deg_lat,
            m_per_deg_lon,
        }
    }

    #[inline]
    pub fn to_local(&self, latitude_deg: f64, longitude_deg: f64) -> (f64, f64) {
        (
            (longitude_deg - self.longitude_deg) * self.m_per_deg_lon,
            (latitude_deg - self.latitude_deg) * self.m_per_deg_lat,
        )
    }

    #[inline]
    pub fn from_local(&self, x_m: f64, y_m: f64) -> (f64, f64) {
        (
            self.latitude_deg + y_m / self.m_per_deg_lat,
            self.longitude_deg + x_m / self.m_per_deg_lon,
        )
    }

    /// Great-circle bearing from site to a geodetic point, degrees true.
    pub fn bearing_to(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let (x, y) = self.to_local(latitude_deg, longitude_deg);
        let b = x.atan2(y).to_degrees();
        if b < 0.0 { b + 360.0 } else { b }
    }

    /// Approximate ground range in nautical miles from site.
    pub fn range_nm_to(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let (x, y) = self.to_local(latitude_deg, longitude_deg);
        ((x * x + y * y).sqrt()) / 1852.0
    }
}

pub const NM_TO_M: f64 = 1852.0;
pub const KT_TO_MS: f64 = 0.514444;

/// Projects a 2-minute leader line for display and conflict lookahead.
///
/// Samples the constant-velocity path at `step_s` intervals returning
/// geodetic `[lat, lon]` vertices including the origin as vertex zero.
pub fn project_leader_line(
    origin: &SiteOrigin,
    x_m: f64,
    y_m: f64,
    vx_ms: f64,
    vy_ms: f64,
    lookahead_s: f64,
    step_s: f64,
) -> Vec<[f64; 2]> {
    let steps = (lookahead_s / step_s).ceil() as usize;
    let mut line = Vec::with_capacity(steps + 1);
    let (lat0, lon0) = origin.from_local(x_m, y_m);
    line.push([lat0, lon0]);
    for i in 1..=steps {
        let t = i as f64 * step_s;
        let (lat, lon) = origin.from_local(x_m + vx_ms * t, y_m + vy_ms * t);
        line.push([lat, lon]);
    }
    line
}

/// Dead-reckoned position after `elapsed_s` of signal loss.
#[inline]
pub fn coast_position(
    x_m: f64,
    y_m: f64,
    z_ft: f64,
    vx_ms: f64,
    vy_ms: f64,
    vz_fps: f64,
    elapsed_s: f64,
) -> (f64, f64, f64) {
    (
        x_m + vx_ms * elapsed_s,
        y_m + vy_ms * elapsed_s,
        z_ft + vz_fps * elapsed_s,
    )
}

/// Time and location of closest approach between two constant-velocity
/// tracks. Returns `(time_s, separation_m)`; negative time means the closest
/// approach already happened.
pub fn closest_approach(
    p_a: [f64; 2],
    v_a: [f64; 2],
    p_b: [f64; 2],
    v_b: [f64; 2],
) -> (f64, f64) {
    let dp = [p_b[0] - p_a[0], p_b[1] - p_a[1]];
    let dv = [v_b[0] - v_a[0], v_b[1] - v_a[1]];
    let dv2 = dv[0] * dv[0] + dv[1] * dv[1];
    let t = if dv2 < 1e-9 {
        0.0
    } else {
        -((dp[0] * dv[0] + dp[1] * dv[1]) / dv2)
    };
    let tc = t.clamp(0.0, f64::MAX);
    let sx = dp[0] + dv[0] * tc;
    let sy = dp[1] + dv[1] * tc;
    (t, (sx * sx + sy * sy).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SITE: (f64, f64) = (12.047_6, 8.524_1); // DNKN Kano

    #[test]
    fn roundtrip_conversions() {
        let site = SiteOrigin::new(SITE.0, SITE.1, 1560.0);
        let (lat, lon) = site.from_local(37_040.0, 18_520.0);
        let (x, y) = site.to_local(lat, lon);
        assert!((x - 37_040.0).abs() < 1.0 && (y - 18_520.0).abs() < 1.0);
    }

    #[test]
    fn bearing_cardinals() {
        let site = SiteOrigin::new(SITE.0, SITE.1, 0.0);
        assert!((site.bearing_to(SITE.0 + 0.1, SITE.1) - 0.0).abs() < 0.5);
        assert!((site.bearing_to(SITE.0, SITE.1 + 0.1) - 90.0).abs() < 0.5);
        assert!((site.bearing_to(SITE.0 - 0.1, SITE.1) - 180.0).abs() < 0.5);
    }

    #[test]
    fn leader_line_has_lookahead_extent() {
        let site = SiteOrigin::new(SITE.0, SITE.1, 0.0);
        let line = project_leader_line(&site, 0.0, 0.0, 200.0, 0.0, 120.0, 10.0);
        assert_eq!(line.len(), 13);
        let last = site.to_local(line[12][0], line[12][1]);
        assert!((last.0 - 24_000.0).abs() < 50.0);
    }

    #[test]
    fn head_on_closest_approach() {
        // Two aircraft closing at combined 400 kt on the same line.
        let (t, sep) = closest_approach([0.0, 0.0], [100.0, 0.0], [40_000.0, 0.0], [-100.0, 0.0]);
        assert!((t - 200.0).abs() < 1.0);
        assert!(sep.abs() < 1.0);
    }
}
