//! Photos, receipts, invoices and other files attached to vehicles, parts, items, jobs and
//! customers. Stored by content hash in the data folder's attachments/ (included in backups).

use crate::db::{self, clean, expect_one};
use crate::error::{AppError, AppResult};
use crate::state::Paths;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use ts_rs::TS;

const MAX_BYTES: u64 = 50 * 1024 * 1024;
const OWNERS: &[&str] = &["vehicle", "part", "item", "job", "maintenance", "customer"];

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Attachment {
    pub id: String,
    pub owner_kind: String,
    pub owner_id: String,
    pub file_name: String,
    /// image/png, image/jpeg, image/webp or application/pdf
    pub mime: String,
    pub size: i64,
    pub caption: String,
    pub created_at: String,
}

/// The type from the file's first bytes (not its name), limited to safe, viewable formats.
pub fn sniff(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        Some("image/webp")
    } else if head.starts_with(b"%PDF-") {
        Some("application/pdf")
    } else {
        None
    }
}

fn ext(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => "pdf",
    }
}

pub fn stored_path(paths: &Paths, sha: &str, mime: &str) -> PathBuf {
    paths.attachments().join(format!("{sha}.{}", ext(mime)))
}

pub fn add(conn: &Connection, paths: &Paths, owner_kind: &str, owner_id: &str, src: &Path, caption: &str) -> AppResult<Attachment> {
    if !OWNERS.contains(&owner_kind) {
        return Err(AppError::msg("Files can't be attached to that."));
    }
    let meta = std::fs::metadata(src)?;
    if !meta.is_file() || meta.len() == 0 {
        return Err(AppError::msg("Choose a file."));
    }
    if meta.len() > MAX_BYTES {
        return Err(AppError::msg("Attachments can be at most 50 MB."));
    }
    let bytes = std::fs::read(src)?;
    let mime = sniff(&bytes[..bytes.len().min(16)])
        .ok_or_else(|| AppError::msg("Only photos (PNG, JPEG, WebP) and PDF documents can be attached."))?;
    let sha: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    let dest = stored_path(paths, &sha, mime);
    if !dest.exists() {
        let partial = dest.with_extension("partial");
        std::fs::write(&partial, &bytes)?;
        std::fs::rename(partial, &dest)?;
    }
    let id = db::new_id();
    let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| format!("file.{}", ext(mime)));
    conn.execute(
        "INSERT INTO attachments (id, owner_kind, owner_id, sha256, file_name, mime, size, caption) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, owner_kind, owner_id, sha, name, mime, bytes.len() as i64, clean(caption)],
    )?;
    get(conn, &id)
}

const SELECT: &str = "SELECT id, owner_kind, owner_id, file_name, mime, size, caption, created_at FROM attachments";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Attachment> {
    Ok(Attachment {
        id: r.get(0)?,
        owner_kind: r.get(1)?,
        owner_id: r.get(2)?,
        file_name: r.get(3)?,
        mime: r.get(4)?,
        size: r.get(5)?,
        caption: r.get(6)?,
        created_at: r.get(7)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Attachment> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], row).optional()?.ok_or_else(|| AppError::not_found("That file"))
}

pub fn list(conn: &Connection, owner_kind: &str, owner_id: &str) -> AppResult<Vec<Attachment>> {
    let mut st = conn.prepare(&format!("{SELECT} WHERE owner_kind = ?1 AND owner_id = ?2 ORDER BY created_at"))?;
    let rows = st.query_map(params![owner_kind, owner_id], row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Where the stored copy is (for opening it in the system's viewer).
pub fn path_of(conn: &Connection, paths: &Paths, id: &str) -> AppResult<PathBuf> {
    let (sha, mime): (String, String) = conn
        .query_row("SELECT sha256, mime FROM attachments WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or_else(|| AppError::not_found("That file"))?;
    let p = stored_path(paths, &sha, &mime);
    if !p.exists() {
        return Err(AppError::Other("The file is missing from the attachments folder. Restore a backup or attach it again.".into()));
    }
    Ok(p)
}

pub fn bytes(conn: &Connection, paths: &Paths, id: &str) -> AppResult<Vec<u8>> {
    Ok(std::fs::read(path_of(conn, paths, id)?)?)
}

/// Remove the record, and the stored file when nothing else uses it.
pub fn delete(conn: &Connection, paths: &Paths, id: &str) -> AppResult<()> {
    let (sha, mime): (String, String) = conn
        .query_row("SELECT sha256, mime FROM attachments WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or_else(|| AppError::not_found("That file"))?;
    expect_one(conn.execute("DELETE FROM attachments WHERE id = ?1", [id])?, "That file")?;
    if !db::exists(conn, "SELECT 1 FROM attachments WHERE sha256 = ?1", [&sha])? {
        let _ = std::fs::remove_file(stored_path(paths, &sha, &mime));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_sniff_dedupe_delete() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("data"));
        paths.ensure().unwrap();
        let conn = db::open_memory().unwrap();
        let png = dir.path().join("photo.png");
        std::fs::write(&png, [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0]).unwrap();
        let a = add(&conn, &paths, "vehicle", "v1", &png, "Front").unwrap();
        let b = add(&conn, &paths, "part", "p1", &png, "").unwrap();
        assert_eq!(a.mime, "image/png");
        assert_eq!(std::fs::read_dir(paths.attachments()).unwrap().count(), 1, "same bytes stored once");
        delete(&conn, &paths, &a.id).unwrap();
        assert!(bytes(&conn, &paths, &b.id).is_ok(), "still used by the part");
        delete(&conn, &paths, &b.id).unwrap();
        assert_eq!(std::fs::read_dir(paths.attachments()).unwrap().count(), 0);
        let exe = dir.path().join("evil.png");
        std::fs::write(&exe, b"MZ\x90\x00 not an image").unwrap();
        assert!(add(&conn, &paths, "vehicle", "v1", &exe, "").is_err(), "the name doesn't decide the type");
        assert!(add(&conn, &paths, "nowhere", "x", &png, "").is_err());
    }
}
