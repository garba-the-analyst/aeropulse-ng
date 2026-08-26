//! Dynamic geofencing: restricted-airspace polygons with vertical bands
//! and stateful breach transitions.

use std::collections::{HashMap, HashSet};

use crate::models::{Geofence, GeofenceBreach};

/// Point-in-polygon test via ray casting on the local tangent plane.
/// `polygon_deg` holds `[lat, lon]` vertices in winding order.
pub fn point_in_polygon(latitude: f64, longitude: f64, polygon_deg: &[[f64; 2]]) -> bool {
    if polygon_deg.len() < 3 {
        return false;
    }
    let mlat = 111_132.92 - 559.82 * (2.0 * latitude.to_radians()).cos();
    let mlon = 111_412.84 * latitude.to_radians().cos();

    let px = longitude * mlon;
    let py = latitude * mlat;

    let mut inside = false;
    let n = polygon_deg.len();
    let mut j = n - 1;
    for i in 0..n {
        let ay = polygon_deg[i][0] * mlat;
        let ax = polygon_deg[i][1] * mlon;
        let by = polygon_deg[j][0] * mlat;
        let bx = polygon_deg[j][1] * mlon;

        let crosses = (ay > py) != (by > py);
        if crosses {
            // Guard against division blow-up on horizontal edges.
            let dy = by - ay;
            let x_at_y = if dy.abs() < f64::EPSILON {
                ax.max(bx)
            } else {
                (bx - ax) * (py - ay) / dy + ax
            };
            if px < x_at_y {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Position fix consumed by the breach scanner.
#[derive(Debug, Clone)]
pub struct FenceFix {
    pub icao24: String,
    pub callsign: String,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_ft: f64,
    /// Engine receive time; copied into emitted breaches.
    pub now_ms: u64,
}

#[derive(Debug, Default)]
pub struct GeofenceMonitor {
    fences: Vec<Geofence>,
    occupancy: HashMap<String, HashSet<String>>,
}

impl GeofenceMonitor {
    pub fn with_fences(fences: Vec<Geofence>) -> Self {
        Self {
            fences,
            occupancy: HashMap::new(),
        }
    }

    pub fn add_fence(&mut self, fence: Geofence) {
        self.fences.push(fence);
    }

    pub fn set_active(&mut self, id: &str, active: bool) -> bool {
        match self.fences.iter_mut().find(|f| f.id == id) {
            Some(f) => {
                f.active = active;
                true
            }
            None => false,
        }
    }

    #[inline]
    pub fn fence_count(&self) -> usize {
        self.fences.len()
    }

    #[inline]
    pub fn fences(&self) -> &[Geofence] {
        &self.fences
    }

    /// Scans fixes against all active fences, emitting ENTRY events only.
    /// Continuous presence and exits re-arm future entries silently.
    pub fn scan(&mut self, fixes: &[FenceFix]) -> Vec<GeofenceBreach> {
        let mut breaches = Vec::new();

        for fence in self.fences.iter().filter(|f| f.active) {
            let occupied = self.occupancy.entry(fence.id.clone()).or_default();
            let mut still_inside: HashSet<String> = HashSet::new();

            for fx in fixes {
                let in_polygon =
                    point_in_polygon(fx.latitude, fx.longitude, &fence.vertices_deg);
                if !in_polygon {
                    continue;
                }
                let in_band =
                    fx.altitude_ft >= fence.floor_ft && fx.altitude_ft <= fence.ceiling_ft;

                if in_band {
                    still_inside.insert(fx.icao24.clone());
                    if !occupied.contains(&fx.icao24) {
                        breaches.push(GeofenceBreach {
                            fence_id: fence.id.clone(),
                            fence_name: fence.name.clone(),
                            icao24: fx.icao24.clone(),
                            callsign: fx.callsign.clone(),
                            entered_ms: fx.now_ms,
                        });
                    }
                }
            }

            *occupied = still_inside;
        }

        breaches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Geofence;

    /// Square around the airfield: 11.95..12.15 N, 8.42..8.62 E.
    fn kna_box() -> Geofence {
        Geofence {
            id: "R-81".into(),
            name: "DNKN CTR RESTRICTED".into(),
            vertices_deg: vec![
                [11.95, 8.42],
                [11.95, 8.62],
                [12.15, 8.62],
                [12.15, 8.42],
            ],
            floor_ft: 0.0,
            ceiling_ft: 8_000.0,
            active: true,
        }
    }

    fn fix(icao: &str, lat: f64, lon: f64, alt: f64) -> FenceFix {
        FenceFix {
            icao24: icao.into(),
            callsign: icao.into(),
            latitude: lat,
            longitude: lon,
            altitude_ft: alt,
            now_ms: 1_000,
        }
    }

    #[test]
    fn point_in_polygon_cardinals() {
        let poly = vec![[0.0, 0.0], [0.0, 10.0], [10.0, 10.0], [10.0, 0.0]];
        assert!(point_in_polygon(5.0, 5.0, &poly));
        assert!(!point_in_polygon(-5.0, 5.0, &poly));
        assert!(!point_in_polygon(15.0, 5.0, &poly));
        assert!(!point_in_polygon(5.0, 10.001, &poly));
        assert!(point_in_polygon(0.001, 9.999, &poly));
    }

    #[test]
    fn degenerate_polygons_are_false() {
        assert!(!point_in_polygon(1.0, 1.0, &[]));
        assert!(!point_in_polygon(1.0, 1.0, &[[1.0, 1.0]]));
    }

    #[test]
    fn entry_and_rearm_cycle() {
        let mut mon = GeofenceMonitor::with_fences(vec![kna_box()]);
        let outside_low = fix("T1", 12.50, 8.52, 5_000.0);

        assert!(mon.scan(&[outside_low]).is_empty());
        let entered = mon.scan(&[fix("T1", 12.05, 8.52, 5_000.0)]);
        assert_eq!(entered.len(), 1);
        assert_eq!(entered[0].fence_id, "R-81");
        assert_eq!(entered[0].icao24, "T1");

        // Continuous presence must not re-trigger.
        assert!(mon.scan(&[fix("T1", 12.05, 8.52, 5_000.0)]).is_empty());

        // Exit via overflight rearms the trigger.
        assert!(mon.scan(&[fix("T1", 12.05, 8.52, 12_500.0)]).is_empty());
        let reentered = mon.scan(&[fix("T1", 12.05, 8.52, 5_000.0)]);
        assert_eq!(reentered.len(), 1);
    }

    #[test]
    fn inactive_fence_is_transparent_until_enabled() {
        let mut fence = kna_box();
        fence.active = false;
        let mut mon = GeofenceMonitor::with_fences(vec![fence]);
        assert!(mon.scan(&[fix("T2", 12.05, 8.52, 3_000.0)]).is_empty());
        assert!(mon.set_active("R-81", true));
        assert_eq!(mon.scan(&[fix("T2", 12.05, 8.52, 3_000.0)]).len(), 1);
        assert!(!mon.set_active("MISSING", true));
    }

    #[test]
    fn two_fences_track_independently() {
        let mut northern = kna_box();
        northern.id = "R-91".into();
        northern.vertices_deg = vec![
            [12.20, 8.42],
            [12.20, 8.62],
            [12.40, 8.62],
            [12.40, 8.42],
        ];
        let mut mon = GeofenceMonitor::with_fences(vec![kna_box(), northern]);

        // Single fix sits inside R-81 only.
        let hits = mon.scan(&[fix("T3", 12.05, 8.52, 4_000.0)]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].fence_id, "R-81");

        // Move into R-91: exit R-81, enter R-91.
        let hits2 = mon.scan(&[fix("T3", 12.30, 8.52, 4_000.0)]);
        assert_eq!(hits2.len(), 1);
        assert_eq!(hits2[0].fence_id, "R-91");
    }
}
