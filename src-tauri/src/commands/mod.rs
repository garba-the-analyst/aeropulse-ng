//! Shared application state for Tauri IPC handlers.

pub mod defense_cmd;
pub mod telemetry_cmd;
pub mod weather_cmd;

use std::sync::{Arc, Mutex};

use crate::engine::SurveillanceEngine;

/// Engine handle shared between the 60 Hz runtime task and every IPC
/// command. Locks are held for sub-millisecond durations only.
#[derive(Clone)]
pub struct AppShared {
    pub engine: Arc<Mutex<SurveillanceEngine>>,
}

impl AppShared {
    pub fn new(engine: Arc<Mutex<SurveillanceEngine>>) -> Self {
        Self { engine }
    }
}
