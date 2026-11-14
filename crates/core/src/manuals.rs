//! The documentation library: imported PDFs (stored by content hash), their links to
//! vehicles, page text for search, bookmarks/notes/component references, and "Where Is It?".
//!
//! Auto Manager never downloads or redistributes manuals itself: people import copies they are
//! entitled to, or follow links to the official publisher.

use crate::db::{self, clean, expect_one};
use crate::error::{AppError, AppResult};
use crate::state::Paths;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use ts_rs::TS;

pub const MAX_PDF_BYTES: u64 = 512 * 1024 * 1024;
pub const KINDS: &[&str] = &[
    "owner",
    "maintenance",
    "service",
    "repair",
    "tsb",
    "wiring",
    "fuse",
    "fluids",
    "procedure",
    "component_location",
    "parts_diagram",
    "troubleshooting",
    "installation",
    "other",
];

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Manual {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub publisher: String,
    pub source_url: String,
    pub license_note: String,
    pub file_name: String,
    pub file_size: i64,
    pub page_count: Option<i64>,
    pub indexed: bool,
    pub created_at: String,
    pub links: Vec<ManualLink>,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct ManualLink {
    pub vehicle_id: Option<String>,
    pub vehicle_title: Option<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn file_path(paths: &Paths, sha: &str) -> PathBuf {
    paths.documents().join(format!("{sha}.pdf"))
}

fn links(conn: &Connection, manual_id: &str) -> AppResult<Vec<ManualLink>> {
    let mut st = conn.prepare(
        "SELECT l.vehicle_id, CASE WHEN v.id IS NULL THEN NULL ELSE trim(coalesce(v.year, '') || ' ' || v.make || ' ' || v.model || ' ' || v.trim) END,
                l.make, l.model, l.year_from, l.year_to
         FROM manual_vehicle_links l LEFT JOIN vehicles v ON v.id = l.vehicle_id WHERE l.manual_id = ?1 ORDER BY l.id",
    )?;
    let rows = st.query_map([manual_id], |r| {
        Ok(ManualLink {
            vehicle_id: r.get(0)?,
            vehicle_title: r.get(1)?,
            make: r.get(2)?,
            model: r.get(3)?,
            year_from: r.get(4)?,
            year_to: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

const SELECT: &str =
    "SELECT id, title, kind, publisher, source_url, license_note, file_name, file_size, page_count, indexed, created_at FROM manuals";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Manual> {
    Ok(Manual {
        id: r.get(0)?,
        title: r.get(1)?,
        kind: r.get(2)?,
        publisher: r.get(3)?,
        source_url: r.get(4)?,
        license_note: r.get(5)?,
        file_name: r.get(6)?,
        file_size: r.get(7)?,
        page_count: r.get(8)?,
        indexed: r.get::<_, i64>(9)? == 1,
        created_at: r.get(10)?,
        links: vec![],
    })
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Manual> {
    let m =
        conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], row).optional()?.ok_or_else(|| AppError::not_found("That document"))?;
    Ok(Manual { links: links(conn, id)?, ..m })
}

pub fn list(conn: &Connection, vehicle_id: Option<&str>) -> AppResult<Vec<Manual>> {
    let ids: Vec<String> = match vehicle_id {
        None => {
            let mut st = conn.prepare("SELECT id FROM manuals ORDER BY title COLLATE NOCASE")?;
            st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
        }
        Some(v) => {
            let (year, make, model): (Option<i32>, String, String) =
                conn.query_row("SELECT year, make, model FROM vehicles WHERE id = ?1", [v], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            let mut st = conn.prepare(
                "SELECT DISTINCT m.id FROM manuals m JOIN manual_vehicle_links l ON l.manual_id = m.id
                 WHERE l.vehicle_id = ?1
                    OR (l.vehicle_id IS NULL AND l.make = ?2 AND (l.model IS NULL OR l.model = ?3)
                        AND (l.year_from IS NULL OR ?4 >= l.year_from) AND (l.year_to IS NULL OR ?4 <= l.year_to))
                 ORDER BY m.title COLLATE NOCASE",
            )?;
            st.query_map(params![v, make, model, year], |r| r.get(0))?.collect::<Result<_, _>>()?
        }
    };
    ids.iter().map(|id| get(conn, id)).collect()
}

#[derive(Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct ManualMeta {
    pub title: String,
    pub kind: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub license_note: String,
}

fn validate_meta(m: &ManualMeta) -> AppResult<()> {
    if clean(&m.title).is_empty() {
        return Err(AppError::invalid("title", "Give the document a title."));
    }
    if !KINDS.contains(&m.kind.as_str()) {
        return Err(AppError::invalid("kind", "Choose a document type."));
    }
    let url = m.source_url.trim();
    if !url.is_empty() && !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(AppError::invalid("source_url", "The source link must start with https://"));
    }
    Ok(())
}

/// Hash and validate a PDF on disk without loading it all into memory.
fn inspect(src: &Path) -> AppResult<(String, u64)> {
    let meta = std::fs::metadata(src)?;
    if !meta.is_file() {
        return Err(AppError::msg("Choose a PDF file."));
    }
    if meta.len() > MAX_PDF_BYTES {
        return Err(AppError::msg("That file is larger than 512 MB, which Auto Manager doesn't import."));
    }
    let mut f = std::fs::File::open(src)?;
    let mut head = [0u8; 5];
    let n = f.read(&mut head)?;
    if n < 5 || &head != b"%PDF-" {
        return Err(AppError::msg("That file isn't a PDF. Only PDF documents can be imported."));
    }
    let mut h = Sha256::new();
    h.update(&head[..n]);
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok((hex(&h.finalize()), meta.len()))
}

/// Copy a PDF into the library. Importing the same file twice returns the existing document.
pub fn import(conn: &Connection, paths: &Paths, src: &Path, meta: &ManualMeta) -> AppResult<Manual> {
    validate_meta(meta)?;
    let (sha, size) = inspect(src)?;
    if let Some(id) = conn.query_row("SELECT id FROM manuals WHERE file_sha256 = ?1", [&sha], |r| r.get::<_, String>(0)).optional()? {
        return get(conn, &id);
    }
    let dest = file_path(paths, &sha);
    if !dest.exists() {
        let partial = dest.with_extension("partial");
        std::fs::copy(src, &partial)?;
        std::fs::rename(&partial, &dest)?;
    }
    let id = db::new_id();
    let file_name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "document.pdf".into());
    conn.execute(
        "INSERT INTO manuals (id, title, kind, publisher, source_url, license_note, file_sha256, file_name, file_size) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![id, clean(&meta.title), meta.kind, clean(&meta.publisher), meta.source_url.trim(), meta.license_note.trim(), sha, file_name, size as i64],
    )?;
    db::audit(conn, "manual", Some(&id), "import", &format!("Imported {}", clean(&meta.title)), None)?;
    get(conn, &id)
}

pub fn update(conn: &Connection, id: &str, meta: &ManualMeta) -> AppResult<Manual> {
    validate_meta(meta)?;
    let n = conn.execute(
        "UPDATE manuals SET title = ?2, kind = ?3, publisher = ?4, source_url = ?5, license_note = ?6 WHERE id = ?1",
        params![id, clean(&meta.title), meta.kind, clean(&meta.publisher), meta.source_url.trim(), meta.license_note.trim()],
    )?;
    expect_one(n, "That document")?;
    get(conn, id)
}

pub fn delete(conn: &Connection, paths: &Paths, id: &str) -> AppResult<()> {
    let sha: String = conn
        .query_row("SELECT file_sha256 FROM manuals WHERE id = ?1", [id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That document"))?;
    conn.execute("DELETE FROM manuals WHERE id = ?1", [id])?;
    let _ = std::fs::remove_file(file_path(paths, &sha));
    db::audit(conn, "manual", Some(id), "delete", "Deleted a document", None)?;
    Ok(())
}

pub fn read_file(conn: &Connection, paths: &Paths, id: &str) -> AppResult<Vec<u8>> {
    let sha: String = conn
        .query_row("SELECT file_sha256 FROM manuals WHERE id = ?1", [id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That document"))?;
    std::fs::read(file_path(paths, &sha))
        .map_err(|_| AppError::Other("The document file is missing from the library folder. Restore a backup or import it again.".into()))
}

pub fn set_links(conn: &Connection, manual_id: &str, links_: &[ManualLink]) -> AppResult<Manual> {
    get(conn, manual_id)?;
    conn.execute("DELETE FROM manual_vehicle_links WHERE manual_id = ?1", [manual_id])?;
    for l in links_ {
        let make = l.make.as_deref().map(clean).filter(|m| !m.is_empty());
        if l.vehicle_id.is_none() && make.is_none() {
            return Err(AppError::invalid("links", "Link the document to a saved vehicle or to a make."));
        }
        if let (Some(a), Some(b)) = (l.year_from, l.year_to)
            && a > b
        {
            return Err(AppError::invalid("links", "The last year must not be before the first."));
        }
        conn.execute(
            "INSERT INTO manual_vehicle_links (manual_id, vehicle_id, make, model, year_from, year_to) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![manual_id, l.vehicle_id, make, l.model.as_deref().map(clean).filter(|m| !m.is_empty()), l.year_from, l.year_to],
        )?;
    }
    get(conn, manual_id)
}

/// Store the text the viewer extracted from each page (1-based), replacing earlier text.
pub fn set_pages_text(conn: &Connection, manual_id: &str, page_count: i64, pages: &[(i64, String)], done: bool) -> AppResult<()> {
    get(conn, manual_id)?;
    let mut del = conn.prepare("DELETE FROM manual_pages WHERE manual_id = ?1 AND page = ?2")?;
    let mut ins = conn.prepare("INSERT INTO manual_pages (manual_id, page, text) VALUES (?1, ?2, ?3)")?;
    for (page, text) in pages {
        if *page < 1 || *page > page_count {
            return Err(AppError::msg("Page number out of range."));
        }
        del.execute(params![manual_id, page])?;
        let t: String = text.chars().take(200_000).collect();
        if !t.trim().is_empty() {
            ins.execute(params![manual_id, page, t])?;
        }
    }
    conn.execute("UPDATE manuals SET page_count = ?2, indexed = ?3 WHERE id = ?1", params![manual_id, page_count, done])?;
    Ok(())
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct PageHit {
    pub manual_id: String,
    pub title: String,
    pub page: i64,
    /// Text around the match; matched words wrapped in [[ ]].
    pub snippet: String,
}

/// Turn what someone typed into a safe FTS5 query: each word quoted, all required.
pub fn fts_query(q: &str) -> Option<String> {
    let words: Vec<String> =
        q.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).take(12).map(|w| format!("\"{w}\"")).collect();
    (!words.is_empty()).then(|| words.join(" "))
}

pub fn search_pages(conn: &Connection, manual_ids: Option<&[String]>, q: &str, limit: i64) -> AppResult<Vec<PageHit>> {
    let Some(fq) = fts_query(q) else { return Ok(vec![]) };
    let mut st = conn.prepare(
        "SELECT p.manual_id, m.title, p.page, snippet(manual_pages, 2, '[[', ']]', ' … ', 18)
         FROM manual_pages p JOIN manuals m ON m.id = p.manual_id WHERE manual_pages MATCH ?1 ORDER BY rank LIMIT ?2",
    )?;
    let rows = st.query_map(params![fq, limit * 4], |r| {
        Ok(PageHit { manual_id: r.get(0)?, title: r.get(1)?, page: r.get(2)?, snippet: r.get(3)? })
    })?;
    let mut hits: Vec<PageHit> = rows.collect::<Result<_, _>>()?;
    if let Some(ids) = manual_ids {
        hits.retain(|h| ids.contains(&h.manual_id));
    }
    hits.truncate(limit as usize);
    Ok(hits)
}

// ---------------------------------------------------------------- annotations

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Annotation {
    pub id: String,
    pub manual_id: String,
    pub manual_title: String,
    pub page: i64,
    /// "bookmark", "note" or "component"
    pub kind: String,
    pub label: String,
    pub body: String,
    pub vehicle_id: Option<String>,
    pub created_at: String,
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct AnnotationInput {
    pub page: i64,
    pub kind: String,
    pub label: String,
    #[serde(default)]
    pub body: String,
    pub vehicle_id: Option<String>,
}

pub fn add_annotation(conn: &Connection, manual_id: &str, a: &AnnotationInput) -> AppResult<Annotation> {
    let m = get(conn, manual_id)?;
    if !matches!(a.kind.as_str(), "bookmark" | "note" | "component") {
        return Err(AppError::invalid("kind", "Choose bookmark, note or component location."));
    }
    if a.page < 1 || m.page_count.is_some_and(|n| a.page > n) {
        return Err(AppError::invalid("page", "That page isn't in this document."));
    }
    let label = clean(&a.label);
    if label.is_empty() {
        return Err(AppError::invalid(
            "label",
            if a.kind == "component" { "Name the component (e.g. Starter motor)." } else { "Give it a short name." },
        ));
    }
    let id = db::new_id();
    conn.execute(
        "INSERT INTO manual_annotations (id, manual_id, page, kind, label, body, vehicle_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, manual_id, a.page, a.kind, label, a.body.trim(), a.vehicle_id],
    )?;
    annotations(conn, Some(manual_id), None)?.into_iter().find(|x| x.id == id).ok_or_else(|| AppError::not_found("That note"))
}

pub fn delete_annotation(conn: &Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM manual_annotations WHERE id = ?1", [id])?;
    expect_one(n, "That note")
}

pub fn annotations(conn: &Connection, manual_id: Option<&str>, kind: Option<&str>) -> AppResult<Vec<Annotation>> {
    let mut st = conn.prepare(
        "SELECT a.id, a.manual_id, m.title, a.page, a.kind, a.label, a.body, a.vehicle_id, a.created_at
         FROM manual_annotations a JOIN manuals m ON m.id = a.manual_id
         WHERE (?1 IS NULL OR a.manual_id = ?1) AND (?2 IS NULL OR a.kind = ?2) ORDER BY m.title, a.page",
    )?;
    let rows = st.query_map(params![manual_id, kind], |r| {
        Ok(Annotation {
            id: r.get(0)?,
            manual_id: r.get(1)?,
            manual_title: r.get(2)?,
            page: r.get(3)?,
            kind: r.get(4)?,
            label: r.get(5)?,
            body: r.get(6)?,
            vehicle_id: r.get(7)?,
            created_at: r.get(8)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

// ---------------------------------------------------------------- Where Is It?

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct InstallNote {
    pub part_id: String,
    pub part_name: String,
    pub notes: String,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct WhereIsIt {
    pub component: String,
    /// Locations someone recorded against a page of a document for this vehicle.
    pub saved: Vec<Annotation>,
    /// Pages in this vehicle's documents that mention the component.
    pub pages: Vec<PageHit>,
    /// Installation notes on catalog parts of this kind that have fitment for the vehicle.
    pub install_notes: Vec<InstallNote>,
    pub searched_documents: i64,
}

/// Look a component up only in material the user has: saved component references, the text of
/// this vehicle's documents, and installation notes of fitting parts. Nothing is generated.
pub fn where_is(conn: &Connection, vehicle_id: &str, component: &str) -> AppResult<WhereIsIt> {
    let component = clean(component);
    if component.is_empty() {
        return Err(AppError::invalid("component", "Enter a component, e.g. starter motor."));
    }
    let docs = list(conn, Some(vehicle_id))?;
    let ids: Vec<String> = docs.iter().map(|d| d.id.clone()).collect();
    let needle = component.to_lowercase();
    let saved: Vec<Annotation> = annotations(conn, None, Some("component"))?
        .into_iter()
        .filter(|a| {
            (ids.contains(&a.manual_id) || a.vehicle_id.as_deref() == Some(vehicle_id))
                && (a.label.to_lowercase().contains(&needle) || needle.contains(&a.label.to_lowercase()))
        })
        .collect();
    let pages = if ids.is_empty() { vec![] } else { search_pages(conn, Some(&ids), &component, 10)? };
    let facts = crate::matches::facts(conn, vehicle_id)?;
    let mut install_notes = Vec::new();
    if let Some(year) = facts.year {
        let mut seen = std::collections::HashSet::new();
        for f in crate::parts::fitments_for_vehicle(conn, year, &facts.make, &facts.model)? {
            if !seen.insert(f.part_id.clone()) {
                continue;
            }
            let p = crate::parts::get(conn, &f.part_id)?;
            let words: Vec<String> = needle.split_whitespace().map(String::from).collect();
            let named = words.iter().all(|w| p.name.to_lowercase().contains(w.trim_end_matches('s')));
            if named && !p.install_notes.trim().is_empty() {
                install_notes.push(InstallNote { part_id: p.id, part_name: p.name, notes: p.install_notes });
            }
        }
    }
    Ok(WhereIsIt { component, saved, pages, install_notes, searched_documents: docs.iter().filter(|d| d.indexed).count() as i64 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pdf(dir: &Path, name: &str, body: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, [b"%PDF-1.7\n".as_slice(), body].concat()).unwrap();
        p
    }

    #[test]
    fn import_dedupe_and_reject_non_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("data"));
        paths.ensure().unwrap();
        let conn = db::open_memory().unwrap();
        let src = pdf(dir.path(), "owner.pdf", b"owner manual");
        let meta = ManualMeta { title: "Owner's manual".into(), kind: "owner".into(), ..Default::default() };
        let m = import(&conn, &paths, &src, &meta).unwrap();
        let again = import(&conn, &paths, &src, &meta).unwrap();
        assert_eq!(m.id, again.id);
        assert!(read_file(&conn, &paths, &m.id).unwrap().starts_with(b"%PDF-"));
        let fake = dir.path().join("fake.pdf");
        std::fs::write(&fake, b"<html>not a pdf").unwrap();
        assert!(import(&conn, &paths, &fake, &meta).is_err());
        delete(&conn, &paths, &m.id).unwrap();
        assert_eq!(std::fs::read_dir(paths.documents()).unwrap().count(), 0, "the stored copy is removed");
    }

    #[test]
    fn where_is_uses_only_recorded_material() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("data"));
        paths.ensure().unwrap();
        let conn = db::open_memory().unwrap();
        conn.execute("INSERT INTO vehicles (id, year, make, model, saved) VALUES ('v1', 2019, 'Nissan', 'Sentra', 1)", []).unwrap();
        let m = import(
            &conn,
            &paths,
            &pdf(dir.path(), "svc.pdf", b"svc"),
            &ManualMeta { title: "Service manual".into(), kind: "service".into(), ..Default::default() },
        )
        .unwrap();
        set_links(
            &conn,
            &m.id,
            &[ManualLink {
                vehicle_id: None,
                vehicle_title: None,
                make: Some("nissan".into()),
                model: Some("Sentra".into()),
                year_from: Some(2016),
                year_to: Some(2019),
            }],
        )
        .unwrap();
        set_pages_text(
            &conn,
            &m.id,
            300,
            &[(212, "Starter motor removal. The starter motor is located below the intake manifold.".into()), (5, "Contents".into())],
            true,
        )
        .unwrap();
        let w = where_is(&conn, "v1", "starter motor").unwrap();
        assert_eq!(w.pages.len(), 1);
        assert_eq!(w.pages[0].page, 212);
        assert!(w.pages[0].snippet.contains("[["));
        assert!(w.saved.is_empty());
        add_annotation(
            &conn,
            &m.id,
            &AnnotationInput {
                page: 212,
                kind: "component".into(),
                label: "Starter motor".into(),
                body: "Below intake manifold".into(),
                vehicle_id: None,
            },
        )
        .unwrap();
        assert_eq!(where_is(&conn, "v1", "Starter").unwrap().saved.len(), 1);
        let none = where_is(&conn, "v1", "flux capacitor").unwrap();
        assert!(
            none.pages.is_empty() && none.saved.is_empty() && none.install_notes.is_empty(),
            "unknown components return nothing, not a guess"
        );
        // A 2010 Sentra isn't covered by the 2016-2019 link.
        conn.execute("INSERT INTO vehicles (id, year, make, model) VALUES ('v2', 2010, 'Nissan', 'Sentra')", []).unwrap();
        assert!(list(&conn, Some("v2")).unwrap().is_empty());
    }

    #[test]
    fn fts_query_is_safe() {
        assert_eq!(fts_query("starter motor").as_deref(), Some("\"starter\" \"motor\""));
        assert_eq!(fts_query("\" OR NEAR(").as_deref(), Some("\"OR\" \"NEAR\""));
        assert_eq!(fts_query("***"), None);
    }
}
