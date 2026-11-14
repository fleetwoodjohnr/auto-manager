//! vPIC "Canadian Vehicle Specifications": dimensions and curb weight by year, make and model.
//! Rows describe a model/body style, not one VIN, so the person picks the matching row and the
//! values are stored as model-level.

use super::vpic::DecodedValue;
use super::{Freshness, seg};
use crate::error::{AppError, AppResult};
use crate::state::Core;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const SOURCE: &str = "vpic_canada";
const WHO: &str = "vPIC Canadian specifications";
const TTL_HOURS: i64 = 24 * 90;

/// Field codes in the response (units=US: inches and pounds).
const MAP: &[(&str, &str, &str, &str)] = &[
    ("OL", "length", "in", "Overall length"),
    ("OW", "width", "in", "Overall width"),
    ("OH", "height", "in", "Overall height"),
    ("WB", "wheelbase", "in", "Wheelbase"),
    ("CW", "curb_weight", "lb", "Curb weight"),
    ("TWF", "track_width", "in", "Track width (front)"),
    ("TWR", "canada.track_width_rear", "in", "Track width (rear)"),
    ("WD", "canada.weight_distribution", "", "Weight distribution front/rear (%)"),
];

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct CanadaRow {
    pub index: i64,
    /// e.g. "SENTRA 4DR SEDAN"
    pub model: String,
    pub values: Vec<DecodedValue>,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct CanadaOptions {
    pub rows: Vec<CanadaRow>,
    pub freshness: Freshness,
}

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Value", default)]
    value: Option<String>,
}
#[derive(Deserialize)]
struct Row {
    #[serde(rename = "Specs", default)]
    specs: Vec<Spec>,
}
#[derive(Deserialize)]
struct Env {
    #[serde(rename = "Results", default)]
    results: Vec<Row>,
}

fn round(v: &str) -> String {
    v.parse::<f64>().map(|f| format!("{}", (f * 10.0).round() / 10.0)).unwrap_or_else(|_| v.to_string())
}

pub fn parse(body: &str) -> AppResult<Vec<CanadaRow>> {
    let e: Env =
        serde_json::from_str(body).map_err(|_| AppError::Provider { message: format!("{WHO}: unreadable response."), offline: false })?;
    Ok(e.results
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            let get = |n: &str| r.specs.iter().find(|s| s.name == n).and_then(|s| s.value.clone()).filter(|v| !v.trim().is_empty());
            let values = MAP
                .iter()
                .filter_map(|(code, key, unit, _)| {
                    get(code).map(|v| DecodedValue {
                        key: (*key).into(),
                        value: if unit.is_empty() { v } else { round(&v) },
                        unit: (!unit.is_empty()).then(|| (*unit).into()),
                        source_ref: format!("Canadian specs {code}"),
                    })
                })
                .collect();
            CanadaRow { index: i as i64, model: get("Model").unwrap_or_default(), values }
        })
        .filter(|r: &CanadaRow| !r.values.is_empty())
        .collect())
}

pub async fn options(core: &Core, year: i32, make: &str, model: &str) -> AppResult<CanadaOptions> {
    let url = format!(
        "{}/GetCanadianVehicleSpecifications/?year={year}&make={}&model={}&units=US&format=json",
        core.endpoints.vpic,
        seg(make),
        seg(model)
    );
    let f = core.cached_text(SOURCE, &url, WHO, TTL_HOURS).await?;
    Ok(CanadaOptions { rows: parse(&f.value)?, freshness: f.freshness })
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_dimensions() {
        let body =
            std::fs::read_to_string(format!("{}/tests/fixtures/vpic/canada_2019_nissan_sentra.json", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let rows = super::parse(&body).unwrap();
        assert_eq!(rows[0].model, "SENTRA 4DR SEDAN");
        let get = |k: &str| rows[0].values.iter().find(|v| v.key == k).map(|v| v.value.clone());
        assert_eq!(get("length").as_deref(), Some("182.3"));
        assert_eq!(get("curb_weight").as_deref(), Some("2899.1"));
        assert_eq!(get("canada.weight_distribution").as_deref(), Some("60/40"));
    }
}
