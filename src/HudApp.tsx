import { useState, useCallback, useEffect } from "react";
import MasterCommandBar from "./components/shared/MasterCommandBar";
import FlightStripTable from "./components/hud/FlightStripTable";
import WeatherGridHUD from "./components/hud/WeatherGridHUD";
import SystemMetrics from "./components/hud/SystemMetrics";
import ThreatMatrix from "./components/defense/ThreatMatrix";
import AudioRoutingPanel from "./components/comms/AudioRoutingPanel";
import AircraftDetailPanel from "./components/hud/AircraftDetailPanel";

function frequencyForIcao(icao24: string): string {
  let h = 0; for (let i = 0; i < icao24.length; i++) h = (h * 31 + icao24.charCodeAt(i)) >>> 0;
  const step = h % 760; const khz = 118000 + step * 25;
  return `${(khz/1000).toFixed(3)}`;
}

export default function HudApp() {
  const isPopOut = typeof window !== "undefined" && !!window.location.hash.slice(1);
  const initialSection = typeof window !== "undefined" ? window.location.hash.slice(1) || "strips" : "strips";
  const validSections = ["strips","detail","weather","alerts","comms","system"];
  const [selected, setSelected] = useState<string | null>(null);
  const [activeSection, setActiveSection] = useState(() => validSections.includes(initialSection) ? initialSection : "strips");
  const [compactLayout, setCompactLayout] = useState(() =>
    isPopOut || (typeof window !== "undefined" &&
    (window.matchMedia("(max-width: 1400px)").matches ||
      window.matchMedia("(max-height: 850px)").matches)),
  );

  const handleSelect = useCallback((icao24: string) => {
    setSelected(prev => prev === icao24 ? null : icao24);
  }, []);

  const handleTransmit = useCallback((icao24: string) => {
    const freq = frequencyForIcao(icao24);
    window.dispatchEvent(new CustomEvent("ap-tune-frequency", { detail: { freq } }));
    setActiveSection("comms");
  }, []);

  const popOut = (id: string, title: string) => {
    const hashUrl = `hud.html#${id}`;
    const absUrl = new URL(hashUrl, window.location.href).href;
    const isTauri = typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in (window as unknown as Record<string, unknown>));
    if (isTauri) {
      import("@tauri-apps/api/webviewWindow").then(({ WebviewWindow }) => {
        const label = `hud-${id}-${Date.now()}`;
        try {
          const win = new WebviewWindow(label, {
            url: hashUrl,
            title: `AeroPulse-NG — ${title}`,
            width: 760,
            height: 680,
            resizable: true,
            center: true,
          });
          // @ts-ignore - Tauri event may not be typed
          win.once?.("tauri://error", () => window.open(absUrl, "_blank"));
        } catch {
          window.open(absUrl, "_blank", "width=760,height=680,scrollbars=yes,resizable=yes");
        }
      }).catch(() => window.open(absUrl, "_blank", "width=760,height=680,scrollbars=yes,resizable=yes"));
      return;
    }
    // Browser fallback — synchronous to avoid popup blocker
    const win = window.open(absUrl, "_blank", "width=760,height=680,scrollbars=yes,resizable=yes");
    if (!win) alert("Pop-out blocked — please allow pop-ups for this site.");
  };

  useEffect(() => {
    const handler = (e: Event) => {
      const icao = (e as CustomEvent<string>).detail;
      if (icao) setSelected(icao);
    };
    window.addEventListener("ap-select-aircraft", handler as EventListener);
    return () => window.removeEventListener("ap-select-aircraft", handler as EventListener);
  }, []);

  useEffect(() => {
    const widthQuery = window.matchMedia("(max-width: 1400px)");
    const heightQuery = window.matchMedia("(max-height: 850px)");
    const updateLayout = () => setCompactLayout(widthQuery.matches || heightQuery.matches);
    widthQuery.addEventListener("change", updateLayout);
    heightQuery.addEventListener("change", updateLayout);
    return () => {
      widthQuery.removeEventListener("change", updateLayout);
      heightQuery.removeEventListener("change", updateLayout);
    };
  }, []);

  const sections: Array<[string, string, React.ReactNode]> = [
    ["strips", "Flight Strips", <FlightStripTable selectedIcao={selected} onSelect={handleSelect} />],
    ["detail", "Aircraft", <AircraftDetailPanel icao24={selected} onClear={() => setSelected(null)} onTransmit={handleTransmit} />],
    ["weather", "Weather", <WeatherGridHUD />],
    ["alerts", "Alerts", <ThreatMatrix />],
    ["comms", "Audio And Communications", <AudioRoutingPanel selectedIcao={selected} />],
    ["system", "System", <SystemMetrics />],
  ];

  // If this window was opened as a pop-out, lock to that single section
  const isSinglePanel = isPopOut && validSections.includes(initialSection);

  return (
    <div className="hud-root">
      {!isSinglePanel && <MasterCommandBar rangeKm={150} onRangeChange={() => undefined} showEmergency={false} />}
      {isSinglePanel ? (
        <div style={{ padding: "8px 12px", background: "var(--ap-panel)", borderBottom: "1px solid var(--ap-border)", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", fontWeight: 600, letterSpacing: "0.06em", textTransform: "uppercase", color: "var(--ap-text-dim)" }}>{String(sections.find(s => s[0] === initialSection)?.[1] ?? initialSection)}</span>
          <button onClick={() => window.close()} style={{ padding: "4px 10px", fontSize: "11px" }}>Close</button>
        </div>
      ) : (
        <nav className="hud-nav">
          {[
            ["strips", "Flight Strips"],
            ["detail", "Aircraft"],
            ["weather", "Weather"],
            ["alerts", "Alerts"],
            ["comms", "Audio"],
            ["system", "System"],
          ].map(([id, label]) => (
            <button
              key={id}
              className={activeSection === id ? "active" : ""}
              onClick={() => setActiveSection(id)}
            >
              {label}
            </button>
          ))}
        </nav>
      )}
      <div className={`hud-body ${isSinglePanel ? "single" : compactLayout ? "compact" : "full"}`} id="hud-scroll">
        {sections.map(([id, label, content]) => {
          if (isSinglePanel && id !== initialSection) return null;
          return (
            <div className={`hud-section hud-${id}`} id={`sec-${id}`} key={id}>
              <div className="panel-header-actions">
                <span style={{ fontSize: "10px", color: "var(--ap-text-dim)", letterSpacing: "0.04em", textTransform: "uppercase" }}>{String(label)}</span>
                <button className="popout-btn" onClick={() => popOut(String(id), String(label))} title="Open this panel in a separate window">Pop Out ↗</button>
              </div>
              {content}
            </div>
          );
        })}
      </div>
    </div>
  );
}
