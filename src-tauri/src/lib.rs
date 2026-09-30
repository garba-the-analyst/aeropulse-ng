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
use {
    engine::SurveillanceEngine,
    hardware::replay::{OsintConfig, SharedReplay},
    std::sync::{Arc, Mutex},
};

/// Boots the desktop host: dual-window workspace, IPC handlers and the
/// 60 Hz surveillance runtime.
#[cfg(feature = "tauri-host")]
pub fn run() {
    let engine = Arc::new(std::sync::Mutex::new(SurveillanceEngine::new(
        Default::default(),
    )));
    let replay: SharedReplay = Arc::new(Mutex::new(None));
    let osint_cfg = Arc::new(Mutex::new(OsintConfig::default()));
    let osint_cache = Arc::new(Mutex::new(Vec::new()));

    // Spawn OSINT poller
    {
        let cfg = osint_cfg.clone();
        let cache = osint_cache.clone();
        tauri::async_runtime::spawn(crate::commands::osint_cmd::osint_poller(cfg, cache));
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppShared::new(engine.clone()))
        .manage(replay.clone())
        .manage(osint_cfg.clone())
        .manage(osint_cache.clone())
        .setup({
            let engine = engine.clone();
            let replay = replay.clone();
            let osint_cache = osint_cache.clone();
            move |app| {
                runtime::start(app.handle().clone(), engine.clone(), replay.clone(), osint_cache.clone());
                Ok(())
            }
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
            commands::replay_cmd::replay_load,
            commands::replay_cmd::replay_control,
            commands::replay_cmd::replay_status,
            commands::osint_cmd::osint_set_config,
            commands::osint_cmd::osint_status,
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
