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
  const [selected, setSelected] = useState<string | null>(null);
  const [activeSection, setActiveSection] = useState("strips");
  const [compactLayout, setCompactLayout] = useState(() =>
    typeof window !== "undefined" &&
    (window.matchMedia("(max-width: 1400px)").matches ||
      window.matchMedia("(max-height: 850px)").matches),
  );

  const handleSelect = useCallback((icao24: string) => {
    setSelected(prev => prev === icao24 ? null : icao24);
  }, []);

  const handleTransmit = useCallback((icao24: string) => {
    const freq = frequencyForIcao(icao24);
    window.dispatchEvent(new CustomEvent("ap-tune-frequency", { detail: { freq } }));
    setActiveSection("comms");
  }, []);

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

  const sections = [
    ["strips", <FlightStripTable selectedIcao={selected} onSelect={handleSelect} />],
    ["detail", <AircraftDetailPanel icao24={selected} onClear={() => setSelected(null)} onTransmit={handleTransmit} />],
    ["weather", <WeatherGridHUD />],
    ["alerts", <ThreatMatrix />],
    ["comms", <AudioRoutingPanel />],
    ["system", <SystemMetrics />],
  ] as const;

  return (
    <div className="hud-root">
      <MasterCommandBar rangeKm={150} onRangeChange={() => undefined} showEmergency={false} />
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
            aria-current={activeSection === id ? "page" : undefined}
            onClick={() => setActiveSection(id)}
          >
            {label}
          </button>
        ))}
      </nav>
      <div className={`hud-body ${compactLayout ? "compact" : "full"}`} id="hud-scroll">
        {sections.map(([id, content]) => {
          if (compactLayout && activeSection !== id) return null;
          return (
            <div className={`hud-section hud-${id} active`} id={`sec-${id}`} key={id}>
              {content}
            </div>
          );
        })}
      </div>
    </div>
  );
}
