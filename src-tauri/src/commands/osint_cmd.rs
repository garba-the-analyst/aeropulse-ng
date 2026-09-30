//! OSINT fetcher — online alternative to RTL-SDR hardware.
//! Polls OpenSky Network (and later adsb.lol/ADS-B Exchange) for decoded state vectors
//! and injects them as ReplaySample → Track. **Online** — not air-gapped.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::State;

use crate::hardware::replay::{fetch_opensky, OsintConfig, ReplaySample};

pub type SharedOsint = Arc<Mutex<OsintConfig>>;
pub type SharedOsintCache = Arc<Mutex<Vec<ReplaySample>>>;

#[tauri::command]
pub fn osint_set_config(cfg: OsintConfig, osint: State<'_, SharedOsint>) -> Result<String, String> {
    *osint.lock().map_err(|e| e.to_string())? = cfg.clone();
    Ok(format!("OSINT {} source={} bbox={:?}", if cfg.enabled { "enabled" } else { "disabled" }, cfg.source, cfg.bbox))
}

#[tauri::command]
pub fn osint_status(osint: State<'_, SharedOsint>) -> Result<OsintConfig, String> {
    Ok(osint.lock().map_err(|e| e.to_string())?.clone())
}

/// Background poller — spawn once at startup. When enabled, fetches every poll_interval_s
/// and pushes into the shared cache for the engine tick to drain.
/// Air-gap default: `fetch_opensky` without `osint-live` returns Err, which is
/// logged and the cache is left untouched (offline, deterministic).
pub async fn osint_poller(osint: SharedOsint, cache: SharedOsintCache) {
    loop {
        let (enabled, bbox, interval) = {
            match osint.lock() {
                Ok(cfg) => (cfg.enabled, cfg.bbox, cfg.poll_interval_s),
                Err(p) => {
                    let cfg = p.into_inner();
                    (cfg.enabled, cfg.bbox, cfg.poll_interval_s)
                }
            }
        };
        if enabled {
            match fetch_opensky(bbox).await {
                Ok(samples) => {
                    match cache.lock() {
                        Ok(mut guard) => *guard = samples,
                        Err(p) => *p.into_inner() = samples,
                    }
                }
                Err(e) => {
                    eprintln!("[osint] fetch failed: {}", e);
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(interval.max(5))).await;
    }
}

pub fn drain_osint(cache: &SharedOsintCache) -> Vec<ReplaySample> {
    let mut guard = match cache.lock() {
        Ok(g) => g,
        Err(_) => return vec![],
    };
    if guard.is_empty() {
        return vec![];
    }
    std::mem::take(&mut *guard)
}
