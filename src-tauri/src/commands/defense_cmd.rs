//! Defense IPC: geofence administration and the tactical intercept solver.

use tauri::State;

use super::AppShared;
use crate::defense::intercept_math::{solve_intercept, InterceptInput};
use crate::kinematics::dead_reckoning::{KT_TO_MS, SiteOrigin};
use crate::models::{Geofence, InterceptSolution, Track};

#[tauri::command]
pub fn list_geofences(state: State<'_, AppShared>) -> Result<Vec<Geofence>, String> {
    let engine = state.engine.lock().map_err(|e| e.to_string())?;
    Ok(engine.list_geofences())
}

/// Creates a restricted-airspace polygon. Vertices are `[lat, lon]` pairs
/// in winding order; the server assigns the identifier.
#[tauri::command]
pub fn create_geofence(
    state: State<'_, AppShared>,
    name: String,
    vertices_deg: Vec<[f64; 2]>,
    floor_ft: f64,
    ceiling_ft: f64,
) -> Result<String, String> {
    if vertices_deg.len() < 3 {
        return Err("geofence requires at least three vertices".into());
    }
    let id = format!(
        "GF-{:04}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| (d.as_millis() % 10_000) as u16)
            .unwrap_or(0)
    );
    let fence = Geofence {
        id: id.clone(),
        name,
        vertices_deg,
        floor_ft,
        ceiling_ft,
        active: true,
    };
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    engine.add_geofence(fence);
    Ok(id)
}

#[tauri::command]
pub fn toggle_geofence(
    state: State<'_, AppShared>,
    id: String,
    active: bool,
) -> Result<bool, String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    Ok(engine.set_fence_active(&id, active))
}

/// Solves a lead-pursuit intercept between two live tracks using their
/// current EKF-filtered states.
#[tauri::command]
pub fn compute_intercept(
    state: State<'_, AppShared>,
    target_icao24: String,
    interceptor_icao24: String,
    interceptor_speed_kt: f64,
) -> Result<InterceptSolution, String> {
    let engine = state.engine.lock().map_err(|e| e.to_string())?;
    let snap = engine.latest();
    let site: SiteOrigin = engine.site_origin();

    let find = |icao: &str| -> Result<Track, String> {
        snap.tracks
            .iter()
            .find(|t| t.icao24 == icao)
            .cloned()
            .ok_or_else(|| format!("track {icao} not in current picture"))
    };
    let target = find(&target_icao24)?;
    let interceptor = find(&interceptor_icao24)?;

    let (tx_m, ty_m) = site.to_local(target.latitude, target.longitude);
    let (ix_m, iy_m) = site.to_local(interceptor.latitude, interceptor.longitude);
    let iv_rad = interceptor.course_deg.to_radians();
    let tv_rad = target.course_deg.to_radians();
    let i_spd = interceptor.ground_speed_kt * KT_TO_MS;
    let t_spd = target.ground_speed_kt * KT_TO_MS;

    let input = InterceptInput {
        ix_m,
        iy_m,
        ivx_ms: i_spd * iv_rad.sin(),
        ivy_ms: i_spd * iv_rad.cos(),
        tx_m,
        ty_m,
        tvx_ms: t_spd * tv_rad.sin(),
        tvy_ms: t_spd * tv_rad.cos(),
        interceptor_speed_kt: if interceptor_speed_kt > 60.0 {
            interceptor_speed_kt
        } else {
            480.0
        },
        max_time_s: 900.0,
    };

    let mut solution = solve_intercept(&input);
    solution.target_icao24 = target_icao24;
    solution.interceptor_icao24 = interceptor_icao24;
    Ok(solution)
}
