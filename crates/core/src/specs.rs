//! The vehicle specification fields Auto Manager knows about. A field with no value from any
//! source is shown as "not available" with a way to add it from a manual: nothing is assumed.

use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Copy, Debug)]
#[ts(export)]
pub struct SpecField {
    pub key: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    /// Canonical unit the value is stored in; the UI converts for display.
    pub unit: Option<&'static str>,
    /// Used to match fitment records.
    pub fitment: bool,
    /// Shown in the Overview tab (the rest appear under Full Specifications).
    pub essential: bool,
}

const fn f(
    key: &'static str,
    label: &'static str,
    group: &'static str,
    unit: Option<&'static str>,
    fitment: bool,
    essential: bool,
) -> SpecField {
    SpecField { key, label, group, unit, fitment, essential }
}

pub const IDENTITY: &str = "Vehicle identity";
pub const ENGINE: &str = "Engine and drivetrain";
pub const DIMENSIONS: &str = "Dimensions and capacities";
pub const MAINTENANCE: &str = "Maintenance specifications";
pub const ECONOMY: &str = "Fuel economy";
pub const OTHER: &str = "Other recorded data";

pub const FIELDS: &[SpecField] = &[
    f("model_year", "Model year", IDENTITY, None, false, true),
    f("make", "Make", IDENTITY, None, false, true),
    f("model", "Model", IDENTITY, None, false, true),
    f("trim", "Trim", IDENTITY, None, true, true),
    f("series", "Series", IDENTITY, None, true, false),
    f("manufacturer", "Manufacturer", IDENTITY, None, false, false),
    f("body_class", "Body type", IDENTITY, None, true, true),
    f("vehicle_type", "Vehicle class", IDENTITY, None, false, false),
    f("doors", "Doors", IDENTITY, None, false, false),
    f("plant_country", "Country of manufacture", IDENTITY, None, false, false),
    f("plant_city", "Assembly plant city", IDENTITY, None, false, false),
    f("plant_state", "Assembly plant state/province", IDENTITY, None, false, false),
    f("plant_company", "Assembly plant company", IDENTITY, None, false, false),
    f("vehicle_descriptor", "Manufacturer identifier (VIN pattern)", IDENTITY, None, false, false),
    f("engine_configuration", "Engine configuration", ENGINE, None, false, true),
    f("engine_displacement_l", "Engine displacement", ENGINE, Some("L"), true, true),
    f("engine_cylinders", "Cylinders", ENGINE, None, true, true),
    f("fuel_type", "Fuel type", ENGINE, None, true, true),
    f("fuel_type_secondary", "Secondary fuel type", ENGINE, None, false, false),
    f("engine_code", "Engine code / designation", ENGINE, None, true, true),
    f("engine_manufacturer", "Engine manufacturer", ENGINE, None, false, false),
    f("engine_hp", "Horsepower", ENGINE, Some("hp"), false, false),
    f("engine_torque", "Torque", ENGINE, Some("lb_ft"), false, false),
    f("valve_train", "Valve train", ENGINE, None, false, false),
    f("turbo", "Turbocharged", ENGINE, None, false, false),
    f("electrification", "Electrification", ENGINE, None, false, false),
    f("transmission_style", "Transmission type", ENGINE, None, true, true),
    f("transmission_code", "Transmission code", ENGINE, None, true, false),
    f("transmission_speeds", "Number of gears", ENGINE, None, false, true),
    f("drive_type", "Drivetrain", ENGINE, None, true, true),
    f("axles", "Axles", ENGINE, None, false, false),
    f("axle_configuration", "Axle configuration", ENGINE, None, false, false),
    f("differential", "Differential", ENGINE, None, false, false),
    f("brake_system", "Brake system", ENGINE, None, false, false),
    f("length", "Length", DIMENSIONS, Some("in"), false, false),
    f("width", "Width", DIMENSIONS, Some("in"), false, false),
    f("height", "Height", DIMENSIONS, Some("in"), false, false),
    f("wheelbase", "Wheelbase", DIMENSIONS, Some("in"), false, false),
    f("track_width", "Track width", DIMENSIONS, Some("in"), false, false),
    f("curb_weight", "Curb weight", DIMENSIONS, Some("lb"), false, false),
    f("gvwr", "Gross vehicle weight rating", DIMENSIONS, None, false, false),
    f("towing_capacity", "Towing capacity", DIMENSIONS, Some("lb"), false, false),
    f("payload_capacity", "Payload capacity", DIMENSIONS, Some("lb"), false, false),
    f("fuel_tank", "Fuel tank capacity", DIMENSIONS, Some("gal"), false, false),
    f("seats", "Seating capacity", DIMENSIONS, None, false, false),
    f("seat_rows", "Seat rows", DIMENSIONS, None, false, false),
    f("tire_size_front", "Front tire size", DIMENSIONS, None, true, false),
    f("tire_size_rear", "Rear tire size", DIMENSIONS, None, true, false),
    f("wheel_size_front", "Front wheel diameter", DIMENSIONS, Some("in"), false, false),
    f("wheel_size_rear", "Rear wheel diameter", DIMENSIONS, Some("in"), false, false),
    f("bed_length", "Bed length", DIMENSIONS, Some("in"), false, false),
    f("mpg_city", "City (EPA, mpg)", ECONOMY, None, false, false),
    f("mpg_highway", "Highway (EPA, mpg)", ECONOMY, None, false, false),
    f("mpg_combined", "Combined (EPA, mpg)", ECONOMY, None, false, false),
    f("ev_range", "Electric range (EPA)", ECONOMY, Some("mi"), false, false),
    f("oil_spec", "Engine oil specification", MAINTENANCE, None, false, true),
    f("oil_capacity", "Engine oil capacity", MAINTENANCE, Some("qt"), false, true),
    f("coolant_spec", "Coolant specification", MAINTENANCE, None, false, false),
    f("coolant_capacity", "Coolant capacity", MAINTENANCE, Some("qt"), false, false),
    f("transmission_fluid", "Transmission fluid", MAINTENANCE, None, false, false),
    f("brake_fluid", "Brake fluid", MAINTENANCE, None, false, false),
    f("differential_fluid", "Differential fluid", MAINTENANCE, None, false, false),
    f("spark_plug", "Spark plugs", MAINTENANCE, None, false, false),
    f("battery", "Battery", MAINTENANCE, None, false, false),
    f("wheel_nut_torque", "Wheel nut torque", MAINTENANCE, Some("lb_ft"), false, false),
    f("tire_pressure_front", "Front tire pressure", MAINTENANCE, Some("psi"), false, false),
    f("tire_pressure_rear", "Rear tire pressure", MAINTENANCE, Some("psi"), false, false),
    f("service_intervals", "Recommended service intervals", MAINTENANCE, None, false, false),
];

pub fn field(key: &str) -> Option<&'static SpecField> {
    FIELDS.iter().find(|f| f.key == key)
}

/// Keys a person may set by hand: every known field, plus free qualifiers used by fitment
/// (brake_package, production_date, ...). Keys are short snake_case words.
pub fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 64 && key.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
}

/// "3.0L V6", "1.8L 4-cyl", "Electric" from effective values.
pub fn engine_summary(get: &dyn Fn(&str) -> Option<String>) -> String {
    let fuel = get("fuel_type").unwrap_or_default();
    if fuel.eq_ignore_ascii_case("electric") {
        return "Electric".into();
    }
    let disp = get("engine_displacement_l").and_then(|d| d.parse::<f64>().ok()).map(|d| format!("{d:.1}L"));
    let cyl = get("engine_cylinders");
    let config = get("engine_configuration").unwrap_or_default().to_lowercase();
    let shape = match (config.as_str(), &cyl) {
        (c, Some(n)) if c.starts_with("v") => Some(format!("V{n}")),
        (c, Some(n)) if c.starts_with("in-line") || c.starts_with("inline") => Some(format!("I{n}")),
        (c, Some(n)) if c.contains("horizontally") || c.contains("flat") => Some(format!("H{n}")),
        (_, Some(n)) => Some(format!("{n}-cyl")),
        _ => None,
    };
    let mut parts: Vec<String> = [disp, shape].into_iter().flatten().collect();
    if !fuel.is_empty() && !fuel.eq_ignore_ascii_case("gasoline") {
        parts.push(fuel);
    }
    parts.join(" ")
}

pub fn transmission_summary(get: &dyn Fn(&str) -> Option<String>) -> String {
    match (get("transmission_speeds"), get("transmission_style")) {
        (Some(n), Some(s)) => format!("{n}-speed {s}"),
        (None, Some(s)) => s,
        (Some(n), None) => format!("{n}-speed"),
        (None, None) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_valid() {
        let mut seen = std::collections::HashSet::new();
        for f in FIELDS {
            assert!(seen.insert(f.key), "duplicate {}", f.key);
            assert!(valid_key(f.key));
        }
    }

    #[test]
    fn summaries() {
        let m = |pairs: &'static [(&'static str, &'static str)]| move |k: &str| pairs.iter().find(|p| p.0 == k).map(|p| p.1.to_string());
        let honda = m(&[
            ("engine_displacement_l", "2.998832712"),
            ("engine_cylinders", "6"),
            ("engine_configuration", "V-Shaped"),
            ("fuel_type", "Gasoline"),
        ]);
        assert_eq!(engine_summary(&honda), "3.0L V6");
        let diesel =
            m(&[("engine_displacement_l", "2.0"), ("engine_cylinders", "4"), ("engine_configuration", "In-Line"), ("fuel_type", "Diesel")]);
        assert_eq!(engine_summary(&diesel), "2.0L I4 Diesel");
        assert_eq!(engine_summary(&m(&[("fuel_type", "Electric")])), "Electric");
        assert_eq!(engine_summary(&m(&[])), "");
        assert_eq!(transmission_summary(&m(&[("transmission_speeds", "5"), ("transmission_style", "Automatic")])), "5-speed Automatic");
    }
}
