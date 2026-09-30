import { describe, expect, it } from "vitest";
import {
  FT_TO_M,
  KT_TO_KMH,
  NM_TO_KM,
  FPM_TO_MS,
  SEPARATION_MIN,
  RANGE_RINGS_KM,
  fmtAltM,
  fmtSpdKmh,
  fmtKm,
  fmtVertM,
  ftToM,
  ktToKmh,
  nmToKm,
} from "./units";

describe("units policy (NCAA/ICAO Annex 5 SI)", () => {
  it("uses exact conversion constants", () => {
    expect(NM_TO_KM).toBeCloseTo(1.852, 6);
    expect(FT_TO_M).toBeCloseTo(0.3048, 6);
    expect(KT_TO_KMH).toBeCloseTo(1.852, 6);
    expect(FPM_TO_MS).toBeCloseTo(0.00508, 6);
  });

  it("renders Doc 4444 minima as SI equivalents", () => {
    expect(SEPARATION_MIN.horizontal_km).toBeCloseTo(9.26, 2);
    expect(SEPARATION_MIN.vertical_m).toBeCloseTo(305, 0);
  });

  it("converts wire ft/kt to display m/km/h", () => {
    expect(ftToM(1000)).toBeCloseTo(304.8, 1);
    expect(ktToKmh(445)).toBeCloseTo(824.14, 1);
    expect(nmToKm(5)).toBeCloseTo(9.26, 2);
  });

  it("formats operator strings in SI", () => {
    expect(fmtAltM(32000)).toBe("9754 m");
    expect(fmtSpdKmh(445)).toContain("km/h");
    expect(fmtKm(5)).toBe("9.3 km");
    expect(fmtVertM(1000)).toBe("305 m");
  });

  it("keeps metric range-ring plan", () => {
    expect([...RANGE_RINGS_KM]).toEqual([100, 200, 300]);
  });
});
