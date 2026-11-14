//! Stock quantities. Stored as whole thousandths of a unit ("milli-units") so the database's
//! integer CHECKs (no negative stock, no over-reservation) and every sum stay exact, while items
//! sold by volume, length or weight (oil by the litre, hose by the foot) can hold 4.5 or 0.25.
//! In JSON a quantity is a plain number with at most three decimals.

use crate::error::{AppError, AppResult};
use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};
use ts_rs::TS;

const SCALE: i64 = 1000;
/// The largest quantity accepted in one operation.
pub const MAX_UNITS: i64 = 1_000_000;

/// Columns that hold quantities in thousandths (for raw exports).
pub const COLUMNS: &[(&str, &str)] = &[
    ("stock_levels", "on_hand"),
    ("stock_levels", "reserved"),
    ("inventory_items", "min_stock"),
    ("inventory_items", "reorder_qty"),
    ("inventory_movements", "qty"),
    ("stock_reservations", "qty"),
    ("part_list_items", "qty"),
    ("po_lines", "qty_ordered"),
    ("po_lines", "qty_received"),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, TS)]
#[ts(export)]
pub struct Qty(i64);

impl Qty {
    pub const ZERO: Qty = Qty(0);

    pub const fn units(n: i64) -> Qty {
        Qty(n * SCALE)
    }
    pub const fn from_milli(m: i64) -> Qty {
        Qty(m)
    }
    pub const fn milli(self) -> i64 {
        self.0
    }
    pub fn is_whole(self) -> bool {
        self.0 % SCALE == 0
    }
    pub fn dec(self) -> Decimal {
        Decimal::new(self.0, 3).normalize()
    }
    pub fn max(self, o: Qty) -> Qty {
        Qty(self.0.max(o.0))
    }
    pub fn min(self, o: Qty) -> Qty {
        Qty(self.0.min(o.0))
    }
    /// A decimal with at most three places ("2", "4.5", "0.125").
    pub fn from_decimal(d: Decimal) -> Option<Qty> {
        use rust_decimal::prelude::ToPrimitive;
        let m = d.checked_mul(Decimal::from(SCALE))?;
        if m.fract().is_zero() { m.to_i64().map(Qty) } else { None }
    }
    pub fn parse(field: &str, s: &str) -> AppResult<Qty> {
        let d: Decimal = s.trim().replace(',', ".").parse().map_err(|_| AppError::invalid(field, "Enter a number, like 2 or 4.5."))?;
        Qty::from_decimal(d).ok_or_else(|| AppError::invalid(field, "Use at most three decimal places."))
    }
    fn from_f64(x: f64) -> Option<Qty> {
        let m = x * SCALE as f64;
        (m.is_finite() && (m - m.round()).abs() < 1e-6 && m.abs() < 9e15).then(|| Qty(m.round() as i64))
    }
}

impl From<i64> for Qty {
    fn from(n: i64) -> Qty {
        Qty::units(n)
    }
}

impl PartialEq<i64> for Qty {
    fn eq(&self, o: &i64) -> bool {
        self.0 == o * SCALE
    }
}

impl PartialOrd<i64> for Qty {
    fn partial_cmp(&self, o: &i64) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&(o * SCALE))
    }
}

impl Add for Qty {
    type Output = Qty;
    fn add(self, o: Qty) -> Qty {
        Qty(self.0 + o.0)
    }
}
impl Sub for Qty {
    type Output = Qty;
    fn sub(self, o: Qty) -> Qty {
        Qty(self.0 - o.0)
    }
}
impl Neg for Qty {
    type Output = Qty;
    fn neg(self) -> Qty {
        Qty(-self.0)
    }
}
impl AddAssign for Qty {
    fn add_assign(&mut self, o: Qty) {
        self.0 += o.0
    }
}
impl SubAssign for Qty {
    fn sub_assign(&mut self, o: Qty) {
        self.0 -= o.0
    }
}
impl std::iter::Sum for Qty {
    fn sum<I: Iterator<Item = Qty>>(it: I) -> Qty {
        Qty(it.map(|q| q.0).sum())
    }
}

impl fmt::Display for Qty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.dec())
    }
}

impl Serialize for Qty {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.is_whole() { s.serialize_i64(self.0 / SCALE) } else { s.serialize_f64(self.0 as f64 / SCALE as f64) }
    }
}

impl<'de> Deserialize<'de> for Qty {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Qty, D::Error> {
        use serde::de::Error;
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            I(i64),
            F(f64),
            S(String),
        }
        let bad = || D::Error::custom("a quantity has at most three decimal places");
        match Raw::deserialize(d)? {
            Raw::I(n) if n.abs() <= i64::MAX / SCALE => Ok(Qty::units(n)),
            Raw::I(_) => Err(bad()),
            Raw::F(x) => Qty::from_f64(x).ok_or_else(bad),
            Raw::S(s) => Qty::parse("qty", &s).map_err(|_| bad()),
        }
    }
}

impl ToSql for Qty {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0))
    }
}

impl FromSql for Qty {
    fn column_result(v: ValueRef<'_>) -> FromSqlResult<Qty> {
        i64::column_result(v).map(Qty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_thousandths() {
        assert_eq!(Qty::parse("q", "4.5").unwrap().milli(), 4500);
        assert_eq!(Qty::parse("q", "0,125").unwrap().milli(), 125);
        assert_eq!(Qty::parse("q", "3").unwrap(), 3);
        assert!(Qty::parse("q", "0.0001").is_err());
        assert!(Qty::parse("q", "abc").is_err());
        let sum: Qty = ["0.1", "0.2"].iter().map(|s| Qty::parse("q", s).unwrap()).sum();
        assert_eq!(sum.to_string(), "0.3", "no floating-point drift");
        assert_eq!(Qty::units(2).dec().to_string(), "2");
        assert!(Qty::parse("q", "1.5").unwrap() > 1 && Qty::parse("q", "1.5").unwrap() < 2);
    }

    #[test]
    fn json_is_a_plain_number() {
        assert_eq!(serde_json::to_string(&Qty::units(4)).unwrap(), "4");
        assert_eq!(serde_json::to_string(&Qty::from_milli(4500)).unwrap(), "4.5");
        assert_eq!(serde_json::from_str::<Qty>("2").unwrap(), 2);
        assert_eq!(serde_json::from_str::<Qty>("0.3").unwrap().milli(), 300);
        assert_eq!(serde_json::from_str::<Qty>("\"1.25\"").unwrap().milli(), 1250);
        assert!(serde_json::from_str::<Qty>("0.0005").is_err());
    }
}
