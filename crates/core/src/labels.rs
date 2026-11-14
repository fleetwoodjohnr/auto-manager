//! Printable inventory labels: Code 128 or QR as SVG markup, drawn by the UI on a print sheet.

use crate::error::{AppError, AppResult};
use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Label {
    pub value: String,
    /// Self-contained SVG markup (no scripts, no external references).
    pub svg: String,
}

/// Code 128 (set B: printable ASCII) as SVG.
pub fn code128(value: &str) -> AppResult<Label> {
    use barcoders::generators::svg::SVG;
    use barcoders::sym::code128::Code128;
    let v = value.trim();
    if v.is_empty() || v.len() > 40 || !v.chars().all(|c| (' '..='~').contains(&c)) {
        return Err(AppError::msg("Code 128 labels need 1–40 printable ASCII characters."));
    }
    let code = Code128::new(format!("\u{0181}{v}")).map_err(|e| AppError::msg(format!("Can't encode “{v}”: {e}")))?;
    let svg = SVG::new(60).generate(code.encode()).map_err(|e| AppError::Other(format!("Barcode drawing failed: {e}")))?;
    Ok(Label { value: v.into(), svg })
}

pub fn qr(value: &str) -> AppResult<Label> {
    use qrcode::QrCode;
    use qrcode::render::svg;
    let v = value.trim();
    if v.is_empty() || v.len() > 500 {
        return Err(AppError::msg("QR labels need 1–500 characters."));
    }
    let code = QrCode::new(v.as_bytes()).map_err(|e| AppError::msg(format!("Can't encode “{v}”: {e}")))?;
    let svg = code.render::<svg::Color>().min_dimensions(96, 96).quiet_zone(true).build();
    Ok(Label { value: v.into(), svg })
}

pub fn make(value: &str, symbology: &str) -> AppResult<Label> {
    if symbology == "qr" { qr(value) } else { code128(value) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn draws_svg() {
        let c = super::code128("OF-100").unwrap();
        assert!(c.svg.starts_with("<svg") && !c.svg.contains("<script"));
        assert!(super::qr("OF-100").unwrap().svg.contains("<svg"));
        assert!(super::code128("").is_err());
        assert!(super::code128("é").is_err());
    }
}
