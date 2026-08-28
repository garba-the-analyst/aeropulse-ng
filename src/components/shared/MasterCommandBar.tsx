import { useEffect, useState } from "react";
import { useTelemetry } from "../../hooks/useFlightTracks";
import { invokeSafe } from "../../lib/ipc";
import { RANGE_OPTIONS_KM } from "../../lib/units";

function zulu(d: Date): string {
  return d.toISOString().slice(11, 19) + "Z";
}

function wat(d: Date): string {
  // West Africa Time = UTC+1, no DST.
  const wat = new Date(d.getTime() + 3_600_000);
  return wat.toISOString().slice(11, 19) + " WAT";
}

interface Props {
  rangeKm: number;
  onRangeChange: (km: number) => void;
  showEmergency?: boolean;
}

export default function MasterCommandBar({ rangeKm, onRangeChange, showEmergency = true }: Props) {
  const snap = useTelemetry(4);
  const [now, setNow] = useState(() => new Date());
  const [armedUntil, setArmedUntil] = useState(0);

  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(t);
  }, []);

  const hw = snap?.status.hardware ?? [];
  const trackingOk = snap?.status.ekf_active ?? false;
  const weatherOk = snap?.status.sidecar_online ?? true;
  const armed = Date.now() < armedUntil;

  async function fireEmergency(): Promise<void> {
    if (armed) {
      setArmedUntil(0);
      await invokeSafe("cancel_emergency_override", {});
      return;
    }
    setArmedUntil(Date.now() + 30_000);
    await invokeSafe("trigger_emergency_override", { durationS: 30 });
  }

  // Auto-clear local armed state when timer expires
  useEffect(() => {
    if (!armed) return;
    const t = setTimeout(() => setArmedUntil(0), armedUntil - Date.now());
    return () => clearTimeout(t);
  }, [armed, armedUntil]);

  return (
    <div className="command-bar">
      <div className="product-id">
        AeroPulse-NG <span className="sub">Air Surveillance System</span>
      </div>

      <div className="status-cluster">
        {hw.map((h) => (
          <span className="chip" key={h.channel_id}>
            <span className={`dot ${h.online ? "ok" : "down"}`} />
            {h.channel_id === "SDR-1" ? "Primary Surveillance" : h.channel_id === "SDR-2" ? "Secondary Surveillance" : h.role} — {h.online ? "Active" : "Offline"}
          </span>
        ))}
        <span className="chip">
          <span className={`dot ${weatherOk ? "ok" : "down"}`} />
          Surface Weather — {weatherOk ? "Active" : "Standby"}
        </span>
        <span className="chip">
          <span className={`dot ${trackingOk ? "ok" : "down"}`} />
          Aircraft Tracking — {trackingOk ? "Active" : "Starting"}
        </span>
      </div>

      <div className="status-cluster">
        <select
          aria-label="Display range"
          value={rangeKm}
          onChange={(e) => onRangeChange(Number(e.target.value))}
        >
          {RANGE_OPTIONS_KM.map((r) => (
            <option key={r} value={r}>
              {r} km
            </option>
          ))}
        </select>

        <div className="clock-block">
          <div className="zulu">{zulu(now)}</div>
          <div className="wat">{wat(now)}</div>
        </div>

        {showEmergency && (
          <button
            className={`emergency-btn ${armed ? "armed" : ""}`}
            onClick={() => void fireEmergency()}
          >
            {armed ? "Emergency Active" : "Declare Emergency"}
          </button>
        )}
      </div>
    </div>
  );
}
