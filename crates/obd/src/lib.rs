//! Read-only OBD-II through an ELM327-compatible adapter (USB, or Bluetooth that the OS
//! exposes as a serial port).
//!
//! Safety: only reading services are ever sent — mode 01 (current data), 02 (freeze frame),
//! 03 (stored codes), 07 (pending codes) and 09 02 (VIN) — plus the adapter's own AT setup
//! commands. Clearing codes (mode 04) and anything that writes to a control module are refused
//! by `Elm327::query`, whatever the caller asks for.

use serde::Serialize;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum ObdError {
    #[error("The adapter couldn't be opened: {0}")]
    Open(String),
    #[error("The adapter stopped responding.")]
    Timeout,
    #[error("The adapter answered “{0}”.")]
    Adapter(String),
    #[error("Auto Manager only sends read-only requests; “{0}” isn't allowed.")]
    NotAllowed(String),
    #[error("Communication error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ObdError>;

/// Something that takes one command line and returns the adapter's reply (up to the '>' prompt).
pub trait Transport {
    fn exchange(&mut self, line: &str) -> Result<String>;
}

pub struct SerialTransport {
    port: Box<dyn serialport::SerialPort>,
}

impl SerialTransport {
    pub fn open(path: &str, baud: u32) -> Result<Self> {
        let port = serialport::new(path, baud).timeout(Duration::from_millis(200)).open().map_err(|e| ObdError::Open(e.to_string()))?;
        Ok(SerialTransport { port })
    }
}

impl Transport for SerialTransport {
    fn exchange(&mut self, line: &str) -> Result<String> {
        self.port.write_all(format!("{line}\r").as_bytes())?;
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut out = Vec::new();
        let mut buf = [0u8; 256];
        loop {
            if Instant::now() > deadline {
                return Err(ObdError::Timeout);
            }
            match self.port.read(&mut buf) {
                Ok(n) if n > 0 => {
                    out.extend_from_slice(&buf[..n]);
                    if out.contains(&b'>') {
                        break;
                    }
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(String::from_utf8_lossy(&out).replace('>', ""))
    }
}

/// Serial ports that could be an adapter.
pub fn ports() -> Vec<String> {
    serialport::available_ports().map(|v| v.into_iter().map(|p| p.port_name).collect()).unwrap_or_default()
}

fn allowed(cmd: &str) -> bool {
    let c = cmd.trim().to_uppercase().replace(' ', "");
    if c.starts_with("AT") {
        // Adapter configuration only: reset, echo, linefeeds, spaces, headers, protocol, info.
        return ["ATZ", "ATE0", "ATL0", "ATS1", "ATH0", "ATSP0", "ATDPN", "ATI", "ATRV", "ATAT1"].contains(&c.as_str());
    }
    let hex = c.chars().all(|x| x.is_ascii_hexdigit());
    hex && (c.starts_with("01") || c.starts_with("02") || c == "03" || c == "07" || c == "0902")
}

pub struct Elm327<T: Transport> {
    t: T,
    pub adapter: String,
    pub protocol: String,
    can: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Reading {
    pub pid: String,
    pub name: String,
    pub value: String,
    pub unit: String,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Scan {
    pub adapter: String,
    pub protocol: String,
    pub vin: Option<String>,
    pub mil_on: Option<bool>,
    pub stored: Vec<String>,
    pub pending: Vec<String>,
    pub live: Vec<Reading>,
    pub freeze_frame: Vec<Reading>,
    pub freeze_dtc: Option<String>,
}

impl<T: Transport> Elm327<T> {
    pub fn query(&mut self, cmd: &str) -> Result<String> {
        if !allowed(cmd) {
            return Err(ObdError::NotAllowed(cmd.into()));
        }
        // Adapters end lines with a bare CR (LF only when ATL1); make them ordinary lines.
        let r = self.t.exchange(cmd)?.replace("\r\n", "\n").replace('\r', "\n");
        let upper = r.to_uppercase();
        for bad in ["UNABLE TO CONNECT", "BUS INIT", "CAN ERROR", "BUFFER FULL"] {
            if upper.contains(bad) {
                return Err(ObdError::Adapter(r.trim().into()));
            }
        }
        Ok(r)
    }

    /// Reset and configure the adapter, then let it find the vehicle's protocol.
    pub fn connect(t: T) -> Result<Self> {
        let mut e = Elm327 { t, adapter: String::new(), protocol: String::new(), can: false };
        e.adapter = e
            .query("ATZ")?
            .lines()
            .map(str::trim)
            .find(|l| l.to_uppercase().contains("ELM"))
            .unwrap_or("ELM327-compatible adapter")
            .to_string();
        for c in ["ATE0", "ATL0", "ATS1", "ATH0", "ATSP0"] {
            e.query(c)?;
        }
        // The first request makes the adapter search for the protocol.
        e.query("0100")?;
        let p = e.query("ATDPN")?.trim().trim_start_matches('A').to_string();
        e.can = matches!(p.chars().next(), Some('6'..='9') | Some('A'..='C'));
        e.protocol = protocol_name(&p).into();
        Ok(e)
    }

    pub fn scan(&mut self) -> Result<Scan> {
        let mut s = Scan { adapter: self.adapter.clone(), protocol: self.protocol.clone(), ..Default::default() };
        if let Ok(r) = self.query("0101")
            && let Some(b) = data_bytes(&r, 0x41, 0x01).first()
        {
            s.mil_on = Some(b & 0x80 != 0);
        }
        s.stored = parse_dtcs(&self.query("03")?, 0x43, self.can);
        s.pending = parse_dtcs(&self.query("07")?, 0x47, self.can);
        s.vin = self.query("0902").ok().and_then(|r| parse_vin(&r));
        for pid in PIDS {
            if let Ok(r) = self.query(&format!("01{:02X}", pid.0))
                && let Some(v) = decode_pid(pid.0, &data_bytes(&r, 0x41, pid.0))
            {
                s.live.push(Reading { pid: format!("01{:02X}", pid.0), name: pid.1.into(), value: v, unit: pid.2.into() });
            }
        }
        if let Ok(r) = self.query("020200") {
            let b = data_bytes(&r, 0x42, 0x02);
            if b.len() >= 3 && (b[1] != 0 || b[2] != 0) {
                s.freeze_dtc = Some(dtc(b[1], b[2]));
                for pid in PIDS {
                    if let Ok(r) = self.query(&format!("02{:02X}00", pid.0)) {
                        let d = data_bytes(&r, 0x42, pid.0);
                        if let Some(v) = d.get(1..).and_then(|x| decode_pid(pid.0, x)) {
                            s.freeze_frame.push(Reading {
                                pid: format!("02{:02X}", pid.0),
                                name: pid.1.into(),
                                value: v,
                                unit: pid.2.into(),
                            });
                        }
                    }
                }
            }
        }
        Ok(s)
    }
}

fn protocol_name(n: &str) -> &'static str {
    match n.chars().next() {
        Some('1') => "SAE J1850 PWM",
        Some('2') => "SAE J1850 VPW",
        Some('3') => "ISO 9141-2",
        Some('4') => "ISO 14230-4 KWP (5 baud init)",
        Some('5') => "ISO 14230-4 KWP (fast init)",
        Some('6') => "ISO 15765-4 CAN (11 bit, 500 kbaud)",
        Some('7') => "ISO 15765-4 CAN (29 bit, 500 kbaud)",
        Some('8') => "ISO 15765-4 CAN (11 bit, 250 kbaud)",
        Some('9') => "ISO 15765-4 CAN (29 bit, 250 kbaud)",
        _ => "Unknown protocol",
    }
}

/// (pid, name, unit)
pub const PIDS: &[(u8, &str, &str)] = &[
    (0x04, "Engine load", "%"),
    (0x05, "Coolant temperature", "°C"),
    (0x0B, "Intake manifold pressure", "kPa"),
    (0x0C, "Engine speed", "rpm"),
    (0x0D, "Vehicle speed", "km/h"),
    (0x0E, "Timing advance", "°"),
    (0x0F, "Intake air temperature", "°C"),
    (0x10, "Mass air flow", "g/s"),
    (0x11, "Throttle position", "%"),
    (0x1F, "Run time since start", "s"),
    (0x2F, "Fuel level", "%"),
    (0x42, "Control module voltage", "V"),
];

pub fn decode_pid(pid: u8, d: &[u8]) -> Option<String> {
    let a = *d.first()? as f64;
    let b = d.get(1).copied().unwrap_or(0) as f64;
    let v = match pid {
        0x04 | 0x11 | 0x2F => a * 100.0 / 255.0,
        0x05 | 0x0F => a - 40.0,
        0x0B | 0x0D => a,
        0x0C => (a * 256.0 + b) / 4.0,
        0x0E => a / 2.0 - 64.0,
        0x10 => (a * 256.0 + b) / 100.0,
        0x1F => a * 256.0 + b,
        0x42 => (a * 256.0 + b) / 1000.0,
        _ => return None,
    };
    Some(if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.1}") })
}

fn hex_bytes(line: &str) -> Vec<u8> {
    line.split_whitespace().filter_map(|t| u8::from_str_radix(t, 16).ok()).collect()
}

/// The bytes after `service pid` in a single-frame reply, e.g. "41 0C 1A F8" → [1A, F8].
pub fn data_bytes(reply: &str, service: u8, pid: u8) -> Vec<u8> {
    for line in reply.lines() {
        let b = hex_bytes(line);
        if let Some(i) = b.windows(2).position(|w| w[0] == service && w[1] == pid) {
            return b[i + 2..].to_vec();
        }
    }
    vec![]
}

pub fn dtc(a: u8, b: u8) -> String {
    let letter = ['P', 'C', 'B', 'U'][(a >> 6) as usize];
    format!("{letter}{}{:X}{:02X}", (a >> 4) & 0x3, a & 0x0F, b)
}

/// Codes from a mode 03/07 reply. CAN replies carry a count byte after the service byte and
/// may span several numbered lines ("0: …", "1: …").
pub fn parse_dtcs(reply: &str, service: u8, can: bool) -> Vec<String> {
    let mut bytes = Vec::new();
    for line in reply.lines() {
        let l = line.trim();
        let l = l.split_once(':').map(|(_, r)| r).unwrap_or(l);
        let b = hex_bytes(l);
        if b.is_empty() || (b.len() == 1 && !can) {
            continue;
        }
        bytes.extend(b);
    }
    // Drop a leading byte-count line value (CAN multi-frame header such as "00A").
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == service {
            i += if can { 2 } else { 1 };
            continue;
        }
        if i + 1 < bytes.len() {
            let (a, b) = (bytes[i], bytes[i + 1]);
            if a != 0 || b != 0 {
                let c = dtc(a, b);
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
        i += 2;
    }
    out
}

pub fn parse_vin(reply: &str) -> Option<String> {
    let mut bytes = Vec::new();
    for line in reply.lines() {
        let l = line.trim();
        let l = l.split_once(':').map(|(_, r)| r).unwrap_or(l);
        bytes.extend(hex_bytes(l));
    }
    // Skip "49 02 01" (and the CAN count) and keep printable characters.
    let start = bytes.windows(2).position(|w| w == [0x49, 0x02]).map(|i| i + 3).unwrap_or(0);
    let vin: String = bytes.get(start..)?.iter().filter(|b| b.is_ascii_alphanumeric()).map(|b| *b as char).collect();
    (vin.len() >= 17).then(|| vin[vin.len() - 17..].to_string())
}

pub fn connect_serial(path: &str) -> Result<Elm327<SerialTransport>> {
    let mut last = None;
    for baud in [38400, 9600, 115200] {
        match SerialTransport::open(path, baud).and_then(Elm327::connect) {
            Ok(e) => return Ok(e),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or(ObdError::Timeout))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A scripted adapter, as an ELM327 on a 2010s CAN vehicle answers.
    struct Fake(HashMap<&'static str, &'static str>, Vec<String>);
    impl Transport for Fake {
        fn exchange(&mut self, line: &str) -> Result<String> {
            self.1.push(line.to_string());
            Ok(self.0.get(line).copied().unwrap_or("NO DATA").to_string())
        }
    }

    fn can_car() -> Fake {
        Fake(
            HashMap::from([
                ("ATZ", "\r\rELM327 v1.5\r"),
                ("ATE0", "OK"),
                ("ATL0", "OK"),
                ("ATS1", "OK"),
                ("ATH0", "OK"),
                ("ATSP0", "OK"),
                ("0100", "SEARCHING...\r41 00 BE 3F A8 13"),
                ("ATDPN", "A6"),
                ("0101", "41 01 82 07 65 04"),
                ("03", "43 02 01 33 04 20"),
                ("07", "47 00"),
                ("0902", "014\r0: 49 02 01 31 48 47\r1: 43 4D 38 32 36 33 33\r2: 41 30 30 34 33 35 32"),
                ("010C", "41 0C 1A F8"),
                ("0105", "41 05 7B"),
                ("0142", "41 42 37 E2"),
                ("020200", "42 02 00 01 33"),
                ("020C00", "42 0C 00 0F A0"),
            ]),
            vec![],
        )
    }

    #[test]
    fn reads_codes_vin_and_data() {
        let mut e = Elm327::connect(can_car()).unwrap();
        assert!(e.protocol.contains("CAN"));
        let s = e.scan().unwrap();
        assert_eq!(s.mil_on, Some(true));
        assert_eq!(s.stored, vec!["P0133", "P0420"]);
        assert!(s.pending.is_empty());
        assert_eq!(s.vin.as_deref(), Some("1HGCM82633A004352"));
        assert!(s.live.contains(&Reading { pid: "010C".into(), name: "Engine speed".into(), value: "1726".into(), unit: "rpm".into() }));
        assert!(s.live.iter().any(|r| r.pid == "0105" && r.value == "83"));
        assert!(s.live.iter().any(|r| r.pid == "0142" && r.value == "14.3"));
        assert_eq!(s.freeze_dtc.as_deref(), Some("P0133"));
        assert!(s.freeze_frame.iter().any(|r| r.pid == "020C" && r.value == "1000"));
        assert!(!e.t.1.iter().any(|c| c.starts_with("04")), "never clears codes");
    }

    #[test]
    fn refuses_anything_but_reading() {
        let mut e = Elm327::connect(can_car()).unwrap();
        for bad in ["04", "ATMA", "2E F1 90", "3101", "14FF00", "10 03", "AT WS"] {
            assert!(matches!(e.query(bad), Err(ObdError::NotAllowed(_))), "{bad}");
        }
    }

    #[test]
    fn older_protocols_have_no_count_byte() {
        assert_eq!(parse_dtcs("43 01 33 00 00 00 00", 0x43, false), vec!["P0133"]);
        assert_eq!(parse_dtcs("43 C1 23 41 00 00 00", 0x43, false), vec!["U0123", "C0100"]);
        assert_eq!(dtc(0x81, 0x00), "B0100");
    }
}
