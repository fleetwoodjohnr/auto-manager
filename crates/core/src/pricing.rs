//! Pricing profiles and rules, and the engine that turns a cost into a selling price with a
//! step-by-step explanation.
//!
//! Markup is on cost: price = cost × (1 + m). Margin is on price: price = cost ÷ (1 − g), so a
//! 40% markup on $50 is $70 while a 40% margin is $83.33. All arithmetic is decimal.

use crate::db::{self, clean, dec, opt_dec};
use crate::error::{AppError, AppResult};
use crate::money::{self, HUNDRED};
use rusqlite::{Connection, OptionalExtension, params};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Rule {
    pub id: String,
    pub profile_id: String,
    pub priority: i64,
    pub scope: String,
    pub scope_value: Option<String>,
    /// Category name, brand or SKU, for display.
    pub scope_label: String,
    pub method: String,
    #[ts(type = "string")]
    pub value: Decimal,
    pub min_qty: i64,
    #[ts(type = "string | null")]
    pub min_price: Option<Decimal>,
    pub enabled: bool,
    pub note: String,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub is_default: bool,
    #[ts(type = "string")]
    pub tax_rate_pct: Decimal,
    pub prices_include_tax: bool,
    #[ts(type = "string")]
    pub rounding_increment: Decimal,
    pub rounding_mode: String,
    #[ts(type = "string | null")]
    pub rounding_ending: Option<Decimal>,
    #[ts(type = "string | null")]
    pub labor_rate: Option<Decimal>,
    pub notes: String,
    pub rules: Vec<Rule>,
}

/// What the engine needs to know about the thing being priced.
#[derive(Clone, Debug, Default)]
pub struct PriceInput {
    pub cost: Decimal,
    /// Category and its ancestors, nearest first.
    pub categories: Vec<String>,
    pub brand: String,
    pub item_id: Option<String>,
    pub qty: i64,
    pub price_override: Option<Decimal>,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct PriceResult {
    /// Unit selling price (before tax unless the profile's prices include tax). None when no
    /// rule applies: the price must be set by hand, nothing is assumed.
    #[ts(type = "string | null")]
    pub price: Option<Decimal>,
    pub steps: Vec<String>,
    pub rule_id: Option<String>,
    pub profile_id: String,
    pub profile_name: String,
    #[ts(type = "string")]
    pub cost: Decimal,
    #[ts(type = "string | null")]
    pub profit: Option<Decimal>,
    #[ts(type = "string | null")]
    pub margin_pct: Option<Decimal>,
    #[ts(type = "string | null")]
    pub markup_pct: Option<Decimal>,
}

fn usd(d: Decimal) -> String {
    let r = money::round_money(d);
    let s = format!("{:.2}", r.abs());
    let (ip, fp) = s.split_once('.').unwrap_or((&s, "00"));
    let mut grouped = String::new();
    for (i, ch) in ip.chars().enumerate() {
        if i > 0 && (ip.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{}${grouped}.{fp}", if r.is_sign_negative() && !r.is_zero() { "−" } else { "" })
}

fn pct(d: Decimal) -> String {
    format!("{}%", d.normalize())
}

fn specificity(scope: &str) -> u8 {
    match scope {
        "item" => 3,
        "brand" => 2,
        "category" => 1,
        _ => 0,
    }
}

fn applies(r: &Rule, i: &PriceInput) -> bool {
    r.enabled
        && i.qty.max(1) >= r.min_qty
        && match r.scope.as_str() {
            "all" => true,
            "category" => r.scope_value.as_ref().is_some_and(|c| i.categories.contains(c)),
            "brand" => r.scope_value.as_ref().is_some_and(|b| b.eq_ignore_ascii_case(i.brand.trim()) && !i.brand.trim().is_empty()),
            "item" => r.scope_value.is_some() && r.scope_value == i.item_id,
            _ => false,
        }
}

/// Round a price per the profile: to an increment (nearest/up/down) or to a price ending
/// such as .99 (the smallest such price not below the calculated one).
pub fn round_price(p: &Profile, price: Decimal) -> Decimal {
    match p.rounding_mode.as_str() {
        "ending" => {
            let ending = p.rounding_ending.unwrap_or(Decimal::new(99, 2)).fract();
            let candidate = price.trunc() + ending;
            if candidate >= price { candidate } else { candidate + Decimal::ONE }
        }
        mode => {
            let inc = if p.rounding_increment > Decimal::ZERO { p.rounding_increment } else { Decimal::new(1, 2) };
            let units = price / inc;
            let units = match mode {
                "up" => units.round_dp_with_strategy(0, RoundingStrategy::AwayFromZero),
                "down" => units.round_dp_with_strategy(0, RoundingStrategy::ToZero),
                _ => units.round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero),
            };
            units * inc
        }
    }
}

pub fn compute(profile: &Profile, input: &PriceInput) -> PriceResult {
    let mut steps = vec![format!("Cost {}", usd(input.cost))];
    let mut result = PriceResult {
        price: None,
        steps: vec![],
        rule_id: None,
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
        cost: input.cost,
        profit: None,
        margin_pct: None,
        markup_pct: None,
    };
    let price = if let Some(fixed) = input.price_override {
        steps.push(format!("Fixed selling price set on this item: {}", usd(fixed)));
        Some(fixed)
    } else {
        let mut candidates: Vec<&Rule> = profile.rules.iter().filter(|r| applies(r, input)).collect();
        candidates.sort_by(|a, b| a.priority.cmp(&b.priority).then(specificity(&b.scope).cmp(&specificity(&a.scope))));
        match candidates.first() {
            None => {
                steps.push(format!(
                    "No rule in “{}” applies, so no price is calculated. Add a rule or set a price on the item.",
                    profile.name
                ));
                None
            }
            Some(rule) => {
                result.rule_id = Some(rule.id.clone());
                let scope = match rule.scope.as_str() {
                    "all" => "all products".to_string(),
                    "category" => format!("category {}", rule.scope_label),
                    "brand" => format!("brand {}", rule.scope_label),
                    _ => format!("item {}", rule.scope_label),
                };
                let qty = if rule.min_qty > 1 { format!(", {}+ units", rule.min_qty) } else { String::new() };
                let raw = match rule.method.as_str() {
                    "fixed" => {
                        steps.push(format!("Rule (priority {}, {scope}{qty}): fixed price {}", rule.priority, usd(rule.value)));
                        rule.value
                    }
                    "markup" => {
                        let p = input.cost * (Decimal::ONE + money::frac(rule.value));
                        steps.push(format!(
                            "Rule (priority {}, {scope}{qty}): {} markup on cost → {} × {} = {}",
                            rule.priority,
                            pct(rule.value),
                            usd(input.cost),
                            (Decimal::ONE + money::frac(rule.value)).normalize(),
                            usd(p)
                        ));
                        p
                    }
                    _ => {
                        let p = input.cost / (Decimal::ONE - money::frac(rule.value));
                        steps.push(format!(
                            "Rule (priority {}, {scope}{qty}): {} target margin → {} ÷ {} = {}",
                            rule.priority,
                            pct(rule.value),
                            usd(input.cost),
                            (Decimal::ONE - money::frac(rule.value)).normalize(),
                            usd(p)
                        ));
                        p
                    }
                };
                let mut p = raw;
                if rule.method != "fixed" {
                    let rounded = round_price(profile, p);
                    if rounded != p {
                        let how = match profile.rounding_mode.as_str() {
                            "ending" => format!(
                                "Rounded up to end in {}",
                                usd(profile.rounding_ending.unwrap_or(Decimal::new(99, 2)).fract()).trim_start_matches("$0")
                            ),
                            "up" => format!("Rounded up to the next {}", usd(profile.rounding_increment)),
                            "down" => format!("Rounded down to {}", usd(profile.rounding_increment)),
                            _ => format!("Rounded to the nearest {}", usd(profile.rounding_increment)),
                        };
                        steps.push(format!("{how} → {}", usd(rounded)));
                        p = rounded;
                    }
                }
                if let Some(min) = rule.min_price
                    && p < min
                {
                    steps.push(format!("Raised to this rule's minimum price {}", usd(min)));
                    p = min;
                }
                Some(p)
            }
        }
    };
    if let Some(p) = price {
        let p = money::round_money(p);
        let net = if profile.prices_include_tax && profile.tax_rate_pct > Decimal::ZERO {
            p * HUNDRED / (HUNDRED + profile.tax_rate_pct)
        } else {
            p
        };
        let profit = net - input.cost;
        result.profit = Some(money::round_money(profit));
        result.margin_pct = (net > Decimal::ZERO).then(|| (profit / net * HUNDRED).round_dp(1));
        result.markup_pct = (input.cost > Decimal::ZERO).then(|| (profit / input.cost * HUNDRED).round_dp(1));
        steps.push(format!("Selling price {}{}", usd(p), if profile.prices_include_tax { " (includes tax)" } else { "" }));
        result.price = Some(p);
    }
    result.steps = steps;
    result
}

// ---------------------------------------------------------------- storage

fn rule_label(conn: &Connection, scope: &str, value: Option<&str>) -> AppResult<String> {
    Ok(match (scope, value) {
        ("category", Some(v)) => crate::parts::category_path(conn, v).unwrap_or_else(|_| v.to_string()),
        ("item", Some(v)) => conn
            .query_row("SELECT sku FROM inventory_items WHERE id = ?1", [v], |r| r.get(0))
            .optional()?
            .unwrap_or_else(|| "(deleted item)".into()),
        (_, Some(v)) => v.to_string(),
        _ => String::new(),
    })
}

pub fn rules(conn: &Connection, profile_id: &str) -> AppResult<Vec<Rule>> {
    let mut st = conn.prepare("SELECT id, profile_id, priority, scope, scope_value, method, value, min_qty, min_price, enabled, note FROM pricing_rules WHERE profile_id = ?1 ORDER BY priority, id")?;
    let rows = st.query_map([profile_id], |r| {
        Ok(Rule {
            id: r.get(0)?,
            profile_id: r.get(1)?,
            priority: r.get(2)?,
            scope: r.get(3)?,
            scope_value: r.get(4)?,
            scope_label: String::new(),
            method: r.get(5)?,
            value: dec(r, "value")?,
            min_qty: r.get(7)?,
            min_price: opt_dec(r, "min_price")?,
            enabled: r.get::<_, i64>(9)? == 1,
            note: r.get(10)?,
        })
    })?;
    let mut out: Vec<Rule> = rows.collect::<Result<_, _>>()?;
    for r in &mut out {
        r.scope_label = rule_label(conn, &r.scope, r.scope_value.as_deref())?;
    }
    Ok(out)
}

const PROFILE_SELECT: &str = "SELECT id, name, kind, is_default, tax_rate_pct, prices_include_tax, rounding_increment, rounding_mode, rounding_ending, labor_rate, notes FROM pricing_profiles";

fn profile_row(r: &rusqlite::Row) -> rusqlite::Result<Profile> {
    Ok(Profile {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        is_default: r.get::<_, i64>(3)? == 1,
        tax_rate_pct: dec(r, "tax_rate_pct")?,
        prices_include_tax: r.get::<_, i64>(5)? == 1,
        rounding_increment: dec(r, "rounding_increment")?,
        rounding_mode: r.get(7)?,
        rounding_ending: opt_dec(r, "rounding_ending")?,
        labor_rate: opt_dec(r, "labor_rate")?,
        notes: r.get(10)?,
        rules: vec![],
    })
}

pub fn profile(conn: &Connection, id: &str) -> AppResult<Profile> {
    let p = conn
        .query_row(&format!("{PROFILE_SELECT} WHERE id = ?1"), [id], profile_row)
        .optional()?
        .ok_or_else(|| AppError::not_found("That pricing profile"))?;
    Ok(Profile { rules: rules(conn, id)?, ..p })
}

pub fn default_profile(conn: &Connection) -> AppResult<Profile> {
    let id: String = conn.query_row("SELECT id FROM pricing_profiles ORDER BY is_default DESC, created_at LIMIT 1", [], |r| r.get(0))?;
    profile(conn, &id)
}

pub fn profiles(conn: &Connection) -> AppResult<Vec<Profile>> {
    let mut st = conn.prepare(&format!("{PROFILE_SELECT} ORDER BY is_default DESC, name"))?;
    let list: Vec<Profile> = st.query_map([], profile_row)?.collect::<Result<_, _>>()?;
    list.into_iter().map(|p| Ok(Profile { rules: rules(conn, &p.id)?, ..p })).collect()
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct ProfileInput {
    pub name: String,
    pub kind: String,
    pub tax_rate_pct: String,
    pub prices_include_tax: bool,
    pub rounding_increment: String,
    pub rounding_mode: String,
    pub rounding_ending: Option<String>,
    pub labor_rate: Option<String>,
    #[serde(default)]
    pub notes: String,
}

fn opt_money(field: &str, label: &str, s: Option<&str>) -> AppResult<Option<Decimal>> {
    s.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| money::parse(field, label, s).and_then(|d| money::non_negative(field, label, d)))
        .transpose()
}

pub fn save_profile(conn: &Connection, id: Option<&str>, p: &ProfileInput) -> AppResult<Profile> {
    let name = clean(&p.name);
    if name.is_empty() {
        return Err(AppError::invalid("name", "Name the profile."));
    }
    if !matches!(p.kind.as_str(), "retail" | "wholesale" | "internal" | "fleet" | "customer" | "custom") {
        return Err(AppError::invalid("kind", "Choose a profile type."));
    }
    let tax = money::pct_0_100("tax_rate_pct", "Tax rate", money::parse("tax_rate_pct", "Tax rate", &p.tax_rate_pct)?)?;
    let inc = money::parse("rounding_increment", "Rounding", &p.rounding_increment)?;
    if inc <= Decimal::ZERO {
        return Err(AppError::invalid("rounding_increment", "Rounding must be more than zero (0.01 means to the cent)."));
    }
    if !matches!(p.rounding_mode.as_str(), "nearest" | "up" | "down" | "ending") {
        return Err(AppError::invalid("rounding_mode", "Choose how prices are rounded."));
    }
    let ending = opt_money("rounding_ending", "Price ending", p.rounding_ending.as_deref())?;
    if p.rounding_mode == "ending" && ending.is_none_or(|e| e >= Decimal::ONE) {
        return Err(AppError::invalid("rounding_ending", "Enter the ending as cents, e.g. 0.99."));
    }
    let labor = opt_money("labor_rate", "Labor rate", p.labor_rate.as_deref())?;
    let params_ = params![
        name,
        p.kind,
        tax.to_string(),
        p.prices_include_tax,
        inc.to_string(),
        p.rounding_mode,
        ending.map(|e| e.to_string()),
        labor.map(|l| l.to_string()),
        p.notes.trim()
    ];
    let unique = |e: rusqlite::Error| {
        if e.to_string().contains("UNIQUE") { AppError::invalid("name", "A profile with that name already exists.") } else { e.into() }
    };
    let id = match id {
        Some(id) => {
            let n = conn
                .execute(
                    "UPDATE pricing_profiles SET name = ?1, kind = ?2, tax_rate_pct = ?3, prices_include_tax = ?4, rounding_increment = ?5, rounding_mode = ?6,
                       rounding_ending = ?7, labor_rate = ?8, notes = ?9 WHERE id = ?10",
                    rusqlite::params_from_iter(params_.iter().copied().chain(std::iter::once(&id as &dyn rusqlite::ToSql))),
                )
                .map_err(unique)?;
            db::expect_one(n, "That pricing profile")?;
            id.to_string()
        }
        None => {
            let id = db::new_id();
            conn.execute(
                "INSERT INTO pricing_profiles (name, kind, tax_rate_pct, prices_include_tax, rounding_increment, rounding_mode, rounding_ending, labor_rate, notes, id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params_from_iter(params_.iter().copied().chain(std::iter::once(&id as &dyn rusqlite::ToSql))),
            )
            .map_err(unique)?;
            id
        }
    };
    profile(conn, &id)
}

pub fn set_default_profile(conn: &Connection, id: &str) -> AppResult<()> {
    profile(conn, id)?;
    conn.execute("UPDATE pricing_profiles SET is_default = 0 WHERE is_default = 1", [])?;
    conn.execute("UPDATE pricing_profiles SET is_default = 1 WHERE id = ?1", [id])?;
    Ok(())
}

pub fn delete_profile(conn: &Connection, id: &str) -> AppResult<()> {
    let p = profile(conn, id)?;
    if p.is_default {
        return Err(AppError::Conflict("Make another profile the default before deleting this one.".into()));
    }
    conn.execute("DELETE FROM pricing_profiles WHERE id = ?1", [id])?;
    Ok(())
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct RuleInput {
    pub priority: i64,
    pub scope: String,
    pub scope_value: Option<String>,
    pub method: String,
    pub value: String,
    #[serde(default = "one")]
    pub min_qty: i64,
    pub min_price: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub note: String,
}
fn one() -> i64 {
    1
}
fn yes() -> bool {
    true
}

pub fn save_rule(conn: &Connection, profile_id: &str, id: Option<&str>, r: &RuleInput) -> AppResult<Profile> {
    profile(conn, profile_id)?;
    if !matches!(r.scope.as_str(), "all" | "category" | "brand" | "item") {
        return Err(AppError::invalid("scope", "Choose what the rule applies to."));
    }
    let scope_value = r.scope_value.as_deref().map(clean).filter(|v| !v.is_empty());
    if (r.scope == "all") != scope_value.is_none() {
        return Err(AppError::invalid("scope_value", "Choose the category, brand or item this rule applies to."));
    }
    if r.scope == "category" && !db::exists(conn, "SELECT 1 FROM part_categories WHERE id = ?1", [scope_value.as_deref()])? {
        return Err(AppError::invalid("scope_value", "Choose a category from the list."));
    }
    if r.scope == "item" && !db::exists(conn, "SELECT 1 FROM inventory_items WHERE id = ?1", [scope_value.as_deref()])? {
        return Err(AppError::invalid("scope_value", "Choose an inventory item."));
    }
    let value = money::parse("value", "Value", &r.value)?;
    let value = match r.method.as_str() {
        "fixed" => money::non_negative("value", "Price", value)?,
        "markup" => money::non_negative("value", "Markup", value)?,
        "margin" => money::pct_below_100("value", "Margin", value)?,
        _ => return Err(AppError::invalid("method", "Choose fixed price, markup or margin.")),
    };
    if r.min_qty < 1 {
        return Err(AppError::invalid("min_qty", "Minimum quantity is at least 1."));
    }
    let min_price = opt_money("min_price", "Minimum price", r.min_price.as_deref())?;
    match id {
        Some(id) => {
            let n = conn.execute(
                "UPDATE pricing_rules SET priority = ?3, scope = ?4, scope_value = ?5, method = ?6, value = ?7, min_qty = ?8, min_price = ?9, enabled = ?10, note = ?11
                 WHERE id = ?1 AND profile_id = ?2",
                params![id, profile_id, r.priority, r.scope, scope_value, r.method, value.to_string(), r.min_qty, min_price.map(|m| m.to_string()), r.enabled, r.note.trim()],
            )?;
            db::expect_one(n, "That rule")?;
        }
        None => {
            conn.execute(
                "INSERT INTO pricing_rules (id, profile_id, priority, scope, scope_value, method, value, min_qty, min_price, enabled, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![db::new_id(), profile_id, r.priority, r.scope, scope_value, r.method, value.to_string(), r.min_qty, min_price.map(|m| m.to_string()), r.enabled, r.note.trim()],
            )?;
        }
    }
    db::audit(conn, "pricing", Some(profile_id), "rule", "Pricing rule saved", None)?;
    profile(conn, profile_id)
}

pub fn delete_rule(conn: &Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM pricing_rules WHERE id = ?1", [id])?;
    db::expect_one(n, "That rule")
}

/// Which profile applies: the job's, else the customer's, else the item's, else the default.
pub fn resolve_profile(conn: &Connection, explicit: &[Option<&str>]) -> AppResult<Profile> {
    for id in explicit.iter().flatten() {
        if let Ok(p) = profile(conn, id) {
            return Ok(p);
        }
    }
    default_profile(conn)
}

/// Price one inventory item.
pub fn price_item(conn: &Connection, item_id: &str, qty: i64, profile_ids: &[Option<&str>], valuation: &str) -> AppResult<PriceResult> {
    let item = crate::inventory::get(conn, item_id)?;
    let mut ids: Vec<Option<&str>> = profile_ids.to_vec();
    ids.push(item.pricing_profile_id.as_deref());
    let p = resolve_profile(conn, &ids)?;
    let categories = match &item.category_id {
        Some(c) => crate::parts::category_chain(conn, c)?,
        None => vec![],
    };
    let cost = if valuation == "standard" || item.avg_cost.is_zero() { item.cost } else { item.avg_cost };
    Ok(compute(
        &p,
        &PriceInput {
            cost,
            categories,
            brand: item.brand.clone(),
            item_id: Some(item.id.clone()),
            qty: qty.max(1),
            price_override: item.price_override,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn profile_with(rules: Vec<Rule>) -> Profile {
        Profile {
            id: "pp".into(),
            name: "Retail".into(),
            kind: "retail".into(),
            is_default: true,
            tax_rate_pct: dec!(8.25),
            prices_include_tax: false,
            rounding_increment: dec!(0.01),
            rounding_mode: "nearest".into(),
            rounding_ending: None,
            labor_rate: None,
            notes: String::new(),
            rules,
        }
    }
    fn rule(id: &str, priority: i64, scope: &str, value: Option<&str>, method: &str, v: Decimal) -> Rule {
        Rule {
            id: id.into(),
            profile_id: "pp".into(),
            priority,
            scope: scope.into(),
            scope_value: value.map(Into::into),
            scope_label: value.unwrap_or("").into(),
            method: method.into(),
            value: v,
            min_qty: 1,
            min_price: None,
            enabled: true,
            note: String::new(),
        }
    }
    fn input(cost: Decimal) -> PriceInput {
        PriceInput {
            cost,
            categories: vec!["brakes.pads".into(), "brakes".into()],
            brand: "Acme".into(),
            item_id: Some("i1".into()),
            qty: 1,
            price_override: None,
        }
    }

    #[test]
    fn markup_versus_margin() {
        let r = compute(&profile_with(vec![rule("m", 10, "all", None, "markup", dec!(40))]), &input(dec!(50)));
        assert_eq!(r.price, Some(dec!(70.00)));
        assert!(r.steps.iter().any(|s| s.contains("40% markup")), "{:?}", r.steps);
        assert_eq!(r.markup_pct, Some(dec!(40.0)));
        assert_eq!(r.margin_pct, Some(dec!(28.6)));
        let g = compute(&profile_with(vec![rule("g", 10, "all", None, "margin", dec!(40))]), &input(dec!(50)));
        assert_eq!(g.price, Some(dec!(83.33)));
        assert_eq!(g.margin_pct, Some(dec!(40.0)));
    }

    #[test]
    fn precedence_by_priority_then_specificity() {
        let rules = vec![
            rule("all", 10, "all", None, "markup", dec!(40)),
            rule("cat", 10, "category", Some("brakes"), "markup", dec!(50)),
            rule("brand", 5, "brand", Some("acme"), "markup", dec!(30)),
        ];
        let r = compute(&profile_with(rules.clone()), &input(dec!(100)));
        assert_eq!(r.rule_id.as_deref(), Some("brand"), "lower priority number wins");
        let mut rules2 = rules;
        rules2[2].priority = 10;
        let r = compute(&profile_with(rules2), &input(dec!(100)));
        assert_eq!(r.rule_id.as_deref(), Some("brand"), "brand is more specific than category at equal priority");
        let r = compute(
            &profile_with(vec![rule("cat", 10, "category", Some("brakes"), "markup", dec!(50))]),
            &PriceInput { categories: vec!["engine".into()], ..input(dec!(10)) },
        );
        assert_eq!(r.price, None, "no applicable rule: no price is invented");
    }

    #[test]
    fn rounding_minimum_and_override() {
        let mut p = profile_with(vec![Rule { min_price: Some(dec!(20)), ..rule("m", 10, "all", None, "markup", dec!(37)) }]);
        p.rounding_mode = "ending".into();
        p.rounding_ending = Some(dec!(0.99));
        assert_eq!(compute(&p, &input(dec!(50))).price, Some(dec!(68.99)), "68.50 → 68.99");
        assert_eq!(compute(&p, &input(dec!(5))).price, Some(dec!(20)), "minimum price");
        p.rounding_mode = "up".into();
        p.rounding_increment = dec!(0.25);
        assert_eq!(compute(&p, &input(dec!(50))).price, Some(dec!(68.50)));
        assert_eq!(compute(&p, &input(dec!(50.10))).price, Some(dec!(68.75)));
        let fixed = compute(&p, &PriceInput { price_override: Some(dec!(12.34)), ..input(dec!(50)) });
        assert_eq!(fixed.price, Some(dec!(12.34)));
        assert!(fixed.profit.unwrap().is_sign_negative());
    }

    #[test]
    fn quantity_breaks() {
        let rules = vec![
            rule("base", 20, "all", None, "markup", dec!(50)),
            Rule { min_qty: 10, ..rule("bulk", 10, "all", None, "markup", dec!(25)) },
        ];
        let p = profile_with(rules);
        assert_eq!(compute(&p, &input(dec!(10))).price, Some(dec!(15.00)));
        assert_eq!(compute(&p, &PriceInput { qty: 10, ..input(dec!(10)) }).price, Some(dec!(12.50)));
    }

    #[test]
    fn precision() {
        // 3 × $0.10 style errors can't happen in decimal.
        let r = compute(&profile_with(vec![rule("m", 1, "all", None, "markup", dec!(33.333))]), &input(dec!(0.30)));
        assert_eq!(r.price, Some(dec!(0.40)));
        assert_eq!(usd(dec!(1234567.891)), "$1,234,567.89");
    }

    #[test]
    fn stored_profiles_validate() {
        let conn = db::open_memory().unwrap();
        let d = default_profile(&conn).unwrap();
        assert!(d.rules.is_empty(), "nothing assumed: the default profile starts without rules");
        let bad = RuleInput {
            priority: 1,
            scope: "all".into(),
            scope_value: None,
            method: "margin".into(),
            value: "100".into(),
            min_qty: 1,
            min_price: None,
            enabled: true,
            note: String::new(),
        };
        assert!(save_rule(&conn, &d.id, None, &bad).is_err(), "a 100% margin is impossible");
        let ok = RuleInput { value: "40".into(), method: "markup".into(), ..bad };
        let p = save_rule(&conn, &d.id, None, &ok).unwrap();
        assert_eq!(p.rules.len(), 1);
        assert!(delete_profile(&conn, &d.id).is_err());
    }
}
