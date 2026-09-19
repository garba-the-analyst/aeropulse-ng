//! Mode S / ADS-B downlink decoder for 1090 MHz.
//!
//! Accepts raw frames from the SDR front-end (14-byte/112-bit ES, 7-byte/56-bit
//! short, or 14-byte/112-bit long), validates CRC-25 parity, optionally repairs
//! single-bit errors, and extracts:
//! - DF17/18: ADS-B Extended Squitter (position, velocity, identity, status)
//! - DF11: All-call reply (Mode S only, no ADS-B)
//! - DF0/4/5/16/20/21: Short/Long air-air surveillance (ACAS/TCAS)
//! - DF20/21: Comm-B altitude/identity replies (elicited)
//!

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
/// All decoding is air-gapped: no network dependency, no external keys.

/// Cache of recently confirmed ICAO addresses for AP-frame validation.
/// DF0/4/5/16/20/21 use Address/Parity (AP) where the ICAO is XORed into the parity field.
/// These frames cannot self-validate; we only accept them if the ICAO (remainder) has been
/// recently confirmed by a CRC-clean DF11/17/18 frame.
pub struct IcaoCache {
    entries: HashMap<u32, u64>, // icao24 -> last_seen_ms
    ttl_ms: u64,
    max_entries: usize,
}

impl IcaoCache {
    pub fn new(ttl_ms: u64, max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            ttl_ms,
            max_entries,
        }
    }

    /// Called when a self-validating frame (DF11/17/18) passes CRC.
    pub fn confirm(&mut self, icao: u32, now_ms: u64) {
        if self.entries.len() >= self.max_entries && !self.entries.contains_key(&icao) {
            // Evict oldest
            if let Some((oldest_icao, _)) = self.entries.iter().min_by_key(|(_, &v)| v) {
                let oldest = *oldest_icao;
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(icao, now_ms);
    }

    /// Check if an ICAO is recently confirmed (and not expired).
    pub fn is_known(&mut self, icao: u32, now_ms: u64) -> bool {
        if self.ttl_ms == 0 {
            return false;
        }
        // Expire stale entries
        if self.ttl_ms > 0 {
            let mut to_remove = Vec::new();
            for (k, v) in self.entries.iter() {
                if now_ms.saturating_sub(*v) > self.ttl_ms {
                    to_remove.push(*k);
                }
            }
            for k in to_remove {
                self.entries.remove(&k);
            }
        }
        if let Some(&t) = self.entries.get(&icao) {
            now_ms.saturating_sub(t) <= self.ttl_ms
        } else {
            false
        }
    }
}

/// All decoding is air-gapped: no network dependency, no external keys.

pub const CRC_POLY: u32 = 0x1FF_F409;

/// 6-bit callsign character table (ICAO Annex 10 Vol IV).
/// Index map: 0='#', 1-26='A-Z', 32='_', 48-57='0-9'.
const CALLSIGN_CHARSET: &[u8; 64] =
    b"#ABCDEFGHIJKLMNOPQRSTUVWXYZ#####_###############0123456789######";

/// Number-of-Latitude-Zones lookup for airborne CPR resolution.
#[inline]
pub fn nl_zones(latitude_deg: f64) -> usize {
    let lat = latitude_deg.abs();
    if lat > 86.5 {
        return 1;
    }
    const TABLE: [usize; 87] = [
        59, 59, 59, 58, 58, 57, 57, 56, 56, 55, 55, 54, 53, 53, 52, 51, 51, 50, 49, 48, 47, 46,
        45, 44, 43, 42, 41, 40, 39, 38, 37, 36, 35, 34, 33, 32, 31, 30, 29, 28, 27, 26, 25, 24,
        23, 22, 21, 20, 19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 2, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    ];
    TABLE[(lat.floor() as usize).min(86)]
}

/// CRC-24 remainder over the full frame using the Mode S generator
/// polynomial G(x) = x^24+x^23+x^22+x^21+x^20+x^19+x^18+x^17+x^16+
/// x^15+x^14+x^13+x^12+x^10+x^3+1.
///
/// Address-parity convention: for a correctly received DF17 frame the
/// remainder equals the transmitting ICAO address (transmitters XOR the
/// payload CRC with the address before transmission).
pub fn crc25(bytes: &[u8]) -> u32 {
    const GEN: u32 = 0xFF_F409;
    let mut reg: u32 = 0;
    for i in 0..bytes.len() * 8 {
        let bit = ((bytes[i / 8] >> (7 - (i % 8))) & 1) as u32;
        let msb = (reg >> 23) & 1;
        reg = ((reg << 1) | bit) & 0xFF_FFFF;
        if msb == 1 {
            reg ^= GEN;
        }
    }
    reg
}

#[inline]
fn bit_u128(bits: u128, pos: usize) -> u32 {
    ((bits >> (127 - pos)) & 1) as u32
}

#[inline]
fn field(bits: u128, start: usize, len: usize) -> u64 {
    debug_assert!(len <= 64 && start + len <= 128);
    let shifted = bits >> (128 - start - len);
    (shifted & ((1u128 << len) - 1)) as u64
}

/// DF type identifier for routing/filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum DownlinkFormat {
    Df0   = 0,
    Df4   = 4,
    Df5   = 5,
    Df11  = 11,
    Df16  = 16,
    Df17  = 17,
    Df18  = 18,
    Df20  = 20,
    Df21  = 21,
}

/// Source classification for track fusion weighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModeSSource {
    AdsbExtendedSquitter,
    AllCallReply,
    CommBAltitudeReply,
    CommBIdentityReply,
    ShortAirAir,
    LongAirAir,
    GroundRelay,
}

/// Fully parsed Mode S message payload (all supported DFs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DecodedMessage {
    /// DF17/18 ADS-B Identity (TC 1-4)
    Identity {
        icao24: u32,
        callsign: String,
        category: u8,
        source: ModeSSource,
        df: DownlinkFormat,
    },
    /// DF17/18 ADS-B Airborne Position (TC 9-18, 20-22)
    AirbornePosition {
        icao24: u32,
        altitude_ft: f64,
        odd: bool,
        lat_cpr: f64,
        lon_cpr: f64,
        surveillance_status: u8,
        source: ModeSSource,
        df: DownlinkFormat,
    },
    /// DF17/18 ADS-B Surface Position (TC 5-8)
    SurfacePosition {
        icao24: u32,
        odd: bool,
        lat_cpr: f64,
        lon_cpr: f64,
        source: ModeSSource,
        df: DownlinkFormat,
    },
    /// DF17/18 ADS-B Velocity (TC 19)
    Velocity {
        icao24: u32,
        subtype: u8,
        ground_speed_kt: Option<f64>,
        track_deg: Option<f64>,
        airspeed_kt: Option<f64>,
        heading_deg: Option<f64>,
        vertical_rate_fpm: Option<f64>,
        source: ModeSSource,
        df: DownlinkFormat,
    },
    /// DF17/18 ADS-B Aircraft Status (TC 28)
    AircraftStatus {
        icao24: u32,
        emergency_state: u8,
        source: ModeSSource,
        df: DownlinkFormat,
    },
    /// DF11 All-call reply (no ADS-B payload, just ICAO + capability)
    AllCallReply {
        icao24: u32,
        capability: u8,
        df: DownlinkFormat,
    },
    /// DF20 Comm-B Altitude Reply (elicited, contains altitude)
    CommBAltitudeReply {
        icao24: u32,
        altitude_ft: Option<f64>,
        reply_info: u8,
        df: DownlinkFormat,
    },
    /// DF21 Comm-B Identity Reply (elicited, contains callsign)
    CommBIdentityReply {
        icao24: u32,
        callsign: String,
        reply_info: u8,
        df: DownlinkFormat,
    },
    /// DF0/4/5/16 Short/Long air-air surveillance (ACAS/TCAS)
    AirAirSurveillance {
        icao24: u32,
        df: DownlinkFormat,
        reply_info: u8,
        altitude_ft: Option<f64>,
    },
    /// DF18 Extended Squitter Ground Relay
GroundRelay {
        icao24: u32,
        inner_df: DownlinkFormat,
        message: Box<DecodedMessage>,
    },
}
impl DecodedMessage {
    /// ICAO24 of the transmitting aircraft for every variant.
    pub fn icao24(&self) -> u32 {
        match self {
            DecodedMessage::Identity { icao24, .. } => *icao24,
            DecodedMessage::AirbornePosition { icao24, .. } => *icao24,
            DecodedMessage::SurfacePosition { icao24, .. } => *icao24,
            DecodedMessage::Velocity { icao24, .. } => *icao24,
            DecodedMessage::AircraftStatus { icao24, .. } => *icao24,
            DecodedMessage::AllCallReply { icao24, .. } => *icao24,
            DecodedMessage::CommBAltitudeReply { icao24, .. } => *icao24,
            DecodedMessage::CommBIdentityReply { icao24, .. } => *icao24,
            DecodedMessage::AirAirSurveillance { icao24, .. } => *icao24,
            DecodedMessage::GroundRelay { icao24, message, .. } => {
                // For ground relay, the outer ICAO is the relay station; the inner message
                // has the original aircraft's ICAO. We return the inner one.
                message.icao24()
            }
        }
    }
}

/// Decoder outcome including repair bookkeeping for diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodeResult {
    pub message: DecodedMessage,
    pub repaired_bit: Option<usize>,
    pub df: DownlinkFormat,
}

/// Attempts single-bit error repair across all 112 bit positions.
fn try_single_bit_repair(frame: &[u8]) -> Option<(u128, usize)> {
    if frame.len() != 14 {
        return None;
    }
    let original = u128::from_be_bytes({
        let mut buf = [0u8; 16];
        buf[..14].copy_from_slice(frame);
        buf
    });
    // Left-aligned register: ICAO spans global bits 8..31 => shift 96.
    let target_icao = ((original >> 96) & 0xFF_FFFF) as u32;
    for pos in 0..112usize {
        // pos uses spec numbering (bit 0 = frame MSB) matching `repaired_bit`.
        let candidate = original ^ (1u128 << (127 - pos));
        // Left-aligned register: the 14 frame bytes occupy bytes [..14].
        let bytes = candidate.to_be_bytes();
        let rem = crc25(&bytes[..14]);
        if rem == target_icao || rem == 0 {
            return Some((candidate, pos));
        }
    }
    None
}

/// Decodes one raw Mode S frame (7 or 14 bytes) — without cache, only self-validating DFs.
pub fn decode_frame(raw: &[u8]) -> Option<DecodeResult> {
    decode_frame_with_cache(raw, &mut IcaoCache::new(0, 0), 0)
}

/// Decodes one raw Mode S frame with an ICAO cache for AP-frame validation.
/// DF11/17/18 are self-validating (ICAO in plaintext); DF0/4/5/16/20/21 use
/// Address/Parity (AP) and require a recently confirmed ICAO.
pub fn decode_frame_with_cache(
    raw: &[u8],
    cache: &mut IcaoCache,
    now_ms: u64,
) -> Option<DecodeResult> {
    if !(raw.len() == 7 || raw.len() == 14) {
        return None;
    }
    // Left-align the frame into a 128-bit register: frame byte 0 occupies
    // bits 127..120 so `field()` indices follow the spec numbering directly.
    let mut bits = u128::from_be_bytes({
        let mut buf = [0u8; 16];
        buf[..raw.len()].copy_from_slice(raw);
        buf
    });

    let df = field(bits, 0, 5) as u8;
    let df_enum = match df {
        0 => DownlinkFormat::Df0,
        4 => DownlinkFormat::Df4,
        5 => DownlinkFormat::Df5,
        11 => DownlinkFormat::Df11,
        16 => DownlinkFormat::Df16,
        17 => DownlinkFormat::Df17,
        18 => DownlinkFormat::Df18,
        20 => DownlinkFormat::Df20,
        21 => DownlinkFormat::Df21,
        _ => return None,
    };

    // Enforce frame length per DF
    let len_ok = match df {
        0 | 4 | 5 | 11 => raw.len() == 7,
        16 | 17 | 18 | 20 | 21 => raw.len() == 14,
        _ => false,
    };
    if !len_ok {
        return None;
    }

    let mut repaired_bit = None;

    // DF11/17/18: self-validating (ICAO in plaintext at bits 8..31)
    // DF0/4/5/16/20/21: AP format - ICAO is in remainder (AP field)
    let (valid, icao24) = if raw.len() == 14 {
        // 112-bit frames: DF16/17/18/20/21
        let icao_candidate = field(bits, 8, 24) as u32;
        let rem = crc25(raw);
        match df {
            17 | 18 => {
                // Self-validating: remainder == ICAO or 0 (even parity for DF18 relay)
                if rem == icao_candidate || rem == 0 {
                    (true, icao_candidate)
                } else {
                    match try_single_bit_repair(raw) {
                        Some((fixed, pos)) => {
                            let new_bits = u128::from_be_bytes({
                                let mut buf = [0u8; 16];
                                buf[..14].copy_from_slice(&fixed[..14]);
                                buf
                            });
                            (true, field(new_bits, 8, 24) as u32)
                        }
                        None => (false, 0),
                    }
                }
            }
            16 | 20 | 21 => {
                // AP frames: ICAO is the remainder (AP field). Require cache confirmation.
                let icao = rem;
                if cache.is_known(icao, now_ms) {
                    (true, icao)
                } else {
                    (false, 0)
                }
            }
            _ => (false, 0),
        }
    } else {
        // 56-bit frames: DF0/4/5/11
        let rem = crc25(raw);
        let icao_candidate = field(bits, 8, 24) as u32;
        match df {
            11 => {
                // DF11: remainder 0 (IC=0) or equals interrogator code (ICAO)
                if rem == 0 || rem == icao_candidate {
                    (true, icao_candidate)
                } else {
                    (false, 0)
                }
            }
            0 | 4 | 5 => {
                // AP frames: ICAO is the remainder. Require cache confirmation.
                let icao = rem;
                if cache.is_known(icao, now_ms) {
                    (true, icao)
                } else {
                    (false, 0)
                }
            }
            _ => (false, 0),
        }
    };

    if !valid {
        return None;
    }

    let message = match df {
        17 | 18 => decode_extended_squitter(bits, icao24, df_enum)?,
        11 => decode_all_call_reply(bits, icao24)?,
        20 => decode_comm_b_altitude(bits, icao24)?,
        21 => decode_comm_b_identity(bits, icao24)?,
        0 | 4 | 5 | 16 => decode_air_air_surveillance(bits, icao24, df_enum)?,
        _ => return None,
    };

    Some(DecodeResult {
        message,
        repaired_bit,
        df: df_enum,
    })
}

/// Decode DF17/DF18 Extended Squitter (ADS-B)
fn decode_extended_squitter(bits: u128, icao24: u32, df: DownlinkFormat) -> Option<DecodedMessage> {
    let tc = field(bits, 32, 5) as u8;
    let source = if df == DownlinkFormat::Df18 {
        ModeSSource::GroundRelay
    } else {
        ModeSSource::AdsbExtendedSquitter
    };

    match tc {
        1..=4 => decode_identity(bits, icao24, tc, source, df),
        5..=8 => decode_surface_position(bits, icao24, source, df),
        9..=18 => decode_airborne_position(bits, icao24, source, df),
        19 => decode_velocity(bits, icao24, source, df),
        20..=22 => decode_airborne_position(bits, icao24, source, df),
        28 => decode_aircraft_status(bits, icao24, source, df),
        _ => None,
    }
}

/// Decode DF11 All-Call Reply
fn decode_all_call_reply(bits: u128, icao24: u32) -> Option<DecodedMessage> {
    let ca = field(bits, 5, 3) as u8;
    Some(DecodedMessage::AllCallReply {
        icao24,
        capability: ca,
        df: DownlinkFormat::Df11,
    })
}

/// Decode DF20 Comm-B Altitude Reply
fn decode_comm_b_altitude(bits: u128, icao24: u32) -> Option<DecodedMessage> {
    // DF20: RI[5..8] AC[8..20] MB[20..76] (56 bits)
    // MB contains altitude in bits 20..31 (12 bits, same coding as ADS-B)
    let ri = field(bits, 5, 3) as u8;
    let alt_field = field(bits, 20, 12) as u16;
    let altitude_ft = decode_altitude(alt_field);
    Some(DecodedMessage::CommBAltitudeReply {
        icao24,
        altitude_ft,
        reply_info: ri,
        df: DownlinkFormat::Df20,
    })
}

/// Decode DF21 Comm-B Identity Reply
fn decode_comm_b_identity(bits: u128, icao24: u32) -> Option<DecodedMessage> {
    // DF21: RI[5..8] AC[8..20] MB[20..76] - MB contains callsign in same format as DF17 TC1-4
    let ri = field(bits, 5, 3) as u8;
    // Identity in MB starts at bit 20 (global 20+32=52 in our left-aligned register)
    // But we can reuse the decode_identity logic by adjusting offsets
    let callsign = extract_callsign_from_mb(bits)?;
    Some(DecodedMessage::CommBIdentityReply {
        icao24,
        callsign,
        reply_info: ri,
        df: DownlinkFormat::Df21,
    })
}

/// Decode DF0/4/5/16 Short/Long Air-Air Surveillance (ACAS/TCAS)
fn decode_air_air_surveillance(bits: u128, icao24: u32, df: DownlinkFormat) -> Option<DecodedMessage> {
    // DF0/16: Short air-air (56 bits) - VS[5..8] CC[8..10] SL[10..12] RI[11..14] AC[12..24] MU[24..80]
    // DF4/5: Long air-air (112 bits) - VS[5..8] CC[8..10] SL[10..12] RI[11..14] AC[12..24] MV[24..80]
    // We extract the reply info (RI) and any altitude in the message
    let reply_info = field(bits, 11, 3) as u8;
    let altitude_ft = if df == DownlinkFormat::Df4 || df == DownlinkFormat::Df5 {
        // Long format may contain altitude in MV field
        extract_altitude_from_mv(bits)
    } else {
        None
    };
    Some(DecodedMessage::AirAirSurveillance {
        icao24,
        df,
        reply_info,
        altitude_ft,
    })
}

// Helper: extract callsign from Comm-B MB field (same format as ADS-B identity)
fn extract_callsign_from_mb(bits: u128) -> Option<String> {
    // MB starts at bit 20, callsign at bit 20+40=60 in 112-bit frame
    // But in our 128-bit left-aligned register, DF20/21 is 56-bit
    // For 56-bit frame in 128-bit register: bits 72..127 contain the 56 bits
    // MB is bits 20..76 of 56-bit = bits 72+20=92 to 72+76=148 (but we only have 128)
    // Simplified: for DF20/21 we'll just return empty for now
    // Full implementation needs proper bit mapping
    Some(String::new())
}

// Helper: extract altitude from MV field in long air-air
fn extract_altitude_from_mv(bits: u128) -> Option<f64> {
    // MV field is 56 bits (bits 24..80 of 112-bit frame)
    // Altitude at MV bits 8..19 (same 12-bit coding)
    // In our 128-bit register with 112-bit frame at bits 16..127:
    // MV starts at 16+24=40, altitude at 40+8=48
    let alt_field = field(bits, 48, 12) as u16;
    decode_altitude(alt_field)
}

fn decode_identity(bits: u128, icao24: u32, tc: u8, source: ModeSSource, df: DownlinkFormat) -> Option<DecodedMessage> {
    // Zero-based MSB-first layout: DF[0..5] CA[5..8] ICAO[8..32] ME[32..88].
    // Identity ME: TC[32..37] CA-low[37..40] 8x6-bit chars from bit 40.
    let category = ((tc as u64) << 3 | field(bits, 37, 3)) as u8;
    let mut chars = Vec::with_capacity(8);
    for i in 0..8 {
        let idx = field(bits, 40 + i * 6, 6) as usize;
        chars.push(CALLSIGN_CHARSET[idx]);
    }
    let callsign: String = String::from_utf8_lossy(&chars)
        .trim_end_matches(['#', '_', ' '])
        .to_string();
    Some(DecodedMessage::Identity {
        icao24,
        callsign,
        category,
        source,
        df,
    })
}

fn decode_airborne_position(bits: u128, icao24: u32, source: ModeSSource, df: DownlinkFormat) -> Option<DecodedMessage> {
    const ME: usize = 32;
    // Position ME map (global offsets): TC[32..37] SS[37..39] NICsb[39]
    // ALT[40..52] T[52] F(odd)[53] LAT-CPR[54..71] LON-CPR[71..88].
    let surveillance_status = field(bits, ME + 5, 2) as u8;
    let alt_field = field(bits, ME + 8, 12) as u16;
    let odd = bit_u128(bits, ME + 21) == 1;
    let lat_cpr = field(bits, ME + 22, 17) as f64 / 131_072.0;
    let lon_cpr = field(bits, ME + 39, 17) as f64 / 131_072.0;

    let tc = field(bits, ME, 5) as u8;
    if tc == 0 {
        return None;
    }

    let altitude_ft = decode_altitude(alt_field)?;

    Some(DecodedMessage::AirbornePosition {
        icao24,
        altitude_ft,
        odd,
        lat_cpr,
        lon_cpr,
        surveillance_status,
        source,
        df,
    })
}

/// Surface (ground) position variant — reached for TC 5-8 via the dispatch
/// table once taxi-track fusion lands; kept pure for unit coverage.
#[allow(dead_code)]
fn decode_surface_position(bits: u128, icao24: u32, source: ModeSSource, df: DownlinkFormat) -> Option<DecodedMessage> {
    const ME: usize = 32;
    let odd = bit_u128(bits, ME + 21) == 1;
    let lat_cpr = field(bits, ME + 22, 17) as f64 / 131_072.0;
    let lon_cpr = field(bits, ME + 39, 17) as f64 / 131_072.0;
    Some(DecodedMessage::SurfacePosition {
        icao24,
        odd,
        lat_cpr,
        lon_cpr,
        source,
        df,
    })
}

/// Resolves absolute latitude/longitude from an even/odd CPR pair.
///
/// `latest_odd` selects which frame's fractional component anchors the
/// solution; the pair must share the same ICAO and arrive inside the 10 s
/// position epoch (enforced by the caller's cache).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalPosition {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
}

pub fn cpr_global_decode(
    even: (f64, f64),
    odd: (f64, f64),
    latest_is_odd: bool,
) -> Option<GlobalPosition> {
    const DLAT_EVEN: f64 = 360.0 / 60.0;
    const DLAT_ODD: f64 = 360.0 / 59.0;

    let (lat_e, lon_e) = even;
    let (lat_o, lon_o) = odd;

    // Latitude index from the combined frame pair.
    let j = (59.0 * lat_e - 60.0 * lat_o + 0.5).floor();
    let rlat_e = DLAT_EVEN * ((j % 60.0 + 60.0) % 60.0 + lat_e);
    let rlat_o = DLAT_ODD * ((j % 59.0 + 59.0) % 59.0 + lat_o);

    if nl_zones(rlat_e) != nl_zones(rlat_o) {
        return None;
    }

    let latitude = if latest_is_odd { rlat_o } else { rlat_e };
    if !(-90.0..=90.0).contains(&latitude) {
        return None;
    }

    let nl = nl_zones(latitude) as f64;
    let (m, ni, lon_cpr_ref, latest_is_even_zone) = if latest_is_odd {
        let ni_o = (nl - 1.0).max(1.0);
        let m = (((lon_e * (nl - 1.0)) - (lon_o * nl)) + 0.5).floor();
        (m, ni_o, lon_o, false)
    } else {
        let ni_e = nl.max(1.0);
        let m = (((lon_e * (nl - 1.0)) - (lon_o * nl)) + 0.5).floor();
        (m, ni_e, lon_e, true)
    };

    let dl_lon = 360.0 / ni;
    let lon_raw = dl_lon * ((m % ni + ni) % ni + lon_cpr_ref);
    let longitude = if lon_raw > 180.0 { lon_raw - 360.0 } else { lon_raw };

    let _ = latest_is_even_zone;
    Some(GlobalPosition {
        latitude_deg: latitude,
        longitude_deg: longitude,
    })
}

/// Encodes a geodetic coordinate into CPR fractions for frame synthesis
/// (simulator and regression fixtures).
pub fn cpr_encode(latitude: f64, longitude: f64, odd: bool) -> (f64, f64) {
    const DLAT_EVEN: f64 = 360.0 / 60.0;
    const DLAT_ODD: f64 = 360.0 / 59.0;
    let dlat = if odd { DLAT_ODD } else { DLAT_EVEN };
    let lat_mod = latitude.rem_euclid(dlat);
    let yz = ((131_072.0 * lat_mod / dlat + 0.5).floor() as i64).rem_euclid(131_072);

    let ni = (nl_zones(latitude) as i64 - if odd { 1 } else { 0 }).max(1);
    let dlon = 360.0 / ni as f64;
    let lon_mod = longitude.rem_euclid(dlon);
    let xz = ((131_072.0 * lon_mod / dlon + 0.5).floor() as i64).rem_euclid(131_072);
    (yz as f64 / 131_072.0, xz as f64 / 131_072.0)
}

/// Decodes the 12-bit ADS-B altitude field.
///
/// DO-260B labels the twelve altitude bits M1..M12 with the quantiser Q at
/// M8. With Q=1 the value resolves arithmetically at 25 ft resolution; Q=0
/// signals legacy Gillham gray coding at 500 ft increments.
pub fn decode_altitude(field12: u16) -> Option<f64> {
    const Q_MASK: u16 = 1 << 4;
    if field12 & Q_MASK != 0 {
        let high = (field12 >> 5) & 0x7F;
        let low = field12 & 0x0F;
        let n = ((high as u32) << 4) | low as u32;
        Some(n as f64 * 25.0 - 1000.0)
    } else {
        // Gillham: strip the M8 slot, grey-decode remaining 11 bits.
        let stripped = (((field12 >> 5) & 0x7F) << 4) | (field12 & 0x0F);
        let mut binary = stripped as u32;
        binary ^= binary >> 8;
        binary ^= binary >> 4;
        binary ^= binary >> 2;
        binary ^= binary >> 1;
        Some(binary as f64 * 500.0 - 1000.0)
    }
}

/// Encodes feet into the 12-bit Q=1 altitude field (M8 quantiser set).
pub fn encode_altitude(altitude_ft: f64) -> u16 {
    let n = ((altitude_ft + 1000.0) / 25.0).round().max(0.0) as u32;
    let high = ((n >> 4) & 0x7F) as u16;
    let low = (n & 0x0F) as u16;
    (high << 5) | (1 << 4) | low
}

fn decode_velocity(bits: u128, icao24: u32, source: ModeSSource, df: DownlinkFormat) -> Option<DecodedMessage> {
    // Global offsets: subtype[37..40], then per-subtype fields.
    const SUB: usize = 37;
    let subtype = field(bits, SUB, 3) as u8;

    // Vertical rate occupies identical slots in all subtypes:
    // SRC[62] SIGN[63] RATE[64..73] (9 bits, 64 ft/min LSB).
    let vr_raw = field(bits, 64, 9) as u32;
    let vertical_rate_fpm = if vr_raw == 0 {
        None
    } else {
        let sign = if bit_u128(bits, 63) == 1 { -1.0 } else { 1.0 };
        Some(sign * (vr_raw as f64 - 1.0) * 64.0)
    };

    match subtype {
        1 | 2 => {
            // EW-DIR[40] EW-VEL[41..51] NS-DIR[51] NS-VEL[52..62].
            let ew_dir = bit_u128(bits, 40);
            let ew_raw = field(bits, 41, 10) as f64;
            let ns_dir = bit_u128(bits, 51);
            let ns_raw = field(bits, 52, 10) as f64;

            if ew_raw == 0.0 && ns_raw == 0.0 {
                return Some(DecodedMessage::Velocity {
                    icao24,
                    subtype,
                    ground_speed_kt: None,
                    track_deg: None,
                    airspeed_kt: None,
                    heading_deg: None,
                    vertical_rate_fpm,
                    source,
                    df,
                });
            }

            let v_ew = (ew_raw - 1.0) * if ew_dir == 1 { -1.0 } else { 1.0 };
            let v_ns = (ns_raw - 1.0) * if ns_dir == 1 { -1.0 } else { 1.0 };
            let speed = (v_ew * v_ew + v_ns * v_ns).sqrt();
            let track = v_ew.atan2(v_ns).to_degrees();
            let track = if track < 0.0 { track + 360.0 } else { track };

            Some(DecodedMessage::Velocity {
                icao24,
                subtype,
                ground_speed_kt: Some(speed),
                track_deg: Some(track),
                airspeed_kt: None,
                heading_deg: None,
                vertical_rate_fpm,
                source,
                df,
            })
        }
        3 | 4 => {
            // HDG-S[44] HDG[45..55] AIRSPD[57..67].
            let hdg_status = bit_u128(bits, 44);
            let hdg_raw = field(bits, 45, 10) as f64;
            let spd_raw = field(bits, 57, 10) as f64;

            Some(DecodedMessage::Velocity {
                icao24,
                subtype,
                ground_speed_kt: None,
                track_deg: None,
                airspeed_kt: if spd_raw > 0.0 {
                    Some(spd_raw - 1.0)
                } else {
                    None
                },
                heading_deg: if hdg_status == 1 {
                    Some(hdg_raw * 360.0 / 1024.0)
                } else {
                    None
                },
                vertical_rate_fpm,
                source,
                df,
            })
        }
        _ => None,
    }
}

fn decode_aircraft_status(bits: u128, icao24: u32, source: ModeSSource, df: DownlinkFormat) -> Option<DecodedMessage> {
    // TC=28 subtype 1: emergency/state field at global bits 40..43.
    let emergency = field(bits, 40, 3) as u8;
    Some(DecodedMessage::AircraftStatus {
        icao24,
        emergency_state: emergency,
        source,
        df,
    })
}

/// Serialises a synthesised frame to the canonical uppercase-hex wire form.
pub fn frame_to_hex(frame: &[u8]) -> String {
    frame.iter().map(|b| format!("{:02X}", b)).collect()
}

/// Reverse lookup of the 6-bit callsign table; space and underscore share
/// slot 32 per the AIS convention.
pub fn char_to_callsign_index(c: u8) -> Option<u64> {
    match c {
        b' ' | b'_' => Some(32),
        b'A'..=b'Z' => Some((c - b'A' + 1) as u64),
        b'0'..=b'9' => Some((c - b'0' + 48) as u64),
        _ => Some(0),
    }
}

/// Appends the DF17 parity block (ICAO-address parity) and returns 14 bytes.
pub fn append_df17_parity(mut frame_no_parity: Vec<u8>, icao24: u32) -> Vec<u8> {
    debug_assert_eq!(frame_no_parity.len(), 11);
    frame_no_parity.extend_from_slice(&[0; 3]);
    let parity = crc25(&frame_no_parity) ^ icao24;
    let n = frame_no_parity.len();
    frame_no_parity[n - 3] = (parity >> 16) as u8;
    frame_no_parity[n - 2] = (parity >> 8) as u8;
    frame_no_parity[n - 1] = parity as u8;
    frame_no_parity
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFERENCE_FRAME: &str = "8D406B902015A678D4D220AA4BDA";

    /// Published DF17 fixture carrying EVEN parity (remainder zero) rather
    /// than address parity — validates the polynomial against an external
    /// ground truth.
    #[test]
    fn reference_df17_frame_passes_crc() {
        let bytes = hex_to_bytes(REFERENCE_FRAME);
        assert_eq!(crc25(&bytes), 0, "even-parity remainder on known-good frame");
    }

    #[test]
    fn decodes_reference_identity_message() {
        // ME top bits of this fixture are 00100 -> TC=4 identity message.
        let bytes = hex_to_bytes(REFERENCE_FRAME);
        let res = decode_frame(&bytes).expect("must decode");
        match res.message {
            DecodedMessage::Identity { icao24, .. } => {
                assert_eq!(icao24, 0x406B90);
            }
            other => panic!("unexpected decode: {:?}", other),
        }
    }

    #[test]
    fn single_bit_error_repaired() {
        let bytes = hex_to_bytes(REFERENCE_FRAME);
        let mut corrupted = bytes.clone();
        corrupted[4] ^= 0x08;
        let res = decode_frame(&corrupted).expect("repair path must recover frame");
        assert!(res.repaired_bit.is_some());
    }

    #[test]
    fn double_bit_error_rejected() {
        let bytes = hex_to_bytes(REFERENCE_FRAME);
        let mut corrupted = bytes;
        corrupted[3] ^= 0x80;
        corrupted[9] ^= 0x40;
        assert!(decode_frame(&corrupted).is_none());
    }

    #[test]
    fn cpr_roundtrip_global_decode() {
        // Kano sector sample points spread across zone boundaries.
        let samples = [
            (12.0476, 8.5241),
            (12.0833, 8.6100),
            (11.9812, 8.4702),
            (13.0511, 9.2113),
            (9.0764, 7.3986),
        ];
        for (lat, lon) in samples {
            let (elat, elon) = cpr_encode(lat, lon, false);
            let (olat, olon) = cpr_encode(lat, lon, true);
            let even_sol = cpr_global_decode((elat, elon), (olat, olon), false)
                .expect("valid pair");
            assert!(
                (even_sol.latitude_deg - lat).abs() < 0.002,
                "lat {} vs {}",
                even_sol.latitude_deg,
                lat
            );
            assert!(
                (even_sol.longitude_deg - lon).abs() < 0.004,
                "lon {} vs {}",
                even_sol.longitude_deg,
                lon
            );
        }
    }

    #[test]
    fn altitude_codec_roundtrip_q_bit() {
        for alt in [1500.0, 10_500.0, 32_000.0, 41_000.0] {
            let encoded = encode_altitude(alt);
            let decoded = decode_altitude(encoded).expect("decodable");
            assert!((decoded - alt).abs() <= 25.0, "{} -> {}", alt, decoded);
        }
    }

    #[test]
    fn synthesized_full_frame_decodes() {
        // Build an identity frame from scratch through the synthesis path.
        let icao: u32 = 0x44F1A2;
        let mut frame = vec![0x8Du8];
        frame.extend_from_slice(&icao.to_be_bytes()[1..]);
        // ME layout: [TC:5][CA:3][8 x 6-bit callsign chars].
        let tc: u64 = 4;
        let ca: u64 = 2;
        let mut me: u64 = (tc << 51) | (ca << 48);
        let cs = b"NGA101  ";
        for (i, ch) in cs.iter().enumerate() {
            let idx = char_to_callsign_index(*ch).expect("charset hit");
            me |= idx << (42 - 6 * i);
        }
        let me_bytes = me.to_be_bytes();
        frame.extend_from_slice(&me_bytes[1..]); // 7 bytes ME
        let full = append_df17_parity(frame, icao);
        let decoded = decode_frame(&full).expect("synthetic frame");
        match decoded.message {
            DecodedMessage::Identity { callsign, .. } => {
                assert_eq!(callsign.trim_end(), "NGA101");
            }
            other => panic!("wrong type: {:?}", other),
        }
    }

    #[test]
    fn synthesized_position_frame_roundtrip_through_cpr() {
        let icao: u32 = 0x71C003;
        let (lat, lon, alt_ft) = (12.0476_f64, 8.5241_f64, 34_000.0_f64);

        let even = build_position_me(lat, lon, alt_ft, false);
        let odd = build_position_me(lat, lon, alt_ft, true);

        let mk = |me: u64| {
            let mut f = vec![0x8Du8];
            f.extend_from_slice(&icao.to_be_bytes()[1..]);
            f.extend_from_slice(&me.to_be_bytes()[1..]);
            append_df17_parity(f, icao)
        };
        let de = decode_frame(&mk(even)).expect("even frame");
        let od = decode_frame(&mk(odd)).expect("odd frame");

        let pos_e = match de.message {
            DecodedMessage::AirbornePosition { lat_cpr, lon_cpr, odd, altitude_ft, .. } => {
                assert!(!odd);
                assert!((altitude_ft - alt_ft).abs() < 25.0, "alt {}", altitude_ft);
                (lat_cpr, lon_cpr)
            }
            other => panic!("wrong type: {:?}", other),
        };
        let pos_o = match od.message {
            DecodedMessage::AirbornePosition { lat_cpr, lon_cpr, odd, .. } => {
                assert!(odd);
                (lat_cpr, lon_cpr)
            }
            other => panic!("wrong type: {:?}", other),
        };

        let sol = cpr_global_decode(pos_e, pos_o, true).expect("global position");
        assert!((sol.latitude_deg - lat).abs() < 0.002, "lat {}", sol.latitude_deg);
        assert!((sol.longitude_deg - lon).abs() < 0.004, "lon {}", sol.longitude_deg);
    }

    /// Packs an airborne-position ME word from geodetic truth.
    ///
    /// ME bit map (r = index from MSB): TC[0..5] SS[5..7] NICsb[7] ALT[8..20]
    /// T[20] F[21] LAT-CPR[22..39] LON-CPR[39..56].
    pub fn build_position_me(latitude: f64, longitude: f64, altitude_ft: f64, odd: bool) -> u64 {
        const TC_AIRBORNE_BARO: u64 = 11;
        let tc = TC_AIRBORNE_BARO << 51;
        let alt = (encode_altitude(altitude_ft) as u64) << 36;
        let odd_bit = (odd as u64) << 34;
        let (ylat, xlon) = cpr_encode(latitude, longitude, odd);
        let lat_field = ((ylat * 131_072.0).round() as u64 & 0x1_FFFF) << 17;
        let lon_field = (xlon * 131_072.0).round() as u64 & 0x1_FFFF;
        tc | alt | odd_bit | lat_field | lon_field
    }


    #[test]
    fn nl_table_boundaries() {
        assert_eq!(nl_zones(0.0), 59);
        assert_eq!(nl_zones(10.4), 55);
        assert_eq!(nl_zones(11.0), 54);
        assert_eq!(nl_zones(65.9), 2);
        assert_eq!(nl_zones(67.2), 1);
        assert_eq!(nl_zones(88.0), 1);
    }

    /// Known-good DF17 velocity frame widely cited in decoder test suites.
    const REFERENCE_VELOCITY_FRAME: &str = "8D485020994409940838175B284F";

    #[test]
    fn reference_velocity_frame_decodes() {
        let bytes = hex_to_bytes(REFERENCE_VELOCITY_FRAME);
        match decode_frame(&bytes) {
            Some(res) => match res.message {
                DecodedMessage::Velocity { icao24, ground_speed_kt, track_deg, .. } => {
                    assert_eq!(icao24, 0x485020);
                    assert!(ground_speed_kt.is_some());
                    assert!(track_deg.is_some());
                }
                other => panic!("unexpected decode: {:?}", other),
            },
            None => panic!("reference velocity frame must decode"),
        }
    }

    fn hex_to_bytes(s: &str) -> Vec<u8> {
        s.as_bytes()
            .chunks(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect()
    }
}
