//! VHF ACARS (Aircraft Communications Addressing and Reporting System)
//! frame decoder for 131.550 MHz.
//!
//! Consumes byte streams produced by the MSK demodulator front-end and
//! extracts structured uplink/downlink messages: tail number, flight id,
//! label, block sequence, message text and BCS integrity. Weather-bearing
//! labels are classified into D-ATIS and METAR/SPECI reports feeding the
//! triple-fusion weather matrix.

#[derive(Debug, Clone, PartialEq)]
pub struct AcarsMessage {
    pub mode: char,
    pub aircraft_tail: String,
    pub ack: char,
    pub label: String,
    pub block_id: char,
    /// True when the message came from an aircraft (downlink).
    pub downlink: bool,
    pub flight_id: String,
    pub text: String,
    pub crc_valid: bool,
    /// Milliseconds since UNIX epoch of reception.
    pub received_ms: u64,
}

/// Classification applied by `classify_weather`.
#[derive(Debug, Clone, PartialEq)]
pub enum WeatherProduct {
    Metar(String),
    Speci(String),
    Datis { station: Option<char>, info_letter: char, body: String },
    Other,
}

pub const SOH: u8 = 0x01;
pub const STX: u8 = 0x02;
pub const ETX: u8 = 0x03;
pub const ETB: u8 = 0x17;
pub const BEL: u8 = 0x07;
pub const DLE: u8 = 0x10;

/// CRC-16/CCITT-FALSE (poly 0x1021, init 0xFFFF) used for the ACARS BCS.
pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Scans a demodulated byte buffer and yields every complete ACARS frame
/// found between SOH markers. Tolerates inter-frame noise.
pub fn parse_stream(buffer: &[u8], now_ms: u64) -> Vec<AcarsMessage> {
    let mut messages = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel) = buffer[cursor..].iter().position(|&b| b == SOH) {
        let start = cursor + rel;
        match locate_terminator(&buffer[start..]) {
            // Frame spans SOH..terminator inclusive plus the 2-byte BCS.
            Some(term_rel) if start + term_rel + 3 <= buffer.len() => {
                let end = start + term_rel + 3;
                if let Some(msg) = parse_frame(&buffer[start..end], now_ms) {
                    messages.push(msg);
                }
                cursor = end;
            }
            _ => break,
        }
    }
    messages
}

fn locate_terminator(buf: &[u8]) -> Option<usize> {
    buf.iter().position(|&b| matches!(b, ETX | ETB))
}

fn parse_frame(frame: &[u8], now_ms: u64) -> Option<AcarsMessage> {
    // Layout: SOH MODE ADDR ACK LABEL BID [STX FLIGHT TEXT] ETX BCS(2)
    if frame.len() < 12 || frame[0] != SOH {
        return None;
    }
    let body_end = frame.len() - 2;
    let body = &frame[..body_end];
    let stored_crc = u16::from_be_bytes([frame[body_end], frame[body_end + 1]]);
    let computed = crc16_ccitt(body);

    let mode = frame[1] as char;
    let addr_end = (2..frame.len()).find(|&i| !frame[i].is_ascii_graphic() || i > 9)?;
    let addr_bytes: Vec<u8> = frame[2..addr_end]
        .iter()
        .copied()
        .filter(|b| b.is_ascii_alphanumeric() || *b == b'.')
        .collect();
    if addr_bytes.is_empty() {
        return None;
    }

    let after_addr = addr_end + 1; // skip ACK char
    if after_addr + 3 >= frame.len() {
        return None;
    }
    let label = String::from_utf8_lossy(&frame[after_addr..after_addr + 2])
        .replace(['\u{7F}', '\u{81}', '\u{82}', '@'], "-");
    let block_id = frame.get(after_addr + 2).copied().unwrap_or(b'?') as char;

    let (downlink, flight_id, text) = match frame.iter().position(|&b| b == STX) {
        Some(stx) => {
            let payload = &frame[stx + 1..body_end];
            // Downlink payloads open with the flight id terminated by a
            // carriage return; fall back to a fixed 4-char field when the
            // encoder omitted it.
            match payload.iter().position(|&b| b == b'\r' || b == b'\n') {
                Some(cr) => (
                    true,
                    String::from_utf8_lossy(&payload[..cr]).trim().to_string(),
                    sanitize_text(&payload[cr + 1..]),
                ),
                None => (
                    true,
                    String::from_utf8_lossy(&payload[..payload.len().min(4)])
                        .trim()
                        .to_string(),
                    sanitize_text(&payload[payload.len().min(4)..]),
                ),
            }
        }
        None => (false, String::new(), String::new()),
    };

    Some(AcarsMessage {
        mode,
        aircraft_tail: String::from_utf8_lossy(&addr_bytes).to_string(),
        ack: frame.get(addr_end).map(|b| *b as char).unwrap_or('?'),
        label: label.trim().to_string(),
        block_id,
        downlink,
        flight_id,
        text,
        crc_valid: stored_crc == computed,
        received_ms: now_ms,
    })
}

fn sanitize_text(raw: &[u8]) -> String {
    raw.iter()
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' || b == b'\n' || b == b'\r' {
                b as char
            } else {
                '\u{00B7}'
            }
        })
        .collect::<String>()
        .replace('\r', "")
        .trim()
        .to_string()
}

/// Routes a decoded message into a weather product when applicable.
pub fn classify_weather(msg: &AcarsMessage) -> WeatherProduct {
    let upper = msg.text.to_uppercase();

    if upper.starts_with("SPECI") {
        return WeatherProduct::Speci(msg.text.clone());
    }
    if upper.starts_with("METAR") {
        return WeatherProduct::Metar(msg.text.clone());
    }

    // D-ATIS labels commonly observed on airband: A7/A6 (ATIS), plus plain
    // 'ATIS' free-text uplinks from ground stations.
    let datis_label =
        matches!(msg.label.as_str(), "A7" | "A6" | "A5" | "AT") || msg.label.contains('A');
    if datis_label && (upper.contains("ATIS") || looks_like_datis(&upper)) {
        let station = msg.text.chars().next();
        let info_letter = extract_info_letter(&upper);
        return WeatherProduct::Datis {
            station,
            info_letter,
            body: msg.text.clone(),
        };
    }

    if upper.starts_with("SA ") || upper.starts_with("SAXXX") {
        return WeatherProduct::Metar(msg.text.trim_start_matches("SA ").to_string());
    }

    WeatherProduct::Other
}

fn looks_like_datis(text: &str) -> bool {
    // Heuristic: "DNKN INFO B 1256Z ..." style openings.
    text.len() > 20 && text.matches(char::is_numeric).count() >= 4
}

fn extract_info_letter(text: &str) -> char {
    const KEYWORDS: [&str; 2] = ["INFO ", "INFORMATION "];
    for kw in KEYWORDS {
        if let Some(pos) = text.find(kw) {
            let rest = &text[pos + kw.len()..];
            if let Some(c) = rest.chars().next() {
                if c.is_ascii_uppercase() {
                    return c;
                }
            }
        }
    }
    '?'
}

/// Builds a wire-format ACARS frame (SOH..ETX + BCS) matching the parser's
/// expectations. Used by the simulator to exercise the production decode
/// path and by bench tooling.
pub fn synthesize_frame(
    mode: char,
    tail: &str,
    ack: char,
    label: &str,
    block_id: char,
    flight: &str,
    text: &str,
) -> Vec<u8> {
    let mut body = Vec::new();
    body.push(SOH);
    body.push(mode as u8);
    body.extend_from_slice(tail.as_bytes());
    body.push(ack as u8);
    body.extend_from_slice(label.as_bytes());
    body.push(block_id as u8);
    if !flight.is_empty() || !text.is_empty() {
        body.push(STX);
        body.extend_from_slice(flight.as_bytes());
        if !flight.is_empty() && !text.is_empty() {
            body.push(b'\r');
        }
        body.extend_from_slice(text.as_bytes());
    }
    body.push(ETX);
    let crc = crc16_ccitt(&body);
    body.extend_from_slice(&crc.to_be_bytes());
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_frame(mode: char, tail: &str, ack: char, label: &str, bid: char, flight: &str, text: &str) -> Vec<u8> {
        synthesize_frame(mode, tail, ack, label, bid, flight, text)
    }

    #[test]
    fn parses_wellformed_metar_downlink() {
        let metar_text =
            "METAR DNKN 261200Z 18012KT 6000 HZ FEW030 33/24 Q1013 NOSIG=";
        let frame = build_frame('2', "5NBYQ", '\u{06}', "SA", '1', "VL604", metar_text);
        let msgs = parse_stream(&frame, 1000);
        assert_eq!(msgs.len(), 1);
        let m = &msgs[0];
        assert!(m.downlink);
        assert_eq!(m.aircraft_tail, "5NBYQ");
        assert_eq!(m.flight_id, "VL604");
        assert!(m.crc_valid);
        match classify_weather(m) {
            WeatherProduct::Metar(t) => assert!(t.contains("Q1013")),
            other => panic!("wrong product {:?}", other),
        }
    }

    #[test]
    fn parses_datis_uplink() {
        let datis_text = "DNKN INFO C 261305Z APCH ILS RWY 06 WIND 190/14 QNH 1014 TEMPO 4000 HZ=";
        let frame = build_frame('A', "KNATIS", '\u{15}', "A7", '2', "", datis_text);
        let msgs = parse_stream(&frame, 2000);
        assert_eq!(msgs.len(), 1, "uplink frame must parse");
        match classify_weather(&msgs[0]) {
            WeatherProduct::Datis { info_letter, .. } => assert_eq!(info_letter, 'C'),
            other => panic!("expected datis, got {:?}", other),
        }
    }

    #[test]
    fn corrupted_bcs_flagged_not_fatal() {
        let mut frame = build_frame('2', "5NBKD", '\u{06}', "_d", '1', "DTR", "POSN REPORT");
        let last = frame.len() - 1;
        frame[last] ^= 0xFF;
        let msgs = parse_stream(&frame, 3000);
        assert_eq!(msgs.len(), 1);
        assert!(!msgs[0].crc_valid);
        assert!(matches!(
            classify_weather(&msgs[0]),
            WeatherProduct::Other
        ));
    }

    #[test]
    fn noise_between_frames_is_skipped() {
        let mut stream = vec![0xAAu8; 37];
        stream.extend(build_frame('2', "5NAZ", '\u{06}', "H1", '1', "NGA", "ENGINE START"));
        stream.extend_from_slice(&[0x55; 11]);
        let msgs = parse_stream(&stream, 4000);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].text.contains("ENGINE START"));
    }

    #[test]
    fn crc_reference_vector() {
        // CRC-16/CCITT-FALSE of ASCII "123456789" is 0x29B1.
        assert_eq!(crc16_ccitt(b"123456789"), 0x29B1);
    }
}
