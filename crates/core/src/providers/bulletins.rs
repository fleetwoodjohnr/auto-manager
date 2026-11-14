//! NHTSA manufacturer communications: technical service bulletins and other notices that
//! manufacturers send NHTSA. They're listed per model year, make and model
//! (`api.nhtsa.gov/vehicles/byYmmt?data=manufacturerCommunications`), so they're model-level:
//! a bulletin may cover only some configurations. The bulletin documents themselves are public
//! PDFs on static.nhtsa.gov.

use super::{Freshness, seg};
use crate::error::{AppError, AppResult};
use crate::state::Core;
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

pub const SOURCE: &str = "nhtsa_bulletins";
const WHO: &str = "NHTSA Manufacturer Communications";
const TTL_HOURS: i64 = 24 * 7;

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Bulletin {
    pub nhtsa_id: i64,
    /// The manufacturer's own bulletin number, e.g. "NTB09-131C".
    pub number: String,
    /// YYYY-MM-DD the communication was received.
    pub date: Option<String>,
    pub title: String,
    pub summary: String,
    pub components: Vec<String>,
    pub documents: i64,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Bulletins {
    pub bulletins: Vec<Bulletin>,
    pub freshness: Option<Freshness>,
}

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct BulletinDocument {
    pub file_name: String,
    pub url: String,
    pub description: String,
}

fn bad() -> AppError {
    AppError::Provider { message: format!("{WHO} sent a response Auto Manager couldn't read."), offline: false }
}

fn text(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).map(str::trim).unwrap_or_default().to_string()
}

fn communications(body: &str) -> AppResult<Vec<Value>> {
    let v: Value = serde_json::from_str(body).map_err(|_| bad())?;
    let results = v.get("results").and_then(Value::as_array).ok_or_else(bad)?;
    Ok(results
        .iter()
        // vehicles/byYmmt nests them under safetyIssues; safetyIssues/byNhtsaId doesn't.
        .flat_map(|r| {
            r.pointer("/safetyIssues/manufacturerCommunications")
                .or_else(|| r.get("manufacturerCommunications"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .collect())
}

/// Every bulletin in a `vehicles/byYmmt` answer (one entry per NHTSA vehicle record), newest first.
pub fn parse(body: &str) -> AppResult<Vec<Bulletin>> {
    let mut out: Vec<Bulletin> = Vec::new();
    for c in communications(body)? {
        let Some(nhtsa_id) = c.get("nhtsaIdNumber").and_then(Value::as_i64) else { continue };
        if out.iter().any(|b| b.nhtsa_id == nhtsa_id) {
            continue;
        }
        let summary = text(&c, "summary");
        let subject = text(&c, "subject");
        // Most bulletins have no subject; their summary starts with the bulletin's title line.
        let title = if subject.is_empty() {
            summary.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Bulletin").to_string()
        } else {
            subject
        };
        out.push(Bulletin {
            nhtsa_id,
            number: text(&c, "manufacturerCommunicationNumber"),
            date: c.get("communicationDate").and_then(Value::as_str).filter(|d| d.len() >= 10).map(|d| d[..10].to_string()),
            title,
            summary,
            components: c
                .get("components")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(|x| text(x, "name")).filter(|n| !n.is_empty()).collect())
                .unwrap_or_default(),
            documents: c.get("associatedDocumentsCount").and_then(Value::as_i64).unwrap_or(0),
        });
    }
    out.sort_by(|a, b| b.date.cmp(&a.date).then(b.nhtsa_id.cmp(&a.nhtsa_id)));
    Ok(out)
}

/// The documents of one bulletin. Only links on `static_base` (static.nhtsa.gov) are kept.
pub fn parse_documents(body: &str, static_base: &str) -> AppResult<Vec<BulletinDocument>> {
    let prefix = format!("{}/", static_base.trim_end_matches('/'));
    Ok(communications(body)?
        .iter()
        .flat_map(|c| c.get("associatedDocuments").and_then(Value::as_array).cloned().unwrap_or_default())
        .filter_map(|d| {
            let url = text(&d, "url");
            url.starts_with(&prefix).then(|| BulletinDocument { file_name: text(&d, "fileName"), description: text(&d, "summary"), url })
        })
        .collect())
}

pub async fn for_model(core: &Core, year: i32, make: &str, model: &str) -> AppResult<Bulletins> {
    let url = format!(
        "{}/vehicles/byYmmt?modelYear={year}&make={}&model={}&data=manufacturerCommunications&productDetail=all&max=100",
        core.endpoints.nhtsa,
        seg(make),
        seg(model)
    );
    let f = core.cached_text(SOURCE, &url, WHO, TTL_HOURS).await?;
    Ok(Bulletins { bulletins: parse(&f.value)?, freshness: Some(f.freshness) })
}

pub async fn documents(core: &Core, nhtsa_id: i64) -> AppResult<Vec<BulletinDocument>> {
    let url = format!(
        "{}/safetyIssues/byNhtsaId?nhtsaId={nhtsa_id}&filter=issueType&filterValue=manufacturerCommunications",
        core.endpoints.nhtsa
    );
    let f = core.cached_text(SOURCE, &url, WHO, TTL_HOURS).await?;
    parse_documents(&f.value, &core.endpoints.nhtsa_static)
}

/// Download one bulletin PDF (only from static.nhtsa.gov) for importing into the library.
pub async fn download(core: &Core, url: &str) -> AppResult<Vec<u8>> {
    if !url.starts_with(&format!("{}/", core.endpoints.nhtsa_static.trim_end_matches('/'))) {
        return Err(AppError::msg("Only bulletin documents from static.nhtsa.gov can be downloaded."));
    }
    let bytes = core.http.get_bytes(url, WHO).await?;
    if !bytes.starts_with(b"%PDF-") {
        return Err(AppError::Provider { message: format!("{WHO} sent a document that isn't a PDF."), offline: false });
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(n: &str) -> String {
        std::fs::read_to_string(format!("{}/tests/fixtures/nhtsa/{n}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn parses_bulletins_for_a_model() {
        let b = parse(&fixture("bulletins_2019_nissan_sentra.json")).unwrap();
        assert_eq!(b.len(), 123, "NHTSA lists 123 communications for the 2019 Sentra");
        assert!(b.windows(2).all(|w| w[0].date >= w[1].date), "newest first");
        let vin_plate = b.iter().find(|x| x.nhtsa_id == 11031704).unwrap();
        assert_eq!(vin_plate.number, "NTB09-131C");
        assert_eq!(vin_plate.title, "CERTIFICATION LABEL AND VIN PLATE REPLACEMENT");
        assert_eq!(vin_plate.date.as_deref(), Some("2026-04-13"));
        assert_eq!(vin_plate.components, vec!["STRUCTURE"]);
        assert_eq!(vin_plate.documents, 1);
    }

    #[test]
    fn keeps_only_nhtsa_document_links() {
        let body = fixture("bulletin_11031704.json");
        let d = parse_documents(&body, "https://static.nhtsa.gov").unwrap();
        assert_eq!(
            d,
            vec![BulletinDocument {
                file_name: "MC-11031704-0001.pdf".into(),
                url: "https://static.nhtsa.gov/odi/tsbs/2026/MC-11031704-0001.pdf".into(),
                description: "Bulletin".into(),
            }]
        );
        assert!(parse_documents(&body, "https://elsewhere.example").unwrap().is_empty());
        assert!(parse("{\"nope\":1}").is_err());
    }
}
