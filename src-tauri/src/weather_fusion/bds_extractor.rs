//! Mode S Comm-B Meteorological Register extraction.
//!
//! Implements the BDS 4,4 (Meteorological Routine Air Report) and BDS 4,5
//! (Meteorological Hazard Report) register codecs over the 7-byte Comm-B
//! BCS field. Encoders are provided so bench fixtures and the simulator can
//! round-trip against the production decoder.

/// Wind report from BDS 4,4.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindReport {
    /// Wind speed, knots.
    pub speed_kt: u16,
    /// Wind direction from-true, degrees.
    pub dir_deg: f32,
    /// Static air temperature, degrees Celsius.
    pub temperature_c: f32,
}

/// Hazard flags from BDS 4,5.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HazardReport {
    /// 0 none .. 3 severe.
    pub turbulence: u8,
    /// 0 no report .. 3 severe.
    pub wind_shear: u8,
    /// 0 no report .. 3 microburst detected.
    pub microburst: u8,
    /// Icing state: 0 none, 1 light, 2 standard, 3 severe.
    pub icing: u8,
    /// Static air temperature deviation from ISA, degrees Celsius.
    pub temp_deviation_c: f32,
}

const INVALID_SPEED: u16 = 0;
const INVALID_DIR: f32 = -1.0;

/// Decodes a 7-byte BDS 4,4 BCS payload.
///
/// Bit map (MSB first): WS[0..12] kt · WD[12..23] scaled 360/2048 deg ·
/// STATUS[23] · SAT[24..40] two's-complement 0.25 degC · reserved[40..56].
pub fn decode_bds44(bcs: &[u8; 7]) -> Option<WindReport> {
    let bits = u64::from_be_bytes({
        let mut b = [0u8; 8];
        b[1..].copy_from_slice(bcs);
        b
    });
    let field = |start: usize, len: usize| -> u64 {
        (bits >> (56 - start - len)) & ((1u64 << len) - 1)
    };

    let speed_raw = field(0, 12) as u16;
    if speed_raw == INVALID_SPEED || speed_raw > 550 {
        return None;
    }
    let dir_raw = field(12, 11);
    // Direction validity flag shares the top status bit region.
    let dir_valid = field(23, 1) == 1;
    let dir_deg = if dir_valid {
        dir_raw as f32 * 360.0 / 2048.0
    } else {
        INVALID_DIR
    };

    let sat_raw = field(24, 16) as i32;
    let sat_signed = if sat_raw >= 0x8000 {
        sat_raw - 0x1_0000
    } else {
        sat_raw
    };
    let temperature_c = sat_signed as f32 * 0.25;

    Some(WindReport {
        speed_kt: speed_raw,
        dir_deg,
        temperature_c,
    })
}

/// Encodes a wind report into the 7-byte BDS 4,4 layout.
pub fn encode_bds44(report: &WindReport) -> [u8; 7] {
    let speed = report.speed_kt.clamp(1, 550) as u64;
    let dir_valid = (report.dir_deg >= 0.0) as u64;
    let dir_q = ((report.dir_deg.rem_euclid(360.0) / 360.0 * 2048.0).round() as u64) & 0x7FF;
    let sat_q = ((report.temperature_c / 0.25).round() as i32) as u16;

    let bits = (speed << 44)
        | (dir_q << 33)
        | (dir_valid << 32)
        | (((sat_q as u64) & 0xFFFF) << 16);

    let be = bits.to_be_bytes();
    {
        let mut out = [0u8; 7];
        out.copy_from_slice(&be[1..]);
        out
    }
}

/// Decodes a 7-byte BDS 4,5 BCS payload.
///
/// Bit map: TURB[0..2] SHEAR[2..4] MICROBURST[4..6] ICING[6..9]
/// TEMPDEV[9..25] signed 0.1 degC from ISA · reserved[25..56].
pub fn decode_bds45(bcs: &[u8; 7]) -> Option<HazardReport> {
    let bits = u64::from_be_bytes({
        let mut b = [0u8; 8];
        b[1..].copy_from_slice(bcs);
        b
    });
    let field = |start: usize, len: usize| -> u64 {
        (bits >> (56 - start - len)) & ((1u64 << len) - 1)
    };

    let turbulence = field(0, 2) as u8;
    let wind_shear = field(2, 2) as u8;
    let microburst = field(4, 2) as u8;
    let icing = field(6, 3) as u8;
    if icing == 0b111 && turbulence == 0 && wind_shear == 0 {
        // All-invalid sentinel: no data reported.
        return None;
    }

    let td_raw = field(9, 16) as i32;
    let td_signed = if td_raw >= 0x8000 { td_raw - 0x1_0000 } else { td_raw };
    let temp_deviation_c = td_signed as f32 * 0.1;

    Some(HazardReport {
        turbulence,
        wind_shear,
        microburst,
        icing,
        temp_deviation_c,
    })
}

/// Encodes a hazard report into the 7-byte BDS 4,5 layout.
pub fn encode_bds45(report: &HazardReport) -> [u8; 7] {
    let td_q = ((report.temp_deviation_c / 0.1).round() as i32) as u16;
    let bits = ((report.turbulence as u64 & 0x3) << 54)
        | ((report.wind_shear as u64 & 0x3) << 52)
        | ((report.microburst as u64 & 0x3) << 50)
        | ((report.icing as u64 & 0x7) << 47)
        | (((td_q as u64) & 0xFFFF) << 31);

    let be = bits.to_be_bytes();
    let mut out = [0u8; 7];
    out.copy_from_slice(&be[1..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bds44_roundtrip() {
        for (spd, dir, temp) in [(35u16, 182.0f32, 12.5f32), (140, 359.9, -54.25), (5, 1.0, 31.75)] {
            let r = WindReport { speed_kt: spd, dir_deg: dir, temperature_c: temp };
            let enc = encode_bds44(&r);
            let dec = decode_bds44(&enc).expect("decodable");
            assert_eq!(dec.speed_kt, spd);
            assert!((dec.dir_deg - dir).abs() < 0.25, "dir {}", dec.dir_deg);
            assert!((dec.temperature_c - temp).abs() < 0.26, "temp {}", dec.temperature_c);
        }
    }

    #[test]
    fn bds44_invalid_speed_rejected() {
        let zero = [0u8; 7];
        assert!(decode_bds44(&zero).is_none());
    }

    #[test]
    fn bds44_negative_temperatures() {
        let r = WindReport { speed_kt: 80, dir_deg: 90.0, temperature_c: -56.5 };
        let dec = decode_bds44(&encode_bds44(&r)).expect("decodable");
        assert!((dec.temperature_c - (-56.5)).abs() < 0.26);
    }

    #[test]
    fn bds45_roundtrip() {
        let r = HazardReport {
            turbulence: 2,
            wind_shear: 1,
            microburst: 0,
            icing: 3,
            temp_deviation_c: -7.4,
        };
        let dec = decode_bds45(&encode_bds45(&r)).expect("decodable");
        assert_eq!(dec.turbulence, 2);
        assert_eq!(dec.wind_shear, 1);
        assert_eq!(dec.icing, 3);
        assert!((dec.temp_deviation_c - (-7.4)).abs() < 0.11);
    }

    #[test]
    fn bds45_sentinel_is_none() {
        let empty = HazardReport {
            turbulence: 0,
            wind_shear: 0,
            microburst: 0,
            icing: 7,
            temp_deviation_c: 0.0,
        };
        // Sentinel requires zeroed temp-deviation bits too.
        let mut enc = encode_bds45(&empty);
        enc[2] &= 0x00;
        enc[3] &= 0xFE;
        assert!(decode_bds45(&enc).is_none());
    }
}
