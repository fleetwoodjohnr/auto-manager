//! EPA fueleconomy.gov web services: engine, transmission, drive and fuel economy for each
//! configuration of a model year/make/model. The person picks the configuration that matches
//! their vehicle; values are stored as model-level.

use super::vpic::DecodedValue;
use super::{Freshness, seg};
use crate::error::{AppError, AppResult};
use crate::state::Core;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

pub const SOURCE: &str = "epa_fueleconomy";
const WHO: &str = "fueleconomy.gov";
const TTL_HOURS: i64 = 24 * 90;

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct EpaOption {
    pub id: String,
    /// e.g. "Auto (variable gear ratios), 4 cyl, 1.8 L"
    pub text: String,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct EpaOptions {
    pub options: Vec<EpaOption>,
    pub freshness: Freshness,
}

#[derive(Deserialize)]
struct Item {
    text: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(Item),
    Many(Vec<Item>),
}

#[derive(Deserialize)]
struct Menu {
    #[serde(rename = "menuItem")]
    item: Option<OneOrMany>,
}

fn bad() -> AppError {
    AppError::Provider { message: format!("{WHO} sent a response Auto Manager couldn't read."), offline: false }
}

/// fueleconomy.gov returns a single object instead of a list when there is one option, and an
/// empty body when there are none.
pub fn parse_options(body: &str) -> AppResult<Vec<EpaOption>> {
    if body.trim().is_empty() || body.trim() == "null" {
        return Ok(vec![]);
    }
    let m: Menu = serde_json::from_str(body).map_err(|_| bad())?;
    Ok(match m.item {
        None => vec![],
        Some(OneOrMany::One(i)) => vec![EpaOption { id: i.value, text: i.text }],
        Some(OneOrMany::Many(v)) => v.into_iter().map(|i| EpaOption { id: i.value, text: i.text }).collect(),
    })
}

/// Map an EPA vehicle record to specification values.
pub fn parse_vehicle(body: &str) -> AppResult<Vec<DecodedValue>> {
    let m: HashMap<String, serde_json::Value> = serde_json::from_str(body).map_err(|_| bad())?;
    let get = |k: &str| match m.get(k) {
        Some(serde_json::Value::String(s)) if !s.is_empty() && s != "-1" => Some(s.clone()),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    };
    let mut out = Vec::new();
    let mut push = |key: &str, value: Option<String>, unit: Option<&str>, field: &str| {
        if let Some(v) = value.filter(|v| v != "0" && v != "0.0") {
            out.push(DecodedValue { key: key.into(), value: v, unit: unit.map(Into::into), source_ref: format!("EPA {field}") });
        }
    };
    push("engine_displacement_l", get("displ"), Some("L"), "displ");
    push("engine_cylinders", get("cylinders"), None, "cylinders");
    let trany = get("trany");
    let style = trany.as_deref().map(|t| {
        if t.to_lowercase().starts_with("auto") {
            "Automatic"
        } else if t.to_lowercase().starts_with("man") {
            "Manual"
        } else {
            t
        }
        .to_string()
    });
    push("transmission_style", style, None, "trany");
    push("epa.transmission", trany.clone(), None, "trany");
    let speeds = trany.as_deref().and_then(|t| t.split(|c: char| !c.is_ascii_digit()).find(|s| !s.is_empty()).map(String::from));
    push("transmission_speeds", speeds, None, "trany");
    push("drive_type", get("drive"), None, "drive");
    push("fuel_type", get("fuelType1"), None, "fuelType1");
    push("mpg_city", get("city08"), None, "city08");
    push("mpg_highway", get("highway08"), None, "highway08");
    push("mpg_combined", get("comb08"), None, "comb08");
    push("ev_range", get("range").filter(|r| r != "0"), Some("mi"), "range");
    push("epa.size_class", get("VClass"), None, "VClass");
    push("epa.engine_description", get("eng_dscr"), None, "eng_dscr");
    Ok(out)
}

pub async fn options(core: &Core, year: i32, make: &str, model: &str) -> AppResult<EpaOptions> {
    let url = format!("{}/vehicle/menu/options?year={year}&make={}&model={}", core.endpoints.fueleconomy, seg(make), seg(model));
    let f = core.cached_json_accept(SOURCE, &url, WHO, TTL_HOURS).await?;
    Ok(EpaOptions { options: parse_options(&f.value)?, freshness: f.freshness })
}

pub async fn vehicle(core: &Core, id: &str) -> AppResult<(Vec<DecodedValue>, Freshness)> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::invalid("id", "Choose one of the listed configurations."));
    }
    let f = core.cached_json_accept(SOURCE, &format!("{}/vehicle/{id}", core.endpoints.fueleconomy), WHO, TTL_HOURS).await?;
    Ok((parse_vehicle(&f.value)?, f.freshness))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(n: &str) -> String {
        std::fs::read_to_string(format!("{}/tests/fixtures/epa/{n}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn options_one_or_many() {
        assert_eq!(parse_options(&fixture("options_2019_nissan_sentra.json")).unwrap().len(), 3);
        assert_eq!(parse_options(r#"{"menuItem":{"text":"Auto 6-spd","value":"1"}}"#).unwrap().len(), 1);
        assert!(parse_options("").unwrap().is_empty());
    }

    #[test]
    fn vehicle_values() {
        let v = parse_vehicle(&fixture("vehicle_40565.json")).unwrap();
        let get = |k: &str| v.iter().find(|x| x.key == k).map(|x| x.value.clone());
        assert_eq!(get("engine_displacement_l").as_deref(), Some("1.8"));
        assert_eq!(get("drive_type").as_deref(), Some("Front-Wheel Drive"));
        assert_eq!(get("transmission_style").as_deref(), Some("Automatic"));
        assert_eq!(get("mpg_combined").as_deref(), Some("32"));
        assert_eq!(get("ev_range"), None, "a zero range isn't a value");
    }
}
