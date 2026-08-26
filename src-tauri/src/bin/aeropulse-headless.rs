//! Headless surveillance console — runs the complete Rust engine against
//! the synthetic RF feed and renders the tactical picture as ANSI text.
//!
//! Usage:  cargo run --no-default-features --bin aeropulse-headless
//! Keys are not read; press Ctrl-C to exit.

use std::{thread, time::Duration};

use aeropulse_ng::engine::{EngineConfig, SurveillanceEngine};
use aeropulse_ng::hardware::simulator::{Simulator, SimulatorConfig};
use aeropulse_ng::models::{AlertState, TrackClass};

const CLEAR: &str = "\x1b[2J\x1b[H";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const VIOLET: &str = "\x1b[35m";
const AMBER: &str = "\x1b[33m";
const RED: &str = "\x1b[91m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

fn class_color(class: TrackClass, alert: AlertState) -> &'static str {
    if alert != AlertState::None {
        return RED;
    }
    match class {
        TrackClass::Civil => GREEN,
        TrackClass::Military => VIOLET,
        _ => AMBER,
    }
}

fn main() {
    let mut engine = SurveillanceEngine::new(EngineConfig::default());
    let mut sim = Simulator::new(SimulatorConfig::default());
    let mut sim_ms: u64 = 1_700_000_000_000;
    let start = std::time::Instant::now();

    println!("{CLEAR}");

    loop {
        let events = sim.tick(0.25, sim_ms);
        engine.ingest(&events, sim_ms);
        let snap = engine.tick(sim_ms);
        sim_ms += 250;

        // Refresh the console four times per simulated second.
        if sim_ms % 1000 < 250 {
            let mut out = String::from(CLEAR);
            let elapsed = start.elapsed().as_secs_f64();

            out.push_str(&format!(
                "{BOLD}AeroPulse-NG v2.4 — HEADLESS SURVEILLANCE CONSOLE{RESET} {DIM}(realtime {:.0}s · DNKN sector){RESET}\n",
                elapsed
            ));
            out.push_str(&format!(
                "{DIM}──────────────────────────────────────────────────────────────────────────────{RESET}\n"
            ));

            out.push_str(&format!(
                "{BOLD}{:<9}{:<8}{:>9}{:>10}{:>8}{:>8}{:>5}{:>6}  {:<8}{RESET}\n",
                "CALLSIGN", "ICAO24", "LAT", "LON", "ALT-M", "KM/H", "CRS", "SQK", "STATE"
            ));

            for t in &snap.tracks {
                let color = class_color(t.class, t.alert);
                let state = match t.alert {
                    AlertState::Emergency => "EMERGENCY".to_string(),
                    AlertState::RadioFailure => "RADIO FAIL".to_string(),
                    AlertState::Hijack => "HIJACK".to_string(),
                    AlertState::GeofenceBreach => "GEO BREACH".to_string(),
                    AlertState::DarkTarget => "DARK TGT".to_string(),
                    AlertState::None => {
                        if t.coasting {
                            format!("{DIM}COASTING{RESET}")
                        } else {
                            "NOMINAL".to_string()
                        }
                    }
                };
                out.push_str(&format!(
                    "{}{:<9}{:<8}{:>9.3}{:>9.3}{:>6}{:>6.0}{:>5.0}{:>6}  {:<8}{}\n",
                    color,
                    t.callsign,
                    t.icao24,
                    t.latitude,
                    t.longitude,
                    (t.altitude_ft * 0.304_8).round() as i64,
                    t.ground_speed_kt * 1.852,
                    t.course_deg,
                    t.squawk,
                    state,
                    RESET
                ));
            }

            out.push('\n');
            for a in &snap.stca_alerts {
                out.push_str(&format!(
                    "{RED}{BOLD}⚠ STCA{RESET} {RED}{} ↔ {} · predicted {:.1} NM / {:.0} ft · CPA T-{:.0}s{RESET}\n",
                    a.callsign_a,
                    a.callsign_b,
                    a.min_horizontal_nm * 1.852,
                    a.min_vertical_ft * 0.304_8,
                    a.time_to_closest_s.round()
                ));
            }
            for b in &snap.geofence_breaches {
                out.push_str(&format!(
                    "{AMBER}⚑ GEOFENCE{RESET} {AMBER}{} entered {} ({}){RESET}\n",
                    b.callsign, b.fence_name, b.fence_id
                ));
            }
            for note in &snap.anomalies {
                out.push_str(&format!(
                    "{AMBER}? ANOMALY{RESET} {AMBER}{} · {}{RESET}\n",
                    note.icao24, note.reason
                ));
            }

            out.push_str(&format!(
                "\n{DIM}── WEATHER ── QNH {:.1} hPa · wind {:0>3}°/{:.0} kt · vis {:.1} km · dust top {:.0} m\n",
                snap.weather
                    .surface_qnh_hpa
                    .map(|v| format!("{v:.1}"))
                    .unwrap_or_else(|| "--".into()),
                snap.weather
                    .surface_wind_dir_deg
                    .map(|v| v.round() as u32)
                    .unwrap_or(0),
                snap.weather
                    .surface_wind_speed_kt
                    .map(|v| v.round() as u32)
                    .unwrap_or(0),
                snap.weather
                    .visibility_m
                    .map(|v| format!("{:.1}", v / 1000.0))
                    .unwrap_or_else(|| "--".into()),
                snap.weather
                    .dust_layer_top_ft
                    .map(|v| v * 0.304_8)
                    .unwrap_or(0.0),
            ));
            out.push_str(&format!(
                "── ENGINE ── EKF {} µs · tracks {} · STCA pairs {} · upper-air nodes {}{RESET}\n",
                snap.status.ekf_latency_us,
                snap.tracks.len(),
                snap.stca_alerts.len(),
                snap.weather.nodes.len(),
            ));

            print!("{out}");
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }

        thread::sleep(Duration::from_millis(250));
    }
}
