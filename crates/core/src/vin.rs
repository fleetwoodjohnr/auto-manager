//! VIN normalization and validation (ISO 3779 / 49 CFR 565).

use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, TS, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
#[ts(export)]
pub enum VinCheck {
    /// 17 characters with a correct check digit.
    Valid {
        vin: String,
    },
    /// 17 characters, but position 9 doesn't match. Normal for many vehicles built for markets
    /// outside North America, so this is a warning, not a rejection.
    CheckDigitMismatch {
        vin: String,
        expected: String,
        found: String,
    },
    /// Has `*` wildcards or fewer than 17 characters: a partial VIN the decoder may still use.
    Partial {
        vin: String,
    },
    Invalid {
        reason: String,
    },
}

const WEIGHTS: [u32; 17] = [8, 7, 6, 5, 4, 3, 2, 10, 0, 9, 8, 7, 6, 5, 4, 3, 2];

fn value(c: char) -> Option<u32> {
    Some(match c {
        '0'..='9' => c as u32 - '0' as u32,
        'A' | 'J' => 1,
        'B' | 'K' | 'S' => 2,
        'C' | 'L' | 'T' => 3,
        'D' | 'M' | 'U' => 4,
        'E' | 'N' | 'V' => 5,
        'F' | 'W' => 6,
        'G' | 'P' | 'X' => 7,
        'H' | 'Y' => 8,
        'R' | 'Z' => 9,
        _ => return None,
    })
}

/// Uppercase and drop spaces and dashes people type or scanners add.
pub fn normalize(input: &str) -> String {
    input.chars().filter(|c| !c.is_whitespace() && *c != '-').flat_map(char::to_uppercase).collect()
}

pub fn check_digit(vin: &str) -> Option<char> {
    let mut sum = 0;
    for (i, c) in vin.chars().enumerate() {
        sum += value(c)? * WEIGHTS.get(i)?;
    }
    let r = sum % 11;
    Some(if r == 10 { 'X' } else { char::from_digit(r, 10)? })
}

pub fn check(input: &str) -> VinCheck {
    let vin = normalize(input);
    if vin.is_empty() {
        return VinCheck::Invalid { reason: "Enter a VIN.".into() };
    }
    if vin.len() > 17 {
        return VinCheck::Invalid { reason: format!("A VIN has 17 characters; this has {}.", vin.len()) };
    }
    if let Some(bad) = vin.chars().find(|c| matches!(c, 'I' | 'O' | 'Q')) {
        return VinCheck::Invalid { reason: format!("VINs never contain the letter {bad} (it looks too much like 1 or 0).") };
    }
    if let Some(bad) = vin.chars().find(|c| !c.is_ascii_alphanumeric() && *c != '*') {
        return VinCheck::Invalid { reason: format!("“{bad}” is not allowed in a VIN.") };
    }
    if vin.len() < 17 || vin.contains('*') {
        return VinCheck::Partial { vin };
    }
    let expected = check_digit(&vin).unwrap_or('?');
    let found = vin.chars().nth(8).unwrap_or('?');
    if expected == found {
        VinCheck::Valid { vin }
    } else {
        VinCheck::CheckDigitMismatch { vin, expected: expected.into(), found: found.into() }
    }
}

/// True when `s` looks enough like a VIN that search should route it to a vehicle lookup.
pub fn looks_like_vin(s: &str) -> bool {
    matches!(check(s), VinCheck::Valid { .. } | VinCheck::CheckDigitMismatch { .. }) && normalize(s).chars().any(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_digits() {
        // The worked example from 49 CFR 565 materials; check digit X.
        assert_eq!(check("1M8GDM9AXKP042788"), VinCheck::Valid { vin: "1M8GDM9AXKP042788".into() });
        assert_eq!(check("11111111111111111"), VinCheck::Valid { vin: "11111111111111111".into() });
        assert_eq!(check(" 1m8gdm9a-xkp042788 "), VinCheck::Valid { vin: "1M8GDM9AXKP042788".into() });
    }

    #[test]
    fn mismatch_is_a_warning() {
        match check("1M8GDM9A1KP042788") {
            VinCheck::CheckDigitMismatch { expected, found, .. } => assert_eq!((expected.as_str(), found.as_str()), ("X", "1")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rejects_bad_input() {
        assert!(matches!(check("1M8GDM9AXKP04278O"), VinCheck::Invalid { .. }));
        assert!(matches!(check("1M8GDM9AXKP0427889"), VinCheck::Invalid { .. }));
        assert!(matches!(check("1M8GDM9AXKP04278#"), VinCheck::Invalid { .. }));
        assert!(matches!(check(""), VinCheck::Invalid { .. }));
    }

    #[test]
    fn partial() {
        assert!(matches!(check("1M8GDM9A*KP042788"), VinCheck::Partial { .. }));
        assert!(matches!(check("1M8GDM9AXK"), VinCheck::Partial { .. }));
        assert!(!looks_like_vin("1M8GDM9AXK"));
        assert!(looks_like_vin("1M8GDM9AXKP042788"));
        assert!(!looks_like_vin("ABCDEFGHJKLMNPRST"), "no digits: probably not a VIN");
    }
}
