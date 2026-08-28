/**
 * Display 2 Panel C — SDR hardware diagnostics: live FFT spectrum plot for
 * the 1090 MHz channel, EKF latency gauge against the 2.0 ms budget and
 * DuckDB write-rate telemetry.
 *
 * The spectrum is synthesised from the live message-rate amplitude with a
 * deterministic noise floor shaped around the centre spikes — standing in
 * for the RF tap until the libusb FFT stage lands on the ingestion path.
 */
import { useEffect, useRef } from "react";
import { getSnapshot, useTelemetry } from "../../hooks/useFlightTracks";

const BINS = 128;
const NOISE_SEED = 0x5eed_1090;

function pseudoNoise(i: number, t: number): number {
  let x = (NOISE_SEED ^ (i * 2654435761) ^ Math.floor(t)) >>> 0;
  x ^= x << 13;
  x ^= x >>> 17;
  x ^= x << 5;
  return ((x >>> 0) / 4294967295) * 2 - 1;
}

export default function SystemMetrics() {
  const snap = useTelemetry(4);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const status = snap?.status;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let raf = 0;
    const draw = (): void => {
      const dpr = window.devicePixelRatio || 1;
      const wCss = canvas.clientWidth;
      const hCss = canvas.clientHeight || 120;
      if (canvas.width !== Math.round(wCss * dpr)) canvas.width = Math.round(wCss * dpr);
      if (canvas.height !== Math.round(hCss * dpr))
        canvas.height = Math.round(hCss * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

      ctx.fillStyle = "#111417";
      ctx.fillRect(0, 0, wCss, hCss);

      // Grid — subtle, uniform
      ctx.strokeStyle = "rgba(74,83,93,0.35)";
      ctx.beginPath();
      for (let gx = 0; gx <= wCss; gx += wCss / 8) {
        ctx.moveTo(gx, 0);
        ctx.lineTo(gx, hCss);
      }
      ctx.stroke();

      const s = getSnapshot();
      const msgRate = s?.status.hardware[0]?.messages_per_second ?? 0;
      const online = s?.status.hardware[0]?.online ?? false;
      const tSec = Date.now() / 120;

      ctx.strokeStyle = online ? "#86B59A" : "#D86161";
      ctx.beginPath();
      for (let i = 0; i < BINS; i++) {
        const frac = i / (BINS - 1);
        // Two spectral peaks: ADS-B at ~78% span, ACARS low band shoulder.
        const peak1 =
          Math.exp(-Math.pow((frac - 0.78) * 26, 2)) * (0.35 + Math.min(0.45, msgRate / 900));
        const peak2 =
          Math.exp(-Math.pow((frac - 0.12) * 34, 2)) * (0.08 + Math.min(0.12, msgRate / 2400));
        const floorDb = -88 + pseudoNoise(i, tSec) * 3.2 + (peak1 + peak2) * 46;
        const y = hCss - ((floorDb + 100) / 100) * hCss;
        const x = frac * wCss;
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();

      // Frequency markers — restrained
      ctx.fillStyle = "#A6B0BA";
      ctx.font = "10px Inter, system-ui, sans-serif";
      ctx.fillText("131.550 MHz", wCss * 0.12 - 22, hCss - 6);
      ctx.fillText("1 090 MHz", wCss * 0.78 - 24, hCss - 6);

      raf = requestAnimationFrame(draw);
    };
    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, []);

  const latencyUs = status?.ekf_latency_us ?? 0;
  const latencyBudget = 2000; // spec ceiling
  const latFrac = Math.min(1, latencyUs / latencyBudget);
  const latClass = latFrac > 0.85 ? "crit" : latFrac > 0.55 ? "warn" : "";

  return (
    <div className="panel">
      <h2>System Status</h2>

      {(status?.hardware ?? []).map((h) => {
        const isSim = !h.online && (status?.tracks_total ?? 0) > 0;
        return (
          <div key={h.channel_id} style={{ display: "flex", gap: 8, fontSize: "11px", fontFamily: "var(--ap-font-mono)", alignItems: "center" }}>
            <span className={`dot ${h.online ? "ok" : "down"}`} />
            <span style={{ width: 56, color: "var(--ap-text-secondary)" }}>{h.channel_id}</span>
            <span style={{ flex: 1 }}>{h.role}</span>
            <span style={{ color: "var(--ap-text-dim)" }}>
              {h.online
                ? (h.messages_per_second > 0
                    ? `${h.messages_per_second.toFixed(0)} messages/sec`
                    : "Monitoring — Listening")
                : isSim
                  ? "Simulated — Active"
                  : "Offline"}
            </span>
          </div>
        );
      })}

      <canvas ref={canvasRef} style={{ width: "100%", height: 110, marginTop: 10, borderRadius: "4px", border: "1px solid var(--ap-border)" }} />

      <div className="gauge-row">
        <span style={{ width: 130 }}>Response Time</span>
        <div className={`gauge-bar ${latClass}`}>
          <div style={{ width: `${latFrac * 100}%` }} />
        </div>
        <span style={{ width: 90, textAlign: "right", fontFamily: "var(--ap-font-mono)", fontSize: "11px" }}>
          {latencyUs.toFixed(0)} µs
        </span>
      </div>

      <div className="gauge-row">
        <span style={{ width: 130 }}>Data Recording</span>
        <div className="gauge-bar">
          <div style={{ width: `${Math.min(100, (status?.duckdb_writes_per_sec ?? 0) / 3)}%` }} />
        </div>
        <span style={{ width: 90, textAlign: "right", fontFamily: "var(--ap-font-mono)", fontSize: "11px" }}>
          {(status?.duckdb_writes_per_sec ?? 0).toFixed(0)} per sec
        </span>
      </div>

      <div className="gauge-row">
        <span style={{ width: 130 }}>Aircraft Tracked</span>
        <div className="gauge-bar">
          <div
            style={{
              width: `${Math.min(100, ((status?.tracks_total ?? 0) / 60) * 100)}%`,
            }}
          />
        </div>
        <span style={{ width: 90, textAlign: "right", fontFamily: "var(--ap-font-mono)", fontSize: "11px" }}>
          {status?.tracks_total ?? 0} aircraft{status?.stca_pairs_active ? ` · ${status.stca_pairs_active} conflict${status.stca_pairs_active > 1 ? "s" : ""}` : ""}
        </span>
      </div>
    </div>
  );
}
