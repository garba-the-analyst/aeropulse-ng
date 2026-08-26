/**
 * Display 1 — hardware-accelerated tactical radar canvas (Screen 1).
 *
 * Renders at display refresh rate directly from the telemetry store ref:
 * range rings, sector grid, rotating sweep, MIL-STD-2525D target symbols,
 * EKF leader vectors, three-line Flight Data Blocks, flashing STCA
 * connectors and geofence polygons. React state is never touched per frame.
 */
import { useEffect, useRef } from "react";
import { getSnapshot } from "../../hooks/useFlightTracks";
import type { Track } from "../../types/telemetry";
import type { Geofence, InterceptSolution } from "../../types/defense";
import { SYMBOLOGY, trackColor } from "../../types/telemetry";
import { formatFDB } from "./FlightDataBlock";
import {
  RANGE_RINGS_KM,
  SEPARATION_MIN,
  ftToM,
  nmToKm,
} from "../../lib/units";
import { conflictLabel, resolveConflictPair } from "./STCABoundBox";
import { invokeSafe } from "../../lib/ipc";

export const SITE = { lat: 12.0476, lon: 8.5241 };

interface Props {
  /** Operator range scale in kilometres. */
  rangeKm: number;
  rulerMode: boolean;
}

interface Projection {
  cx: number;
  cy: number;
  pxPerDegLat: number;
  pxPerDegLon: number;
  mPerNmPx: number;
}

function metersPerDegree(latRad: number): { latM: number; lonM: number } {
  return {
    latM: 111_132.92 - 559.82 * Math.cos(2 * latRad),
    lonM: 111_412.84 * Math.cos(latRad),
  };
}

export default function RadarCanvas({ rangeKm, rulerMode }: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const rangeRef = useRef(rangeKm);
  const rulerRef = useRef(rulerMode);
  const anchorRef = useRef<{ x: number; y: number } | null>(null);
  const cursorRef = useRef<{ x: number; y: number } | null>(null);
  const fencesRef = useRef<Geofence[]>([]);
  const solutionRef = useRef<InterceptSolution | null>(null);

  rangeRef.current = rangeKm;
  rulerRef.current = rulerMode;

  // Geofences are administrative data — pull once and on demand.
  useEffect(() => {
    void invokeSafe<Geofence[]>("list_geofences").then((f) => {
      if (f) fencesRef.current = f;
    });
    const t = setInterval(() => {
      void invokeSafe<Geofence[]>("list_geofences").then((f) => {
        if (f) fencesRef.current = f;
      });
    }, 15_000);
    return () => clearInterval(t);
  }, []);

  // Expose intercept results arriving via custom event bus.
  useEffect(() => {
    const onSolution = (e: Event): void => {
      solutionRef.current = (e as CustomEvent<InterceptSolution>).detail ?? null;
    };
    window.addEventListener("ap-intercept-solution", onSolution as EventListener);
    return () =>
      window.removeEventListener("ap-intercept-solution", onSolution as EventListener);
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let raf = 0;

    const draw = (): void => {
      const dpr = window.devicePixelRatio || 1;
      const wCss = canvas.clientWidth;
      const hCss = canvas.clientHeight;
      if (canvas.width !== Math.round(wCss * dpr)) canvas.width = Math.round(wCss * dpr);
      if (canvas.height !== Math.round(hCss * dpr)) canvas.height = Math.round(hCss * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

      const cx = wCss / 2;
      const cy = hCss / 2;
      const latRad = (SITE.lat * Math.PI) / 180;
      const { latM, lonM } = metersPerDegree(latRad);
      // One kilometre in projected pixels at the current operator scale.
      const kmPx = Math.min(wCss, hCss) / 2 / rangeRef.current;
      const pxPerDegLat = (latM / 1000) * kmPx;
      const pxPerDegLon = (lonM / 1000) * kmPx;
      const toPx = (lat: number, lon: number): [number, number] => [
        cx + (lon - SITE.lon) * pxPerDegLon,
        cy - (lat - SITE.lat) * pxPerDegLat,
      ];

      // ---- background -------------------------------------------------
      ctx.fillStyle = SYMBOLOGY.background;
      ctx.fillRect(0, 0, wCss, hCss);

      // ---- range rings + crosshair grid -------------------------------
      ctx.strokeStyle = SYMBOLOGY.grid;
      ctx.globalAlpha = 0.4;
      ctx.lineWidth = 1;
      for (const km of RANGE_RINGS_KM) {
        if (km >= rangeRef.current) continue;
        ctx.beginPath();
        ctx.arc(cx, cy, km * kmPx, 0, Math.PI * 2);
        ctx.stroke();
        ctx.fillStyle = "#6B7689";
        ctx.font = "10px JetBrains Mono, monospace";
        ctx.fillText(`${km} KM`, cx + 4, cy - km * kmPx - 4);
      }
      // Outer selected-range boundary.
      ctx.beginPath();
      ctx.arc(cx, cy, rangeRef.current * kmPx, 0, Math.PI * 2);
      ctx.stroke();

      // Crosshairs every 25% of radius.
      ctx.beginPath();
      for (const frac of [-1, -0.5, 0.5, 1]) {
        ctx.moveTo(cx + frac * cx, 0);
        ctx.lineTo(cx + frac * cx, hCss);
        ctx.moveTo(0, cy + frac * cy);
        ctx.lineTo(wCss, cy + frac * cy);
      }
      ctx.stroke();

      // Bearing spokes each 30°.
      ctx.beginPath();
      for (let deg = 0; deg < 360; deg += 30) {
        const rad = (deg * Math.PI) / 180;
        ctx.moveTo(cx, cy);
        ctx.lineTo(
          cx + Math.sin(rad) * rangeRef.current * kmPx,
          cy - Math.cos(rad) * rangeRef.current * kmPx,
        );
      }
      ctx.stroke();
      ctx.globalAlpha = 1;

      // ---- rotating sweep ---------------------------------------------
      const sweepAngle = ((Date.now() % 4000) / 4000) * Math.PI * 2;
      const grad = ctx.createConicGradient?.(sweepAngle, cx, cy);
      if (grad) {
        grad.addColorStop(0, "rgba(0,255,102,0.16)");
        grad.addColorStop(0.08, "rgba(0,255,102,0)");
        grad.addColorStop(1, "rgba(0,255,102,0)");
        ctx.fillStyle = grad;
        ctx.beginPath();
        ctx.arc(cx, cy, rangeRef.current * kmPx, 0, Math.PI * 2);
        ctx.fill();
      }

      // ---- geofences ----------------------------------------------------
      for (const fence of fencesRef.current) {
        if (!fence.active || fence.vertices_deg.length < 3) continue;
        ctx.strokeStyle = SYMBOLOGY.unknown;
        ctx.setLineDash([6, 4]);
        ctx.beginPath();
        fence.vertices_deg.forEach(([lat, lon]: [number, number], i: number) => {
          const [x, y] = toPx(lat, lon);
          if (i === 0) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
        });
        ctx.closePath();
        ctx.stroke();
        ctx.setLineDash([]);
        const first = fence.vertices_deg[0];
        const [fx, fy] = toPx(first[0], first[1]);
        ctx.fillStyle = SYMBOLOGY.unknown;
        ctx.fillText(fence.name, fx + 4, fy - 4);
      }

      // ---- tracks -------------------------------------------------------
      const snap = getSnapshot();
      const byIcao = new Map<string, Track>();
      if (snap) for (const t of snap.tracks) byIcao.set(t.icao24, t);

      if (snap) {
        // Leader lines beneath symbols.
        for (const t of snap.tracks) {
          if (t.leader_line.length < 2) continue;
          ctx.strokeStyle = trackColor(t);
          ctx.globalAlpha = t.coasting ? 0.35 : 0.7;
          ctx.beginPath();
          t.leader_line.forEach(([lat, lon]: [number, number], i: number) => {
            const [x, y] = toPx(lat, lon);
            if (i === 0) ctx.moveTo(x, y);
            else ctx.lineTo(x, y);
          });
          ctx.stroke();
          ctx.globalAlpha = 1;
        }

        for (const t of snap.tracks) {
          const [x, y] = toPx(t.latitude, t.longitude);
          const color = trackColor(t);
          ctx.strokeStyle = color;
          ctx.fillStyle = color;
          ctx.lineWidth = 1.5;

          // Symbology frame per MIL-STD-2525D mapping.
          ctx.beginPath();
          if (t.class === "civil") {
            // Diamond.
            ctx.moveTo(x, y - 7);
            ctx.lineTo(x + 6, y);
            ctx.lineTo(x, y + 7);
            ctx.lineTo(x - 6, y);
            ctx.closePath();
          } else if (t.class === "military") {
            // Caret frame.
            ctx.moveTo(x - 7, y + 6);
            ctx.lineTo(x, y - 7);
            ctx.lineTo(x + 7, y + 6);
          } else {
            // Quadrangle.
            ctx.rect(x - 6, y - 6, 12, 12);
          }
          ctx.stroke();

          if (t.coasting) {
            ctx.setLineDash([3, 3]);
            ctx.beginPath();
            ctx.arc(x, y, 11, 0, Math.PI * 2);
            ctx.stroke();
            ctx.setLineDash([]);
          }
          if (t.alert === "emergency") {
            ctx.beginPath();
            ctx.arc(x, y, 14, 0, Math.PI * 2);
            ctx.stroke();
          }

          // Flight Data Block.
          const fdb = formatFDB(t);
          ctx.font = "11px JetBrains Mono, monospace";
          ctx.textAlign = "left";
          ctx.fillText(fdb.line1, x + 14, y - 8);
          ctx.fillText(fdb.line2, x + 14, y + 4);
          ctx.fillText(fdb.line3, x + 14, y + 16);
        }

        // ---- STCA connectors -------------------------------------------
        const flashOn = Date.now() % 500 < 250; // 2 Hz
        if (flashOn) {
          ctx.strokeStyle = SYMBOLOGY.alert;
          ctx.setLineDash([8, 5]);
          ctx.lineWidth = 2;
          for (const alert of snap.stca_alerts) {
            const geom = resolveConflictPair(alert, byIcao);
            if (!geom.drawable) continue;
            const [ax, ay] = toPx(geom.a!.latitude, geom.a!.longitude);
            const [bx, by] = toPx(geom.b!.latitude, geom.b!.longitude);
            ctx.beginPath();
            ctx.moveTo(ax, ay);
            ctx.lineTo(bx, by);
            ctx.stroke();
            const label = conflictLabel(geom);
            if (label) {
              ctx.fillStyle = SYMBOLOGY.alert;
              ctx.font = "10px JetBrains Mono, monospace";
              ctx.fillText(label.text, (ax + bx) / 2 + 6, (ay + by) / 2 - 6);
            }
          }
          ctx.setLineDash([]);
        }
      }

      // ---- R&B ruler ------------------------------------------------------
      if (rulerRef.current && anchorRef.current && cursorRef.current) {
        const { x: ax2, y: ay2 } = anchorRef.current;
        const { x: bx2, y: by2 } = cursorRef.current;
        ctx.strokeStyle = "#FFFFFF";
        ctx.beginPath();
        ctx.moveTo(ax2, ay2);
        ctx.lineTo(bx2, by2);
        ctx.stroke();
        const dxM = (bx2 - ax2) / pxPerDegLon * lonM;
        const dyM = (ay2 - by2) / pxPerDegLat * latM;
        const rngNm = Math.hypot(dxM, dyM) / 1852;
        let brg = Math.atan2(dxM, dyM) * (180 / Math.PI);
        if (brg < 0) brg += 360;
        ctx.fillStyle = "#FFFFFF";
        ctx.font = "11px JetBrains Mono, monospace";
        ctx.fillText(`R&B ${brg.toFixed(0)}° / ${rngNm.toFixed(1)}NM`, bx2 + 8, by2 - 6);
      }

      // ---- site marker ----------------------------------------------------
      ctx.fillStyle = SYMBOLOGY.civil;
      ctx.beginPath();
      ctx.arc(cx, cy, 3, 0, Math.PI * 2);
      ctx.fill();
      ctx.font = "10px JetBrains Mono, monospace";
      ctx.fillText("DNKN", cx + 6, cy + 12);

      raf = requestAnimationFrame(draw);
    };

    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <canvas
      ref={canvasRef}
      className="radar-canvas"
      onClick={(e) => {
        if (!rulerMode) return;
        anchorRef.current = { x: e.nativeEvent.offsetX, y: e.nativeEvent.offsetY };
      }}
      onMouseMove={(e) => {
        cursorRef.current = { x: e.nativeEvent.offsetX, y: e.nativeEvent.offsetY };
      }}
      onDoubleClick={() => {
        anchorRef.current = null;
        solutionRef.current = null;
      }}
    />
  );
}
