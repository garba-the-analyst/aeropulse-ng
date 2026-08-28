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
  const [sectionOrder, setSectionOrder] = useState<string[]>(() => [...validSections]);

  const handleSelect = useCallback((icao24: string) => {
    setSelected(prev => prev === icao24 ? null : icao24);
  }, []);

  const handleTransmit = useCallback((icao24: string) => {
    const freq = frequencyForIcao(icao24);
    window.dispatchEvent(new CustomEvent("ap-tune-frequency", { detail: { freq } }));
    setActiveSection("comms");
    document.getElementById("sec-comms")?.scrollIntoView({ behavior: "smooth" });
  }, []);

  const popOut = (id: string, title: string) => {
    const hashUrl = `hud.html#${id}`;
    const absUrl = new URL(hashUrl, window.location.href).href;
    const isTauri = typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in (window as unknown as Record<string, unknown>));
    if (isTauri) {
      import("@tauri-apps/api/webviewWindow").then(({ WebviewWindow }) => {
        const label = `hud-${id}-${Date.now()}`;
        try {
          new WebviewWindow(label, { url: hashUrl, title: `AeroPulse-NG — ${title}`, width: 780, height: 680, resizable: true, center: true });
        } catch {
          window.open(absUrl, "_blank", "width=780,height=680,scrollbars=yes,resizable=yes");
        }
      }).catch(() => window.open(absUrl, "_blank", "width=780,height=680,scrollbars=yes,resizable=yes"));
      return;
    }
    const win = window.open(absUrl, "_blank", "width=780,height=680,scrollbars=yes,resizable=yes");
    if (!win) alert("Pop-out blocked — please allow pop-ups for this site.");
  };

  const handleDragStart = (id: string) => (e: React.DragEvent) => {
    e.dataTransfer.setData("text/plain", id);
    e.dataTransfer.effectAllowed = "move";
  };
  const handleDragOver = (e: React.DragEvent) => { e.preventDefault(); e.dataTransfer.dropEffect = "move"; };
  const handleDrop = (targetId: string) => (e: React.DragEvent) => {
    e.preventDefault();
    const src = e.dataTransfer.getData("text/plain");
    if (!src || src === targetId) return;
    setSectionOrder(prev => {
      const arr = [...prev];
      const sIdx = arr.indexOf(src);
      const tIdx = arr.indexOf(targetId);
      if (sIdx === -1 || tIdx === -1) return prev;
      arr.splice(sIdx, 1);
      arr.splice(tIdx, 0, src);
      return arr;
    });
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
    const updateLayout = () => setCompactLayout(isPopOut || widthQuery.matches || heightQuery.matches);
    widthQuery.addEventListener("change", updateLayout);
    heightQuery.addEventListener("change", updateLayout);
    return () => {
      widthQuery.removeEventListener("change", updateLayout);
      heightQuery.removeEventListener("change", updateLayout);
    };
  }, [isPopOut]);

  const sectionsMap: Record<string, [string, React.ReactNode]> = {
    strips: ["Flight Strips", <FlightStripTable selectedIcao={selected} onSelect={handleSelect} />],
    detail: ["Aircraft", <AircraftDetailPanel icao24={selected} onClear={() => setSelected(null)} onTransmit={handleTransmit} />],
    weather: ["Weather", <WeatherGridHUD />],
    alerts: ["Alerts", <ThreatMatrix />],
    comms: ["Audio And Communications", <AudioRoutingPanel selectedIcao={selected} />],
    system: ["System", <SystemMetrics />],
  };

  const isSinglePanel = isPopOut && validSections.includes(initialSection);
  const orderedSections = isSinglePanel ? [[initialSection, ...sectionsMap[initialSection]] as [string, string, React.ReactNode]] : sectionOrder.map(id => [id, ...sectionsMap[id]] as [string, string, React.ReactNode]);

  return (
    <div className="hud-root">
      {!isSinglePanel && <MasterCommandBar rangeKm={150} onRangeChange={() => undefined} showEmergency={false} />}
      {isSinglePanel ? (
        <div style={{ padding: "8px 12px", background: "var(--ap-panel)", borderBottom: "1px solid var(--ap-border)", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", fontWeight: 600, letterSpacing: "0.06em", textTransform: "uppercase", color: "var(--ap-text-dim)" }}>{String(sectionsMap[initialSection]?.[0] ?? initialSection)}</span>
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
              onClick={() => {
                setActiveSection(id);
                document.getElementById(`sec-${id}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
              }}
            >
              {label}
            </button>
          ))}
        </nav>
      )}
      <div className={`hud-body ${isSinglePanel ? "single" : compactLayout ? "compact" : "full"}`} id="hud-scroll">
        {orderedSections.map(([id, label, content]) => (
          <div
            className={`hud-section hud-${id}`}
            id={`sec-${id}`}
            key={id}
            draggable={!isSinglePanel}
            onDragStart={handleDragStart(id)}
            onDragOver={handleDragOver}
            onDrop={handleDrop(id)}
          >
            <div className="panel-header-actions" draggable={!isSinglePanel} onDragStart={handleDragStart(id)} style={{ cursor: isSinglePanel ? "default" : "grab" }}>
              <span style={{ fontSize: "10px", color: "var(--ap-text-dim)", letterSpacing: "0.04em", textTransform: "uppercase", display: "flex", alignItems: "center", gap: "6px" }}>
                {!isSinglePanel && <span style={{ opacity: 0.5, cursor: "grab" }}>⋮⋮</span>} {String(label)}
              </span>
              {!isSinglePanel && <button className="popout-btn" onClick={() => popOut(String(id), String(label))} title="Open in separate window">Pop Out ↗</button>}
            </div>
            {content}
          </div>
        ))}
      </div>
    </div>
  );
}
