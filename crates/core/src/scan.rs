//! Reading VINs and part barcodes from a photo or a camera frame.
//!
//! * Barcodes (Code 39 / Code 128 on door-jamb labels, Data Matrix and QR on newer labels,
//!   UPC/EAN on parts) are decoded with rxing.
//! * Printed or stamped VINs (windshield plates) are read with the ocrs text recognizer, whose
//!   models ship with the app (CC BY-SA 4.0, trained on HierText).
//!
//! A 17-character candidate is only offered as a VIN when its ISO 3779 check digit is right, or
//! when a single common misreading (S/5, B/8, Z/2, G/6) makes it right, which is then said.
//! Anything else is shown as "couldn't read a VIN" rather than guessed.

use crate::error::{AppError, AppResult};
use crate::vin;
use image::{DynamicImage, GenericImageView};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use ts_rs::TS;

/// Photos larger than this (longest side, pixels) are scaled down first.
const MAX_SIDE: u32 = 2200;
const MAX_BYTES: usize = 40 * 1024 * 1024;

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct VinFound {
    pub vin: String,
    /// "barcode" or "text"
    pub from: String,
    /// Set when one misread character was corrected to make the check digit right.
    pub corrected: Option<String>,
}

#[derive(Serialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct ScanResult {
    pub vins: Vec<VinFound>,
    /// Every barcode read, as text (part numbers, UPCs, SKUs).
    pub barcodes: Vec<String>,
    /// Lines of text the recognizer read (shown when no VIN was found, to help retake).
    pub text: Vec<String>,
    /// Whether text recognition was available (its models are bundled with the app).
    pub ocr: bool,
}

/// Where the text-recognition models are (set once at start-up from the app's resources).
static MODELS: OnceLock<PathBuf> = OnceLock::new();
static ENGINE: OnceLock<Option<ocrs::OcrEngine>> = OnceLock::new();

pub fn set_models_dir(dir: PathBuf) {
    let _ = MODELS.set(dir);
}

fn engine() -> Option<&'static ocrs::OcrEngine> {
    ENGINE
        .get_or_init(|| {
            let dir = MODELS.get()?;
            let load = |n: &str| rten::Model::load_file(dir.join(n)).map_err(|e| tracing::warn!("OCR model {n}: {e}")).ok();
            let params = ocrs::OcrEngineParams {
                detection_model: Some(load("text-detection.onnx")?),
                recognition_model: Some(load("text-recognition.onnx")?),
                ..Default::default()
            };
            ocrs::OcrEngine::new(params).map_err(|e| tracing::warn!("OCR engine: {e}")).ok()
        })
        .as_ref()
}

pub fn decode_image(bytes: &[u8]) -> AppResult<DynamicImage> {
    if bytes.len() > MAX_BYTES {
        return Err(AppError::msg("That picture is larger than 40 MB."));
    }
    let img = image::load_from_memory(bytes)
        .map_err(|_| AppError::msg("That file isn't a picture Auto Manager can read (use JPEG, PNG or WebP)."))?;
    let (w, h) = img.dimensions();
    Ok(if w.max(h) > MAX_SIDE { img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle) } else { img })
}

fn barcodes(img: &DynamicImage) -> Vec<String> {
    let mut hints = rxing::DecodeHints::default().with(rxing::DecodeHintValue::TryHarder(true));
    let mut out: Vec<String> = rxing::helpers::detect_multiple_in_image_with_hints(img.clone(), &mut hints)
        .map(|r| r.into_iter().map(|x| x.getText().trim().to_string()).collect())
        .unwrap_or_default();
    // Labels are often photographed sideways.
    if out.is_empty() {
        out = rxing::helpers::detect_multiple_in_image_with_hints(img.rotate90(), &mut hints)
            .map(|r| r.into_iter().map(|x| x.getText().trim().to_string()).collect())
            .unwrap_or_default();
    }
    out.retain(|t| !t.is_empty());
    out.dedup();
    out
}

fn ocr_lines(img: &DynamicImage) -> Option<Vec<String>> {
    let engine = engine()?;
    let rgb = img.to_rgb8();
    let src = ocrs::ImageSource::from_bytes(rgb.as_raw(), rgb.dimensions()).ok()?;
    let input = engine.prepare_input(src).ok()?;
    let text = engine.get_text(&input).ok()?;
    Some(text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
}

/// Characters a VIN can't contain, read as the digits they're mistaken for.
fn vin_chars(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| match c.to_ascii_uppercase() {
            'O' | 'Q' => '0',
            'I' => '1',
            c => c,
        })
        .collect()
}

const MISREADS: &[(char, char)] =
    &[('S', '5'), ('5', 'S'), ('B', '8'), ('8', 'B'), ('Z', '2'), ('2', 'Z'), ('G', '6'), ('6', 'G'), ('U', 'V'), ('V', 'U')];

fn plausible(v: &str) -> bool {
    // Position 10 is the model year, which is never U, Z or 0.
    matches!(vin::check(v), vin::VinCheck::Valid { .. }) && !matches!(v.as_bytes()[9], b'U' | b'Z' | b'0')
}

/// VINs in a piece of text: whole words (or 2–4 neighbouring words, as OCR often splits a VIN)
/// of exactly 17 characters with a correct check digit, or one common misreading away from it.
/// Sliding over longer runs isn't done: one random 17 characters in 11 passes the check digit.
pub fn find_vins(text: &str, from: &str) -> Vec<VinFound> {
    let words: Vec<String> = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let w = w.to_ascii_uppercase();
            match w.strip_prefix("VIN") {
                Some(rest) if rest.len() == 17 => vin_chars(rest),
                _ => vin_chars(&w),
            }
        })
        .collect();
    let mut candidates: Vec<String> = Vec::new();
    for start in 0..words.len() {
        let mut joined = String::new();
        for w in words.iter().skip(start).take(4) {
            joined.push_str(w);
            if joined.len() == 17 {
                candidates.push(joined.clone());
            }
            if joined.len() >= 17 {
                break;
            }
        }
    }
    let mut out: Vec<VinFound> = Vec::new();
    for c in &candidates {
        if plausible(c) && !out.iter().any(|x| &x.vin == c) {
            out.push(VinFound { vin: c.clone(), from: from.into(), corrected: None });
        }
    }
    if out.is_empty() {
        for c in &candidates {
            let chars: Vec<char> = c.chars().collect();
            for (i, ch) in chars.iter().enumerate() {
                for (a, b) in MISREADS.iter().filter(|(a, _)| a == ch) {
                    let mut fixed = chars.clone();
                    fixed[i] = *b;
                    let v: String = fixed.iter().collect();
                    if plausible(&v) && !out.iter().any(|x| x.vin == v) {
                        out.push(VinFound {
                            vin: v,
                            from: from.into(),
                            corrected: Some(format!("Read “{a}” at position {} as “{b}”", i + 1)),
                        });
                    }
                }
            }
        }
        // More than one fix can pass the check digit; all are offered, most likely first: the
        // production number (positions 13–17) is normally all digits.
        out.sort_by_key(|v| v.vin[12..].bytes().filter(u8::is_ascii_alphabetic).count());
    }
    out
}

/// Look for VINs and barcodes in a picture.
pub fn scan(bytes: &[u8]) -> AppResult<ScanResult> {
    let started = std::time::Instant::now();
    let img = decode_image(bytes)?;
    let codes = barcodes(&img);
    tracing::debug!(ms = started.elapsed().as_millis() as u64, "scan: barcodes");
    let mut r = ScanResult { ocr: engine().is_some(), ..Default::default() };
    for c in &codes {
        // Door-jamb Code 39 VINs sometimes carry a leading "I" (imported).
        let c = if c.len() == 18 && c.starts_with('I') { &c[1..] } else { c.as_str() };
        r.vins.extend(find_vins(c, "barcode").into_iter().filter(|v| v.corrected.is_none()));
    }
    r.barcodes = codes;
    if r.vins.is_empty()
        && let Some(lines) = ocr_lines(&img)
    {
        for l in &lines {
            r.vins.extend(find_vins(l, "text"));
        }
        if r.vins.is_empty() {
            r.vins = find_vins(&lines.join(" "), "text");
        }
        r.text = lines;
        tracing::debug!(ms = started.elapsed().as_millis() as u64, "scan: text");
    }
    r.vins.dedup_by(|a, b| a.vin == b.vin);
    Ok(r)
}

pub fn scan_file(path: &Path) -> AppResult<ScanResult> {
    scan(&std::fs::read(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn models() -> Option<PathBuf> {
        let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/ocr");
        d.join("text-recognition.onnx").exists().then_some(d)
    }

    #[test]
    fn finds_only_check_digit_valid_vins() {
        assert_eq!(find_vins("VIN: 1HGCM82633A004352", "text")[0].vin, "1HGCM82633A004352");
        // O and I can't be in a VIN: they're read as 0 and 1.
        assert_eq!(find_vins("1HGCM82633AOO4352", "text")[0].vin, "1HGCM82633A004352");
        // One common misread (S for 5) is corrected, and said so.
        let fixes = find_vins("1HGCM82633A0043S2", "text");
        assert_eq!(fixes[0].vin, "1HGCM82633A004352");
        assert!(fixes[0].corrected.as_deref().unwrap().contains("position 16"));
        assert!(fixes.iter().all(|f| f.corrected.is_some()), "every guess is labelled as one");
        assert!(find_vins("1HGCM82633A004353", "text").is_empty(), "wrong check digit, no guess");
        assert!(find_vins("ABC123", "text").is_empty());
        // Split by the recognizer into groups, or run into a label.
        assert_eq!(find_vins("1HGCM8 2633A 004352", "text")[0].vin, "1HGCM82633A004352");
        assert_eq!(find_vins("VIN1HGCM82633A004352", "text")[0].vin, "1HGCM82633A004352");
        // Not found inside longer runs of characters, where chance matches are common.
        assert!(find_vins("X1HGCM82633A004352X", "text").is_empty());
    }

    fn png(img: &DynamicImage) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn reads_a_code_39_vin_label() {
        // A Code 39 barcode as on door-jamb labels, drawn with the barcoders crate.
        use barcoders::sym::code39::Code39;
        let bars = Code39::new("1HGCM82633A004352").unwrap().encode();
        let (scale, h, pad) = (3u32, 120u32, 40u32);
        let w = bars.len() as u32 * scale + 2 * pad;
        let img = image::GrayImage::from_fn(w, h + 2 * pad, |x, y| {
            let inside = x >= pad && x < w - pad && y >= pad && y < h + pad;
            let black = inside && bars[((x - pad) / scale) as usize] == 1;
            image::Luma([if black { 0 } else { 255 }])
        });
        let r = scan(&png(&DynamicImage::ImageLuma8(img))).unwrap();
        assert_eq!(r.barcodes, vec!["1HGCM82633A004352"]);
        assert_eq!(r.vins, vec![VinFound { vin: "1HGCM82633A004352".into(), from: "barcode".into(), corrected: None }]);
    }

    #[test]
    fn reads_a_printed_vin_when_the_models_are_present() {
        let Some(dir) = models() else {
            eprintln!("skipped: run `node scripts/fetch-ocr-models.mjs` to fetch the text-recognition models");
            return;
        };
        set_models_dir(dir);
        let photo = std::fs::read(format!("{}/tests/fixtures/scan/vin-plate.jpg", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let r = scan(&photo).unwrap();
        assert!(r.ocr);
        assert!(r.vins.iter().any(|v| v.vin == "1HGCM82633A004352"), "read: {:?}", r.text);
    }

    #[test]
    fn refuses_non_pictures() {
        assert!(scan(b"%PDF-1.4 not a picture").is_err());
    }
}
