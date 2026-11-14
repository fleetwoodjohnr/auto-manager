//! Specifications and service intervals found in the vehicle's own documents.
//!
//! No free source publishes fluid capacities, torque figures, tire pressures or service
//! schedules, but the owner's and service manuals people import do. This reads the searchable
//! text of the documents linked to a vehicle and offers what it finds, each with its document and
//! page. Nothing is saved until the person confirms it, and then it's saved as "From a manual"
//! with that page cited.

use crate::error::AppResult;
use crate::manuals;
use regex::Regex;
use rusqlite::Connection;
use serde::Serialize;
use std::sync::LazyLock;
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct SpecFound {
    pub key: String,
    pub label: String,
    pub value: String,
    pub unit: Option<String>,
    pub manual_id: String,
    pub manual_title: String,
    pub page: i64,
    /// The words around the value, as printed.
    pub context: String,
}

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct IntervalFound {
    pub service: String,
    pub every_miles: Option<i64>,
    pub every_months: Option<i64>,
    pub manual_id: String,
    pub manual_title: String,
    pub page: i64,
    pub context: String,
}

struct Rule {
    key: &'static str,
    anchors: &'static [&'static str],
    re: &'static LazyLock<Regex>,
    /// How far after an anchor the value may appear (characters).
    reach: usize,
}

static VOLUME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(\d{1,2}(?:\.\d{1,2})?)\s*(?:us\s*)?(qt|quarts?|l|liters?|litres?)\b").expect("static regex"));
static TORQUE_RPM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(\d{2,4})\s*(lb[\s·.-]*ft|ft[\s·.-]*lbs?|n[\s·.-]*m)\b\.?\s*(?:@|at)\s*[\d,]{3,6}\s*rpm").expect("static regex")
});
static TORQUE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(\d{2,3})\s*(lb[\s·.-]*ft|ft[\s·.-]*lbs?|n[\s·.-]*m)\b").expect("static regex"));
static PRESSURE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(\d{2,3})\s*(psi|kpa)\b").expect("static regex"));
static VISCOSITY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(0|5|10|15|20)\s*w\s*-?\s*(\d{2})\b").expect("static regex"));
static DOT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bdot\s?([345](?:\.1)?)\b").expect("static regex"));
static GROUP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bgroup\s*(?:size\s*)?(\d{2,3}[a-z]?)\b").expect("static regex"));
static GAP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)\bgap\b[^0-9]{0,20}(0?\.\d{2,3})\s*(in|mm|")"#).expect("static regex"));
static EVERY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\bevery\s+([\d,]{3,7})\s*(miles|mi|km|kilometers|kilometres)\b(?:[^.;\n]{0,40}?\bor\s+(?:every\s+)?(\d{1,2})\s*months?)?",
    )
    .expect("static regex")
});

const RULES: &[Rule] = &[
    Rule { key: "oil_capacity", anchors: &["oil capacity", "engine oil", "with filter", "including filter"], re: &VOLUME, reach: 160 },
    Rule { key: "coolant_capacity", anchors: &["coolant capacity", "cooling system", "engine coolant"], re: &VOLUME, reach: 160 },
    Rule { key: "oil_spec", anchors: &["viscosity", "engine oil", "sae"], re: &VISCOSITY, reach: 160 },
    Rule { key: "engine_torque", anchors: &["torque"], re: &TORQUE_RPM, reach: 80 },
    Rule { key: "wheel_nut_torque", anchors: &["wheel nut", "lug nut", "wheel lug"], re: &TORQUE, reach: 80 },
    Rule { key: "brake_fluid", anchors: &["brake fluid"], re: &DOT, reach: 120 },
    Rule { key: "battery", anchors: &["battery"], re: &GROUP, reach: 120 },
    Rule { key: "spark_plug", anchors: &["spark plug"], re: &GAP, reach: 160 },
];

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn unit_of(raw: &str) -> String {
    let u = raw.to_lowercase().replace([' ', '·', '.', '-'], "");
    match u.as_str() {
        "qt" | "quart" | "quarts" => "qt".into(),
        "l" | "liter" | "liters" | "litre" | "litres" => "L".into(),
        "lbft" | "ftlb" | "ftlbs" => "lb_ft".into(),
        "nm" => "N·m".into(),
        "psi" => "psi".into(),
        "kpa" => "kPa".into(),
        "in" | "\"" => "in".into(),
        _ => raw.into(),
    }
}

fn context(text: &str, start: usize, end: usize) -> String {
    let a = text[..start].char_indices().rev().nth(70).map(|(i, _)| i).unwrap_or(0);
    let b = text[end..].char_indices().nth(70).map(|(i, _)| end + i).unwrap_or(text.len());
    squash(&text[a..b])
}

/// Values in one page's text (pure; tested directly).
pub fn specs_in(text: &str) -> Vec<(String, String, Option<String>, String)> {
    // ASCII-only lowercasing keeps byte offsets identical to `text` (the anchors are ASCII).
    let lower = text.to_ascii_lowercase();
    let mut out: Vec<(String, String, Option<String>, String)> = Vec::new();
    let mut push = |key: &str, value: String, unit: Option<String>, ctx: String| {
        if !out.iter().any(|(k, v, u, _)| k == key && *v == value && *u == unit) {
            out.push((key.into(), value, unit, ctx));
        }
    };
    for rule in RULES {
        for anchor in rule.anchors {
            for (at, _) in lower.match_indices(anchor) {
                let mut end = (at + rule.reach).min(text.len());
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                let window = &text[at..end];
                let Some(c) = rule.re.captures(window) else { continue };
                let m = c.get(0).map(|m| (at + m.start(), at + m.end())).unwrap_or((at, at));
                let ctx = context(text, m.0, m.1);
                let (value, unit) = match rule.key {
                    "oil_spec" => (format!("{}W-{}", &c[1], &c[2]), None),
                    "brake_fluid" => (format!("DOT {}", &c[1]), None),
                    "battery" => (format!("Group {}", c[1].to_uppercase()), None),
                    "spark_plug" => (format!("Gap {} {}", &c[1], unit_of(&c[2])), None),
                    _ => (c[1].to_string(), Some(unit_of(&c[2]))),
                };
                push(rule.key, value, unit, ctx);
            }
        }
    }
    // Tire pressures: front and rear when the page says so, otherwise one figure for both.
    for (at, _) in
        lower.match_indices("tire pressure").chain(lower.match_indices("tyre pressure")).chain(lower.match_indices("inflation pressure"))
    {
        let mut end = (at + 220).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let window = &lower[at..end];
        let side = |w: &str| -> Option<(String, String, usize, usize)> {
            let i = window.find(w)?;
            let c = PRESSURE.captures(&window[i..i + (window.len() - i).min(40)])?;
            let m = c.get(0)?;
            Some((c[1].to_string(), unit_of(&c[2]), at + i + m.start(), at + i + m.end()))
        };
        match (side("front"), side("rear")) {
            (None, None) => {
                if let Some(c) = PRESSURE.captures(window)
                    && let Some(m) = c.get(0)
                {
                    let ctx = context(text, at + m.start(), at + m.end());
                    push("tire_pressure_front", c[1].to_string(), Some(unit_of(&c[2])), ctx.clone());
                    push("tire_pressure_rear", c[1].to_string(), Some(unit_of(&c[2])), ctx);
                }
            }
            (f, r) => {
                for (key, v) in [("tire_pressure_front", f), ("tire_pressure_rear", r)] {
                    if let Some((value, unit, a, b)) = v {
                        push(key, value, Some(unit), context(text, a, b));
                    }
                }
            }
        }
    }
    out
}

/// Service intervals in one page's text: "Replace engine oil and filter every 5,000 miles or 6 months".
pub fn intervals_in(text: &str) -> Vec<(String, Option<i64>, Option<i64>, String)> {
    let mut out: Vec<(String, Option<i64>, Option<i64>, String)> = Vec::new();
    for c in EVERY.captures_iter(text) {
        let Some(m) = c.get(0) else { continue };
        let n: f64 = c[1].replace(',', "").parse().unwrap_or(0.0);
        let km = c[2].to_lowercase().starts_with('k');
        let miles = if km { (n / 1.609_344).round() as i64 } else { n as i64 };
        let months = c.get(3).and_then(|x| x.as_str().parse().ok());
        // The service is what the sentence says before "every".
        let before = &text[..m.start()];
        let start = before.rfind(['.', ';', '\n', '•', ':']).map(|i| i + before[i..].chars().next().map_or(1, char::len_utf8)).unwrap_or(0);
        let service = squash(&before[start..]).trim_matches(|ch: char| !ch.is_alphanumeric() && ch != ')').to_string();
        if service.len() < 4 || miles <= 0 {
            continue;
        }
        let service: String = service.chars().take(80).collect();
        if !out.iter().any(|(s, mi, mo, _)| *s == service && *mi == Some(miles) && *mo == months) {
            out.push((service, Some(miles), months, context(text, m.start(), m.end())));
        }
    }
    out
}

fn pages(conn: &Connection, vehicle_id: &str) -> AppResult<Vec<(String, String, i64, String)>> {
    let docs = manuals::list(conn, Some(vehicle_id))?;
    let mut out = Vec::new();
    let mut st = conn.prepare("SELECT page, text FROM manual_pages WHERE manual_id = ?1 ORDER BY page")?;
    for d in docs {
        for row in st.query_map([&d.id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
            let (page, text) = row?;
            out.push((d.id.clone(), d.title.clone(), page, text));
        }
    }
    Ok(out)
}

/// Specification values in the vehicle's documents (at most three per specification).
pub fn find_specs(conn: &Connection, vehicle_id: &str) -> AppResult<Vec<SpecFound>> {
    let mut out: Vec<SpecFound> = Vec::new();
    for (manual_id, manual_title, page, text) in pages(conn, vehicle_id)? {
        for (key, value, unit, context) in specs_in(&text) {
            let same = out.iter().filter(|f| f.key == key).count();
            if same >= 3 || out.iter().any(|f| f.key == key && f.value == value && f.unit == unit) {
                continue;
            }
            let label = crate::specs::field(&key).map(|f| f.label.to_string()).unwrap_or_else(|| key.clone());
            out.push(SpecFound {
                key,
                label,
                value,
                unit,
                manual_id: manual_id.clone(),
                manual_title: manual_title.clone(),
                page,
                context,
            });
        }
    }
    Ok(out)
}

pub fn find_intervals(conn: &Connection, vehicle_id: &str) -> AppResult<Vec<IntervalFound>> {
    let mut out: Vec<IntervalFound> = Vec::new();
    for (manual_id, manual_title, page, text) in pages(conn, vehicle_id)? {
        for (service, every_miles, every_months, context) in intervals_in(&text) {
            if out.len() >= 40 || out.iter().any(|i| i.service == service && i.every_miles == every_miles) {
                continue;
            }
            out.push(IntervalFound {
                service,
                every_miles,
                every_months,
                manual_id: manual_id.clone(),
                manual_title: manual_title.clone(),
                page,
                context,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "CAPACITIES AND SPECIFICATIONS\n\
        Engine oil (with filter) .......... 4.4 US qt (4.2 L)\n\
        Recommended viscosity: SAE 0W-20\n\
        Engine coolant capacity: 6.9 qt\n\
        Brake fluid: Genuine NISSAN Brake Fluid or equivalent DOT 3\n\
        Engine: 1.8 L DOHC. Horsepower 124 hp @ 6,000 rpm. Torque 125 lb-ft @ 3,600 rpm.\n\
        Wheel nut torque 80 ft-lb (108 N·m)\n\
        Spark plug gap (nominal): 0.044 in\n\
        Battery: Group 35\n\
        Recommended cold tire pressure: Front 33 psi, Rear 33 psi";

    fn find<'a>(all: &'a [(String, String, Option<String>, String)], key: &str) -> Vec<(&'a str, Option<&'a str>)> {
        all.iter().filter(|x| x.0 == key).map(|x| (x.1.as_str(), x.2.as_deref())).collect()
    }

    #[test]
    fn reads_a_capacities_page() {
        let all = specs_in(PAGE);
        assert_eq!(find(&all, "oil_capacity")[0], ("4.4", Some("qt")));
        assert_eq!(find(&all, "oil_spec"), vec![("0W-20", None)]);
        assert_eq!(find(&all, "coolant_capacity"), vec![("6.9", Some("qt"))]);
        assert_eq!(find(&all, "brake_fluid"), vec![("DOT 3", None)]);
        assert_eq!(find(&all, "engine_torque"), vec![("125", Some("lb_ft"))], "the one quoted with rpm");
        assert_eq!(find(&all, "wheel_nut_torque")[0], ("80", Some("lb_ft")));
        assert_eq!(find(&all, "spark_plug"), vec![("Gap 0.044 in", None)]);
        assert_eq!(find(&all, "battery"), vec![("Group 35", None)]);
        assert_eq!(find(&all, "tire_pressure_front"), vec![("33", Some("psi"))]);
        assert_eq!(find(&all, "tire_pressure_rear"), vec![("33", Some("psi"))]);
        assert!(all.iter().all(|x| !x.3.is_empty()), "every value comes with its context");
    }

    #[test]
    fn survives_any_text() {
        // Characters whose lowercase is longer in bytes, and multi-byte bullets near values.
        let odd = "İSTANBUL ŞUBESİ • Engine oil (with filter) • 4.4 qt • Tire pressure • front 33 psi • every 5,000 km • ünlü";
        let found = specs_in(odd);
        assert!(found.iter().any(|f| f.0 == "oil_capacity" && f.1 == "4.4"));
        assert_eq!(intervals_in(odd).len(), 0, "no service named before “every”");
    }

    #[test]
    fn says_nothing_when_the_page_says_nothing() {
        assert!(specs_in("Starter motor removal: the starter motor is below the intake manifold.").is_empty());
        // A torque figure without rpm isn't an engine rating.
        assert!(specs_in("Tighten the drain plug to torque 25 lb-ft.").iter().all(|x| x.0 != "engine_torque"));
    }

    #[test]
    fn reads_service_intervals() {
        let text = "Maintenance schedule (normal driving).\n• Replace engine oil and oil filter every 5,000 miles (8,000 km) or 6 months.\n• Rotate tires every 7,500 miles.\nInspect brake pads every 12,000 km or 12 months.";
        let found = intervals_in(text);
        assert_eq!(found[0].0, "Replace engine oil and oil filter");
        assert_eq!((found[0].1, found[0].2), (Some(5000), Some(6)));
        assert_eq!((found[1].0.as_str(), found[1].1, found[1].2), ("Rotate tires", Some(7500), None));
        assert_eq!((found[2].0.as_str(), found[2].1, found[2].2), ("Inspect brake pads", Some(7456), Some(12)), "km converted to miles");
    }
}
