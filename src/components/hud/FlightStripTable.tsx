/**
 * Display 2 Panel A — Digital Flight Data Strip bay.
 * Active Arrivals and En-Route Departures with status edge indicators:
 * green standard · yellow priority/approach · red emergency/diverted.
 */
import { useTelemetry } from "../../hooks/useFlightTracks";
import { ftToM, ktToKmh } from "../../lib/units";
import type { Track } from "../../types/telemetry";

function edgeClass(t: Track): string {
  if (t.alert === "emergency" || t.alert === "hijack" || t.alert === "radio_failure") {
    return "emergency";
  }
  if (t.alert !== "none" || t.altitude_ft < 4_000) return "priority";
  return "standard";
}

function etaText(t: Track): string {
  // Rough ETA to the site volume boundary (120 NM ≈ 222 km) at present
  // ground speed; internal wire units remain knots per DO-260B.
  if (t.ground_speed_kt < 40) return "--";
  const minutes = Math.round((120 / t.ground_speed_kt) * 60);
  const d = new Date(Date.now() + minutes * 60_000);
  return d.toISOString().slice(11, 16) + "Z";
}

export default function FlightStripTable() {
  const snap = useTelemetry(4);
  const tracks = [...(snap?.tracks ?? [])].sort((a, b) =>
    a.callsign.localeCompare(b.callsign),
  );

  const arrivals = tracks.filter(
    (t) => t.vertical_trend === "descent" || t.altitude_ft < 5_500,
  );
  const departures = tracks.filter(
    (t) => t.vertical_trend !== "descent" && t.altitude_ft >= 5_500,
  );

  const renderStrip = (t: Track, keyPrefix: string) => (
    <div className="strip" key={`${keyPrefix}-${t.icao24}`}>
      <span className={`edge ${edgeClass(t)}`} />
      <span>{t.callsign}</span>
      <span>{t.class.toUpperCase()}</span>
      <span>{ftToM(t.altitude_ft).toFixed(0)} m</span>
      <span>{t.squawk}</span>
      <span>{ktToKmh(t.ground_speed_kt).toFixed(0)} km/h</span>
      <span style={{ textAlign: "right" }}>{etaText(t)}</span>
    </div>
  );

  return (
    <div className="panel">
      <h2>Flight Progress Strips</h2>

      <div className="bay-title">Arriving Aircraft</div>
      <div className="strip-bay">
        {arrivals.length === 0 && <div style={{ color: "var(--ap-text-dim)", fontSize: "12px", padding: "8px 0" }}>No aircraft on approach</div>}
        {arrivals.map((t) => renderStrip(t, "arr"))}
      </div>

      <div className="bay-title">Departing and En-Route Aircraft</div>
      <div className="strip-bay">
        {departures.length === 0 && <div style={{ color: "var(--ap-text-dim)", fontSize: "12px", padding: "8px 0" }}>No aircraft in this sector</div>}
        {departures.map((t) => renderStrip(t, "dep"))}
      </div>
    </div>
  );
}
