import { describe, it, expect } from "vitest";
import { formatFDB } from "./FlightDataBlock";
import type { Track } from "../../types/telemetry";

function mkTrack(overrides: Partial<Track> = {}): Track {
  return {
    icao24: "342157",
    callsign: "VL604",
    class: "civil",
    alert: "none",
    latitude: 12.35,
    longitude: 8.13,
    altitude_ft: 32000,
    ground_speed_kt: 445,
    course_deg: 205,
    vertical_rate_fpm: 0,
    vertical_trend: "level",
    squawk: "3421",
    on_ground: false,
    last_update_ms: 1700000000000,
    age_s: 0.2,
    coasting: false,
    position_sigma_m: 41,
    leader_line: [[12.35, 8.13]],
    mode_s_capable: false,
    last_baro_altitude_ft: null,
    ...overrides,
  };
}

describe("formatFDB line3 — mirrors Rust FlightDataBlock::from_track", () => {
  it("coasting => DR regardless of sigma/MS", () => {
    const t = mkTrack({ coasting: true, position_sigma_m: 10, mode_s_capable: true });
    expect(formatFDB(t).line3).toBe("DR");
  });

  it("Q1 when sigma <30", () => {
    const t = mkTrack({ position_sigma_m: 29.9, mode_s_capable: false });
    expect(formatFDB(t).line3).toBe("Q1");
  });

  it("Q2 when 30 <= sigma <80", () => {
    const t = mkTrack({ position_sigma_m: 41, mode_s_capable: false });
    expect(formatFDB(t).line3).toBe("Q2");
  });

  it("Q3 when sigma >=80", () => {
    const t = mkTrack({ position_sigma_m: 120, mode_s_capable: false });
    expect(formatFDB(t).line3).toBe("Q3");
  });

  it("MS suffix when mode_s_capable true", () => {
    const t1 = mkTrack({ position_sigma_m: 20, mode_s_capable: true });
    expect(formatFDB(t1).line3).toBe("Q1 MS");
    const t2 = mkTrack({ position_sigma_m: 50, mode_s_capable: true });
    expect(formatFDB(t2).line3).toBe("Q2 MS");
    const t3 = mkTrack({ position_sigma_m: 90, mode_s_capable: true });
    expect(formatFDB(t3).line3).toBe("Q3 MS");
  });

  it("without MS when mode_s_capable false", () => {
    const t = mkTrack({ position_sigma_m: 20, mode_s_capable: false });
    expect(formatFDB(t).line3).toBe("Q1");
  });

  it("alert states override quality/coasting", () => {
    const cases: Array<[Track["alert"], string]> = [
      ["emergency", "EMRG"],
      ["radio_failure", "RADO"],
      ["hijack", "HIJK"],
      ["geofence_breach", "GEO!"],
      ["dark_target", "DARK"],
    ];
    for (const [alert, expected] of cases) {
      const t = mkTrack({ alert, coasting: true, position_sigma_m: 10, mode_s_capable: true });
      expect(formatFDB(t).line3, `alert ${alert}`).toBe(expected);
    }
  });

  it("different squawks do not affect line3 when no alert/coasting (quality only)", () => {
    const tA = mkTrack({ squawk: "1200", position_sigma_m: 41, mode_s_capable: false });
    const tB = mkTrack({ squawk: "7700", position_sigma_m: 41, mode_s_capable: false });
    // Both should be Q2 (no alert mapping via squawk; line3 is quality-based)
    expect(formatFDB(tA).line3).toBe("Q2");
    expect(formatFDB(tB).line3).toBe("Q2");
    // When emergency alert active, squawk irrelevant — line3 is EMRG
    const tC = mkTrack({ squawk: "7500", alert: "emergency", position_sigma_m: 41 });
    expect(formatFDB(tC).line3).toBe("EMRG");
  });

  it("line1/line2 remain SI-metric (metres/kmh) as per unit policy", () => {
    const t = mkTrack({ callsign: "TEST", altitude_ft: 10000, ground_speed_kt: 100, squawk: "1200" });
    const fdb = formatFDB(t);
    // altitude 10000 ft -> 3048 m
    expect(fdb.line1).toContain("3048M");
    // speed 100 kt -> 185 km/h
    expect(fdb.line2).toContain("185KM/H");
  });
});
