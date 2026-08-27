import { useEffect, useState } from "react";
import MasterCommandBar from "./components/shared/MasterCommandBar";
import RadarCanvas from "./components/radar/RadarCanvas";
import InterceptPanel from "./components/defense/InterceptPanel";
import { useTelemetry } from "./hooks/useFlightTracks";
import { openOpsHud } from "./lib/ipc";

type Tool = "none" | "ruler";

export default function App() {
  const [rangeKm, setRangeKm] = useState(280);
  const [tool, setTool] = useState<Tool>("none");
  const snap = useTelemetry(2);

  useEffect(() => {
    // Auto-open the operations HUD alongside the radar workspace.
    void openOpsHud();
  }, []);

  const stcaCount = snap?.stca_alerts.length ?? 0;

  return (
    <div className="radar-root">
      <MasterCommandBar rangeKm={rangeKm} onRangeChange={setRangeKm} />
      <div className="radar-body">
        <div className="toolbar">
          <button
            className={tool === "ruler" ? "active" : ""}
            onClick={() => setTool(tool === "ruler" ? "none" : "ruler")}
            title="Measure distance and bearing between points — click to set start, move to measure, double-click to clear"
          >
            {tool === "ruler" ? "Measuring — Click Again" : "Measure Distance"}
          </button>
          <button onClick={() => void openOpsHud()} title="Open operations display in new window">
            Operations Display
          </button>
          <span
            style={{
              color: stcaCount > 0 ? "var(--ap-critical)" : "var(--ap-text-dim)",
              fontWeight: stcaCount > 0 ? "600" : "400",
              fontFamily: "var(--ap-font-mono)",
              fontSize: "12px",
              padding: "6px 8px",
              border: "1px solid var(--ap-border)",
              borderRadius: "4px",
              background: stcaCount > 0 ? "rgba(229,72,77,0.1)" : "transparent",
            }}
          >
            {stcaCount === 0 ? "No Conflicts" : `${stcaCount} Conflict${stcaCount > 1 ? "s" : ""}`}
          </span>
        </div>
        {tool === "ruler" && (
          <div style={{ position: "absolute", bottom: 16, left: "50%", transform: "translateX(-50%)", background: "var(--ap-panel)", border: "1px solid var(--ap-border)", borderRadius: "4px", padding: "6px 12px", fontSize: "11px", color: "var(--ap-text-secondary)", zIndex: 5 }}>
            Click on the radar to set start point — move cursor to measure — double-click to clear
          </div>
        )}

        <RadarCanvas rangeKm={rangeKm} rulerMode={tool === "ruler"} />

        <div
          style={{
            position: "absolute",
            right: 12,
            top: 12,
            width: 300,
            zIndex: 5,
          }}
        >
          <InterceptPanel />
        </div>

        {/* STCA full-frame alert border */}
        <div
          style={{
            position: "absolute",
            inset: 0,
            pointerEvents: "none",
            border:
              stcaCount > 0 && Date.now() % 1000 < 500
                ? "2px solid var(--ap-alert)"
                : "2px solid transparent",
          }}
        />
      </div>
    </div>
  );
}
