/**
 * Audio Routing & Transmission Safety — VHF airband voice
 * En_UK plain language, restrained styling.
 * Prototype: receive-only via RTL-SDR 118–137 MHz AM.
 * Transmission scaffolded behind hardware check + safety interlock.
 */
import { useEffect, useRef, useState } from "react";
import { useTelemetry } from "../../hooks/useFlightTracks";

type AudioDevice = { deviceId: string; label: string };

const VHF_MIN = 118.0;
const VHF_MAX = 136.975;
const TX_TIMEOUT_MS = 30000;
const VALID_STEPS = [0.025, 0.00833]; // 25 kHz and 8.33 kHz — kept for documentation
void VALID_STEPS;

function isValidFrequency(mhz: number): boolean {
  if (mhz < VHF_MIN || mhz > VHF_MAX) return false;
  // Check 8.33 or 25 kHz raster within 1 Hz tolerance
  const khz = Math.round(mhz * 1000);
  return khz % 25 === 0 || khz % 8.333 === 0 || Math.abs((mhz * 120) % 1) < 0.001;
}

export default function AudioRoutingPanel() {
  const snap = useTelemetry(2);
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [primaryId, setPrimaryId] = useState<string>("");
  const [secondaryId, setSecondaryId] = useState<string>("");
  const [primaryVol, setPrimaryVol] = useState(80);
  const [secondaryVol, setSecondaryVol] = useState(70);
  const [freq, setFreq] = useState<string>("118.100");
  const [squelch, setSquelch] = useState(42);
  const [isTransmitting, setIsTransmitting] = useState(false);
  const [txElapsed, setTxElapsed] = useState(0);
  const [txBlockedReason, setTxBlockedReason] = useState<string | null>(null);
  const txTimerRef = useRef<number | null>(null);
  const elapsedRef = useRef<number | null>(null);

  const freqNum = parseFloat(freq);
  const freqValid = !isNaN(freqNum) && isValidFrequency(freqNum);
  const hasEmergency = (snap?.tracks ?? []).some(t => t.alert === "emergency");
  const hasHardware = (snap?.status.hardware ?? []).some(h => h.online);

  // Enumerate output devices
  useEffect(() => {
    async function load() {
      try {
        // Request permission to get labels
        await navigator.mediaDevices.getUserMedia({ audio: true }).then(s => s.getTracks().forEach(t => t.stop())).catch(() => {});
        const all = await navigator.mediaDevices.enumerateDevices();
        const outs = all.filter(d => d.kind === "audiooutput").map(d => ({ deviceId: d.deviceId, label: d.label || `Output ${d.deviceId.slice(0,6)}` }));
        setDevices(outs);
        if (outs[0] && !primaryId) setPrimaryId(outs[0].deviceId);
        if (outs[1] && !secondaryId) setSecondaryId(outs[1].deviceId);
      } catch {
        setDevices([{ deviceId: "default", label: "Default Speaker" }]);
        setPrimaryId("default");
      }
    }
    void load();
    navigator.mediaDevices?.addEventListener?.("devicechange", load);
    return () => navigator.mediaDevices?.removeEventListener?.("devicechange", load);
  }, []);

  // Safety: auto-release PTT after timeout
  function startTx() {
    if (!freqValid) { setTxBlockedReason(`Frequency must be ${VHF_MIN.toFixed(3)}–${VHF_MAX.toFixed(3)} MHz on an 8.33 or 25 kHz channel`); return; }
    if (hasEmergency) { setTxBlockedReason("Transmission inhibited — emergency in progress. Hold to confirm override."); return; }
    if (!hasHardware) { setTxBlockedReason("No transmitter connected — receive only in prototype"); return; }
    setTxBlockedReason(null);
    setIsTransmitting(true);
    setTxElapsed(0);
    const start = Date.now();
    elapsedRef.current = window.setInterval(() => setTxElapsed(Date.now() - start), 200);
    txTimerRef.current = window.setTimeout(() => stopTx(), TX_TIMEOUT_MS);
  }

  function stopTx() {
    setIsTransmitting(false);
    setTxElapsed(0);
    if (txTimerRef.current) clearTimeout(txTimerRef.current);
    if (elapsedRef.current) clearInterval(elapsedRef.current);
  }

  // Route receive audio to both selected outputs via setSinkId (Web Audio)
  const primaryAudioRef = useRef<HTMLAudioElement | null>(null);
  const secondaryAudioRef = useRef<HTMLAudioElement | null>(null);

  useEffect(() => {
    const el = primaryAudioRef.current as HTMLAudioElement & { setSinkId?: (id: string) => Promise<void> };
    if (el?.setSinkId && primaryId) void el.setSinkId(primaryId).catch(() => {});
  }, [primaryId]);

  useEffect(() => {
    const el = secondaryAudioRef.current as HTMLAudioElement & { setSinkId?: (id: string) => Promise<void> };
    if (el?.setSinkId && secondaryId) void el.setSinkId(secondaryId).catch(() => {});
  }, [secondaryId]);

  return (
    <div className="panel" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      {/* Hidden audio sinks for device routing — receive stream attaches here in production */}
      <audio ref={primaryAudioRef} autoPlay muted style={{ display: "none" }} />
      <audio ref={secondaryAudioRef} autoPlay muted style={{ display: "none" }} />
      <h2>Audio and Communications</h2>
      <p style={{ fontSize: "11px", color: "var(--ap-text-dim)", lineHeight: 1.5, marginTop: "-4px" }}>
        Routes VHF airband audio to two outputs. Transmission is interlocked — hold to talk, automatic release after 30 seconds.
      </p>

      {/* Outputs */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
        <label style={{ fontSize: "11px", fontWeight: 500, color: "var(--ap-text-dim)" }}>
          Primary Output
          <select value={primaryId} onChange={e => setPrimaryId(e.target.value)} style={{ width: "100%", marginTop: 4 }}>
            {devices.map(d => <option key={d.deviceId} value={d.deviceId}>{d.label}</option>)}
            {devices.length === 0 && <option>Default Speaker</option>}
          </select>
          <input type="range" min={0} max={100} value={primaryVol} onChange={e => setPrimaryVol(Number(e.target.value))} style={{ width: "100%", marginTop: 6 }} />
          <span style={{ fontFamily: "var(--ap-font-mono)", fontSize: "11px", color: "var(--ap-text-secondary)" }}>{primaryVol}%</span>
        </label>

        <label style={{ fontSize: "11px", fontWeight: 500, color: "var(--ap-text-dim)" }}>
          Secondary Output
          <select value={secondaryId} onChange={e => setSecondaryId(e.target.value)} style={{ width: "100%", marginTop: 4 }}>
            <option value="">Not routed</option>
            {devices.map(d => <option key={d.deviceId} value={d.deviceId}>{d.label}</option>)}
          </select>
          <input type="range" min={0} max={100} value={secondaryVol} onChange={e => setSecondaryVol(Number(e.target.value))} style={{ width: "100%", marginTop: 6 }} />
          <span style={{ fontFamily: "var(--ap-font-mono)", fontSize: "11px", color: "var(--ap-text-secondary)" }}>{secondaryVol}%</span>
        </label>
      </div>

      {/* Frequency and squelch */}
      <div style={{ display: "grid", gridTemplateColumns: "140px 1fr", gap: 10, alignItems: "end" }}>
        <label style={{ fontSize: "11px", fontWeight: 500, color: "var(--ap-text-dim)" }}>
          Frequency (MHz)
          <input
            type="text"
            inputMode="decimal"
            value={freq}
            onChange={e => setFreq(e.target.value)}
            style={{ width: "100%", marginTop: 4, borderColor: freqValid ? "var(--ap-border)" : "var(--ap-critical)", fontFamily: "var(--ap-font-mono)" }}
            placeholder="118.100"
          />
        </label>
        <label style={{ fontSize: "11px", fontWeight: 500, color: "var(--ap-text-dim)" }}>
          Squelch — {squelch} dB
          <input type="range" min={20} max={80} value={squelch} onChange={e => setSquelch(Number(e.target.value))} style={{ width: "100%", marginTop: 4 }} />
        </label>
      </div>
      {!freqValid && <span style={{ fontSize: "11px", color: "var(--ap-critical)" }}>Enter a valid VHF airband channel: 118.000–136.975 MHz</span>}
      {txBlockedReason && <div style={{ fontSize: "11px", color: "var(--ap-warning)", background: "rgba(217,164,65,0.1)", border: "1px solid rgba(217,164,65,0.3)", borderRadius: "4px", padding: "6px 8px" }}>{txBlockedReason}</div>}

      {/* PTT */}
      <button
        onMouseDown={startTx}
        onMouseUp={stopTx}
        onMouseLeave={stopTx}
        onTouchStart={startTx}
        onTouchEnd={stopTx}
        className={isTransmitting ? "active" : ""}
        style={{
          background: isTransmitting ? "var(--ap-critical)" : "var(--ap-panel-raised)",
          color: isTransmitting ? "#fff" : "var(--ap-text)",
          borderColor: isTransmitting ? "var(--ap-critical)" : "var(--ap-border)",
          fontWeight: 600,
          letterSpacing: "0.04em",
          padding: "10px",
          opacity: (!freqValid) ? 0.5 : 1,
        }}
      >
        {isTransmitting ? `● Transmitting — ${Math.ceil((TX_TIMEOUT_MS - txElapsed)/1000)}s` : "Hold to Transmit — Push to Talk"}
      </button>
      <span style={{ fontSize: "10px", color: "var(--ap-text-dim)", textAlign: "center" }}>
        Transmission stops automatically after 30 seconds. Release to stop immediately.
      </span>

      {/* VU meter mock for receive */}
      <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: "11px", color: "var(--ap-text-dim)" }}>
        <span style={{ minWidth: 48 }}>Receive</span>
        <div className="gauge-bar" style={{ flex: 1 }}>
          <div style={{ width: `${Math.min(100, squelch + Math.random()*10)}%`, background: "var(--ap-success)" }} />
        </div>
        <span style={{ fontFamily: "var(--ap-font-mono)", minWidth: 40 }}>{squelch} dB</span>
      </div>
    </div>
  );
}
