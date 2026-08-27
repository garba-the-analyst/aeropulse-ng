/**
 * Defense overlay: live threat matrix (anomalies, emergency squawks,
 * geofence breaches) with the tactical intercept calculator beneath.
 */
import { useState } from "react";
import { useTelemetry } from "../../hooks/useFlightTracks";
import { invokeSafe } from "../../lib/ipc";
import type { InterceptSolution } from "../../types/defense";

export default function ThreatMatrix() {
  const snap = useTelemetry(3);

  const rows: { icao24: string; label: string; reason: string }[] = [];
  for (const t of snap?.tracks ?? []) {
    if (t.alert !== "none") {
      rows.push({
        icao24: t.icao24,
        label: `${t.callsign} — Squawk ${t.squawk}`,
        reason:
          t.alert === "emergency"
            ? "Emergency declared"
            : t.alert === "radio_failure"
              ? "Radio failure"
              : t.alert === "hijack"
                ? "Security alert"
                : t.alert === "dark_target"
                  ? "Aircraft without identification"
                  : "Restricted airspace entered",
      });
    }
  }
  for (const note of snap?.anomalies ?? []) {
    const known = rows.some((r) => r.icao24 === note.icao24);
    if (!known) rows.push({ icao24: note.icao24, label: note.icao24, reason: note.reason });
  }

  return (
    <div className="panel">
      <h2>Active Alerts</h2>
      {rows.length === 0 && (
        <div style={{ color: "var(--ap-text-dim)", fontSize: "12px", padding: "8px 0" }}>No active alerts — Airspace is clear</div>
      )}
      {rows.map((r) => (
        <div className="threat-row" key={r.icao24}>
          <span style={{ color: "var(--ap-critical)", fontWeight: "600", fontFamily: "var(--ap-font-mono)", fontSize: "11px" }}>
            {r.label}
          </span>
          <span style={{ color: "var(--ap-text-secondary)", fontSize: "11px" }}>{r.reason}</span>
        </div>
      ))}
    </div>
  );
}

export function InterceptPanel() {
  const snap = useTelemetry(2);
  const [target, setTarget] = useState("");
  const [interceptor, setInterceptor] = useState("");
  // QRA speed in km/h (SI presentation); backend solver converts.
  const [speed, setSpeed] = useState(800);
  const [result, setResult] = useState<InterceptSolution | null>(null);

  const tracks = snap?.tracks ?? [];
  const options = tracks.map((t) => (
    <option key={t.icao24} value={t.icao24}>
      {t.callsign} ({t.icao24})
    </option>
  ));

  async function solve(): Promise<void> {
    if (!target || !interceptor) return;
    const sol = await invokeSafe<InterceptSolution>("compute_intercept", {
      targetIcao24: target,
      interceptorIcao24: interceptor,
      interceptorSpeedKt: speed / 1.852, // km/h -> kt for the solver
    });
    if (sol) {
      setResult(sol);
      window.dispatchEvent(
        new CustomEvent("ap-intercept-solution", { detail: sol }),
      );
    } else {
      // Browser-dev fallback: local lead-pursuit approximation.
      const tgt = tracks.find((t) => t.icao24 === target);
      const itc = tracks.find((t) => t.icao24 === interceptor);
      if (!tgt || !itc) return;
      const dLat = tgt.latitude - itc.latitude;
      const dLon = tgt.longitude - itc.longitude;
      let brg = Math.atan2(dLon, dLat) * (180 / Math.PI);
      if (brg < 0) brg += 360;
      const rangeNm =
        Math.hypot(dLat * 60.04, dLon * 60.04 * Math.cos((itc.latitude * Math.PI) / 180));
      const rangeKm = rangeNm * 1.852;
      setResult({
        target_icao24: target,
        interceptor_icao24: interceptor,
        intercept_heading_deg: brg,
        required_speed_kt: speed / 1.852,
        time_to_intercept_s: (rangeKm / Math.max(1, speed)) * 3600,
        initial_bearing_deg: brg,
        range_nm: rangeNm,
        feasible: true,
      });
    }
  }

  return (
    <div className="panel" style={{ background: "var(--ap-panel)", border: "1px solid var(--ap-border)" }}>
      <h2>Intercept Guidance</h2>

      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <select value={target} onChange={(e) => setTarget(e.target.value)}>
          <option value="">Select aircraft to intercept</option>
          {options}
        </select>
        <select value={interceptor} onChange={(e) => setInterceptor(e.target.value)}>
          <option value="">Select intercepting aircraft</option>
          {options}
        </select>
        <label style={{ color: "var(--ap-text-dim)", fontSize: "11px", fontWeight: 500 }}>
          Required Speed (km/h)
          <input
            type="number"
            min={120}
            max={1200}
            step={10}
            value={speed}
            onChange={(e) => setSpeed(Number(e.target.value))}
            style={{ width: "100%", marginTop: 2 }}
          />
        </label>
        <button onClick={() => void solve()} disabled={!target || !interceptor} title={!target || !interceptor ? "Select both aircraft first" : "Calculate intercept vector"}>
          Calculate Intercept
        </button>
      </div>

      {result && (
        <div className="intercept-result">
          <div style={{ color: "var(--ap-text-dim)", fontSize: "11px", fontWeight: 500 }}>Required Heading</div>
          <div className={`big ${result.feasible ? "" : "infeasible"}`}>
            {result.intercept_heading_deg.toFixed(0)}°
          </div>
          <div style={{ marginTop: 8, lineHeight: 1.6, fontSize: "12px" }}>
            Speed {(result.required_speed_kt * 1.852).toFixed(0)} km/h · Distance{" "}
            {(result.range_nm * 1.852).toFixed(1)} km
            <br />
            Estimated time{" "}
            {Number.isFinite(result.time_to_intercept_s)
              ? `${Math.round(result.time_to_intercept_s / 60)} minutes`
              : "—"}
            <br />
            <span style={{ color: result.feasible ? "var(--ap-success)" : "var(--ap-critical)" }}>
              {result.feasible ? "Intercept is feasible" : "No intercept solution"}
            </span>
          </div>
        </div>
      )}
    </div>
  );
}
