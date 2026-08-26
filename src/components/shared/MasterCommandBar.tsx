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
  const ekfOk = snap?.status.ekf_active ?? false;
  const awosOk = snap?.status.sidecar_online ?? true; // demo feed implies mast
  const armed = Date.now() < armedUntil;

  async function fireEmergency(): Promise<void> {
    setArmedUntil(Date.now() + 30_000);
    await invokeSafe("trigger_emergency_override", { durationS: 30 });
  }

  return (
    <div className="command-bar">
      <div className="product-id">
        AeroPulse-NG v2.4
        <span className="sub">| TACTICAL SURVEILLANCE ENGINE</span>
      </div>

      <div className="status-cluster">
        {hw.map((h) => (
          <span className="chip" key={h.channel_id}>
            <span className={`dot ${h.online ? "ok" : "down"}`} />
            {h.channel_id}: {h.role} {h.online ? "OK" : "DOWN"}
          </span>
        ))}
        <span className="chip">
          <span className={`dot ${awosOk ? "ok" : "down"}`} />
          AWOS: RS-485 {awosOk ? "OK" : "SIM"}
        </span>
        <span className="chip">
          <span className={`dot ${ekfOk ? "ok" : "down"}`} />
          EKF ENGINE: {ekfOk ? "ACTIVE" : "INIT"}
        </span>
      </div>

      <div className="status-cluster">
        <select
          aria-label="Range scale"
          value={rangeKm}
          onChange={(e) => onRangeChange(Number(e.target.value))}
        >
          {RANGE_OPTIONS_KM.map((r) => (
            <option key={r} value={r}>
              {r} KM
            </option>
          ))}
        </select>

        <div className="clock-block">
          <div className="zulu">{zulu(now)}</div>
          <div>{wat(now)}</div>
        </div>

        {showEmergency && (
          <button
            className={`emergency-btn ${armed ? "armed" : ""}`}
            onClick={() => void fireEmergency()}
          >
            {armed ? "EMERGENCY ARMED" : "EMERG 7700"}
          </button>
        )}
      </div>
    </div>
  );
}
