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
            title="Range & Bearing ruler"
          >
            R&B RULER
          </button>
          <button onClick={() => void openOpsHud()} title="Open Display 2">
            OPS HUD
          </button>
          <span
            style={{
              color: stcaCount > 0 ? "var(--ap-alert)" : "var(--ap-text-dim)",
              fontWeight: stcaCount > 0 ? "bold" : "normal",
              animation: stcaCount > 0 ? "blink 1s steps(2) infinite" : undefined,
              padding: "4px 2px",
            }}
          >
            STCA {stcaCount}
          </span>
        </div>

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
