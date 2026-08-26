# Hardware Layer — SDR Ingestion & Signal Decoding

Path: `src-tauri/src/hardware/`
Tests: 30 (decoder 12 · ACARS 5 · registry 3 · simulator 10)

Responsibility: turn *bytes* (real or synthesised) into typed surveillance
events. Nothing here knows about tracks, filters or displays — that boundary
starts at `engine.rs`.

---

## Files

| File | Role |
|---|---|
| `mode_s_decoder.rs` | DF17 Extended Squitter decode: CRC, CPR, altitude, velocity, identity |
| `acars_decoder.rs` | VHF ACARS framing + weather classification |
| `sdr_registry.rs` | USB channel plan with serial-lock binding |
| `simulator.rs` | Truth-state fleet → genuine DF17/ACARS frame synthesis |

## Mode S Decoder

**Frame intake.** Accepts 7-byte (56-bit) or 14-byte (112-bit) frames. Frames are
left-aligned into a `u128` register so every field accessor uses the spec's own
MSB-first bit numbering (`bit 0` = first bit of the DF field) — the single most
important invariant; three separate bugs in this file's history were numbering
conventions colliding.

**Parity.** CRC-24 over the full frame with generator
`G(x) = x²⁴+x²³+x²²+x²¹+x²⁰+x¹⁹+x¹⁸+x¹⁷+x¹⁶+x¹⁵+x¹⁴+x¹³+x¹²+x¹⁰+x³+1`.
Validity rule: remainder == extracted ICAO (address parity) **or** remainder == 0
(even parity — some published fixtures and DF18 relays). Single-bit repair scans
all 112 positions for a flip restoring either property.

**CPR global position.** Even/odd fractions pair inside a 10 s epoch:

```
dlat_even = 6°      dlat_odd = 360/59 °
j        = floor(59·lat_e − 60·lat_o + 0.5)
rlat     = dlat · ((j mod NZ) + cpr_fraction)
guard    = NL(rlat_e) == NL(rlat_o), else reject
longitude: m = floor(lon_e·(NL−1) − lon_o·NL + 0.5); dl = 360/max(NL−i,1)
```

`nl_zones()` is the canonical 87-entry lookup (53 zones at Kano's latitude).
Round-trip tests encode truth points via `cpr_encode()` and assert < 0.002° /
< 0.004° recovery, including across zone boundaries.

**Altitude.** 12-bit field, Q-bit at M8: arithmetic ×25 ft when set; Gillham
gray-decode ×500 ft legacy path otherwise. Encoder provided for synthesis.

**Velocity (ME TC 19).** Subtypes 1/2: EW/NS signed 10-bit vectors → speed =
hypot, track = atan2(E,N); subtypes 3/4 heading status + airspeed. Vertical rate
9-bit ×64 ft/min with sign slot shared by all subtypes.

## ACARS Decoder

Frame grammar `SOH MODE TAIL ACK LABEL BID [STX FLIGHT CR TEXT] ETX BCS₁₆`.
The scanner tolerates inter-frame noise, frames span SOH..terminator+2 BCS bytes,
and CRC-16/CCITT-FALSE (0x1021, init 0xFFFF — reference vector "123456789"→0x29B1)
flags but never discards corrupt payloads. Classification heuristics route
`METAR`/`SPECI` prefixes and D-ATIS label shapes (`A7/A6/A5/AT`, prose containing
ATIS patterns) into `WeatherProduct`s consumed by the fusion engine.

`synthesize_frame()` is public on purpose: the simulator (and your fixtures) build
wire-exact uplinks/downlinks through it.

## SDR Registry

Three logical channels (`SDR-1` ADS-B 1090 MHz, `SDR-2` ACARS 131.55, `SDR-3`
VHF guard 121.5) bind to fixed factory serial numbers from a `ChannelPlan`.
Enumeration is a trait (`UsbProbe`) with two impls: `LibUsbProbe` walks sysfs for
RTL2832U vendor/product IDs (no libusb C dependency needed for discovery);
`SimulatedProbe` yields the bench inventory. Mismatched hardware renders channels
offline rather than refusing boot — an air-gapped station must degrade, not die.

## Simulator

Six-aircraft roster around DNKN (12.0476 N, 8.5241 E) with scripted drama:

* **NAF911/912** — intercept geometry solved analytically to cross VL604's
  projected path at T+70 s with ≈1 km lateral miss (STCA guaranteed).
* **VL604** — squawk flips to 7700 at T+45 s (event + TC-28 status frame);
  RF dropout window each cycle end (coasting demo).
* **0BADC0** — dark target: transmits positions only, identity never broadcast.
* Parity alternates per *emitted frame* (not per tick — an aliasing bug that once
  produced all-odd streams), silence windows land at cycle ends so targets are
  visible immediately at boot.

Determinism: xorshift64* stream seeded by `SimulatorConfig::seed`; same seed ⇒
byte-identical frame timeline (asserted by test).

## Extending to real RF

The intended insertion point is a task performing libusb bulk transfers →
DC/IQ correction → Manchester bit sync → byte assembly → `decode_frame()`. The
registry already models per-channel ppm/gain/sample-rate status; only the DSP
front-end is missing (see ARCHITECTURE roadmap #1).
