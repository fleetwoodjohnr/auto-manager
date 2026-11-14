//! The OBD client against a stand-in ELM327 on a real pseudo-terminal (tests/elm327_sim.py), so
//! the serial port settings, echo, the '>' prompt, chunked replies and multi-frame CAN answers go
//! through the same code as with a USB adapter. Skipped when python3 isn't available.
//!
//! Linux only: macOS pseudo-terminals refuse the serial-speed ioctl (IOSSIOSPEED) the serial
//! library applies there ("Not a typewriter"), which real USB adapters accept; Windows has no
//! pseudo-terminals.
#![cfg(target_os = "linux")]

use auto_manager_obd::connect_serial;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

#[test]
fn scans_a_car_through_a_serial_port() {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/elm327_sim.py");
    let Ok(mut sim) = Command::new("python3").arg(script).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn() else {
        eprintln!("skipped: python3 isn't available");
        return;
    };
    let mut out = BufReader::new(sim.stdout.take().expect("simulator output"));
    let mut path = String::new();
    out.read_line(&mut path).expect("device path");

    let scan = connect_serial(path.trim()).and_then(|mut e| e.scan());
    drop(sim.stdin.take()); // lets the simulator finish
    let sent: Vec<String> = out.lines().map_while(Result::ok).filter_map(|l| l.strip_prefix("CMD ").map(String::from)).collect();
    let _ = sim.wait();
    let scan = scan.expect("scan");

    assert_eq!(scan.adapter, "ELM327 v1.5");
    assert!(scan.protocol.contains("CAN (11 bit, 500 kbaud)"), "{}", scan.protocol);
    assert_eq!(scan.vin.as_deref(), Some("1HGCM82633A004352"));
    assert_eq!(scan.mil_on, Some(true));
    assert_eq!(scan.stored, vec!["P0133", "P0300"]);
    assert!(scan.pending.is_empty());
    let live = |name: &str| scan.live.iter().find(|r| r.name == name).map(|r| r.value.clone());
    assert_eq!(live("Engine speed").as_deref(), Some("1726"));
    assert_eq!(live("Coolant temperature").as_deref(), Some("83"));
    assert_eq!(live("Control module voltage").as_deref(), Some("14.3"));
    assert_eq!(live("Fuel level"), None, "unsupported PIDs (NO DATA) are left out, not invented");
    assert_eq!(scan.freeze_dtc.as_deref(), Some("P0133"));
    assert!(scan.freeze_frame.iter().any(|r| r.name == "Engine speed" && r.value == "750"));
    // Only setup and reading requests ever went to the "car".
    assert!(!sent.is_empty());
    assert!(!sent.iter().any(|c| c.replace(' ', "") == "04"), "codes were never cleared: {sent:?}");
    assert!(
        sent.iter().all(|c| c.starts_with("AT") || c.starts_with("01") || c.starts_with("02") || c == "03" || c == "07" || c == "0902"),
        "{sent:?}"
    );
}
