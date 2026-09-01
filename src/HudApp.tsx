import { useState, useCallback, useEffect, useRef } from "react";
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
  const validSections = ["strips","detail","weather","alerts","comms","system"];
  const getHashPanel = () => {
    if (typeof window === "undefined") return "";
    if (window.location.hash && window.location.hash.length > 1) return window.location.hash.slice(1).split("?")[0].split("&")[0];
    const href = window.location.href;
    const idx = href.indexOf("#");
    if (idx !== -1 && idx < href.length - 1) return href.slice(idx + 1).split("?")[0].split("&")[0];
    try {
      const sp = new URLSearchParams(window.location.search);
      const q = sp.get("panel") || sp.get("id");
      if (q && (validSections as string[]).includes(q)) return q;
    } catch { /* ignore */ }
    return "";
  };
  const isPopOut = typeof window !== "undefined" && !!getHashPanel();
  const initialSection = typeof window !== "undefined" ? getHashPanel() || "strips" : "strips";
  const [selected, setSelected] = useState<string | null>(null);
  const [activeSection, setActiveSection] = useState(() => validSections.includes(initialSection) ? initialSection : "strips");
  const [compactLayout, setCompactLayout] = useState(() =>
    isPopOut || (typeof window !== "undefined" &&
    (window.matchMedia("(max-width: 1400px)").matches ||
      window.matchMedia("(max-height: 850px)").matches)),
  );
  const [sectionOrder, setSectionOrder] = useState<string[]>(() => [...validSections]);
  const hudBodyRef = useRef<HTMLDivElement>(null);
  const [freeEnabled] = useState(true);
  const [panes, setPanes] = useState<Record<string, { x: number; y: number; w: number; h: number; z: number }>>({});
  const [zTick, setZTick] = useState(10);
  const isFree = freeEnabled && !isPopOut && Object.keys(panes).length > 0;
  const bringToFront = useCallback((id: string) => {
    setPanes(p => (p[id] ? { ...p, [id]: { ...p[id], z: zTick } } : p));
    setZTick(v => v + 1);
  }, [zTick]);

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
    const hashUrlQ = `hud.html?panel=${id}#${id}`;
    try { localStorage.setItem("ap-popout-panel", id); } catch { /* ignore */ }
    try { sessionStorage.setItem("ap-popout-panel", id); } catch { /* ignore */ }
    let absUrl: string;
    try { absUrl = new URL(hashUrlQ, window.location.href).href; } catch { absUrl = `${window.location.origin}/hud.html?panel=${id}#${id}`; }
    if (absUrl.startsWith("tauri://") || absUrl.startsWith("https://tauri.localhost")) {
      absUrl = `http://localhost:1420/hud.html?panel=${id}#${id}`;
    }
    const hashUrlForTauri = hashUrlQ;
    const isTauri = typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in (window as unknown as Record<string, unknown>));
    if (isTauri) {
      import("@tauri-apps/api/webviewWindow").then(({ WebviewWindow }) => {
        const label = `hud-${id}-${Date.now()}`;
        try {
          new WebviewWindow(label, { url: hashUrlForTauri, title: `AeroPulse-NG — ${title}`, width: 900, height: 700, resizable: true, center: true, visible: true });
        } catch {
          window.open(absUrl, "_blank", "width=900,height=700,scrollbars=yes,resizable=yes");
        }
      }).catch(() => window.open(absUrl, "_blank", "width=900,height=700,scrollbars=yes,resizable=yes"));
      return;
    }
    const win = window.open(absUrl, "_blank", "width=900,height=700,scrollbars=yes,resizable=yes");
    if (!win) alert("Pop-out blocked — please allow pop-ups for this site.");
    else win.focus?.();
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

  const onHeaderPointerDown = (id: string) => (e: React.PointerEvent) => {
    if (!isFree) return;
    if ((e.target as HTMLElement).closest(".popout-btn")) return;
    const cur = panes[id];
    if (!cur) return;
    bringToFront(id);
    const sx = cur.x, sy = cur.y, startX = e.clientX, startY = e.clientY;
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    const onMove = (ev: PointerEvent) => {
      setPanes(prev => {
        const c = prev[id]; if (!c) return prev;
        const body = hudBodyRef.current;
        const bw = body ? body.clientWidth : window.innerWidth;
        const bh = body ? body.clientHeight : 900;
        let nx = sx + ev.clientX - startX;
        let ny = sy + ev.clientY - startY;
        nx = Math.max(0, Math.min(nx, Math.max(0, bw - c.w - 2)));
        ny = Math.max(0, Math.min(ny, Math.max(0, bh - 60)));
        return { ...prev, [id]: { ...c, x: nx, y: ny } };
      });
    };
    const onUp = () => { window.removeEventListener("pointermove", onMove); window.removeEventListener("pointerup", onUp); };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };
  const onResizePointerDown = (id: string) => (e: React.PointerEvent) => {
    if (!isFree) return;
    e.stopPropagation();
    const cur = panes[id];
    if (!cur) return;
    bringToFront(id);
    const sw = cur.w, sh = cur.h, sx = e.clientX, sy = e.clientY;
    (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
    const onMove = (ev: PointerEvent) => {
      setPanes(prev => {
        const c = prev[id]; if (!c) return prev;
        let nw = sw + ev.clientX - sx;
        let nh = sh + ev.clientY - sy;
        nw = Math.max(300, Math.min(nw, 900));
        nh = Math.max(260, Math.min(nh, 850));
        return { ...prev, [id]: { ...c, w: nw, h: nh } };
      });
    };
    const onUp = () => { window.removeEventListener("pointermove", onMove); window.removeEventListener("pointerup", onUp); };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
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

  useEffect(() => {
    if (!isPopOut) {
      try { localStorage.removeItem("ap-popout-panel"); } catch { /* ignore */ }
      try { sessionStorage.removeItem("ap-popout-panel"); } catch { /* ignore */ }
    }
  }, [isPopOut]);

  // Initialize independent tiled panes — evenly fill window width
  useEffect(() => {
    if (isPopOut) return;
    const GAP = 12;
    const tile = () => {
      const body = hudBodyRef.current;
      // body may be 0 on first paint — fallback to viewport
      const bwRaw = body ? body.clientWidth : 0;
      const bw = bwRaw > 200 ? bwRaw : Math.max(900, window.innerWidth - 24);
      if (bw < 100) return;
      const cols = bw > 1400 ? 3 : bw > 900 ? 2 : 1;
      const colW = Math.max(360, Math.floor((bw - GAP * (cols + 1)) / cols));
      const rowH = 380;
      setPanes(() => {
        const init: Record<string, { x: number; y: number; w: number; h: number; z: number }> = {};
        validSections.forEach((id, i) => {
          const col = i % cols;
          const row = Math.floor(i / cols);
          init[id] = { x: GAP + col * (colW + GAP), y: GAP + row * (rowH + GAP), w: colW, h: rowH, z: i + 1 };
        });
        return init;
      });
      setZTick(validSections.length + 10);
    };
    tile();
    // Re-tile once after layout stabilizes (body gets correct width after grid→free switch)
    const t = setTimeout(tile, 120);
    return () => clearTimeout(t);
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
    <div className={`hud-root ${isSinglePanel ? "single" : ""}`}>
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
      <div
        ref={hudBodyRef}
        className={`hud-body ${isSinglePanel ? "single" : isFree ? "free" : compactLayout ? "compact" : "full"}`}
        id="hud-scroll"
      >
        {orderedSections.map(([id, label, content]) => {
          const freeStyle: React.CSSProperties | undefined = isFree && panes[id]
            ? { left: panes[id].x, top: panes[id].y, width: panes[id].w, height: panes[id].h, position: "absolute" as const, zIndex: panes[id].z }
            : undefined;
          return (
            <div
              className={`hud-section hud-${id} ${isFree ? "free-pane" : ""}`}
              id={`sec-${id}`}
              key={id}
              draggable={!isFree && !isSinglePanel}
              onDragStart={!isFree ? handleDragStart(id) : undefined}
              onDragOver={!isFree ? handleDragOver : undefined}
              onDrop={!isFree ? handleDrop(id) : undefined}
              onMouseDown={isFree ? () => bringToFront(String(id)) : undefined}
              style={freeStyle}
            >
              <div
                className="panel-header-actions"
                draggable={!isFree && !isSinglePanel}
                onDragStart={!isFree ? handleDragStart(id) : undefined}
                onPointerDown={isFree ? onHeaderPointerDown(String(id)) : undefined}
                style={{ cursor: isSinglePanel ? "default" : "grab" }}
              >
                <span style={{ fontSize: "10px", color: "var(--ap-text-dim)", letterSpacing: "0.04em", textTransform: "uppercase", display: "flex", alignItems: "center", gap: "6px" }}>
                  {!isSinglePanel && <span style={{ opacity: 0.5, cursor: "grab" }}>⋮⋮</span>} {String(label)}
                </span>
                {!isSinglePanel && <button className="popout-btn" onClick={() => popOut(String(id), String(label))} title="Open in separate window">Pop Out ↗</button>}
              </div>
              {content}
              {isFree && <div className="free-resizer" onPointerDown={onResizePointerDown(String(id))} title="Drag to resize" />}
            </div>
          );
        })}
      </div>
    </div>
  );
}
