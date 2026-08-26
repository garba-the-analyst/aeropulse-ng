/**
 * Module-level telemetry store.
 *
 * The 60 Hz snapshot stream lands here once; canvas renderers read the ref
 * directly inside requestAnimationFrame (zero React churn), while panel
 * components subscribe through useTelemetry() at human-readable rates.
 */
import { useEffect, useState } from "react";
import type { EngineSnapshot } from "../types/telemetry";
import { isTauri, listenSafe } from "./useTauriEvent";

export const EVT_SNAPSHOT = "telemetry://snapshot";

let latest: EngineSnapshot | null = null;
const listeners = new Set<() => void>();
let started = false;

export function getSnapshot(): EngineSnapshot | null {
  return latest;
}

function publish(next: EngineSnapshot): void {
  latest = next;
  for (const cb of listeners) cb();
}

// ---------------------------------------------------------------------------
// Browser demo feed: mirrors the Rust simulator roster closely enough for
// offline UI development and NDAIE screen captures.
// ---------------------------------------------------------------------------

interface DemoAircraft {
  icao24: string;
  callsign: string;
  class: "civil" | "military" | "anomaly";
  lat: number;
  lon: number;
  vN: number;
  vE: number;
  alt_ft: number;
  spd_kt: number;
  crs_deg: number;
  squawk: string;
}

const DEMO_FLEET: DemoAircraft[] = [
  { icao24: "342157", callsign: "VL604", class: "civil", lat: 12.42, lon: 8.16, vN: -207, vE: -97, alt_ft: 32000, spd_kt: 445, crs_deg: 205, squawk: "3421" },
  { icao24: "348AC2", callsign: "VL702", class: "civil", lat: 11.62, lon: 9.05, vN: 178, vE: -125, alt_ft: 28500, spd_kt: 430, crs_deg: 335, squawk: "4203" },
  { icao24: "3C641D", callsign: "DLH501", class: "civil", lat: 13.35, lon: 7.62, vN: -236, vE: 297, alt_ft: 38000, spd_kt: 490, crs_deg: 148, squawk: "5172" },
  { icao24: "061104", callsign: "NAF911", class: "military", lat: 12.1395, lon: 8.0196, vN: 229, vE: 117, alt_ft: 32000, spd_kt: 520, crs_deg: 27, squawk: "6101" },
  { icao24: "061105", callsign: "NAF912", class: "military", lat: 12.095, lon: 8.005, vN: 221, vE: 112, alt_ft: 31000, spd_kt: 500, crs_deg: 27, squawk: "6102" },
  { icao24: "0BADC0", callsign: "", class: "anomaly", lat: 12.75, lon: 8.95, vN: -129, vE: -166, alt_ft: 6500, spd_kt: 210, crs_deg: 232, squawk: "0000" },
];

const SITE = { lat: 12.0476, lon: 8.5241 };

function demoSnapshot(tMs: number): EngineSnapshot {
  const tS = tMs / 1000;
  const tracks = DEMO_FLEET.map((a) => {
    const lat = a.lat + (a.vN * tS) / 111_132;
    const lon = a.lon + (a.vE * tS) / (111_320 * Math.cos((SITE.lat * Math.PI) / 180));
    const leader: [number, number][] = Array.from({ length: 13 }, (_, i) => [
      lat + (a.vN * i * 10) / 111_132,
      lon + (a.vE * i * 10) / (111_320 * Math.cos((SITE.lat * Math.PI) / 180)),
    ]);
    return {
      icao24: a.icao24,
      callsign: a.callsign || `UNK-${a.icao24}`,
      class: a.class,
      alert: a.squawk === "7700" && tS > 45 ? ("emergency" as const) : ("none" as const),
      latitude: ((lat + 90) % 180) - 90,
      longitude: ((lon + 180) % 360) - 180,
      altitude_ft: a.alt_ft,
      ground_speed_kt: a.spd_kt,
      course_deg: a.crs_deg,
      vertical_rate_fpm: 0,
      vertical_trend: "level" as const,
      squawk: a.squawk === "7700" && tS > 45 ? "7700" : a.squawk,
      on_ground: false,
      last_update_ms: tMs,
      age_s: 0.2,
      coasting: false,
      position_sigma_m: 41,
      leader_line: leader,
    };
  });

  // STCA demo pair: NAF911 vs VL604 inside the engineered geometry window.
  const stca =
    tS > 20 && tS < 140
      ? [
          {
            id: "342157-061104",
            icao_a: "342157",
            callsign_a: "VL604",
            icao_b: "061104",
            callsign_b: "NAF911",
            min_horizontal_nm: 2.8,
            min_vertical_ft: 120,
            time_to_closest_s: Math.max(5, 70 - tS),
            triggered_ms: tMs,
          },
        ]
      : [];

  return {
    tracks,
    flight_data_blocks: tracks.map((t) => ({
      icao24: t.icao24,
      line1: `${t.callsign.padEnd(8)} ${String(Math.round(t.altitude_ft / 100)).padStart(3, "0")}`,
      line2: `1090 ${Math.round(t.ground_speed_kt)}K ${t.squawk}`,
      line3: `S-${t.squawk.length.toString().padStart(2, "0")}`,
      color:
        t.class === "civil" ? "#00FF66" : t.class === "military" ? "#7C4DFF" : "#FFB300",
    })),
    stca_alerts: stca,
    geofence_breaches: [],
    anomalies:
      tS > 40
        ? [{ icao24: "0BADC0", reason: "NO IDENTITY BROADCAST (40s OBSERVED)" }]
        : [],
    weather: {
      nodes: DEMO_FLEET.map((a) => ({
        latitude: a.lat,
        longitude: a.lon,
        altitude_ft: a.alt_ft,
        wind_dir_deg: 225,
        wind_speed_kt: 15 + (a.alt_ft / 10_000) * 14,
        temperature_c: 33 - (a.alt_ft / 10_000) * 6.4,
        pressure_hpa: 1013.25 - (a.alt_ft * 0.0295),
        fusion_sources: 0b100,
        observed_ms: tMs,
      })),
      surface_qnh_hpa: 1013.2,
      surface_wind_dir_deg: 182,
      surface_wind_speed_kt: 11.4,
      surface_temperature_c: 33.5,
      surface_dewpoint_c: 24.2,
      visibility_m: 8000,
      dust_layer_top_ft: 4200,
      datis_text: "DNKN INFO C 261305Z APCH ILS RWY 06 WIND 190/14 QNH 1014 TEMPO 4000 HZ=",
      metar_text: "METAR DNKN 261200Z 18012KT 6000 HZ FEW030 33/24 Q1013 NOSIG=",
      updated_ms: tMs,
    },
    status: {
      hardware: [
        { channel_id: "SDR-1", role: "1090MHz ADS-B", serial_lock: "00000001", frequency_hz: 1090000000, sample_rate_sps: 2400000, gain_db: 42, messages_per_second: 380 + 40 * Math.sin(tS / 7), ppm_error: 0.4, online: true, usb_port: "bus1-port1" },
        { channel_id: "SDR-2", role: "ACARS", serial_lock: "00000002", frequency_hz: 131550000, sample_rate_sps: 2400000, gain_db: 38, messages_per_second: 2.1, ppm_error: 1.1, online: true, usb_port: "bus1-port3" },
        { channel_id: "SDR-3", role: "VHF GUARD", serial_lock: "00000003", frequency_hz: 121500000, sample_rate_sps: 2400000, gain_db: 36, messages_per_second: 0.2, ppm_error: 0.8, online: false, usb_port: null },
      ],
      ekf_active: true,
      ekf_latency_us: 340 + 60 * Math.sin(tS / 3),
      stca_pairs_active: stca.length,
      tracks_total: tracks.length,
      duckdb_writes_per_sec: 118,
      sidecar_online: true,
      updated_ms: tMs,
    },
  };
}

function startDemoFeed(): void {
  const timer = setInterval(() => {
    publish(demoSnapshot(Date.now()));
  }, 50); // 20 Hz — plenty for UI panels
  // Never stop; dev-page lifecycle only.
  void timer;
}

/** Idempotent boot of the telemetry stream (Tauri event or demo feed). */
export async function startTelemetry(): Promise<void> {
  if (started) return;
  started = true;
  if (isTauri()) {
    await listenSafe<EngineSnapshot>(EVT_SNAPSHOT, publish);
  } else {
    startDemoFeed();
  }
}

/** React binding throttled to `hz` updates per second. */
export function useTelemetry(hz = 8): EngineSnapshot | null {
  const [snap, setSnap] = useState<EngineSnapshot | null>(getSnapshot());
  useEffect(() => {
    void startTelemetry();
    const interval = setInterval(() => setSnap(getSnapshot()), Math.max(31, 1000 / hz));
    return () => clearInterval(interval);
  }, [hz]);
  return snap;
}
