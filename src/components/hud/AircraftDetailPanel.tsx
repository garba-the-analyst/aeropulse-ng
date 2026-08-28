import { useTelemetry } from "../../hooks/useFlightTracks";
import { ftToM, ktToKmh } from "../../lib/units";

interface Props {
  icao24: string | null;
  onClear: () => void;
  onTransmit: (icao24: string, callsign: string) => void;
}

function frequencyForIcao(icao24: string): string {
  // Deterministic VHF channel per aircraft for demo — 118.000–136.975, 25 kHz raster
  let h = 0;
  for (let i = 0; i < icao24.length; i++) h = (h * 31 + icao24.charCodeAt(i)) >>> 0;
  const steps = 760; // (136975-118000)/25
  const step = h % steps;
  const khz = 118000 + step * 25;
  return `${(khz / 1000).toFixed(3)}`;
}

export default function AircraftDetailPanel({ icao24, onClear, onTransmit }: Props) {
  const snap = useTelemetry(3);
  const track = snap?.tracks.find(t => t.icao24 === icao24) ?? null;

  if (!icao24) {
    return (
      <div className="panel" style={{ display: "flex", alignItems: "center", justifyContent: "center", minHeight: 180 }}>
        <div style={{ textAlign: "center", color: "var(--ap-text-dim)" }}>
          <div style={{ fontSize: "13px", fontWeight: 500, marginBottom: 4 }}>No Aircraft Selected</div>
          <div style={{ fontSize: "11px" }}>Select a flight strip or radar contact to view details</div>
        </div>
      </div>
    );
  }

  if (!track) {
    return (
      <div className="panel">
        <h2>Aircraft Details</h2>
        <div style={{ color: "var(--ap-text-dim)", fontSize: "12px" }}>Aircraft {icao24} no longer tracked</div>
        <button onClick={onClear} style={{ marginTop: 8 }}>Clear selection</button>
      </div>
    );
  }

  const freq = frequencyForIcao(track.icao24);

  return (
    <div className="panel">
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12, paddingBottom: 8, borderBottom: "1px solid var(--ap-border)" }}>
        <h2 style={{ margin: 0, border: "none", padding: 0 }}>{track.callsign}</h2>
        <button onClick={onClear} style={{ padding: "4px 8px", fontSize: "11px" }}>Clear</button>
      </div>

      <div className="kv-grid" style={{ marginBottom: 12 }}>
        <div className="kv">
          <div className="k">Aircraft Type</div>
          <div className="v" style={{ fontSize: "13px" }}>{track.class === "civil" ? "Civil" : track.class === "military" ? "Military" : "Unidentified"}</div>
        </div>
        <div className="kv">
          <div className="k">Squawk</div>
          <div className="v" style={{ fontFamily: "var(--ap-font-mono)" }}>{track.squawk}</div>
        </div>
        <div className="kv">
          <div className="k">Altitude</div>
          <div className="v">{ftToM(track.altitude_ft).toFixed(0)} m</div>
        </div>
        <div className="kv">
          <div className="k">Ground Speed</div>
          <div className="v">{ktToKmh(track.ground_speed_kt).toFixed(0)} km/h</div>
        </div>
        <div className="kv">
          <div className="k">Heading</div>
          <div className="v">{Math.round(track.course_deg)}°</div>
        </div>
        <div className="kv">
          <div className="k">Vertical Rate</div>
          <div className="v">{track.vertical_rate_fpm > 0 ? "↑" : track.vertical_rate_fpm < 0 ? "↓" : "—"} {Math.abs(Math.round(track.vertical_rate_fpm * 0.3048))} m/min</div>
        </div>
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8, fontSize: "11px", color: "var(--ap-text-secondary)", marginBottom: 12, fontFamily: "var(--ap-font-mono)" }}>
        <span>ICAO24: {track.icao24}</span>
        <span>Age: {track.age_s.toFixed(1)} s</span>
        <span>Lat: {track.latitude.toFixed(3)}°</span>
        <span>Lon: {track.longitude.toFixed(3)}°</span>
        <span>Accuracy: ±{Math.round(track.position_sigma_m)} m</span>
        <span>Status: {track.coasting ? "Estimated" : "Live"} {track.alert !== "none" ? `· ${track.alert}` : ""}</span>
      </div>

      <div style={{ display: "flex", gap: 8 }}>
        <button onClick={() => onTransmit(track.icao24, track.callsign)} style={{ flex: 1, background: "var(--ap-accent-soft)", borderColor: "var(--ap-accent)", color: "var(--ap-accent)" }}>
          Transmit to {track.callsign} ({freq} MHz)
        </button>
      </div>
      <div style={{ fontSize: "10px", color: "var(--ap-text-dim)", marginTop: 6, textAlign: "center" }}>
        Tunes audio panel to {freq} MHz and prepares push-to-talk
      </div>
    </div>
  );
}
