/**
 * Audio Routing & Transmission Safety — VHF airband voice
 * En_UK plain language, restrained styling.
 * Routes to two physical outputs, validates frequency, and
 * simulates VHF AM static for bench testing.
 */
import { useEffect, useRef, useState } from "react";
import { useTelemetry } from "../../hooks/useFlightTracks";

type AudioDevice = { deviceId: string; label: string };

const VHF_MIN = 118.0;
const VHF_MAX = 136.975;
const TX_TIMEOUT_MS = 30000;

function isValidFrequency(mhz: number): boolean {
  if (mhz < VHF_MIN || mhz > VHF_MAX) return false;
  const khz = Math.round(mhz * 1000);
  // 25 kHz raster check (8.33 is 25/3, allow tolerance)
  if (khz % 25 === 0) return true;
  const remainder = Math.abs((mhz * 1000) % 8.333);
  return remainder < 0.01 || remainder > 8.32;
}

function frequencyForIcao(icao24: string): string {
  let h = 0; for (let i = 0; i < icao24.length; i++) h = (h * 31 + icao24.charCodeAt(i)) >>> 0;
  const step = h % 760; const khz = 118000 + step * 25;
  return `${(khz/1000).toFixed(3)}`;
}

interface Props {
  selectedIcao?: string | null;
}

export default function AudioRoutingPanel({ selectedIcao }: Props = {}) {
  const snap = useTelemetry(2);
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [primaryId, setPrimaryId] = useState<string>("");
  const [secondaryId, setSecondaryId] = useState<string>("");
  const [primaryVol, setPrimaryVol] = useState(80);
  const [secondaryVol, setSecondaryVol] = useState(70);
  const [freq, setFreq] = useState<string>("118.100");
  const [mode, setMode] = useState<"general" | "aircraft">("general");
  const [squelch, setSquelch] = useState(42);
  const [isTransmitting, setIsTransmitting] = useState(false);
  const [txElapsed, setTxElapsed] = useState(0);
  const [txBlockedReason, setTxBlockedReason] = useState<string | null>(null);
  const [staticEnabled, setStaticEnabled] = useState(true);
  const txTimerRef = useRef<number | null>(null);
  const elapsedRef = useRef<number | null>(null);
  const audioCtxRef = useRef<AudioContext | null>(null);
  const gainRef = useRef<GainNode | null>(null);

  const freqNum = parseFloat(freq);
  const freqValid = !isNaN(freqNum) && isValidFrequency(freqNum);
  const hasEmergency = (snap?.tracks ?? []).some(t => t.alert === "emergency");
  const hasHardware = (snap?.status.hardware ?? []).some(h => h.online);
  const selectedTrack = snap?.tracks.find(t => t.icao24 === selectedIcao);
  const aircraftFreq = selectedIcao ? frequencyForIcao(selectedIcao) : null;

  // Enumerate outputs
  useEffect(() => {
    async function load() {
      try {
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

  // Simulated VHF static — Web Audio white noise through band-pass
  useEffect(() => {
    if (!staticEnabled) return;
    try {
      const ctx = new (window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext)();
      audioCtxRef.current = ctx;
      const bufferSize = 4096;
      const processor = ctx.createScriptProcessor(bufferSize, 1, 1);
      const filter = ctx.createBiquadFilter();
      filter.type = "bandpass";
      filter.frequency.value = 1200;
      filter.Q.value = 0.7;
      const gain = ctx.createGain();
      gain.gain.value = 0.02 * (primaryVol / 100) * (squelch / 60);
      gainRef.current = gain;
      processor.onaudioprocess = (e) => {
        if (isTransmitting) return;
        const output = e.outputBuffer.getChannelData(0);
        for (let i = 0; i < bufferSize; i++) {
          // White noise with occasional squelch burst
          output[i] = (Math.random() * 2 - 1) * 0.15 + Math.sin(Date.now()/180 + i*0.1) * 0.02;
        }
      };
      processor.connect(filter);
      filter.connect(gain);
      gain.connect(ctx.destination);
      return () => { try { processor.disconnect(); filter.disconnect(); gain.disconnect(); ctx.close(); } catch {} };
    } catch { /* Web Audio unavailable */ }
    return () => {};
  }, [staticEnabled, primaryVol, squelch, isTransmitting]);

  useEffect(() => {
    if (gainRef.current) gainRef.current.gain.value = isTransmitting ? 0 : 0.02 * (primaryVol / 100) * (squelch / 60);
  }, [primaryVol, squelch, isTransmitting]);

  function startTx() {
    if (!freqValid) { setTxBlockedReason(`Frequency must be ${VHF_MIN.toFixed(3)}–${VHF_MAX.toFixed(3)} MHz`); return; }
    if (hasEmergency && mode === "general") { setTxBlockedReason("Transmission inhibited — emergency in progress. Select a specific aircraft to transmit."); return; }
    if (mode === "aircraft" && !selectedIcao) { setTxBlockedReason("Select an aircraft first"); return; }
    // Prototype: show warning but allow after second hold in real hardware check
    if (!hasHardware) {
      // Allow simulated transmission for demo
      setTxBlockedReason(null);
    } else {
      setTxBlockedReason(null);
    }
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

  useEffect(() => {
    const handler = (e: Event) => {
      const f = (e as CustomEvent<{ freq: string }>).detail?.freq;
      if (f) { setFreq(f); setMode("aircraft"); }
    };
    window.addEventListener("ap-tune-frequency", handler as EventListener);
    return () => window.removeEventListener("ap-tune-frequency", handler as EventListener);
  }, []);

  const activeFreq = mode === "aircraft" && aircraftFreq ? aircraftFreq : freq;
  const activeValid = mode === "aircraft" && aircraftFreq ? true : freqValid;

  return (
    <div className="panel" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <audio ref={primaryAudioRef} autoPlay muted style={{ display: "none" }} />
      <audio ref={secondaryAudioRef} autoPlay muted style={{ display: "none" }} />
      <h2>Audio and Communications</h2>
      <p style={{ fontSize: "11px", color: "var(--ap-text-dim)", lineHeight: 1.5, marginTop: "-4px" }}>
        Routes VHF airband audio to two outputs. Simulated static is audible when squelch is open.
      </p>

      {/* Mode toggle */}
      <div style={{ display: "flex", gap: 6, background: "var(--ap-panel-raised)", padding: 4, borderRadius: 4, border: "1px solid var(--ap-border)" }}>
        <button onClick={() => setMode("general")} className={mode === "general" ? "active" : ""} style={{ flex: 1, padding: "6px", fontSize: "11px" }}>General — All Channels</button>
        <button onClick={() => setMode("aircraft")} className={mode === "aircraft" ? "active" : ""} style={{ flex: 1, padding: "6px", fontSize: "11px" }}>Selected Aircraft</button>
      </div>
      {mode === "aircraft" && (
        <div style={{ fontSize: "11px", color: selectedTrack ? "var(--ap-text)" : "var(--ap-text-dim)", background: "var(--ap-panel-raised)", padding: "6px 8px", borderRadius: 4, border: "1px solid var(--ap-border)" }}>
          {selectedTrack ? `${selectedTrack.callsign} — ${aircraftFreq} MHz · Squawk ${selectedTrack.squawk}` : "No aircraft selected — select a flight strip or radar contact"}
        </div>
      )}

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
            value={mode === "aircraft" && aircraftFreq ? aircraftFreq : freq}
            onChange={e => setFreq(e.target.value)}
            disabled={mode === "aircraft" && !!aircraftFreq}
            style={{ width: "100%", marginTop: 4, borderColor: activeValid ? "var(--ap-border)" : "var(--ap-critical)", fontFamily: "var(--ap-font-mono)", opacity: mode === "aircraft" && aircraftFreq ? 0.7 : 1 }}
            placeholder="118.100"
          />
        </label>
        <label style={{ fontSize: "11px", fontWeight: 500, color: "var(--ap-text-dim)" }}>
          Squelch — {squelch} dB
          <input type="range" min={20} max={80} value={squelch} onChange={e => setSquelch(Number(e.target.value))} style={{ width: "100%", marginTop: 4 }} />
        </label>
      </div>
      {!activeValid && <span style={{ fontSize: "11px", color: "var(--ap-critical)" }}>Enter a valid VHF airband channel: 118.000–136.975 MHz</span>}
      {txBlockedReason && <div style={{ fontSize: "11px", color: "var(--ap-warning)", background: "rgba(217,164,65,0.1)", border: "1px solid rgba(217,164,65,0.3)", borderRadius: "4px", padding: "6px 8px" }}>{txBlockedReason}</div>}

      <label style={{ display: "flex", gap: 6, alignItems: "center", fontSize: "11px", color: "var(--ap-text-dim)" }}>
        <input type="checkbox" checked={staticEnabled} onChange={e => setStaticEnabled(e.target.checked)} />
        Simulated VHF static (bench testing)
      </label>

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
          opacity: (!activeValid) ? 0.5 : 1,
        }}
      >
        {isTransmitting ? `● Transmitting to ${mode === "aircraft" && selectedTrack ? selectedTrack.callsign : "All"} — ${Math.ceil((TX_TIMEOUT_MS - txElapsed)/1000)}s` : mode === "aircraft" && selectedTrack ? `Hold to Transmit to ${selectedTrack.callsign}` : "Hold to Transmit — Push to Talk"}
      </button>
      <span style={{ fontSize: "10px", color: "var(--ap-text-dim)", textAlign: "center" }}>
        {mode === "aircraft" && selectedTrack ? `Tuned to ${aircraftFreq} MHz for ${selectedTrack.callsign}` : "General broadcast on selected frequency"} · Auto-release after 30 seconds
      </span>

      {/* VU meters */}
      <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: "11px", color: "var(--ap-text-dim)" }}>
        <span style={{ minWidth: 48 }}>Receive</span>
        <div className="gauge-bar" style={{ flex: 1 }}>
          <div style={{ width: `${Math.min(100, squelch + (isTransmitting ? 95 : Math.random()*12))}%`, background: isTransmitting ? "var(--ap-critical)" : "var(--ap-success)" }} />
        </div>
        <span style={{ fontFamily: "var(--ap-font-mono)", minWidth: 40 }}>{isTransmitting ? "TX" : `${squelch} dB`}</span>
      </div>
    </div>
  );
}
