/**
 * STCA pair geometry helpers: screen-space endpoints and bounding box for
 * the flashing conflict connector drawn by the radar canvas.
 */
import { SEPARATION_MIN, nmToKm } from "../../lib/units";
import type { STCAAlert, Track } from "../../types/telemetry";

export interface ConflictPairGeometry {
  alert: STCAAlert;
  a: Track | null;
  b: Track | null;
  /** Both endpoints resolved — safe to draw. */
  drawable: boolean;
}

export function resolveConflictPair(
  alert: STCAAlert,
  tracksByIcao: Map<string, Track>,
): ConflictPairGeometry {
  const a = tracksByIcao.get(alert.icao_a) ?? null;
  const b = tracksByIcao.get(alert.icao_b) ?? null;
  return { alert, a, b, drawable: a !== null && b !== null };
}

/** Midpoint label anchor in projected metres (caller converts to px). */
export function conflictLabel(
  geom: ConflictPairGeometry,
): { text: string } | null {
  if (!geom.drawable) return null;
  const { alert } = geom;
  // SI rendering of the Doc-4444 separation minima.
  void SEPARATION_MIN;
  return {
    text: `${nmToKm(alert.min_horizontal_nm).toFixed(1)} KM / ${Math.round(
      alert.min_vertical_ft * 0.3048,
    )} M / T-${Math.round(alert.time_to_closest_s)}s`,
  };
}
