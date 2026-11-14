//! NHTSA vPIC (https://vpic.nhtsa.dot.gov/api/): VIN decoding, makes and models.
//! U.S. Government work; free, no key. NHTSA notes that a missing value means "no data", not
//! that a feature is absent, so missing values are shown as unavailable, never guessed.

use super::{Fetched, Freshness, seg};
use crate::error::{AppError, AppResult};
use crate::state::Core;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

pub const SOURCE: &str = "nhtsa_vpic";
const WHO: &str = "NHTSA vPIC";
const DECODE_TTL_HOURS: i64 = 24 * 30;
const LIST_TTL_HOURS: i64 = 24 * 30;

/// vPIC field → (our key, stored unit). Unmapped non-empty fields are kept as `vpic.<Field>`.
const MAP: &[(&str, &str, Option<&str>)] = &[
    ("ModelYear", "model_year", None),
    ("Make", "make", None),
    ("Model", "model", None),
    ("Trim", "trim", None),
    ("Series", "series", None),
    ("Manufacturer", "manufacturer", None),
    ("BodyClass", "body_class", None),
    ("VehicleType", "vehicle_type", None),
    ("Doors", "doors", None),
    ("PlantCountry", "plant_country", None),
    ("PlantCity", "plant_city", None),
    ("PlantState", "plant_state", None),
    ("PlantCompanyName", "plant_company", None),
    ("VehicleDescriptor", "vehicle_descriptor", None),
    ("EngineConfiguration", "engine_configuration", None),
    ("DisplacementL", "engine_displacement_l", Some("L")),
    ("EngineCylinders", "engine_cylinders", None),
    ("FuelTypePrimary", "fuel_type", None),
    ("FuelTypeSecondary", "fuel_type_secondary", None),
    ("EngineModel", "engine_code", None),
    ("EngineManufacturer", "engine_manufacturer", None),
    ("EngineHP", "engine_hp", Some("hp")),
    ("ValveTrainDesign", "valve_train", None),
    ("Turbo", "turbo", None),
    ("ElectrificationLevel", "electrification", None),
    ("TransmissionStyle", "transmission_style", None),
    ("TransmissionSpeeds", "transmission_speeds", None),
    ("DriveType", "drive_type", None),
    ("Axles", "axles", None),
    ("AxleConfiguration", "axle_configuration", None),
    ("BrakeSystemType", "brake_system", None),
    ("WheelBaseShort", "wheelbase", Some("in")),
    ("TrackWidth", "track_width", Some("in")),
    ("CurbWeightLB", "curb_weight", Some("lb")),
    ("GVWR", "gvwr", None),
    ("Seats", "seats", None),
    ("SeatRows", "seat_rows", None),
    ("WheelSizeFront", "wheel_size_front", Some("in")),
    ("WheelSizeRear", "wheel_size_rear", Some("in")),
    ("BedLengthIN", "bed_length", Some("in")),
];

/// Bookkeeping fields, not vehicle facts.
const SKIP: &[&str] =
    &["VIN", "ErrorCode", "ErrorText", "AdditionalErrorText", "SuggestedVIN", "PossibleValues", "MakeID", "ModelID", "ManufacturerId"];

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct DecodedValue {
    pub key: String,
    pub value: String,
    pub unit: Option<String>,
    /// The vPIC variable it came from.
    pub source_ref: String,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Decoded {
    pub vin: String,
    pub error_codes: Vec<String>,
    /// vPIC's own explanation, shown as returned.
    pub message: String,
    pub identified: bool,
    pub values: Vec<DecodedValue>,
    pub suggested_vin: Option<String>,
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "Results")]
    results: Vec<HashMap<String, serde_json::Value>>,
}

fn text(v: &serde_json::Value) -> Option<String> {
    let s = match v {
        serde_json::Value::String(s) => s.trim().to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return None,
    };
    (!s.is_empty() && s != "Not Applicable").then_some(s)
}

/// Parse one vPIC flat-format result object.
pub fn parse_result(r: &HashMap<String, serde_json::Value>) -> Decoded {
    let get = |k: &str| r.get(k).and_then(text);
    let error_codes: Vec<String> = get("ErrorCode").map(|c| c.split(',').map(|s| s.trim().to_string()).collect()).unwrap_or_default();
    let mut message = get("ErrorText").unwrap_or_default();
    if let Some(extra) = get("AdditionalErrorText") {
        message = format!("{message} {extra}").trim().to_string();
    }
    let mut values = Vec::new();
    let mut used: Vec<&str> = SKIP.to_vec();
    for (field, key, unit) in MAP {
        used.push(field);
        if let Some(mut v) = get(field) {
            // A range such as horsepower "240" to "260".
            let to = format!("{field}_to");
            if let Some(hi) = get(&to).filter(|hi| *hi != v) {
                v = format!("{v}–{hi}");
            }
            values.push(DecodedValue { key: (*key).into(), value: v, unit: unit.map(Into::into), source_ref: (*field).into() });
        }
    }
    let mut rest: Vec<(&String, String)> = r
        .iter()
        .filter(|(k, _)| !used.contains(&k.as_str()) && !k.ends_with("_to") && !k.starts_with("NCSA"))
        .filter_map(|(k, v)| text(v).map(|t| (k, t)))
        .collect();
    rest.sort();
    for (k, v) in rest {
        values.push(DecodedValue { key: format!("vpic.{k}"), value: v, unit: None, source_ref: k.clone() });
    }
    let identified = values.iter().any(|v| v.key == "make") && values.iter().any(|v| v.key == "model" || v.key == "model_year");
    Decoded {
        vin: get("VIN").unwrap_or_default(),
        error_codes,
        message,
        identified,
        values,
        suggested_vin: get("SuggestedVIN").filter(|s| !s.contains('!')),
    }
}

pub fn parse_decode(body: &str) -> AppResult<Decoded> {
    let env: Envelope = serde_json::from_str(body)
        .map_err(|_| AppError::Provider { message: format!("{WHO} sent a response Auto Manager couldn't read."), offline: false })?;
    env.results
        .first()
        .map(parse_result)
        .ok_or_else(|| AppError::Provider { message: format!("{WHO} returned no result."), offline: false })
}

pub async fn decode(core: &Core, vin: &str, model_year: Option<i32>) -> AppResult<Fetched<Decoded>> {
    decode_with(core, vin, model_year, false).await
}

/// `refresh` asks NHTSA again even when a fresh saved answer exists (the saved one is still
/// used if NHTSA can't be reached).
pub async fn decode_with(core: &Core, vin: &str, model_year: Option<i32>, refresh: bool) -> AppResult<Fetched<Decoded>> {
    let mut url = format!("{}/DecodeVinValuesExtended/{}?format=json", core.endpoints.vpic, seg(vin));
    if let Some(y) = model_year {
        url.push_str(&format!("&modelyear={y}"));
    }
    if refresh {
        core.tx(|c| Ok(c.execute("UPDATE api_cache SET expires_at = '1970-01-01T00:00:00.000Z' WHERE key = ?1", [&url])?))?;
    }
    let f = core.cached_text(SOURCE, &url, WHO, DECODE_TTL_HOURS).await?;
    Ok(Fetched { value: parse_decode(&f.value)?, freshness: f.freshness })
}

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct NameList {
    pub names: Vec<String>,
    pub freshness: Freshness,
}

fn names(body: &str, field: &str) -> AppResult<Vec<String>> {
    let env: Envelope = serde_json::from_str(body)
        .map_err(|_| AppError::Provider { message: format!("{WHO} sent a response Auto Manager couldn't read."), offline: false })?;
    let mut out: Vec<String> = env.results.iter().filter_map(|r| r.get(field).and_then(text)).collect();
    out.sort_by_key(|s| s.to_lowercase());
    out.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    Ok(out)
}

/// Makes of cars, multipurpose vehicles, trucks and motorcycles.
pub async fn makes(core: &Core) -> AppResult<NameList> {
    let mut all = Vec::new();
    let mut freshness = None;
    for t in ["car", "multipurpose passenger vehicle", "truck", "motorcycle"] {
        let url = format!("{}/GetMakesForVehicleType/{}?format=json", core.endpoints.vpic, seg(t));
        let f = core.cached_text(SOURCE, &url, WHO, LIST_TTL_HOURS).await?;
        all.extend(names(&f.value, "MakeName")?);
        freshness.get_or_insert(f.freshness);
    }
    all.sort_by_key(|s| s.to_lowercase());
    all.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    Ok(NameList { names: all, freshness: freshness.ok_or_else(|| AppError::Other("no makes".into()))? })
}

pub async fn models(core: &Core, make: &str, year: i32) -> AppResult<NameList> {
    let url = format!("{}/GetModelsForMakeYear/make/{}/modelyear/{year}?format=json", core.endpoints.vpic, seg(make));
    let f = core.cached_text(SOURCE, &url, WHO, LIST_TTL_HOURS).await?;
    Ok(NameList { names: names(&f.value, "Model_Name")?, freshness: f.freshness })
}

/// Batch decode (at most 50 VINs per request, per NHTSA; vPIC only accepts this as a POST).
/// Returns one result per input VIN, in input order. Not cached: each VIN's own decode is.
pub async fn decode_batch(core: &Core, vins: &[String]) -> AppResult<Vec<Decoded>> {
    if vins.len() > 50 {
        return Err(AppError::Other("vPIC batches are limited to 50 VINs.".into()));
    }
    let data = vins.join(";");
    let url = format!("{}/DecodeVINValuesBatch/", core.endpoints.vpic);
    let body = core.http.post_form(&url, &[("format", "json"), ("data", &data)], WHO).await?;
    parse_batch(&body, vins)
}

pub fn parse_batch(body: &str, vins: &[String]) -> AppResult<Vec<Decoded>> {
    let env: Envelope = serde_json::from_str(body)
        .map_err(|_| AppError::Provider { message: format!("{WHO} sent a response Auto Manager couldn't read."), offline: false })?;
    let parsed: Vec<Decoded> = env.results.iter().map(parse_result).collect();
    Ok(vins
        .iter()
        .map(|v| {
            parsed.iter().find(|d| d.vin.eq_ignore_ascii_case(v)).cloned().unwrap_or_else(|| Decoded {
                vin: v.clone(),
                error_codes: vec![],
                message: "NHTSA returned no result for this VIN.".into(),
                identified: false,
                values: vec![],
                suggested_vin: None,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/tests/fixtures/vpic/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }
    fn val<'a>(d: &'a Decoded, k: &str) -> Option<&'a str> {
        d.values.iter().find(|v| v.key == k).map(|v| v.value.as_str())
    }

    #[test]
    fn decodes_clean_vin() {
        let d = parse_decode(&fixture("decode_1HGCM82633A004352.json")).unwrap();
        assert!(d.identified);
        assert_eq!(d.error_codes, vec!["0"]);
        assert_eq!(val(&d, "model_year"), Some("2003"));
        assert_eq!(val(&d, "make"), Some("HONDA"));
        assert_eq!(val(&d, "model"), Some("Accord"));
        assert_eq!(val(&d, "trim"), Some("EX-V6"));
        assert_eq!(val(&d, "engine_cylinders"), Some("6"));
        assert_eq!(val(&d, "engine_code"), Some("J30A4"));
        assert_eq!(val(&d, "transmission_style"), Some("Automatic"));
        // vPIC gives no drive type for this VIN: it must stay absent, not be guessed.
        assert_eq!(val(&d, "drive_type"), None);
        assert_eq!(val(&d, "oil_capacity"), None);
        assert!(d.values.iter().any(|v| v.key == "vpic.AirBagLocFront"), "unmapped facts are kept");
        assert!(!d.values.iter().any(|v| v.key.contains("NCSA") || v.key.contains("ErrorCode")));
    }

    #[test]
    fn decodes_nissan_with_drive_and_weight() {
        let d = parse_decode(&fixture("decode_3N1AB7AP4KY250873.json")).unwrap();
        assert_eq!(val(&d, "drive_type"), Some("4x2"));
        assert_eq!(val(&d, "curb_weight"), Some("3090"));
        assert_eq!(val(&d, "engine_displacement_l"), Some("1.8"));
        assert_eq!(val(&d, "plant_country"), Some("MEXICO"));
    }

    #[test]
    fn reports_provider_errors_as_returned() {
        let d = parse_decode(&fixture("decode_checkdigit_error.json")).unwrap();
        assert_eq!(d.error_codes, vec!["1"]);
        assert!(d.message.contains("Check Digit"));
        let p = parse_decode(&fixture("decode_partial.json")).unwrap();
        assert_eq!(p.error_codes, vec!["6"]);
        let u = parse_decode(&fixture("decode_unknown.json")).unwrap();
        assert!(!u.identified);
        assert!(u.error_codes.contains(&"7".to_string()));
        assert_eq!(u.suggested_vin, None, "vPIC's '!' placeholders are not a usable suggestion");
    }

    #[test]
    fn model_names_are_deduplicated() {
        let n = names(&fixture("models_honda_2003.json"), "Model_Name").unwrap();
        assert_eq!(n.iter().filter(|m| m.as_str() == "Accord").count(), 1);
        assert!(n.contains(&"Civic".to_string()));
    }

    #[test]
    fn batch_results_follow_input_order() {
        let vins: Vec<String> =
            ["3N1AB7AP4KY250873", "1HGCM82633A004352", "ZZZZZZZZZZZZZZZZZ", "MISSING"].iter().map(|s| s.to_string()).collect();
        let r = parse_batch(&fixture("batch_3.json"), &vins).unwrap();
        assert_eq!(r[0].values.iter().find(|v| v.key == "model").map(|v| v.value.as_str()), Some("Sentra"));
        assert_eq!(r[1].values.iter().find(|v| v.key == "model").map(|v| v.value.as_str()), Some("Accord"));
        assert!(!r[2].identified);
        assert!(!r[3].identified && r[3].message.contains("no result"));
    }

    #[tokio::test]
    async fn batch_is_posted_as_a_form() {
        use wiremock::matchers::{body_string_contains, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/DecodeVINValuesBatch/"))
            .and(body_string_contains("format=json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture("batch_3.json")))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let mut core = Core::memory(dir.path().to_path_buf()).unwrap();
        core.endpoints.vpic = server.uri();
        let r = decode_batch(&core, &["1HGCM82633A004352".to_string()]).await.unwrap();
        assert!(r[0].identified);
    }

    #[tokio::test]
    async fn decode_caches_and_falls_back_offline() {
        use wiremock::matchers::path_regex;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(path_regex("/DecodeVinValuesExtended/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture("decode_1HGCM82633A004352.json")))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let mut core = Core::memory(dir.path().to_path_buf()).unwrap();
        core.endpoints.vpic = server.uri();
        let first = decode(&core, "1HGCM82633A004352", None).await.unwrap();
        assert!(!first.freshness.from_cache);
        let second = decode(&core, "1HGCM82633A004352", None).await.unwrap();
        assert!(second.freshness.from_cache && !second.freshness.stale, "fresh copy served without a request");
        // Expire the copy and take the provider away: the saved copy is used, marked stale.
        core.read(|c| Ok(c.execute("UPDATE api_cache SET expires_at = '2000-01-01T00:00:00.000Z'", [])?)).unwrap();
        drop(server);
        let third = decode(&core, "1HGCM82633A004352", None).await.unwrap();
        assert!(third.freshness.stale);
        assert!(third.value.identified);
        // A VIN never looked up has no saved copy: the offline error comes through.
        let e = decode(&core, "3N1AB7AP4KY250873", None).await.err().unwrap();
        assert!(matches!(e.kind(), "offline" | "provider"), "{e}");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_decode("<html>").is_err());
        assert!(parse_decode("{\"Results\": []}").is_err());
    }
}
