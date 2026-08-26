//! Weather IPC: triple-fusion matrix and point sampling for Display 2.

use tauri::State;

use super::AppShared;
use crate::models::WeatherMatrix;

#[tauri::command]
pub fn get_weather_matrix(state: State<'_, AppShared>) -> Result<WeatherMatrix, String> {
    let engine = state.engine.lock().map_err(|e| e.to_string())?;
    Ok(engine.weather_matrix())
}

/// Atmospheric density / wind correction query at an arbitrary coordinate —
/// consumed by the dead-reckoning refinement overlay.
#[tauri::command]
pub fn sample_atmosphere(
    state: State<'_, AppShared>,
    latitude: f64,
    longitude: f64,
    altitude_ft: f64,
) -> Result<crate::weather_fusion::spatial_interp::AtmosphericSample, String> {
    let engine = state.engine.lock().map_err(|e| e.to_string())?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Ok(engine.sample_at(latitude, longitude, altitude_ft, now_ms))
}
