//! Host runtime: owns the 60 Hz fusion loop, sidecar pump and window event
//! emission. Compiled only under the `tauri-host` feature.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::engine::SurveillanceEngine;
use crate::hardware::simulator::{IngestEvent, Simulator, SimulatorConfig};
use crate::sidecar::{encode_log_tracks, spawn as spawn_sidecar, SidecarConfig, SidecarEvent, SidecarHandle};

/// Telemetry bus event names shared with the TypeScript layer.
pub const EVT_SNAPSHOT: &str = "telemetry://snapshot";

/// Launches the surveillance loop against the provided windows.
pub fn start(app: AppHandle, engine: Arc<Mutex<SurveillanceEngine>>) {
    tauri::async_runtime::spawn(async move {
        let mut sim = Simulator::new(SimulatorConfig::default());
        let (sidecar, mut sidecar_rx): (
            Option<SidecarHandle>,
            Option<mpsc::Receiver<SidecarEvent>>,
        ) = match spawn_sidecar(&SidecarConfig::default()).await {
            Some((handle, rx)) => (Some(handle), Some(rx)),
            None => (None, None),
        };

        if let Some(s) = sidecar.as_ref() {
            use crate::sidecar::encode_init;
            s.send_raw(encode_init("aeropulse.duckdb")).await;
        }

        // Wall-clock anchor for the simulated RF timeline.
        let t0_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut sim_ms = t0_ms;
        let mut last_db_push_ms = t0_ms;

        let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / 60.0));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                maybe_ev = async {
                    match sidecar_rx.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending::<Option<SidecarEvent>>().await,
                    }
                } => {
                    match maybe_ev {
                        Some(SidecarEvent::Ready) => {
                            if let Ok(mut e)=engine.lock(){ e.set_sidecar_online(true); }
                        }
                        Some(SidecarEvent::Awos(obs)) => {
                            if let Ok(mut e)=engine.lock(){ e.ingest_awos(&obs); }
                        }
                        Some(SidecarEvent::DbAck{rows}) => {
                            if let Ok(mut e)=engine.lock(){ e.report_duckdb_rate(rows as f64); }
                        }
                        Some(SidecarEvent::Failed(msg)) => {
                            eprintln!("[sidecar] {msg}");
                        }
                        None => {/* channel closed; keep looping */}
                    }
                }
                _ = ticker.tick() => {
                    let events: Vec<IngestEvent> = sim.tick(1.0/60.0, sim_ms);
                    sim_ms = sim_ms.saturating_add(1000/60);

                    let snapshot = {
                        let mut e = match engine.lock() { Ok(g)=>g, Err(p)=>p.into_inner() };
                        e.ingest(&events, sim_ms);
                        e.tick(sim_ms)
                    };

                    if let Err(e) = app.emit(EVT_SNAPSHOT, &*snapshot) {
                        eprintln!("[runtime] emit failed: {e}");
                    }

                    if sim_ms.saturating_sub(last_db_push_ms) >= 2_000 {
                        last_db_push_ms = sim_ms;
                        if let Some(s)=sidecar.as_ref(){
                            s.send_raw(encode_log_tracks(&snapshot.tracks)).await;
                        }
                    }
                }
            }
        }
    });
}
