//! NHTSA 5-Star Safety Ratings (https://api.nhtsa.gov/SafetyRatings/). Ratings are per model
//! variant (e.g. "4 DR FWD"), so they're shown as model-level information.

use super::{Freshness, seg};
use crate::error::{AppError, AppResult};
use crate::state::Core;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

pub const SOURCE: &str = "nhtsa_ratings";
const WHO: &str = "NHTSA Safety Ratings";
const TTL_HOURS: i64 = 24 * 30;

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Rating {
    pub vehicle_id: i64,
    pub description: String,
    pub overall: Option<String>,
    pub front: Option<String>,
    pub side: Option<String>,
    pub rollover: Option<String>,
    pub rollover_risk: Option<String>,
    pub esc: Option<String>,
    pub forward_collision_warning: Option<String>,
    pub lane_departure_warning: Option<String>,
    pub complaints: Option<i64>,
    pub recalls: Option<i64>,
    pub investigations: Option<i64>,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Ratings {
    pub ratings: Vec<Rating>,
    pub freshness: Option<Freshness>,
}

#[derive(Deserialize)]
struct Env {
    #[serde(rename = "Results", default)]
    results: Vec<HashMap<String, serde_json::Value>>,
}

fn s(m: &HashMap<String, serde_json::Value>, k: &str) -> Option<String> {
    match m.get(k)? {
        serde_json::Value::String(v) if !v.is_empty() && v != "Not Rated" => Some(v.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn bad() -> AppError {
    AppError::Provider { message: format!("{WHO} sent a response Auto Manager couldn't read."), offline: false }
}

pub fn parse_variants(body: &str) -> AppResult<Vec<(i64, String)>> {
    let e: Env = serde_json::from_str(body).map_err(|_| bad())?;
    Ok(e.results.iter().filter_map(|r| Some((r.get("VehicleId")?.as_i64()?, s(r, "VehicleDescription").unwrap_or_default()))).collect())
}

pub fn parse_rating(body: &str) -> AppResult<Option<Rating>> {
    let e: Env = serde_json::from_str(body).map_err(|_| bad())?;
    Ok(e.results.first().map(|r| Rating {
        vehicle_id: r.get("VehicleId").and_then(|v| v.as_i64()).unwrap_or_default(),
        description: s(r, "VehicleDescription").unwrap_or_default(),
        overall: s(r, "OverallRating"),
        front: s(r, "OverallFrontCrashRating"),
        side: s(r, "OverallSideCrashRating"),
        rollover: s(r, "RolloverRating"),
        rollover_risk: r.get("RolloverPossibility").and_then(|v| v.as_f64()).filter(|p| *p > 0.0).map(|p| format!("{:.1}%", p * 100.0)),
        esc: s(r, "NHTSAElectronicStabilityControl"),
        forward_collision_warning: s(r, "NHTSAForwardCollisionWarning"),
        lane_departure_warning: s(r, "NHTSALaneDepartureWarning"),
        complaints: r.get("ComplaintsCount").and_then(|v| v.as_i64()),
        recalls: r.get("RecallsCount").and_then(|v| v.as_i64()),
        investigations: r.get("InvestigationCount").and_then(|v| v.as_i64()),
    }))
}

pub async fn for_model(core: &Core, year: i32, make: &str, model: &str) -> AppResult<Ratings> {
    let url = format!("{}/SafetyRatings/modelyear/{year}/make/{}/model/{}", core.endpoints.nhtsa, seg(make), seg(model));
    let list = core.cached_text(SOURCE, &url, WHO, TTL_HOURS).await?;
    let mut ratings = Vec::new();
    for (id, _) in parse_variants(&list.value)?.into_iter().take(6) {
        let f = core.cached_text(SOURCE, &format!("{}/SafetyRatings/VehicleId/{id}", core.endpoints.nhtsa), WHO, TTL_HOURS).await?;
        if let Some(r) = parse_rating(&f.value)? {
            ratings.push(r);
        }
    }
    Ok(Ratings { ratings, freshness: Some(list.freshness) })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(n: &str) -> String {
        std::fs::read_to_string(format!("{}/tests/fixtures/nhtsa/{n}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn parses_variants_and_ratings() {
        assert_eq!(parse_variants(&fixture("ratings_list.json")).unwrap(), vec![(13418, "2019 Nissan Sentra 4 DR FWD".to_string())]);
        let r = parse_rating(&fixture("rating_13418.json")).unwrap().unwrap();
        assert_eq!(r.overall.as_deref(), Some("4"));
        assert_eq!(r.side.as_deref(), Some("5"));
        assert_eq!(r.rollover_risk.as_deref(), Some("11.8%"));
        assert_eq!(r.recalls, Some(2));
    }
}
