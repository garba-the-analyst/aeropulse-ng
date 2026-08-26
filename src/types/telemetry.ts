/**
 * AeroPulse-NG telemetry contracts — mirror of src-tauri/src/models.rs.
 * Field names follow the serde wire format exactly (snake_case).
 */

export type TrackClass = "civil" | "military" | "unidentified" | "anomaly";

export type AlertState =
  | "none"
  | "emergency"
  | "radio_failure"
  | "hijack"
  | "geofence_breach"
  | "dark_target";

export type VerticalTrend = "level" | "climb" | "descent";

export interface Track {
  icao24: string;
  callsign: string;
  class: TrackClass;
  alert: AlertState;
  latitude: number;
  longitude: number;
  altitude_ft: number;
  ground_speed_kt: number;
  course_deg: number;
  vertical_rate_fpm: number;
  vertical_trend: VerticalTrend;
  squawk: string;
  on_ground: boolean;
  last_update_ms: number;
  age_s: number;
  /** True while position is dead-reckoned rather than measured. */
  coasting: boolean;
  position_sigma_m: number;
  /** Projected 2-minute trajectory as [lat, lon] vertices. */
  leader_line: [number, number][];
}

export interface FlightDataBlock {
  icao24: string;
  line1: string;
  line2: string;
  line3: string;
  color: string;
}

export interface STCAAlert {
  id: string;
  icao_a: string;
  callsign_a: string;
  icao_b: string;
  callsign_b: string;
  min_horizontal_nm: number;
  min_vertical_ft: number;
  time_to_closest_s: number;
  triggered_ms: number;
}

export interface HardwareStatus {
  channel_id: string;
  role: string;
  serial_lock: string;
  frequency_hz: number;
  sample_rate_sps: number;
  gain_db: number;
  messages_per_second: number;
  ppm_error: number;
  online: boolean;
  usb_port: string | null;
}

export interface EngineStatus {
  hardware: HardwareStatus[];
  ekf_active: boolean;
  ekf_latency_us: number;
  stca_pairs_active: number;
  tracks_total: number;
  duckdb_writes_per_sec: number;
  sidecar_online: boolean;
  updated_ms: number;
}

export interface AnomalyNote {
  icao24: string;
  reason: string;
}

export interface EngineSnapshot {
  tracks: Track[];
  flight_data_blocks: FlightDataBlock[];
  stca_alerts: STCAAlert[];
  geofence_breaches: {
    fence_id: string;
    fence_name: string;
    icao24: string;
    callsign: string;
    entered_ms: number;
  }[];
  anomalies: AnomalyNote[];
  weather: import("./weather").WeatherMatrix;
  status: EngineStatus;
}

/** HF-STD-010A / MIL-STD-2525D symbology palette. */
export const SYMBOLOGY = {
  background: "#0B0E14",
  grid: "#2A3241",
  civil: "#00FF66",
  military: "#7C4DFF",
  unknown: "#FFB300",
  alert: "#FF1744",
} as const;

export function trackColor(t: Track): string {
  if (t.alert !== "none") return SYMBOLOGY.alert;
  switch (t.class) {
    case "civil":
      return SYMBOLOGY.civil;
    case "military":
      return SYMBOLOGY.military;
    default:
      return SYMBOLOGY.unknown;
  }
}
