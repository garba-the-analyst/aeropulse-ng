//! Replay control — loads pre-recorded files and streams them in real time.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::State;

use crate::hardware::replay::{read_any, SharedReplay, ReplayController};

#[tauri::command]
pub async fn replay_load(
    path: String,
    speed: Option<f64>,
    replay: State<'_, SharedReplay>,
) -> Result<String, String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("file not found: {}", path));
    }
    let samples = read_any(&p).map_err(|e| e.to_string())?;
    if samples.is_empty() {
        return Err("no replay samples found (unsupported format or empty)".to_string());
    }
    let count = samples.len();
    let ctrl = ReplayController::new(samples, speed.unwrap_or(1.0));
    *replay.lock().map_err(|e| e.to_string())? = Some(ctrl);
    Ok(format!("loaded {} samples from {}", count, path))
}

#[tauri::command]
pub fn replay_control(
    paused: Option<bool>,
    speed: Option<f64>,
    replay: State<'_, SharedReplay>,
) -> Result<String, String> {
    let mut guard = replay.lock().map_err(|e| e.to_string())?;
    if let Some(ctrl) = guard.as_mut() {
        if let Some(p) = paused {
            ctrl.set_paused(p);
        }
        if let Some(s) = speed {
            ctrl.set_speed(s);
        }
        Ok(format!(
            "replay {} speed {:.1} progress {:.0}%",
            if ctrl.is_finished() {
                "finished"
            } else if paused.unwrap_or(false) {
                "paused"
            } else {
                "playing"
            },
            speed.unwrap_or(1.0),
            ctrl.progress() * 100.0
        ))
    } else {
        Err("no replay loaded".to_string())
    }
}

#[tauri::command]
pub fn replay_status(replay: State<'_, SharedReplay>) -> Result<String, String> {
    let guard = replay.lock().map_err(|e| e.to_string())?;
    if let Some(ctrl) = guard.as_ref() {
        Ok(format!(
            "{:.0}% {}",
            ctrl.progress() * 100.0,
            if ctrl.is_finished() { "done" } else { "playing" }
        ))
    } else {
        Ok("idle".to_string())
    }
}

/// Tick helper — wired to engine `now_ms` (60 Hz `sim_ms`), not wall `Instant`.
pub fn drain_replay_at(replay: &SharedReplay, now_ms: u64) -> Vec<crate::hardware::replay::ReplaySample> {
    let mut guard = match replay.lock() {
        Ok(g) => g,
        Err(_) => return vec![],
    };
    if let Some(ctrl) = guard.as_mut() {
        ctrl.tick_at(now_ms)
    } else {
        vec![]
    }
}

/// Backward-compat wall-clock helper.
pub fn drain_replay(replay: &SharedReplay) -> Vec<crate::hardware::replay::ReplaySample> {
    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    drain_replay_at(replay, now_ms)
}
