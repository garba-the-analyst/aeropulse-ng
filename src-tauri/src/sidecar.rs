//! Python sidecar process bridge (NDJSON over stdio).
//!
//! The sidecar hosts the DuckDB persistence layer and the RS-485 AWOS
//! serial reader. This module owns the child process lifecycle and the
//! line protocol; every message in either direction is a single-line JSON
//! object terminated by `\n`.
//!
//! Rust → Python:
//!   {"cmd":"init","db_path":"..."}
//!   {"cmd":"log_tracks","tracks":[...]}
//!   {"cmd":"shutdown"}
//!
//! Python → Rust:
//!   {"ev":"ready"}
//!   {"ev":"awos","qnh_hpa":1013.2,...}
//!   {"ev":"db_ack","rows":120}
//!
//! Degradation contract: if the interpreter or script cannot be spawned,
//! `spawn` returns `None` and the engine continues on the synthetic AWOS
//! feed — an air-gapped station must never hard-fail on optional telemetry.

use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc;

use crate::models::AwosObservation;
use crate::models::Track;

#[derive(Debug, Clone)]
pub struct SidecarConfig {
    pub enabled: bool,
    pub python_bin: String,
    pub script_path: String,
    pub db_path: String,
}

impl Default for SidecarConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            python_bin: "python3".into(),
            script_path: "python-sidecar/main.py".into(),
            db_path: "aeropulse.duckdb".into(),
        }
    }
}

/// Messages routed from the sidecar reader into the engine loop.
#[derive(Debug, Clone)]
pub enum SidecarEvent {
    Ready,
    Awos(AwosObservation),
    DbAck { rows: u64 },
    Failed(String),
}

// ---------------------------------------------------------------------------
// Line codec (pure functions — unit tested without any process I/O)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct InitCmd<'a> {
    cmd: &'static str,
    db_path: &'a str,
}

#[derive(Debug, Serialize)]
struct LogTracksCmd<'a> {
    cmd: &'static str,
    tracks: &'a [Track],
}

/// Encodes the init handshake.
pub fn encode_init(db_path: &str) -> String {
    serde_json::to_string(&InitCmd { cmd: "init", db_path }).expect("serialisable") + "\n"
}

/// Encodes a track batch for persistence.
pub fn encode_log_tracks(tracks: &[Track]) -> String {
    serde_json::to_string(&LogTracksCmd { cmd: "log_tracks", tracks }).expect("serialisable")
        + "\n"
}

/// Encodes the graceful shutdown request.
pub fn encode_shutdown() -> String {
    "{\"cmd\":\"shutdown\"}\n".to_string()
}

/// Inbound sidecar frames.
#[derive(Debug, Deserialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub enum SidecarFrame {
    Ready,
    Awos(AwosObservation),
    DbAck { rows: u64 },
}

/// Parses one inbound NDJSON line.
pub fn parse_line(line: &str) -> Option<SidecarEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    match serde_json::from_str::<SidecarFrame>(trimmed) {
        Ok(SidecarFrame::Ready) => Some(SidecarEvent::Ready),
        Ok(SidecarFrame::Awos(obs)) => Some(SidecarEvent::Awos(obs)),
        Ok(SidecarFrame::DbAck { rows }) => Some(SidecarEvent::DbAck { rows }),
        Err(e) => Some(SidecarEvent::Failed(format!("bad frame: {e}"))),
    }
}

// ---------------------------------------------------------------------------
// Process management
// ---------------------------------------------------------------------------

/// Handle to a running sidecar. Dropping it kills the child.
pub struct SidecarHandle {
    child: Child,
    stdin: mpsc::Sender<String>,
}

impl SidecarHandle {
    /// Queues one outbound frame; drops silently on closed pipe.
    pub async fn send_raw(&self, line: String) {
        let _ = self.stdin.send(line).await;
    }

    pub async fn shutdown(mut self) {
        let _ = self.stdin.send(encode_shutdown()).await;
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let _ = self.child.kill().await;
    }
}

/// Attempts to spawn the sidecar. Returns `None` when disabled or when the
/// interpreter/script is unavailable (offline bench machines without the
/// venv still boot the full surveillance stack).
pub async fn spawn(cfg: &SidecarConfig) -> Option<(SidecarHandle, mpsc::Receiver<SidecarEvent>)> {
    if !cfg.enabled {
        return None;
    }

    let mut child = match Command::new(&cfg.python_bin)
        .arg(&cfg.script_path)
        .arg("--db")
        .arg(&cfg.db_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[sidecar] spawn failed ({e}) — continuing without persistence");
            return None;
        }
    };

    let stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;

    let (line_tx, mut line_rx) = mpsc::channel::<String>(256);
    let (event_tx, event_rx) = mpsc::channel::<SidecarEvent>(256);
    let (write_tx, mut write_rx) = mpsc::channel::<String>(256);

    // Reader task: stdout lines -> parsed events.
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut buf = String::new();
        loop {
            buf.clear();
            match reader.read_line(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if let Some(ev) = parse_line(&buf) {
                        if event_tx.send(ev).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });

    // Writer task: serialises access to the child's stdin.
    let mut pinned_stdin: Option<ChildStdin> = Some(stdin);
    tokio::spawn(async move {
        while let Some(line) = write_rx.recv().await {
            match pinned_stdin.as_mut() {
                Some(pipe) => {
                    if pipe.write_all(line.as_bytes()).await.is_err()
                        || pipe.flush().await.is_err()
                    {
                        break;
                    }
                }
                None => break,
            }
        }
    });

    // Forwarding pump: keeps `line_tx` alive only while handle lives.
    tokio::spawn(async move {
        while let Some(line) = line_rx.recv().await {
            if write_tx.send(line).await.is_err() {
                break;
            }
        }
    });

    Some((
        SidecarHandle {
            child,
            stdin: line_tx,
        },
        event_rx,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_track() -> Track {
        serde_json::from_str(
            r#"{"icao24":"342157","callsign":"VL604","class":"civil","alert":"none",
            "latitude":12.35,"longitude":8.13,"altitude_ft":32000,"ground_speed_kt":445,
            "course_deg":205,"vertical_rate_fpm":0,"vertical_trend":"level","squawk":"7700",
            "on_ground":false,"last_update_ms":1700000000000,"age_s":0,"coasting":false,
            "position_sigma_m":41,"leader_line":[[12.35,8.13]]}"#,
        )
        .expect("track fixture")
    }

    #[test]
    fn init_frame_encodes() {
        let s = encode_init("/tmp/a.duckdb");
        assert!(s.starts_with("{\"cmd\":\"init\",\"db_path\":\"/tmp/a.duckdb\"}"));
        assert!(s.ends_with('\n'));
    }

    #[test]
    fn track_batch_roundtrips_through_json() {
        let tracks = vec![sample_track()];
        let line = encode_log_tracks(&tracks);
        let v: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v["cmd"], "log_tracks");
        assert_eq!(v["tracks"][0]["callsign"], "VL604");
        assert_eq!(v["tracks"][0]["squawk"], "7700");
    }

    #[test]
    fn inbound_frames_parse() {
        assert!(matches!(
            parse_line("{\"ev\":\"ready\"}"),
            Some(SidecarEvent::Ready)
        ));
        let awos = parse_line(
            r#"{"ev":"awos","qnh_hpa":1013.2,"wind_dir_deg":180,"wind_speed_kt":12,
               "wind_gust_kt":16,"temperature_c":33.5,"dewpoint_c":24.1,
               "visibility_m":8000,"observed_ms":1700000000000}"#,
        )
        .expect("awos frame");
        match awos {
            SidecarEvent::Awos(o) => assert_eq!(o.qnh_hpa, 1013.2),
            other => panic!("wrong event {:?}", other),
        }
        match parse_line("{\"ev\":\"db_ack\",\"rows\":42}") {
            Some(SidecarEvent::DbAck { rows }) => assert_eq!(rows, 42),
            other => panic!("wrong event {:?}", other),
        }
    }

    #[test]
    fn garbage_lines_yield_failed_not_panic() {
        match parse_line("<<<not json>>>") {
            Some(SidecarEvent::Failed(msg)) => assert!(msg.contains("bad frame")),
            other => panic!("expected failure event, got {:?}", other),
        }
        assert!(parse_line("").is_none());
    }

    #[tokio::test]
    async fn missing_interpreter_degrades_to_none() {
        let cfg = SidecarConfig {
            python_bin: "/nonexistent/interp".into(),
            ..Default::default()
        };
        assert!(spawn(&cfg).await.is_none());
    }

    #[tokio::test]
    async fn disabled_sidecar_spawns_nothing() {
        let cfg = SidecarConfig {
            enabled: false,
            ..Default::default()
        };
        assert!(spawn(&cfg).await.is_none());
    }
}
