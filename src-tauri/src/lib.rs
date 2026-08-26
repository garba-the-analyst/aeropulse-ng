//! AeroPulse-NG tactical surveillance engine core.

pub mod defense;
pub mod engine;
pub mod hardware;
pub mod kinematics;
pub mod models;
pub mod sidecar;
pub mod weather_fusion;

#[cfg(feature = "tauri-host")]
pub mod commands;
#[cfg(feature = "tauri-host")]
mod runtime;

#[cfg(feature = "tauri-host")]
use commands::AppShared;
#[cfg(feature = "tauri-host")]
use {engine::SurveillanceEngine, std::sync::Arc};

/// Boots the desktop host: dual-window workspace, IPC handlers and the
/// 60 Hz surveillance runtime.
#[cfg(feature = "tauri-host")]
pub fn run() {
    let engine = Arc::new(std::sync::Mutex::new(SurveillanceEngine::new(
        Default::default(),
    )));

    tauri::Builder::default()
        .manage(AppShared::new(engine.clone()))
        .setup(move |app| {
            runtime::start(app.handle().clone(), engine.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::telemetry_cmd::get_latest_snapshot,
            commands::telemetry_cmd::get_engine_status,
            commands::telemetry_cmd::get_hardware_status,
            commands::telemetry_cmd::trigger_emergency_override,
            commands::telemetry_cmd::cancel_emergency_override,
            commands::telemetry_cmd::tracks_by_alert,
            commands::weather_cmd::get_weather_matrix,
            commands::weather_cmd::sample_atmosphere,
            commands::defense_cmd::list_geofences,
            commands::defense_cmd::create_geofence,
            commands::defense_cmd::toggle_geofence,
            commands::defense_cmd::compute_intercept,
        ])
        .run(tauri::generate_context!())
        .expect("AeroPulse-NG runtime terminated unexpectedly");
}

/// Engine-only builds (no webkit system libraries) still expose a runnable
/// binary for headless regression benches.
#[cfg(not(feature = "tauri-host"))]
pub fn run() {
    println!("AeroPulse-NG engine core built without the tauri-host feature.");
    println!("Run `cargo test --no-default-features` to execute the suite.");
}
