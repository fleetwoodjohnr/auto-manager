//! Two-way sync between computers through a shared folder (Syncthing, Dropbox, OneDrive, a
//! network share or a USB stick). No server and no account.
//!
//! * Every change to a synced table is recorded in `sync_log` by triggers, with a hybrid
//!   logical clock (`sync_clock.hlc`, milliseconds that never go backwards and move past any
//!   clock seen from another computer).
//! * Each computer writes one file, `devices/<id>.amstate`: its complete state (every row and
//!   deletion it knows, with versions), compressed and encrypted with the group key.
//! * Syncing reads the other computers' files and keeps, per row, the version with the highest
//!   (clock, computer id): last writer wins, per row. Deletions are kept as tombstones.
//! * Then it repairs what a row-by-row merge can't: rows that point at deleted rows, the same
//!   VIN/SKU/document added on two computers (merged into one), clashing job, invoice and order
//!   numbers (the later one is renumbered), and stock. Stock levels are recomputed from the
//!   movement ledger; if two computers both took the last units, a correction is recorded and
//!   reported so someone counts the item.
//!
//! Every repair is deterministic, so all computers reach the same result.

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::{Core, Paths};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use ts_rs::TS;

const SEP: char = '\u{1f}';
const FORMAT: &str = "auto-manager-sync";
const MAGIC: &[u8] = b"AMSYNC2\n";
const CHECK: &[u8] = b"auto-manager sync group v1";

struct Child {
    table: &'static str,
    fk: &'static str,
    skip: &'static [&'static str],
    fts: bool,
}

struct Spec {
    table: &'static str,
    key: &'static [&'static str],
    /// Columns never sent (local integer ids).
    skip: &'static [&'static str],
    /// When set, only these columns (besides the key) are taken from other computers.
    only: Option<&'static [&'static str]>,
    children: &'static [Child],
}

const ID: &[&str] = &["id"];
const fn row(table: &'static str) -> Spec {
    Spec { table, key: ID, skip: &[], only: None, children: &[] }
}
const fn natural(table: &'static str, key: &'static [&'static str]) -> Spec {
    Spec { table, key, skip: &["id"], only: None, children: &[] }
}
const fn pair(table: &'static str, key: &'static [&'static str]) -> Spec {
    Spec { table, key, skip: &[], only: None, children: &[] }
}

/// Synced tables, parents before children. Device-local data (settings, caches, the audit
/// history, the bundled code reference) isn't synced.
const TABLES: &[Spec] = &[
    row("pricing_profiles"),
    row("pricing_rules"),
    row("customers"),
    row("vehicles"),
    natural("vehicle_specifications", &["vehicle_id", "key", "source_id"]),
    pair("recall_status", &["vehicle_id", "campaign"]),
    row("part_categories"),
    row("parts"),
    natural("part_identifiers", &["part_id", "kind", "value_norm"]),
    row("part_fitments"),
    natural("fitment_qualifiers", &["fitment_id", "key"]),
    pair("part_supersessions", &["old_part_id", "new_part_id"]),
    pair("part_interchanges", &["part_a", "part_b"]),
    row("suppliers"),
    row("inventory_locations"),
    row("inventory_items"),
    pair("supplier_parts", &["supplier_id", "item_id"]),
    Spec { table: "stock_levels", key: &["item_id", "location_id"], skip: &[], only: Some(&["bin"]), children: &[] },
    row("service_jobs"),
    row("service_lines"),
    row("stock_reservations"),
    row("inventory_movements"),
    row("part_lists"),
    natural("part_list_items", &["list_id", "part_id"]),
    Spec {
        table: "manuals",
        key: ID,
        skip: &[],
        only: None,
        children: &[
            Child { table: "manual_vehicle_links", fk: "manual_id", skip: &["id"], fts: false },
            Child { table: "manual_pages", fk: "manual_id", skip: &[], fts: true },
        ],
    },
    row("manual_annotations"),
    row("attachments"),
    row("maintenance_intervals"),
    row("maintenance_records"),
    row("purchase_orders"),
    row("po_lines"),
    row("saved_searches"),
    pair("external_links", &["system", "entity", "external_id"]),
    Spec {
        table: "obd_sessions",
        key: ID,
        skip: &[],
        only: None,
        children: &[Child { table: "obd_readings", fk: "session_id", skip: &["id"], fts: false }],
    },
];

/// Rows that are the same thing when these columns match: merged into one, keeping the row with
/// the smaller (older) id.
const SAME_THING: &[(&str, &str)] = &[
    ("vehicles", "vin"),
    ("manuals", "file_sha256"),
    ("inventory_items", "sku"),
    ("inventory_locations", "name"),
    ("pricing_profiles", "name"),
    ("part_categories", "name"),
];
/// Numbers people see on paper: the later row gets the next free number.
const NUMBERS: &[(&str, &str)] = &[("service_jobs", "number"), ("service_jobs", "invoice_number"), ("purchase_orders", "number")];

fn spec(table: &str) -> Option<&'static Spec> {
    TABLES.iter().find(|s| s.table == table)
}

fn key_sql(prefix: &str, cols: &[&str]) -> String {
    cols.iter()
        .map(|c| if prefix.is_empty() { format!("\"{c}\"") } else { format!("{prefix}.\"{c}\"") })
        .collect::<Vec<_>>()
        .join(" || char(31) || ")
}

fn log_sql(table: &str, key: &str, deleted: bool) -> String {
    format!(
        "UPDATE sync_clock SET hlc = max(hlc + 1, CAST(unixepoch('subsec') * 1000 AS INTEGER));
         INSERT INTO sync_log (tbl, key, hlc, device, deleted) VALUES ('{table}', {key}, (SELECT hlc FROM sync_clock), '', {})
           ON CONFLICT (tbl, key) DO UPDATE SET hlc = excluded.hlc, device = '', deleted = excluded.deleted;",
        deleted as i32
    )
}

/// Install the change-tracking triggers (idempotent) and record rows that predate them.
pub fn install(conn: &Connection) -> AppResult<()> {
    let guard = "WHEN (SELECT applying FROM sync_clock) = 0";
    let mut sql = String::new();
    for s in TABLES {
        let (t, new, old) = (s.table, key_sql("NEW", s.key), key_sql("OLD", s.key));
        let upd = match s.only {
            Some(cols) => format!("UPDATE OF {} ", cols.join(", ")),
            None => "UPDATE ".into(),
        };
        // A row whose key changed: the old key is gone.
        let moved = format!(
            "INSERT INTO sync_log (tbl, key, hlc, device, deleted) SELECT '{t}', {old}, (SELECT hlc FROM sync_clock) + 1, '', 1 WHERE {old} IS NOT {new}
               ON CONFLICT (tbl, key) DO UPDATE SET hlc = excluded.hlc, device = '', deleted = 1;"
        );
        sql.push_str(&format!(
            "CREATE TRIGGER IF NOT EXISTS amsync_{t}_ins AFTER INSERT ON {t} {guard} BEGIN {} END;
             CREATE TRIGGER IF NOT EXISTS amsync_{t}_upd AFTER {upd}ON {t} {guard} BEGIN {} {} END;
             CREATE TRIGGER IF NOT EXISTS amsync_{t}_del AFTER DELETE ON {t} {guard} BEGIN {} END;",
            log_sql(t, &new, false),
            moved,
            log_sql(t, &new, false),
            log_sql(t, &old, true),
        ));
        for c in s.children.iter().filter(|c| !c.fts) {
            let (ct, fk) = (c.table, c.fk);
            for (op, row) in [("ins", "NEW"), ("upd", "NEW"), ("del", "OLD")] {
                let when = match op {
                    "ins" => "INSERT",
                    "upd" => "UPDATE",
                    _ => "DELETE",
                };
                sql.push_str(&format!(
                    "CREATE TRIGGER IF NOT EXISTS amsync_{ct}_{op} AFTER {when} ON {ct} {guard} AND EXISTS (SELECT 1 FROM {t} WHERE id = {row}.{fk}) BEGIN {} END;",
                    log_sql(t, &format!("{row}.{fk}"), false)
                ));
            }
        }
    }
    conn.execute_batch(&sql)?;
    if conn.query_row("SELECT seeded FROM sync_clock", [], |r| r.get::<_, i64>(0))? == 0 {
        for s in TABLES {
            conn.execute(
                &format!(
                    "INSERT OR IGNORE INTO sync_log (tbl, key, hlc, device, deleted) SELECT '{}', {}, 0, '', 0 FROM {}",
                    s.table,
                    key_sql("", s.key),
                    s.table
                ),
                [],
            )?;
        }
        conn.execute("UPDATE sync_clock SET seeded = 1", [])?;
    }
    Ok(())
}

// ---------------------------------------------------------------- configuration

/// This computer's sync settings. Kept in the data folder (not in the database), so a backup
/// restored onto another computer doesn't make it think it's this one.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Config {
    pub folder: PathBuf,
    pub device_id: String,
    pub device_name: String,
    /// The group key (hex). The database next to it isn't encrypted either; the key protects
    /// the shared folder, which may be in someone else's cloud.
    key: String,
    #[serde(default = "yes")]
    pub auto: bool,
    #[serde(default)]
    pub last: Option<Report>,
}
fn yes() -> bool {
    true
}

fn config_path(paths: &Paths) -> PathBuf {
    paths.root.join("sync.json")
}

pub fn load_config(paths: &Paths) -> Option<Config> {
    serde_json::from_slice(&std::fs::read(config_path(paths)).ok()?).ok()
}

fn save_config(paths: &Paths, c: &Config) -> AppResult<()> {
    let p = config_path(paths);
    let tmp = p.with_extension("json.partial");
    std::fs::write(&tmp, serde_json::to_vec_pretty(c)?)?;
    std::fs::rename(tmp, p)?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Group {
    format: String,
    version: i64,
    salt: String,
    check: String,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn unhex(s: &str) -> Option<Vec<u8>> {
    s.len()
        .is_multiple_of(2)
        .then(|| (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect::<Option<Vec<_>>>())?
}

fn random<const N: usize>() -> AppResult<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|e| AppError::Other(format!("No secure random numbers available: {e}")))?;
    Ok(b)
}

fn seal(key: &[u8], plain: &[u8]) -> AppResult<Vec<u8>> {
    let nonce = random::<12>()?;
    let ct = ChaCha20Poly1305::new(Key::from_slice(key))
        .encrypt(Nonce::from_slice(&nonce), plain)
        .map_err(|_| AppError::Other("Encryption failed.".into()))?;
    Ok([MAGIC, &nonce, &ct].concat())
}

fn open_sealed(key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < MAGIC.len() + 28 || &data[..MAGIC.len()] != MAGIC {
        return None;
    }
    let (nonce, ct) = data[MAGIC.len()..].split_at(12);
    ChaCha20Poly1305::new(Key::from_slice(key)).decrypt(Nonce::from_slice(nonce), ct).ok()
}

fn derive(passphrase: &str, salt: &[u8]) -> AppResult<Vec<u8>> {
    let mut out = [0u8; 32];
    argon2::Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut out)
        .map_err(|e| AppError::Other(format!("Key derivation failed: {e}")))?;
    Ok(out.to_vec())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let tmp = path.with_extension("partial");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

/// Start syncing this computer with `folder`. The first computer creates the group with the
/// passphrase; the others join with the same passphrase.
pub fn setup(paths: &Paths, folder: &Path, passphrase: &str, device_name: &str) -> AppResult<Config> {
    if !folder.is_dir() {
        return Err(AppError::invalid("folder", "Choose an existing folder."));
    }
    if folder.starts_with(&paths.root) {
        return Err(AppError::invalid("folder", "Choose a folder outside Auto Manager's own data folder."));
    }
    if passphrase.chars().count() < 10 {
        return Err(AppError::invalid("passphrase", "Use a passphrase of at least 10 characters."));
    }
    let group_file = folder.join("auto-manager-sync.json");
    let key = match std::fs::read(&group_file) {
        Ok(bytes) => {
            let g: Group =
                serde_json::from_slice(&bytes).map_err(|_| AppError::msg("That folder has a sync file Auto Manager can't read."))?;
            if g.format != FORMAT || g.version != 1 {
                return Err(AppError::msg("That folder is used by a different version of Auto Manager's sync."));
            }
            let key = derive(passphrase, &unhex(&g.salt).ok_or_else(|| AppError::msg("The sync file is damaged."))?)?;
            let check = unhex(&g.check).and_then(|c| open_sealed(&key, &c));
            if check.as_deref() != Some(CHECK) {
                return Err(AppError::invalid("passphrase", "That isn't the passphrase this folder was set up with."));
            }
            key
        }
        Err(_) => {
            let salt = random::<16>()?;
            let key = derive(passphrase, &salt)?;
            let g = Group { format: FORMAT.into(), version: 1, salt: hex(&salt), check: hex(&seal(&key, CHECK)?) };
            write_atomic(&group_file, &serde_json::to_vec_pretty(&g)?)?;
            key
        }
    };
    std::fs::create_dir_all(folder.join("devices"))?;
    std::fs::create_dir_all(folder.join("files"))?;
    let previous = load_config(paths);
    let c = Config {
        folder: folder.to_path_buf(),
        device_id: previous.map(|p| p.device_id).unwrap_or_else(db::new_id),
        device_name: Some(db::clean(device_name)).filter(|n| !n.is_empty()).unwrap_or_else(|| "This computer".into()),
        key: hex(&key),
        auto: true,
        last: None,
    };
    save_config(paths, &c)?;
    Ok(c)
}

pub fn stop(paths: &Paths) -> AppResult<()> {
    match std::fs::remove_file(config_path(paths)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

pub fn set_auto(paths: &Paths, auto: bool) -> AppResult<()> {
    let mut c = load_config(paths).ok_or_else(|| AppError::msg("Sync isn't set up on this computer."))?;
    c.auto = auto;
    save_config(paths, &c)
}

// ---------------------------------------------------------------- state files

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Entry {
    t: String,
    k: String,
    v: i64,
    d: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    x: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    r: Option<Map<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    c: Option<BTreeMap<String, Vec<Map<String, Value>>>>,
}

#[derive(Serialize, Deserialize)]
struct State {
    format: String,
    schema: i64,
    device: String,
    name: String,
    written_at: String,
    entries: Vec<Entry>,
}

fn to_json(v: ValueRef) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(n) => n.into(),
        ValueRef::Real(f) => f.into(),
        ValueRef::Text(s) => String::from_utf8_lossy(s).into(),
        ValueRef::Blob(b) => hex(b).into(),
    }
}

fn to_sql(v: &Value) -> SqlValue {
    match v {
        Value::Null => SqlValue::Null,
        Value::Bool(b) => SqlValue::Integer(*b as i64),
        Value::Number(n) => n.as_i64().map(SqlValue::Integer).unwrap_or_else(|| SqlValue::Real(n.as_f64().unwrap_or_default())),
        Value::String(s) => SqlValue::Text(s.clone()),
        other => SqlValue::Text(other.to_string()),
    }
}

fn rows(conn: &Connection, table: &str, skip: &[&str]) -> AppResult<Vec<Map<String, Value>>> {
    let mut st = conn.prepare(&format!("SELECT * FROM {table}"))?;
    let cols: Vec<String> = st.column_names().map(|c| c.to_string()).collect();
    let out = st.query_map([], |r| {
        let mut m = Map::new();
        for (i, c) in cols.iter().enumerate() {
            if !skip.contains(&c.as_str()) {
                m.insert(c.clone(), to_json(r.get_ref(i)?));
            }
        }
        Ok(m)
    })?;
    Ok(out.collect::<Result<_, _>>()?)
}

fn key_of(m: &Map<String, Value>, cols: &[&str]) -> String {
    cols.iter()
        .map(|c| match m.get(*c) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Null) | None => String::new(),
            Some(v) => v.to_string(),
        })
        .collect::<Vec<_>>()
        .join(&SEP.to_string())
}

fn snapshot(conn: &Connection, me: &str) -> AppResult<Vec<Entry>> {
    let mut out = Vec::new();
    for s in TABLES {
        let mut by_key: HashMap<String, Map<String, Value>> =
            rows(conn, s.table, s.skip)?.into_iter().map(|m| (key_of(&m, s.key), m)).collect();
        let mut kids: HashMap<String, BTreeMap<String, Vec<Map<String, Value>>>> = HashMap::new();
        for c in s.children {
            let list = if c.fts { rows_fts(conn, c.table)? } else { rows(conn, c.table, c.skip)? };
            for m in list {
                let parent = key_of(&m, &[c.fk]);
                kids.entry(parent).or_default().entry(c.table.to_string()).or_default().push(m);
            }
        }
        let mut st = conn.prepare("SELECT key, hlc, device, deleted FROM sync_log WHERE tbl = ?1")?;
        let log = st.query_map([s.table], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)? == 1))
        })?;
        for l in log {
            let (k, v, d, deleted) = l?;
            let r = by_key.remove(&k);
            let x = deleted || r.is_none();
            let c = if x || s.children.is_empty() { None } else { Some(kids.remove(&k).unwrap_or_default()) };
            out.push(Entry { t: s.table.into(), k, v, d: if d.is_empty() { me.into() } else { d }, x, r: if x { None } else { r }, c });
        }
    }
    Ok(out)
}

fn rows_fts(conn: &Connection, table: &str) -> AppResult<Vec<Map<String, Value>>> {
    let mut st = conn.prepare(&format!("SELECT manual_id, page, text FROM {table}"))?;
    let out = st.query_map([], |r| {
        let mut m = Map::new();
        m.insert("manual_id".into(), to_json(r.get_ref(0)?));
        m.insert("page".into(), to_json(r.get_ref(1)?));
        m.insert("text".into(), to_json(r.get_ref(2)?));
        Ok(m)
    })?;
    Ok(out.collect::<Result<_, _>>()?)
}

fn pack(key: &[u8], st: &State) -> AppResult<Vec<u8>> {
    let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&serde_json::to_vec(st)?)?;
    seal(key, &z.finish()?)
}

fn unpack(key: &[u8], data: &[u8]) -> Option<State> {
    let plain = open_sealed(key, data)?;
    let mut json = Vec::new();
    flate2::read::DeflateDecoder::new(&plain[..]).take(2 << 30).read_to_end(&mut json).ok()?;
    serde_json::from_slice(&json).ok()
}

// ---------------------------------------------------------------- merge

#[derive(Serialize, Deserialize, TS, Clone, Debug, Default)]
#[ts(export, rename = "SyncReport")]
pub struct Report {
    pub at: String,
    /// Rows taken from other computers.
    pub received: i64,
    /// Other computers whose files were read (by name).
    pub from: Vec<String>,
    /// Things people should know about: merged duplicates, renumbered invoices, stock corrections.
    pub notes: Vec<String>,
    /// Files that couldn't be used this time (another version, being written, wrong key).
    pub skipped: Vec<String>,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct SyncPeer {
    pub name: String,
    pub written_at: String,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct SyncStatus {
    pub enabled: bool,
    pub folder: Option<String>,
    pub device_name: Option<String>,
    pub auto: bool,
    pub last: Option<Report>,
    pub peers: Vec<SyncPeer>,
}

pub fn status(paths: &Paths) -> SyncStatus {
    let Some(c) = load_config(paths) else {
        return SyncStatus { enabled: false, folder: None, device_name: None, auto: false, last: None, peers: vec![] };
    };
    let key = unhex(&c.key).unwrap_or_default();
    let peers = peer_files(&c)
        .into_iter()
        .filter_map(|p| unpack(&key, &std::fs::read(p).ok()?))
        .map(|s| SyncPeer { name: s.name, written_at: s.written_at })
        .collect();
    SyncStatus {
        enabled: true,
        folder: Some(c.folder.to_string_lossy().into()),
        device_name: Some(c.device_name.clone()),
        auto: c.auto,
        last: c.last,
        peers,
    }
}

fn peer_files(c: &Config) -> Vec<PathBuf> {
    let mine = format!("{}.amstate", c.device_id);
    std::fs::read_dir(c.folder.join("devices"))
        .map(|d| {
            d.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "amstate") && p.file_name().is_some_and(|n| n != mine.as_str()))
                .collect()
        })
        .unwrap_or_default()
}

type Ver = (i64, String);

#[derive(PartialEq, Clone, Copy)]
enum Placed {
    /// In as the other computer sent it.
    AsSent,
    /// In, but changed here (a new number, say); recorded as this computer's change.
    Changed,
    /// Left out: another row is the same thing.
    Dropped,
}

struct Merge<'a> {
    conn: &'a Connection,
    me: &'a str,
    cols: HashMap<String, HashSet<String>>,
    /// (child table, column) → parent table, for rewriting merged-away ids.
    fks: Vec<(String, String, String, String)>,
    deleted: HashMap<(String, String), Map<String, Value>>,
    notes: Vec<String>,
    touched_stock: bool,
    /// Items whose movements came from another computer (their costs are replayed).
    moved: HashSet<String>,
}

/// Is a version from another computer newer than ours?
fn newer(remote: &Ver, local: Option<&Ver>) -> bool {
    match local {
        None => true,
        Some(l) => (remote.0, &remote.1) > (l.0, &l.1),
    }
}

impl<'a> Merge<'a> {
    fn new(conn: &'a Connection, me: &'a str) -> AppResult<Self> {
        let mut cols = HashMap::new();
        let mut fks = Vec::new();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for t in &tables {
            let c: HashSet<String> =
                conn.prepare(&format!("PRAGMA table_info({t})"))?.query_map([], |r| r.get(1))?.collect::<Result<_, _>>()?;
            cols.insert(t.clone(), c);
            let mut st = conn.prepare(&format!("PRAGMA foreign_key_list({t})"))?;
            for fk in st.query_map([], |r| Ok((r.get::<_, String>(3)?, r.get::<_, String>(2)?, r.get::<_, String>(6)?)))? {
                let (from, parent, on_delete) = fk?;
                fks.push((t.clone(), from, parent, on_delete));
            }
        }
        Ok(Merge { conn, me, cols, fks, deleted: HashMap::new(), notes: Vec::new(), touched_stock: false, moved: HashSet::new() })
    }

    fn applying(&self, on: bool) -> AppResult<()> {
        self.conn.execute("UPDATE sync_clock SET applying = ?1", [on as i64])?;
        Ok(())
    }

    fn alias(&self, table: &str, id: &str) -> AppResult<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT to_id FROM sync_aliases WHERE tbl = ?1 AND from_id = ?2", params![table, id], |r| r.get(0))
            .optional()?)
    }

    /// Point references at rows that were merged into another.
    fn rewrite(&self, table: &str, m: &mut Map<String, Value>) -> AppResult<()> {
        for (t, col, parent, _) in &self.fks {
            if t == table
                && let Some(Value::String(v)) = m.get(col)
                && let Some(to) = self.alias(parent, v)?
            {
                m.insert(col.clone(), to.into());
            }
        }
        Ok(())
    }

    fn set_version(&self, table: &str, key: &str, v: &Ver, deleted: bool) -> AppResult<()> {
        let device = if v.1 == self.me { "" } else { v.1.as_str() };
        self.conn.execute(
            "INSERT INTO sync_log (tbl, key, hlc, device, deleted) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (tbl, key) DO UPDATE SET hlc = excluded.hlc, device = excluded.device, deleted = excluded.deleted",
            params![table, key, v.0, device, deleted],
        )?;
        Ok(())
    }

    fn insert(&self, s: &Spec, m: &Map<String, Value>) -> rusqlite::Result<usize> {
        let have = &self.cols[s.table];
        let names: Vec<&String> = m
            .keys()
            .filter(|c| have.contains(*c) && !s.skip.contains(&c.as_str()))
            .filter(|c| s.only.is_none_or(|o| o.contains(&c.as_str()) || s.key.contains(&c.as_str())))
            .collect();
        let updates: Vec<String> =
            names.iter().filter(|c| !s.key.contains(&c.as_str())).map(|c| format!("\"{c}\" = excluded.\"{c}\"")).collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT ({}) DO {}",
            s.table,
            names.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", "),
            (1..=names.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", "),
            s.key.join(", "),
            if updates.is_empty() { "NOTHING".into() } else { format!("UPDATE SET {}", updates.join(", ")) }
        );
        let vals: Vec<SqlValue> = names.iter().map(|c| to_sql(&m[c.as_str()])).collect();
        self.conn.execute(&sql, rusqlite::params_from_iter(vals))
    }

    fn row_of(&self, s: &Spec, key: &str) -> AppResult<Option<Map<String, Value>>> {
        let parts: Vec<&str> = key.split(SEP).collect();
        let cond = s.key.iter().enumerate().map(|(i, c)| format!("\"{c}\" = ?{}", i + 1)).collect::<Vec<_>>().join(" AND ");
        let mut st = self.conn.prepare(&format!("SELECT * FROM {} WHERE {cond}", s.table))?;
        let names: Vec<String> = st.column_names().map(|c| c.to_string()).collect();
        Ok(st
            .query_row(rusqlite::params_from_iter(parts.iter()), |r| {
                let mut m = Map::new();
                for (i, c) in names.iter().enumerate() {
                    m.insert(c.clone(), to_json(r.get_ref(i)?));
                }
                Ok(m)
            })
            .optional()?)
    }

    fn delete(&mut self, s: &Spec, key: &str) -> AppResult<()> {
        if let Some(m) = self.row_of(s, key)? {
            self.deleted.insert((s.table.into(), key.into()), m);
        }
        let parts: Vec<&str> = key.split(SEP).collect();
        let cond = s.key.iter().enumerate().map(|(i, c)| format!("\"{c}\" = ?{}", i + 1)).collect::<Vec<_>>().join(" AND ");
        self.conn.execute(&format!("DELETE FROM {} WHERE {cond}", s.table), rusqlite::params_from_iter(parts.iter()))?;
        Ok(())
    }

    fn children(&self, s: &Spec, key: &str, c: &BTreeMap<String, Vec<Map<String, Value>>>) -> AppResult<()> {
        for ch in s.children {
            self.conn.execute(&format!("DELETE FROM {} WHERE {} = ?1", ch.table, ch.fk), [key])?;
            for m in c.get(ch.table).into_iter().flatten() {
                let names: Vec<&String> =
                    m.keys().filter(|k| !ch.skip.contains(&k.as_str()) && (ch.fts || self.cols[ch.table].contains(*k))).collect();
                let sql = format!(
                    "INSERT INTO {} ({}) VALUES ({})",
                    ch.table,
                    names.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", "),
                    (1..=names.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ")
                );
                let mut vals: Vec<SqlValue> = names.iter().map(|n| to_sql(&m[n.as_str()])).collect();
                if let Some(i) = names.iter().position(|n| n.as_str() == ch.fk) {
                    vals[i] = SqlValue::Text(key.into());
                }
                self.conn.execute(&sql, rusqlite::params_from_iter(vals))?;
            }
        }
        Ok(())
    }

    fn version(&self, table: &str, key: &str) -> AppResult<Option<Ver>> {
        Ok(self
            .conn
            .query_row("SELECT hlc, device FROM sync_log WHERE tbl = ?1 AND key = ?2", params![table, key], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })
            .optional()?
            .map(|(h, d)| (h, if d.is_empty() { self.me.to_string() } else { d })))
    }

    /// Apply one row from another computer. Returns whether anything changed.
    fn apply(&mut self, e: &Entry) -> AppResult<bool> {
        let Some(s) = spec(&e.t) else { return Ok(false) };
        if s.key == ID && self.alias(s.table, &e.k)?.is_some() {
            return Ok(false); // merged into another row
        }
        let v: Ver = (e.v, e.d.clone());
        if e.x {
            let k = e.k.clone();
            if !newer(&v, self.version(s.table, &k)?.as_ref()) {
                return Ok(false);
            }
            self.delete(s, &k)?;
            self.set_version(s.table, &k, &v, true)?;
            return Ok(true);
        }
        let mut m = e.r.clone().unwrap_or_default();
        self.rewrite(s.table, &mut m)?;
        let k = key_of(&m, s.key);
        if !newer(&v, self.version(s.table, &k)?.as_ref()) {
            return Ok(false);
        }
        let placed = match self.insert(s, &m) {
            Ok(_) => Placed::AsSent,
            Err(err) => match unique_columns(&err) {
                Some((t, cols)) if t == s.table => self.resolve_unique(s, &k, &mut m, &cols, &v)?,
                _ => return Err(err.into()),
            },
        };
        if placed == Placed::Dropped {
            return Ok(false);
        }
        if let Some(c) = &e.c {
            self.children(s, &k, c)?;
        }
        if placed == Placed::AsSent {
            self.set_version(s.table, &k, &v, false)?;
        }
        Ok(true)
    }

    /// Record a change made here by the merge itself, so other computers take it too.
    fn log_local(&self, table: &str, key: &str, deleted: bool) -> AppResult<()> {
        self.conn.execute("UPDATE sync_clock SET hlc = max(hlc + 1, CAST(unixepoch('subsec') * 1000 AS INTEGER))", [])?;
        self.conn.execute(
            "INSERT INTO sync_log (tbl, key, hlc, device, deleted) VALUES (?1, ?2, (SELECT hlc FROM sync_clock), '', ?3)
             ON CONFLICT (tbl, key) DO UPDATE SET hlc = excluded.hlc, device = '', deleted = excluded.deleted",
            params![table, key, deleted],
        )?;
        Ok(())
    }

    /// Two different rows claim the same unique value.
    fn resolve_unique(&mut self, s: &Spec, key: &str, m: &mut Map<String, Value>, cols: &[String], v: &Ver) -> AppResult<Placed> {
        let cond = cols.iter().enumerate().map(|(i, c)| format!("\"{c}\" IS ?{}", i + 1)).collect::<Vec<_>>().join(" AND ");
        let vals: Vec<SqlValue> = cols.iter().map(|c| to_sql(m.get(c).unwrap_or(&Value::Null))).collect();
        let local: Option<String> = self
            .conn
            .query_row(&format!("SELECT id FROM {} WHERE {cond}", s.table), rusqlite::params_from_iter(vals), |r| r.get(0))
            .optional()?;
        let Some(local) = local else { return Err(AppError::Other(format!("Sync couldn't place a row in {}.", s.table))) };
        let col = cols.join(", ");
        // Default flags: the more recently changed default wins.
        if col == "is_default" {
            if newer(v, self.version(s.table, &local)?.as_ref()) {
                self.conn.execute(&format!("UPDATE {} SET is_default = 0 WHERE id = ?1", s.table), [&local])?;
                self.log_local(s.table, &local, false)?;
                self.insert(s, m)?;
                return Ok(Placed::AsSent);
            }
            m.insert("is_default".into(), 0.into());
            self.insert(s, m)?;
            self.log_local(s.table, key, false)?;
            return Ok(Placed::Changed);
        }
        // Numbers people see on paper: the later row (larger id) takes the next free number.
        if NUMBERS.contains(&(s.table, col.as_str())) {
            let next: i64 = self.conn.query_row(&format!("SELECT coalesce(max({col}), 1000) + 1 FROM {}", s.table), [], |r| r.get(0))?;
            let old = m.get(&col).cloned().unwrap_or_default();
            let what = if s.table == "purchase_orders" {
                "Purchase order"
            } else if col == "invoice_number" {
                "Invoice"
            } else {
                "Job"
            };
            self.notes.push(format!("{what} number {old} was used on two computers; the later one is now {next}."));
            if key > local.as_str() {
                m.insert(col.clone(), next.into());
                self.insert(s, m)?;
                self.log_local(s.table, key, false)?;
                return Ok(Placed::Changed);
            }
            self.conn.execute(&format!("UPDATE {} SET {col} = ?2 WHERE id = ?1", s.table), params![local, next])?;
            self.log_local(s.table, &local, false)?;
            self.insert(s, m)?;
            return Ok(Placed::AsSent);
        }
        // The same thing (or a clash with no better rule): one row survives, the older id.
        self.conn.execute(
            "INSERT OR REPLACE INTO sync_aliases (tbl, from_id, to_id) VALUES (?1, ?2, ?3)",
            params![s.table, key.max(local.as_str()), key.min(local.as_str())],
        )?;
        if SAME_THING.iter().any(|(t, c)| *t == s.table && cols.iter().any(|x| x == c)) {
            let label = m.get(cols[cols.len() - 1].as_str()).and_then(Value::as_str).unwrap_or_default().to_string();
            let what = match s.table {
                "vehicles" => format!("The vehicle with VIN {label}"),
                "manuals" => "A document".into(),
                "inventory_items" => format!("Inventory item {label}"),
                "inventory_locations" => format!("Location “{label}”"),
                "pricing_profiles" => format!("Pricing profile “{label}”"),
                _ => format!("Category “{label}”"),
            };
            self.notes.push(format!("{what} was added on two computers; they're now one record."));
        }
        if s.table == "inventory_items" {
            self.touched_stock = true;
        }
        if key < local.as_str() {
            // The incoming row is older: move our references to it, then replace ours.
            self.repoint(s.table, &local, key)?;
            self.delete(s, &local)?;
            self.log_local(s.table, &local, true)?;
            self.insert(s, m)?;
            Ok(Placed::AsSent)
        } else {
            // Ours is older: keep it, and drop the incoming one on every computer.
            self.log_local(s.table, key, true)?;
            Ok(Placed::Dropped)
        }
    }

    /// Move every reference from `from` to `to` (rows that would duplicate are dropped). These
    /// are this computer's changes, so the triggers record them.
    fn repoint(&self, parent: &str, from: &str, to: &str) -> AppResult<()> {
        self.applying(false)?;
        for (t, col, p, _) in &self.fks {
            if p == parent {
                self.conn.execute(&format!("UPDATE OR IGNORE {t} SET \"{col}\" = ?2 WHERE \"{col}\" = ?1"), params![from, to])?;
                self.conn.execute(&format!("DELETE FROM {t} WHERE \"{col}\" = ?1"), [from])?;
            }
        }
        self.applying(true)
    }

    /// Rows pointing at rows that no longer exist: do what the database would have done on
    /// delete (clear the reference or remove the row), or bring back a protected parent.
    fn fix_references(&mut self) -> AppResult<()> {
        for _ in 0..10 {
            let bad: Vec<(String, i64, String, i64)> = self
                .conn
                .prepare("PRAGMA foreign_key_check")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<Result<_, _>>()?;
            if bad.is_empty() {
                return Ok(());
            }
            for (table, rowid, parent, fkid) in bad {
                let (col, on_delete): (String, String) = self.conn.query_row(
                    &format!("SELECT \"from\", on_delete FROM pragma_foreign_key_list('{table}') WHERE id = ?1"),
                    [fkid],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                let value: Option<String> = self
                    .conn
                    .query_row(&format!("SELECT \"{col}\" FROM {table} WHERE rowid = ?1"), [rowid], |r| r.get(0))
                    .optional()?
                    .flatten();
                let restore = value.as_ref().and_then(|v| self.deleted.remove(&(parent.clone(), v.clone())));
                self.applying(false)?;
                match (on_delete.as_str(), restore) {
                    (_, Some(m)) if !matches!(on_delete.as_str(), "CASCADE" | "SET NULL") => {
                        // A part (say) was deleted on one computer while another started using it.
                        let s = spec(&parent).ok_or_else(|| AppError::Other(format!("Sync can't restore {parent}.")))?;
                        self.insert(s, &m)?;
                        self.notes.push(format!(
                            "A deleted {} was kept because another computer still uses it.",
                            parent.trim_end_matches('s').replace('_', " ")
                        ));
                    }
                    ("SET NULL", _) => {
                        self.conn.execute(&format!("UPDATE {table} SET \"{col}\" = NULL WHERE rowid = ?1"), [rowid])?;
                    }
                    ("CASCADE", _) => {
                        self.conn.execute(&format!("DELETE FROM {table} WHERE rowid = ?1"), [rowid])?;
                    }
                    _ => {
                        // Its parent hasn't arrived yet: leave it out now and take it next time.
                        self.applying(true)?;
                        if let Some(s) = spec(&table) {
                            let m = self.conn.query_row(
                                &format!("SELECT {} FROM {table} WHERE rowid = ?1", key_sql("", s.key)),
                                [rowid],
                                |r| r.get::<_, String>(0),
                            )?;
                            self.conn.execute("DELETE FROM sync_log WHERE tbl = ?1 AND key = ?2", params![table, m])?;
                        }
                        self.conn.execute(&format!("DELETE FROM {table} WHERE rowid = ?1"), [rowid])?;
                    }
                }
                self.applying(true)?;
            }
        }
        Err(AppError::Other("Sync couldn't settle references between computers.".into()))
    }

    /// Stock levels follow the movement ledger and active reservations. Logged as this
    /// computer's changes (so every computer agrees) only where something had to be corrected.
    fn settle_stock(&mut self) -> AppResult<()> {
        self.applying(false)?;
        let names: HashMap<String, String> = self
            .conn
            .prepare("SELECT i.id, i.sku FROM inventory_items i")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        let sums: Vec<(String, String, i64)> = self
            .conn
            .prepare("SELECT item_id, location_id, sum(qty) FROM inventory_movements GROUP BY 1, 2")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?;
        for (item, loc, sum) in &sums {
            if *sum < 0 {
                let ids: Vec<String> = self
                    .conn
                    .prepare("SELECT id FROM inventory_movements WHERE item_id = ?1 AND location_id = ?2 ORDER BY id")?
                    .query_map(params![item, loc], |r| r.get(0))?
                    .collect::<Result<_, _>>()?;
                let id = db::stable_id(&["sync-correction", item, loc, &ids.join(",")]);
                self.conn.execute(
                    "INSERT OR IGNORE INTO inventory_movements (id, item_id, location_id, kind, qty, reason, created_at)
                     VALUES (?1, ?2, ?3, 'adjust', ?4, 'Sync conflict: two computers took stock that was only there once. Count this item.', ?5)",
                    params![id, item, loc, -sum, db::now_utc()],
                )?;
                self.notes.push(format!(
                    "Two computers took the same stock of {} ({}); it was set to 0. Count it.",
                    names.get(item).map(String::as_str).unwrap_or("an item"),
                    crate::qty::Qty::from_milli(-sum)
                ));
            }
        }
        // Over-reserved: release the newest reservations first.
        self.conn.execute_batch(
            "INSERT OR IGNORE INTO stock_levels (item_id, location_id) SELECT DISTINCT item_id, location_id FROM inventory_movements;
             INSERT OR IGNORE INTO stock_levels (item_id, location_id) SELECT DISTINCT item_id, location_id FROM stock_reservations WHERE status = 'active';",
        )?;
        loop {
            let over: Option<String> = self
                .conn
                .query_row(
                    "SELECT r.id FROM stock_reservations r
                     WHERE r.status = 'active' AND (SELECT coalesce(sum(qty), 0) FROM stock_reservations x WHERE x.item_id = r.item_id AND x.location_id = r.location_id AND x.status = 'active')
                         > (SELECT coalesce(sum(qty), 0) FROM inventory_movements m WHERE m.item_id = r.item_id AND m.location_id = r.location_id)
                     ORDER BY r.created_at DESC, r.id DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(id) = over else { break };
            self.conn
                .execute("UPDATE stock_reservations SET status = 'released', closed_at = ?2 WHERE id = ?1", params![id, db::now_utc()])?;
            self.notes.push("Two computers reserved the same stock; the newer reservation was released.".into());
        }
        self.conn.execute_batch(
            "UPDATE stock_levels SET
               on_hand = (SELECT coalesce(sum(qty), 0) FROM inventory_movements m WHERE m.item_id = stock_levels.item_id AND m.location_id = stock_levels.location_id),
               reserved = (SELECT coalesce(sum(qty), 0) FROM stock_reservations r WHERE r.item_id = stock_levels.item_id AND r.location_id = stock_levels.location_id AND r.status = 'active');",
        )?;
        self.settle_costs()?;
        self.applying(true)
    }

    /// Last and moving-average cost, replayed from the ledger (what receiving did on each
    /// computer, in time order). Only rows that differ are written.
    fn settle_costs(&self) -> AppResult<()> {
        use rust_decimal::Decimal;
        for id in &self.moved {
            let Some((cost, avg)): Option<(String, String)> = self
                .conn
                .query_row("SELECT cost, avg_cost FROM inventory_items WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?
            else {
                continue;
            };
            let moves: Vec<(String, crate::qty::Qty, Option<String>)> = self
                .conn
                .prepare("SELECT kind, qty, unit_cost FROM inventory_movements WHERE item_id = ?1 ORDER BY created_at, id")?
                .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<_, _>>()?;
            if !moves.iter().any(|m| m.0 == "receive") {
                continue;
            }
            let (mut total, mut a, mut last) = (crate::qty::Qty::ZERO, Decimal::ZERO, Decimal::ZERO);
            for (kind, q, c) in moves {
                if kind == "receive" {
                    let c: Decimal = c.and_then(|c| c.parse().ok()).unwrap_or_default();
                    a = if total <= 0 { c } else { ((a * total.dec() + c * q.dec()) / (total + q).dec()).round_dp(6) };
                    last = c;
                }
                total += q;
            }
            let (a, last) = (a.normalize().to_string(), last.to_string());
            if a != avg || last != cost {
                self.conn.execute("UPDATE inventory_items SET cost = ?2, avg_cost = ?3 WHERE id = ?1", params![id, last, a])?;
            }
        }
        Ok(())
    }
}

/// "UNIQUE constraint failed: t.a, t.b" → (t, [a, b]).
fn unique_columns(e: &rusqlite::Error) -> Option<(String, Vec<String>)> {
    let msg = e.to_string();
    let rest = msg.strip_prefix("UNIQUE constraint failed: ")?;
    let mut table = String::new();
    let cols = rest
        .split(", ")
        .filter_map(|tc| {
            let (t, c) = tc.split_once('.')?;
            table = t.to_string();
            Some(c.to_string())
        })
        .collect::<Vec<_>>();
    (!cols.is_empty()).then_some((table, cols))
}

fn file_for(paths: &Paths, table: &str, m: &Map<String, Value>) -> Option<(String, PathBuf)> {
    match table {
        "manuals" => {
            let sha = m.get("file_sha256")?.as_str()?.to_string();
            let p = crate::manuals::file_path(paths, &sha);
            Some((sha, p))
        }
        "attachments" => {
            let sha = m.get("sha256")?.as_str()?.to_string();
            let p = crate::attachments::stored_path(paths, &sha, m.get("mime")?.as_str()?);
            Some((sha, p))
        }
        _ => None,
    }
}

/// Sync now: take what other computers have, then publish this computer's state.
pub fn sync(core: &Core) -> AppResult<Report> {
    let paths = &core.paths;
    let mut cfg = load_config(paths).ok_or_else(|| AppError::msg("Sync isn't set up on this computer."))?;
    let key = unhex(&cfg.key).ok_or_else(|| AppError::msg("This computer's sync settings are damaged; set sync up again."))?;
    if !cfg.folder.is_dir() {
        return Err(AppError::msg(format!(
            "The sync folder {} isn't available. Connect the drive or check the folder.",
            cfg.folder.display()
        )));
    }
    let schema = db::schema_version();
    let mut report = Report { at: db::now_utc(), ..Default::default() };
    let mut peers = Vec::new();
    for p in peer_files(&cfg) {
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        match std::fs::read(&p).ok().and_then(|b| unpack(&key, &b)) {
            Some(s) if s.format == FORMAT && s.schema == schema => {
                report.from.push(s.name.clone());
                peers.push(s);
            }
            Some(s) if s.schema > schema => {
                report.skipped.push(format!("{} uses a newer Auto Manager. Update this computer to sync with it.", s.name))
            }
            Some(s) => report.skipped.push(format!("{} uses an older Auto Manager. Update it to sync with this computer.", s.name)),
            None => report.skipped.push(format!("{name} couldn't be read (it may still be copying); it'll be tried next time.")),
        }
    }
    // The newest version of every row across the other computers.
    let mut best: HashMap<(String, String), Entry> = HashMap::new();
    let mut max_clock = 0;
    for s in peers {
        for e in s.entries {
            max_clock = max_clock.max(e.v);
            let k = (e.t.clone(), e.k.clone());
            if best.get(&k).is_none_or(|b| (e.v, &e.d) > (b.v, &b.d)) {
                best.insert(k, e);
            }
        }
    }
    let order: HashMap<&str, usize> = TABLES.iter().enumerate().map(|(i, s)| (s.table, i)).collect();
    let mut entries: Vec<Entry> = best.into_values().collect();
    // Parents before children for new rows; children before parents for deletions.
    entries.sort_by_key(|e| {
        if e.x {
            (1, usize::MAX - order.get(e.t.as_str()).copied().unwrap_or(0))
        } else {
            (0, order.get(e.t.as_str()).copied().unwrap_or(0))
        }
    });
    let me = cfg.device_id.clone();
    let folder_files = cfg.folder.join("files");
    let (received, notes) = core.tx(|c| {
        c.execute("UPDATE sync_clock SET hlc = max(hlc, ?1)", [max_clock])?;
        c.pragma_update(None, "defer_foreign_keys", true)?;
        let mut m = Merge::new(c, &me)?;
        m.applying(true)?;
        let mut n = 0;
        let result = (|| {
            for e in &entries {
                if m.apply(e)? {
                    n += 1;
                    if matches!(e.t.as_str(), "inventory_movements" | "stock_reservations" | "inventory_items" | "stock_levels") {
                        m.touched_stock = true;
                    }
                    if e.t == "inventory_movements"
                        && let Some(Value::String(item)) = e.r.as_ref().and_then(|r| r.get("item_id"))
                    {
                        m.moved.insert(item.clone());
                    }
                    if let Some((sha, p)) = e.r.as_ref().and_then(|r| file_for(paths, &e.t, r))
                        && !p.exists()
                    {
                        // Documents and photos travel separately, by content hash.
                        if let Some(bytes) = std::fs::read(folder_files.join(&sha)).ok().and_then(|b| open_sealed(&key, &b)) {
                            write_atomic(&p, &bytes)?;
                        }
                    }
                }
            }
            m.fix_references()?;
            if m.touched_stock {
                m.settle_stock()?;
            }
            m.fix_references()
        })();
        m.applying(false)?;
        result?;
        Ok((n, m.notes))
    })?;
    report.received = received;
    report.notes = notes;
    // Publish this computer's state and any documents the folder doesn't have yet.
    let (entries, files) = core.read(|c| {
        let entries = snapshot(c, &me)?;
        let mut files = Vec::new();
        for e in entries.iter().filter(|e| !e.x) {
            if let Some(f) = e.r.as_ref().and_then(|r| file_for(paths, &e.t, r)) {
                files.push(f);
            }
        }
        Ok((entries, files))
    })?;
    for (sha, p) in files {
        let dest = folder_files.join(&sha);
        if !dest.exists()
            && let Ok(bytes) = std::fs::read(&p)
        {
            write_atomic(&dest, &seal(&key, &bytes)?)?;
        }
    }
    let state =
        State { format: FORMAT.into(), schema, device: me.clone(), name: cfg.device_name.clone(), written_at: db::now_utc(), entries };
    write_atomic(&cfg.folder.join("devices").join(format!("{me}.amstate")), &pack(&key, &state)?)?;
    cfg.last = Some(report.clone());
    save_config(paths, &cfg)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qty::Qty;
    use crate::{customers, inventory as inv, jobs, parts, vehicles};

    struct Pc {
        core: Core,
        _dir: tempfile::TempDir,
    }

    fn pc(folder: &Path, name: &str) -> Pc {
        let dir = tempfile::tempdir().unwrap();
        let core = Core::memory(dir.path().join("data")).unwrap();
        setup(&core.paths, folder, "correct horse battery", name).unwrap();
        Pc { core, _dir: dir }
    }

    fn item(c: &Connection, sku: &str) -> String {
        let p = parts::create(c, &parts::PartInput { name: format!("Part {sku}"), ..Default::default() }, "user").unwrap();
        inv::create(
            c,
            &p.id,
            &inv::ItemInput {
                sku: sku.into(),
                condition: "new".into(),
                unit: "each".into(),
                fractional: false,
                cost: "10".into(),
                price_override: Some("20".into()),
                pricing_profile_id: None,
                min_stock: Qty::ZERO,
                reorder_qty: Qty::ZERO,
                supplier_id: None,
                warranty: String::new(),
                notes: String::new(),
            },
        )
        .unwrap()
        .id
    }

    fn change(item: &str, qty: i64) -> inv::Change {
        inv::Change {
            item_id: item.into(),
            location_id: "loc-main".into(),
            qty: Qty::units(qty),
            reason: String::new(),
            ref_kind: None,
            ref_id: None,
        }
    }

    fn receive(c: &Connection, item: &str, qty: i64) {
        inv::receive(
            c,
            &inv::Receive {
                item_id: item.into(),
                location_id: "loc-main".into(),
                qty: Qty::units(qty),
                unit_cost: None,
                bin: None,
                reason: String::new(),
                ref_kind: None,
                ref_id: None,
            },
        )
        .unwrap();
    }

    fn on_hand(p: &Pc, item: &str) -> Qty {
        p.core.read(|c| Ok(inv::get(c, item)?.on_hand)).unwrap()
    }

    #[test]
    fn quickbooks_links_travel_so_the_other_computer_updates_instead_of_copying() {
        use crate::quickbooks::{self, QbCustomer, QbData, QbOptions};
        let folder = tempfile::tempdir().unwrap();
        let (a, b) = (pc(folder.path(), "Front desk"), pc(folder.path(), "Workshop"));
        let data = |phone: &str| QbData {
            customers: vec![QbCustomer {
                external_id: "58".into(),
                name: "Ada Lovelace".into(),
                phone: phone.into(),
                active: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        a.core.tx(|c| quickbooks::apply(c, "qbo:1", &data("111"), &QbOptions::default())).unwrap();
        sync(&a.core).unwrap();
        sync(&b.core).unwrap();
        // The customer was renamed here; QuickBooks' next refresh on the other computer still finds it.
        b.core.tx(|c| Ok(c.execute("UPDATE customers SET name = 'Ada King'", [])?)).unwrap();
        let s = b.core.tx(|c| quickbooks::apply(c, "qbo:1", &data("222"), &QbOptions::default())).unwrap();
        assert_eq!((s.added, s.updated), (0, 1));
        sync(&b.core).unwrap();
        sync(&a.core).unwrap();
        for p in [&a, &b] {
            let all = p.core.read(|c| customers::list(c, None)).unwrap();
            assert_eq!(all.len(), 1, "one customer on {}", p.core.paths.root.display());
            assert_eq!((all[0].name.as_str(), all[0].phone.as_str()), ("Ada Lovelace", "222"));
        }
    }

    #[test]
    fn edits_travel_both_ways_and_the_latest_wins() {
        let folder = tempfile::tempdir().unwrap();
        let (a, b) = (pc(folder.path(), "Front desk"), pc(folder.path(), "Workshop"));
        let cust =
            a.core.tx(|c| customers::create(c, &customers::CustomerInput { name: "Ada Lovelace".into(), ..Default::default() })).unwrap();
        sync(&a.core).unwrap();
        let r = sync(&b.core).unwrap();
        assert_eq!(r.from, vec!["Front desk"]);
        assert_eq!(b.core.read(|c| customers::get(c, &cust.id)).unwrap().name, "Ada Lovelace");
        // Both edit while apart; the later edit wins on both.
        a.core.tx(|c| Ok(c.execute("UPDATE customers SET phone = '111' WHERE id = ?1", [&cust.id])?)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        b.core.tx(|c| Ok(c.execute("UPDATE customers SET phone = '222' WHERE id = ?1", [&cust.id])?)).unwrap();
        sync(&a.core).unwrap();
        sync(&b.core).unwrap();
        sync(&a.core).unwrap();
        for p in [&a, &b] {
            assert_eq!(p.core.read(|c| customers::get(c, &cust.id)).unwrap().phone, "222");
        }
        // Deletions travel too, and a second round changes nothing.
        b.core.tx(|c| customers::delete(c, &cust.id)).unwrap();
        sync(&b.core).unwrap();
        sync(&a.core).unwrap();
        assert!(a.core.read(|c| customers::get(c, &cust.id)).is_err());
        assert_eq!(sync(&b.core).unwrap().received, 0, "settled");
        assert_eq!(sync(&a.core).unwrap().received, 0, "settled");
    }

    #[test]
    fn stock_taken_twice_is_corrected_once_and_reported() {
        let folder = tempfile::tempdir().unwrap();
        let (a, b) = (pc(folder.path(), "A"), pc(folder.path(), "B"));
        let id = a.core.tx(|c| Ok(item(c, "BP-1"))).unwrap();
        a.core
            .tx(|c| {
                receive(c, &id, 4);
                Ok(())
            })
            .unwrap();
        sync(&a.core).unwrap();
        sync(&b.core).unwrap();
        assert_eq!(on_hand(&b, &id), 4);
        // Both sell 3 of the 4 while out of touch.
        a.core.tx(|c| inv::issue(c, &change(&id, 3))).unwrap();
        b.core.tx(|c| inv::issue(c, &change(&id, 3))).unwrap();
        sync(&a.core).unwrap();
        let rb = sync(&b.core).unwrap();
        let ra = sync(&a.core).unwrap();
        sync(&b.core).unwrap();
        assert!(ra.notes.iter().chain(rb.notes.iter()).any(|n| n.contains("Count it")), "{ra:?} {rb:?}");
        for p in [&a, &b] {
            assert_eq!(on_hand(p, &id), 0, "never negative, and corrected only once");
            let corrections = p
                .core
                .read(|c| {
                    Ok(c.query_row("SELECT count(*) FROM inventory_movements WHERE reason LIKE 'Sync conflict%'", [], |r| {
                        r.get::<_, i64>(0)
                    })?)
                })
                .unwrap();
            assert_eq!(corrections, 1);
        }
    }

    #[test]
    fn same_sku_on_two_computers_becomes_one_item_with_both_stocks() {
        let folder = tempfile::tempdir().unwrap();
        let (a, b) = (pc(folder.path(), "A"), pc(folder.path(), "B"));
        let ia = a.core.tx(|c| Ok(item(c, "OF-1"))).unwrap();
        a.core
            .tx(|c| {
                receive(c, &ia, 2);
                Ok(())
            })
            .unwrap();
        let ib = b.core.tx(|c| Ok(item(c, "OF-1"))).unwrap();
        b.core
            .tx(|c| {
                receive(c, &ib, 5);
                Ok(())
            })
            .unwrap();
        sync(&a.core).unwrap();
        let r = sync(&b.core).unwrap();
        sync(&a.core).unwrap();
        sync(&b.core).unwrap();
        assert!(r.notes.iter().any(|n| n.contains("now one record")), "{r:?}");
        let keep = ia.clone().min(ib.clone());
        for p in [&a, &b] {
            let items = p.core.read(|c| inv::list(c, &inv::ItemQuery::default())).unwrap();
            assert_eq!(items.iter().filter(|i| i.sku == "OF-1").count(), 1);
            assert_eq!(items[0].id, keep);
            assert_eq!(on_hand(p, &keep), 7);
        }
    }

    #[test]
    fn a_job_completed_on_both_computers_takes_stock_once() {
        let folder = tempfile::tempdir().unwrap();
        let (a, b) = (pc(folder.path(), "A"), pc(folder.path(), "B"));
        let (id, job) = a
            .core
            .tx(|c| {
                let id = item(c, "SP-1");
                receive(c, &id, 4);
                let j = jobs::create(c, &jobs::JobInput { title: "Plugs".into(), ..Default::default() })?;
                jobs::add_part_line(c, &j.job.id, &jobs::PartLineInput { item_id: id.clone(), qty: "2".into(), reserve: true }, "average")?;
                jobs::set_status(c, &j.job.id, "approved")?;
                Ok((id, j.job.id))
            })
            .unwrap();
        sync(&a.core).unwrap();
        sync(&b.core).unwrap();
        a.core.tx(|c| jobs::set_status(c, &job, "completed")).unwrap();
        b.core.tx(|c| jobs::set_status(c, &job, "completed")).unwrap();
        sync(&a.core).unwrap();
        sync(&b.core).unwrap();
        sync(&a.core).unwrap();
        for p in [&a, &b] {
            assert_eq!(on_hand(p, &id), 2, "2 of 4 used, once");
        }
    }

    #[test]
    fn references_to_deleted_rows_are_settled() {
        let folder = tempfile::tempdir().unwrap();
        let (a, b) = (pc(folder.path(), "A"), pc(folder.path(), "B"));
        let cust = a.core.tx(|c| customers::create(c, &customers::CustomerInput { name: "Grace".into(), ..Default::default() })).unwrap();
        sync(&a.core).unwrap();
        sync(&b.core).unwrap();
        a.core.tx(|c| customers::delete(c, &cust.id)).unwrap();
        let job = b
            .core
            .tx(|c| jobs::create(c, &jobs::JobInput { title: "Brakes".into(), customer_id: Some(cust.id.clone()), ..Default::default() }))
            .unwrap();
        sync(&a.core).unwrap();
        sync(&b.core).unwrap();
        sync(&a.core).unwrap();
        for p in [&a, &b] {
            let j = p.core.read(|c| jobs::get_job(c, &job.job.id)).unwrap();
            assert_eq!(j.customer_id, None, "as if the customer had been deleted first");
        }
    }

    #[test]
    fn invoice_numbers_used_twice_are_renumbered() {
        let folder = tempfile::tempdir().unwrap();
        let (a, b) = (pc(folder.path(), "A"), pc(folder.path(), "B"));
        let ja = a.core.tx(|c| jobs::create(c, &jobs::JobInput { title: "A job".into(), ..Default::default() })).unwrap();
        let jb = b.core.tx(|c| jobs::create(c, &jobs::JobInput { title: "B job".into(), ..Default::default() })).unwrap();
        assert_eq!(ja.job.number, jb.job.number);
        sync(&a.core).unwrap();
        let r = sync(&b.core).unwrap();
        sync(&a.core).unwrap();
        assert!(r.notes.iter().any(|n| n.contains("used on two computers")), "{r:?}");
        let numbers = |p: &Pc| p.core.read(|c| Ok((jobs::get_job(c, &ja.job.id)?.number, jobs::get_job(c, &jb.job.id)?.number))).unwrap();
        assert_eq!(numbers(&a), numbers(&b));
        assert_ne!(numbers(&a).0, numbers(&a).1);
    }

    #[test]
    fn photos_travel_and_the_folder_is_encrypted() {
        let folder = tempfile::tempdir().unwrap();
        let (a, b) = (pc(folder.path(), "A"), pc(folder.path(), "B"));
        let v = a
            .core
            .tx(|c| {
                vehicles::create_manual(
                    c,
                    &vehicles::ManualVehicle { year: Some(2019), make: "Nissan".into(), model: "Sentra".into(), ..Default::default() },
                )
            })
            .unwrap();
        let png = folder.path().join("..").join(format!("{}.png", db::new_id()));
        std::fs::write(&png, [0x89, b'P', b'N', b'G', 1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let paths = a.core.paths.clone();
        let att = a.core.tx(|c| crate::attachments::add(c, &paths, "vehicle", &v.id, &png, "Front")).unwrap();
        std::fs::remove_file(&png).unwrap();
        sync(&a.core).unwrap();
        sync(&b.core).unwrap();
        let bpaths = b.core.paths.clone();
        assert_eq!(b.core.read(|c| crate::attachments::bytes(c, &bpaths, &att.id)).unwrap()[..4], [0x89, b'P', b'N', b'G']);
        for f in walk(folder.path()) {
            let bytes = std::fs::read(&f).unwrap();
            assert!(
                !String::from_utf8_lossy(&bytes).contains("Sentra") && !bytes.windows(4).any(|w| w == [0x89, b'P', b'N', b'G']),
                "{f:?} is readable"
            );
        }
        // Joining with the wrong passphrase is refused.
        let c = tempfile::tempdir().unwrap();
        assert!(setup(&Paths::new(c.path().join("d")), folder.path(), "wrong passphrase!", "C").is_err());
    }

    fn walk(p: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(p).unwrap().flatten() {
            let path = e.path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else if path.file_name().is_some_and(|n| n != "auto-manager-sync.json") {
                out.push(path);
            }
        }
        out
    }
}
