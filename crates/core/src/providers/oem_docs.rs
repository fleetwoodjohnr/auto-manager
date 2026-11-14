//! Official owner-information portals, by make. Auto Manager only links to them (they open in
//! the browser); it never downloads or copies manuals. Every link here returned HTTP 200 when
//! checked with scripts/check-doc-links.sh on the date below; makes whose sites refused an
//! automated check aren't listed rather than listed unverified.

use serde::Serialize;
use ts_rs::TS;

pub const CHECKED_ON: &str = "2026-11-14";

const LINKS: &[(&str, &str)] = &[
    ("ACURA", "https://mygarage.honda.com/s/find-acura"),
    ("BUICK", "https://www.buick.com/support/vehicle/manuals-guides"),
    ("CADILLAC", "https://www.cadillac.com/support/vehicle/manuals-guides"),
    ("CHEVROLET", "https://www.chevrolet.com/support/vehicle/manuals-guides"),
    ("GMC", "https://www.gmc.com/support/vehicle/manuals-guides"),
    ("HONDA", "https://mygarage.honda.com/s/find-honda"),
    ("HYUNDAI", "https://owners.hyundaiusa.com/us/en/resources/manuals-warranties"),
    ("KIA", "https://owners.kia.com/us/en/manuals.html"),
    ("MERCEDES-BENZ", "https://www.mbusa.com/en/owners/manuals"),
    ("NISSAN", "https://www.nissanusa.com/owners/manuals-guides.html"),
    ("SUBARU", "https://www.subaru.com/owners/vehicle-resources.html"),
    ("TOYOTA", "https://www.toyota.com/owners/warranty-owners-manuals"),
];

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct OemLink {
    pub make: String,
    pub url: String,
    pub checked_on: String,
}

pub fn for_make(make: &str) -> Option<OemLink> {
    let m = make.trim().to_uppercase();
    LINKS.iter().find(|(k, _)| *k == m).map(|(k, u)| OemLink { make: (*k).into(), url: (*u).into(), checked_on: CHECKED_ON.into() })
}

pub fn all() -> Vec<OemLink> {
    LINKS.iter().map(|(k, u)| OemLink { make: (*k).into(), url: (*u).into(), checked_on: CHECKED_ON.into() }).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn lookup_is_case_insensitive_and_https_only() {
        assert!(super::for_make("Nissan").is_some());
        assert!(super::for_make("Unknown").is_none());
        assert!(super::all().iter().all(|l| l.url.starts_with("https://")));
    }
}
