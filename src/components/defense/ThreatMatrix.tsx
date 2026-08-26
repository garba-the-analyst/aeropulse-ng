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
        label: `${t.callsign} · SQK ${t.squawk}`,
        reason:
          t.alert === "emergency"
            ? "GENERAL EMERGENCY (7700)"
            : t.alert === "radio_failure"
              ? "RADIO FAILURE (7600)"
              : t.alert === "hijack"
                ? "UNLAWFUL INTERFERENCE (7500)"
                : t.alert === "dark_target"
                  ? "DARK TARGET / NO IDENTITY"
                  : "GEOFENCE BREACH",
      });
    }
  }
  for (const note of snap?.anomalies ?? []) {
    const known = rows.some((r) => r.icao24 === note.icao24);
    if (!known) rows.push({ icao24: note.icao24, label: note.icao24, reason: note.reason });
  }

  return (
    <div className="panel">
      <h2>THREAT MATRIX</h2>
      {rows.length === 0 && (
        <div style={{ color: "var(--ap-text-dim)" }}>— nominal airspace —</div>
      )}
      {rows.map((r) => (
        <div className="threat-row" key={r.icao24}>
          <span style={{ color: "var(--ap-alert)", fontWeight: "bold" }}>
            {r.label}
          </span>
          <span style={{ color: "var(--ap-text-dim)" }}>{r.reason}</span>
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
      setResult({
        target_icao24: target,
        interceptor_icao24: interceptor,
        intercept_heading_deg: brg,
        required_speed_kt: speed,
        time_to_intercept_s: (rangeNm / Math.max(1, speed)) * 3600,
        initial_bearing_deg: brg,
        range_nm: rangeNm,
        feasible: true,
      });
    }
  }

  return (
    <div className="panel" style={{ background: "rgba(16,20,29,0.92)" }}>
      <h2>TACTICAL INTERCEPT CALCULATOR</h2>

      <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        <select value={target} onChange={(e) => setTarget(e.target.value)}>
          <option value="">— TARGET —</option>
          {options}
        </select>
        <select value={interceptor} onChange={(e) => setInterceptor(e.target.value)}>
          <option value="">— INTERCEPTOR —</option>
          {options}
        </select>
        <label style={{ color: "var(--ap-text-dim)", fontSize: 10 }}>
          QRA GROUND SPEED (km/h)
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
        <button onClick={() => void solve()}>COMPUTE INTERCEPT VECTOR</button>
      </div>

      {result && (
        <div className="intercept-result">
          <div style={{ color: "var(--ap-text-dim)", fontSize: 10 }}>REQUIRED HEADING</div>
          <div className={`big ${result.feasible ? "" : "infeasible"}`}>
            {result.intercept_heading_deg.toFixed(0)}°T
          </div>
          <div style={{ marginTop: 6, lineHeight: 1.5 }}>
            SPEED {(result.required_speed_kt * 1.852).toFixed(0)} km/h · RANGE{" "}
            {(result.range_nm * 1.852).toFixed(1)} KM
            <br />
            TTI{" "}
            {Number.isFinite(result.time_to_intercept_s)
              ? `${Math.round(result.time_to_intercept_s / 60)} min`
              : "—"}
            <br />
            STATUS:{" "}
            <span style={{ color: result.feasible ? "var(--ap-ok)" : "var(--ap-alert)" }}>
              {result.feasible ? "FEASIBLE" : "NO INTERCEPT SOLUTION"}
            </span>
          </div>
        </div>
      )}
    </div>
  );
}
