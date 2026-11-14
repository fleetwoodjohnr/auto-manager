//! Fitment: does part P fit vehicle V? Decided only from fitment records, never from a
//! make/model match alone.
//!
//! Each record (year range + make + model + qualifiers) is evaluated attribute by attribute
//! against the vehicle's effective specifications:
//!   - a known mismatch means the record does not apply;
//!   - a qualifier the vehicle has no value for becomes a requirement ("needs: brake package").
//!
//! Status, strongest evidence first:
//!   Not compatible  an exclusion matches, or verified records cover this vehicle and every one
//!                   fails on a known attribute;
//!   Confirmed fit   a fully matching *verified* include record;
//!   Conditional fit a matching include record with open requirements, or records that conflict;
//!   Possible fit    a fully matching but unverified record;
//!   Unknown         no records for this vehicle.

use crate::parts::Fitment;
use crate::specs;
use serde::Serialize;
use std::collections::HashMap;
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FitStatus {
    Confirmed,
    Conditional,
    Possible,
    NotCompatible,
    Unknown,
}

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Requirement {
    pub key: String,
    pub label: String,
    /// The value the record needs, e.g. "Sport".
    pub value: String,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct FitResult {
    pub status: FitStatus,
    pub reasons: Vec<String>,
    pub requirements: Vec<Requirement>,
    /// The fitment records this result was based on.
    pub record_ids: Vec<String>,
    pub conflict: bool,
    pub position: String,
    pub quantity: Option<i64>,
}

/// What we know about the vehicle: identity plus effective specification values.
#[derive(Clone, Debug, Default)]
pub struct VehicleFacts {
    pub year: Option<i32>,
    pub make: String,
    pub model: String,
    pub attrs: HashMap<String, String>,
}

enum Outcome {
    Match,
    Needs(Vec<Requirement>),
    Mismatch(String),
}

fn label(key: &str) -> String {
    specs::field(key).map(|f| f.label.to_string()).unwrap_or_else(|| {
        let s = key.replace(['_', '.'], " ");
        let mut c = s.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    })
}

fn simplify(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Drive types as the set of layouts a value could mean. vPIC's "4x2" is either FWD or RWD.
pub fn drive_set(s: &str) -> Vec<&'static str> {
    let t = s.to_lowercase();
    if t.contains("awd") || t.contains("all-wheel") || t.contains("all wheel") {
        vec!["awd"]
    } else if t.contains("4wd") || t.contains("4x4") || t.contains("four-wheel") || t.contains("4-wheel") {
        vec!["4wd"]
    } else if t.contains("fwd") || t.contains("front-wheel") || t.contains("front wheel") {
        vec!["fwd"]
    } else if t.contains("rwd") || t.contains("rear-wheel") || t.contains("rear wheel") {
        vec!["rwd"]
    } else if t.contains("4x2") || t.contains("2wd") {
        vec!["fwd", "rwd"]
    } else {
        vec![]
    }
}

/// Compare one qualifier with the vehicle's value. `wanted` may list alternatives "A|B".
fn compare(key: &str, wanted: &str, have: &str) -> Option<bool> {
    let alts: Vec<&str> = wanted.split('|').map(str::trim).filter(|a| !a.is_empty()).collect();
    match key {
        "engine_displacement_l" => {
            let h: f64 = have.trim().trim_end_matches(['L', 'l']).trim().parse().ok()?;
            Some(alts.iter().any(|a| a.trim_end_matches(['L', 'l']).trim().parse::<f64>().is_ok_and(|w| (w - h).abs() < 0.05)))
        }
        "engine_cylinders" | "doors" | "transmission_speeds" => {
            let h: i64 = have.trim().parse().ok()?;
            Some(alts.iter().any(|a| a.parse::<i64>() == Ok(h)))
        }
        "drive_type" => {
            let h = drive_set(have);
            if h.is_empty() {
                return Some(alts.iter().any(|a| simplify(a) == simplify(have)));
            }
            let w: Vec<&str> = alts.iter().flat_map(|a| drive_set(a)).collect();
            if w.is_empty() {
                return Some(alts.iter().any(|a| simplify(a) == simplify(have)));
            }
            if h.iter().all(|d| w.contains(d)) {
                Some(true)
            } else if h.iter().any(|d| w.contains(d)) {
                None // e.g. vehicle "4x2", record "FWD": can't tell which
            } else {
                Some(false)
            }
        }
        _ => Some(alts.iter().any(|a| simplify(a) == simplify(have))),
    }
}

fn evaluate_record(v: &VehicleFacts, f: &Fitment) -> Outcome {
    let mut needs = Vec::new();
    for q in &f.qualifiers {
        match v.attrs.get(&q.key).filter(|s| !s.trim().is_empty()) {
            None => needs.push(Requirement { key: q.key.clone(), label: label(&q.key), value: q.value.clone() }),
            Some(have) => match compare(&q.key, &q.value, have) {
                Some(true) => {}
                Some(false) => return Outcome::Mismatch(format!("{} is {have}; this record is for {}", label(&q.key), q.value)),
                None => needs.push(Requirement {
                    key: q.key.clone(),
                    label: format!("Confirm {}", label(&q.key).to_lowercase()),
                    value: q.value.clone(),
                }),
            },
        }
    }
    if needs.is_empty() { Outcome::Match } else { Outcome::Needs(needs) }
}

fn covers(v: &VehicleFacts, f: &Fitment) -> bool {
    v.year.is_some_and(|y| (f.year_from..=f.year_to).contains(&y))
        && f.make.eq_ignore_ascii_case(v.make.trim())
        && f.model.eq_ignore_ascii_case(v.model.trim())
}

/// Evaluate all of one part's fitment records against a vehicle.
pub fn evaluate(v: &VehicleFacts, records: &[Fitment]) -> FitResult {
    let relevant: Vec<&Fitment> = records.iter().filter(|f| covers(v, f)).collect();
    let mut result = FitResult {
        status: FitStatus::Unknown,
        reasons: vec![],
        requirements: vec![],
        record_ids: relevant.iter().map(|f| f.id.clone()).collect(),
        conflict: false,
        position: String::new(),
        quantity: None,
    };
    if v.year.is_none() {
        result.reasons.push("The vehicle's model year is unknown.".into());
        return result;
    }
    if relevant.is_empty() {
        result.reasons.push("No fitment records cover this vehicle.".into());
        return result;
    }
    let outcomes: Vec<(&Fitment, Outcome)> = relevant.iter().map(|f| (*f, evaluate_record(v, f))).collect();
    let fits_match: Vec<&Fitment> =
        outcomes.iter().filter(|(f, _)| f.kind == "fits").filter(|(_, o)| matches!(o, Outcome::Match)).map(|(f, _)| *f).collect();
    let excl_match: Vec<&Fitment> =
        outcomes.iter().filter(|(f, _)| f.kind == "excludes").filter(|(_, o)| matches!(o, Outcome::Match)).map(|(f, _)| *f).collect();
    let mut reqs: Vec<Requirement> = Vec::new();
    let mut add_reqs = |o: &Outcome| {
        if let Outcome::Needs(n) = o {
            for r in n {
                if !reqs.contains(r) {
                    reqs.push(r.clone());
                }
            }
        }
    };
    let fits_need: Vec<&(&Fitment, Outcome)> =
        outcomes.iter().filter(|(f, _)| f.kind == "fits").filter(|(_, o)| matches!(o, Outcome::Needs(_))).collect();
    let excl_need: Vec<&(&Fitment, Outcome)> =
        outcomes.iter().filter(|(f, _)| f.kind == "excludes").filter(|(_, o)| matches!(o, Outcome::Needs(_))).collect();
    let note = |f: &Fitment| if f.notes.is_empty() { String::new() } else { format!(" ({})", f.notes) };
    if let Some(best) = fits_match.iter().find(|f| f.verified_by.is_some()).or(fits_match.first()) {
        result.position = best.position.clone();
        result.quantity = best.quantity;
    }

    if !excl_match.is_empty() {
        if !fits_match.is_empty() {
            result.status = FitStatus::Conditional;
            result.conflict = true;
            result.reasons.push("Fitment records disagree: one says it fits, another says it does not.".into());
        } else {
            result.status = FitStatus::NotCompatible;
            for f in &excl_match {
                result.reasons.push(format!("Excluded for this vehicle{}.", note(f)));
            }
        }
    } else if !fits_match.is_empty() {
        if !excl_need.is_empty() {
            for (_, o) in &excl_need {
                add_reqs(o);
            }
            result.status = FitStatus::Conditional;
            result.reasons.push("Fits unless an exclusion applies; more vehicle details are needed.".into());
        } else if fits_match.iter().any(|f| f.verified_by.is_some()) {
            result.status = FitStatus::Confirmed;
            result.reasons.push("A verified fitment record matches this vehicle's configuration.".into());
        } else {
            result.status = FitStatus::Possible;
            result.reasons.push("A fitment record matches, but nobody has verified it.".into());
        }
    } else if !fits_need.is_empty() {
        for (_, o) in &fits_need {
            add_reqs(o);
        }
        result.status = FitStatus::Conditional;
        result.reasons.push("Fits only some configurations of this vehicle; more details are needed.".into());
    } else {
        let mismatches: Vec<String> =
            outcomes.iter().filter_map(|(_, o)| if let Outcome::Mismatch(m) = o { Some(m.clone()) } else { None }).collect();
        let any_verified = relevant.iter().any(|f| f.kind == "fits" && f.verified_by.is_some());
        let has_fits = relevant.iter().any(|f| f.kind == "fits");
        if has_fits && any_verified {
            result.status = FitStatus::NotCompatible;
            result.reasons.push("Verified records for this vehicle don't match its configuration.".into());
        } else {
            result.reasons.push("No record matches this vehicle's configuration.".into());
        }
        result.reasons.extend(mismatches);
    }
    result.requirements = reqs;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parts::Qualifier;

    fn rec(id: &str, kind: &str, years: (i32, i32), verified: bool, qs: &[(&str, &str)]) -> Fitment {
        Fitment {
            id: id.into(),
            part_id: "p".into(),
            kind: kind.into(),
            year_from: years.0,
            year_to: years.1,
            make: "Nissan".into(),
            model: "Sentra".into(),
            position: "Front".into(),
            quantity: Some(1),
            notes: String::new(),
            source_id: "user".into(),
            source_name: "You".into(),
            source_ref: None,
            verified_by: verified.then(|| "user".into()),
            verified_at: None,
            qualifiers: qs.iter().map(|(k, v)| Qualifier { key: (*k).into(), value: (*v).into() }).collect(),
        }
    }

    fn sentra(extra: &[(&str, &str)]) -> VehicleFacts {
        let mut attrs: HashMap<String, String> =
            [("engine_displacement_l", "1.8"), ("engine_cylinders", "4"), ("drive_type", "4x2"), ("trim", "SV")]
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect();
        for (k, v) in extra {
            attrs.insert((*k).into(), (*v).into());
        }
        VehicleFacts { year: Some(2019), make: "NISSAN".into(), model: "Sentra".into(), attrs }
    }

    #[test]
    fn confirmed_needs_verified_full_match() {
        let r = evaluate(&sentra(&[]), &[rec("a", "fits", (2016, 2019), true, &[("engine_displacement_l", "1.8")])]);
        assert_eq!(r.status, FitStatus::Confirmed);
        assert_eq!(r.record_ids, vec!["a"]);
        let r = evaluate(&sentra(&[]), &[rec("a", "fits", (2016, 2019), false, &[("engine_displacement_l", "1.8")])]);
        assert_eq!(r.status, FitStatus::Possible);
    }

    #[test]
    fn make_model_alone_is_not_enough_without_records() {
        assert_eq!(evaluate(&sentra(&[]), &[]).status, FitStatus::Unknown);
        // A record for other years doesn't count either.
        assert_eq!(evaluate(&sentra(&[]), &[rec("a", "fits", (2010, 2012), true, &[])]).status, FitStatus::Unknown);
    }

    #[test]
    fn conditional_when_details_missing() {
        let r = evaluate(&sentra(&[]), &[rec("a", "fits", (2019, 2019), true, &[("brake_package", "Sport")])]);
        assert_eq!(r.status, FitStatus::Conditional);
        assert_eq!(r.requirements[0].key, "brake_package");
        assert_eq!(r.requirements[0].value, "Sport");
        // Once the user supplies the detail, it resolves.
        let r = evaluate(&sentra(&[("brake_package", "sport")]), &[rec("a", "fits", (2019, 2019), true, &[("brake_package", "Sport")])]);
        assert_eq!(r.status, FitStatus::Confirmed);
        let r = evaluate(&sentra(&[("brake_package", "Standard")]), &[rec("a", "fits", (2019, 2019), true, &[("brake_package", "Sport")])]);
        assert_eq!(r.status, FitStatus::NotCompatible);
    }

    #[test]
    fn engine_and_trim_variants() {
        let two_litre = rec("a", "fits", (2019, 2019), true, &[("engine_displacement_l", "2.0")]);
        let r = evaluate(&sentra(&[]), std::slice::from_ref(&two_litre));
        assert_eq!(r.status, FitStatus::NotCompatible, "verified records cover the vehicle but not its 1.8L engine");
        assert!(r.reasons.iter().any(|m| m.contains("1.8")));
        let unverified = rec("a", "fits", (2019, 2019), false, &[("engine_displacement_l", "2.0")]);
        assert_eq!(evaluate(&sentra(&[]), &[unverified]).status, FitStatus::Unknown, "unverified mismatches aren't proof");
        let trims = rec("b", "fits", (2019, 2019), true, &[("trim", "S|SV")]);
        assert_eq!(evaluate(&sentra(&[]), &[trims]).status, FitStatus::Confirmed);
        let close = rec("c", "fits", (2019, 2019), true, &[("engine_displacement_l", "1.798")]);
        assert_eq!(evaluate(&sentra(&[]), &[close]).status, FitStatus::Confirmed, "displacement within 0.05 L");
    }

    #[test]
    fn ambiguous_drive_type_needs_confirmation() {
        let fwd = rec("a", "fits", (2019, 2019), true, &[("drive_type", "FWD")]);
        let r = evaluate(&sentra(&[]), std::slice::from_ref(&fwd));
        assert_eq!(r.status, FitStatus::Conditional, "vPIC's 4x2 could be FWD or RWD");
        let awd = rec("b", "fits", (2019, 2019), true, &[("drive_type", "AWD")]);
        assert_eq!(evaluate(&sentra(&[]), &[awd]).status, FitStatus::NotCompatible);
        let r = evaluate(&sentra(&[("drive_type", "FWD/Front-Wheel Drive")]), &[fwd]);
        assert_eq!(r.status, FitStatus::Confirmed);
    }

    #[test]
    fn exclusions_and_conflicts() {
        let fits = rec("a", "fits", (2016, 2019), true, &[]);
        let excl = rec("b", "excludes", (2019, 2019), true, &[("trim", "SV")]);
        let r = evaluate(&sentra(&[]), &[fits.clone(), excl]);
        assert_eq!(r.status, FitStatus::Conditional);
        assert!(r.conflict);
        let excl_only = rec("c", "excludes", (2019, 2019), true, &[]);
        assert_eq!(evaluate(&sentra(&[]), &[excl_only]).status, FitStatus::NotCompatible);
        // An exclusion that might apply turns a fit into a conditional fit.
        let maybe = rec("d", "excludes", (2019, 2019), true, &[("brake_package", "Sport")]);
        let r = evaluate(&sentra(&[]), &[fits, maybe]);
        assert_eq!(r.status, FitStatus::Conditional);
        assert_eq!(r.requirements[0].key, "brake_package");
    }

    #[test]
    fn unknown_year() {
        let mut v = sentra(&[]);
        v.year = None;
        assert_eq!(evaluate(&v, &[rec("a", "fits", (2019, 2019), true, &[])]).status, FitStatus::Unknown);
    }
}
