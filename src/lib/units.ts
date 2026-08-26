/**
 * AeroPulse-NG unit policy — NCAA / ICAO Annex 5 SI presentation layer.
 *
 * ┌────────────────────┬─────────────────────┬──────────────────────────┐
 * │ Quantity           │ Display (SI primary)│ Regulatory invariant     │
 * ├────────────────────┼─────────────────────┼──────────────────────────┤
 * │ Distance           │ kilometre (km)      │ —                        │
 * │ Altitude           │ metre (m)           │ flight levels ref. only  │
 * │ Speed              │ km/h                │ —                        │
 * │ Vertical rate      │ m/s                 │ —                        │
 * │ Visibility         │ km                  │ WMO reporting            │
 * │ Pressure           │ hPa                 │ QNH convention (ICAO)    │
 * │ Direction          │ degrees true        │ ICAO Annex 2             │
 * │ Squawk             │ 4-digit octal       │ Annex 10 Vol III         │
 * └────────────────────┴─────────────────────┴──────────────────────────┘
 *
 * Engine internals stay native: the EKF integrates in metres and m/s;
 * ADS-B wire values decode to feet/knots per DO-260B and convert at the
 * presentation boundary only. Separation minima remain the ICAO Doc 4444
 * regulatory constants (5 NM / 1 000 ft) inside the safety logic and are
 * rendered as their SI equivalents (9.26 km / 305 m).
 */

export const NM_TO_KM = 1.852;
export const FT_TO_M = 0.3048;
export const KT_TO_KMH = 1.852;
export const FPM_TO_MS = 0.00508;

/** ICAO Doc 4444 radar-separation minima, SI rendering. */
export const SEPARATION_MIN = {
  horizontal_km: 5 * NM_TO_KM, // 9.26 km
  vertical_m: 1000 * FT_TO_M, // 305 m
} as const;

export const nmToKm = (nm: number): number => nm * NM_TO_KM;
export const ftToM = (ft: number): number => ft * FT_TO_M;
export const ktToKmh = (kt: number): number => kt * KT_TO_KMH;
export const fpmToMs = (fpm: number): number => fpm * FPM_TO_MS;

/** Altitude in whole metres, e.g. "9750 m". */
export function fmtAltM(ft: number): string {
  return `${Math.round(ftToM(ft))} m`;
}

/** Ground speed in whole km/h, e.g. "824 km/h". */
export function fmtSpdKmh(kt: number): string {
  return `${Math.round(ktToKmh(kt))} km/h`;
}

/** Distance in km with fixed precision, e.g. "9.3 km". */
export function fmtKm(nm: number, digits = 1): string {
  return `${nmToKm(nm).toFixed(digits)} km`;
}

/** Vertical separation in whole metres, e.g. "305 m". */
export function fmtVertM(ft: number): string {
  return `${Math.round(ftToM(ft))} m`;
}

/**
 * Metric radar range-ring plan: rings drawn at these distances when they
 * fall inside the operator-selected range scale.
 */
export const RANGE_RINGS_KM = [100, 200, 300] as const;

/** Operator range-scale options for the master command bar (km). */
export const RANGE_OPTIONS_KM = [25, 50, 100, 150, 200, 300, 450] as const;
