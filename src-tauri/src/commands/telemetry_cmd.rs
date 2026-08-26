//! Telemetry IPC: snapshot access, status cluster and master-bar actions.

use tauri::State;

use super::AppShared;
use crate::models::{EngineSnapshot, HardwareStatus};
use crate::models::AlertState;

/// Full display-layer snapshot — the pull fallback alongside the 60 Hz
/// `telemetry://snapshot` event stream.
#[tauri::command]
pub fn get_latest_snapshot(state: State<'_, AppShared>) -> Result<EngineSnapshot, String> {
    let engine = state.engine.lock().map_err(|e| e.to_string())?;
    Ok((*engine.latest()).clone())
}

/// Screen-0 status cluster payload.
#[tauri::command]
pub fn get_engine_status(
    state: State<'_, AppShared>,
) -> Result<crate::models::EngineStatus, String> {
    let engine = state.engine.lock().map_err(|e| e.to_string())?;
    Ok(engine.latest().status.clone())
}

/// SDR channel health for the diagnostics panel.
#[tauri::command]
pub fn get_hardware_status(state: State<'_, AppShared>) -> Result<Vec<HardwareStatus>, String> {
    let engine = state.engine.lock().map_err(|e| e.to_string())?;
    Ok(engine.latest().status.hardware.clone())
}

/// Emergency override switch (Screen 0 right cluster): flags every active
/// track emergency-red for `duration_s`.
#[tauri::command]
pub fn trigger_emergency_override(
    state: State<'_, AppShared>,
    duration_s: u64,
) -> Result<String, String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    let until = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
        + duration_s.saturating_mul(1000);
    engine.arm_emergency_override(until);
    Ok(format!("EMERGENCY OVERRIDE ARMED FOR {duration_s}s"))
}

/// Clears an armed override ahead of schedule.
#[tauri::command]
pub fn cancel_emergency_override(state: State<'_, AppShared>) -> Result<(), String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    engine.arm_emergency_override(0);
    Ok(())
}

/// Convenience classification probe used by the FDS bay filters.
#[tauri::command]
pub fn tracks_by_alert(
    state: State<'_, AppShared>,
    alert: AlertState,
) -> Result<Vec<String>, String> {
    let engine = state.engine.lock().map_err(|e| e.to_string())?;
    Ok(engine
        .latest()
        .tracks
        .iter()
        .filter(|t| t.alert == alert)
        .map(|t| t.icao24.clone())
        .collect())
}
