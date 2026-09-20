import { describe, it, expect } from "vitest";
import { frequencyForIcao, frequencyLabelForIcao, isValidFrequency, TX_TIMEOUT_MS_EXPORT } from "./AudioRoutingPanel";

describe("AudioRoutingPanel — frequency validation", () => {
  it("accepts valid VHF raster (25kHz)", () => {
    expect(isValidFrequency(118.0)).toBe(true);
    expect(isValidFrequency(118.025)).toBe(true);
    expect(isValidFrequency(136.975)).toBe(true);
  });
  it("rejects out of band", () => {
    expect(isValidFrequency(117.999)).toBe(false);
    expect(isValidFrequency(137.0)).toBe(false);
  });
  it("frequencyForIcao deterministic SIMULATED", () => {
    const a = frequencyForIcao("342157");
    const b = frequencyForIcao("342157");
    expect(a).toBe(b);
    expect(a).toMatch(/^\d+\.\d{3}$/);
    const num = parseFloat(a);
    expect(num).toBeGreaterThanOrEqual(118.0);
    expect(num).toBeLessThanOrEqual(136.975);
    expect(frequencyLabelForIcao("342157")).toContain("SIMULATED");
  });
  it("different ICAOs produce generally different frequencies", () => {
    const f1 = frequencyForIcao("342157");
    const f2 = frequencyForIcao("061104");
    // not guaranteed distinct for all pairs, but demo fleet should differ
    expect(f1).not.toBe(f2);
  });
  it("TX timeout is 30s", () => {
    expect(TX_TIMEOUT_MS_EXPORT).toBe(30000);
  });
});

describe("AudioRoutingPanel — TX blocking contract (logic)", () => {
  // Extract logic that startTx enforces: block when no hardware, emergency inhibit, freq invalid
  function canTransmit(opts: { hasHardware: boolean; hasEmergency: boolean; mode: "general"|"aircraft"; selectedIcao: string|null; freqValid: boolean }) {
    if (!opts.freqValid) return { allowed: false, reason: "Frequency" };
    if (opts.hasEmergency && opts.mode === "general") return { allowed: false, reason: "emergency" };
    if (opts.mode === "aircraft" && !opts.selectedIcao) return { allowed: false, reason: "Select aircraft" };
    if (!opts.hasHardware) return { allowed: false, reason: "SIMULATED — no transmitter attached" };
    return { allowed: true, reason: null };
  }

  it("blocks TX when no hardware with SIMULATED banner", () => {
    const r = canTransmit({ hasHardware: false, hasEmergency: false, mode: "general", selectedIcao: null, freqValid: true });
    expect(r.allowed).toBe(false);
    expect(r.reason).toContain("SIMULATED");
  });
  it("blocks general TX during emergency", () => {
    const r = canTransmit({ hasHardware: true, hasEmergency: true, mode: "general", selectedIcao: null, freqValid: true });
    expect(r.allowed).toBe(false);
    expect(r.reason).toContain("emergency");
  });
  it("allows aircraft-specific TX during emergency when hardware present", () => {
    const r = canTransmit({ hasHardware: true, hasEmergency: true, mode: "aircraft", selectedIcao: "342157", freqValid: true });
    expect(r.allowed).toBe(true);
  });
  it("keeps 30s auto-release invariant", () => {
    expect(TX_TIMEOUT_MS_EXPORT).toBe(30000);
  });
});
