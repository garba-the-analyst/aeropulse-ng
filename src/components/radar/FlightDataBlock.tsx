/**
 * Flight Data Block formatting — canonical three-line tactical tag,
 * SI-metric presentation per NCAA/ICAO Annex 5 unit policy:
 *   line1: callsign · altitude (metres) · trend arrow
 *   line2: source · ground speed (km/h) · squawk
 *   line3: system annotation
 */
import { fmtAltM, ktToKmh } from "../../lib/units";
import type { Track } from "../../types/telemetry";

export interface FDBText {
  line1: string;
  line2: string;
  line3: string;
}

export function formatFDB(track: Track): FDBText {
  const altM = Math.round(track.altitude_ft * 0.3048);
  const arrow =
    track.vertical_trend === "climb"
      ? "\u2191"
      : track.vertical_trend === "descent"
        ? "\u2193"
        : "";
  const source = track.coasting ? "DR" : track.squawk === "----" ? "----" : "1090";
  const ann = (() => {
    switch (track.alert) {
      case "emergency": return "EMRG";
      case "radio_failure": return "RADO";
      case "hijack": return "HIJK";
      case "geofence_breach": return "GEO!";
      case "dark_target": return "DARK";
      default: return `S-${track.squawk.length.toString().padStart(2, "0")}`;
    }
  })();

  return {
    line1: `${track.callsign.padEnd(8)} ${altM}M${arrow}`,
    line2: `${source} ${Math.round(ktToKmh(track.ground_speed_kt))}KM/H ${track.squawk}`,
    line3: ann,
  };
}
