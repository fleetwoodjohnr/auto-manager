//! Backups: a consistent SQLite snapshot (`VACUUM INTO`) plus documents and attachments plus a manifest with
//! SHA-256 checksums, in one zip. Restores are verified (checksums, integrity_check,
//! foreign_key_check, schema version) in a scratch folder before anything is replaced.

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::Paths;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const FORMAT: &str = "auto-manager-backup";
pub const FORMAT_VERSION: u32 = 1;
const DB_NAME: &str = "auto-manager.db";
const AUTO_KEEP: usize = 14;
const AUTO_INTERVAL_HOURS: i64 = 24;

#[derive(Serialize, Deserialize, ts_rs::TS, Clone, Debug)]
#[ts(export)]
pub struct ManifestFile {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Serialize, Deserialize, ts_rs::TS, Clone, Debug)]
#[ts(export)]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub app_version: String,
    pub schema_version: i64,
    pub created_at: String,
    pub reason: String,
    pub files: Vec<ManifestFile>,
}

#[derive(Serialize, ts_rs::TS, Clone, Debug)]
#[ts(export)]
pub struct BackupInfo {
    pub path: String,
    pub file_name: String,
    pub size: u64,
    pub manifest: Option<Manifest>,
    pub error: Option<String>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_file(p: &Path) -> AppResult<(String, u64)> {
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        h.update(&buf[..n]);
    }
    Ok((hex(&h.finalize()), size))
}

fn stamp() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ").to_string()
}

/// Write a backup zip into `dir` (file name derived from reason + time) and return its path.
pub fn create(conn: &Connection, paths: &Paths, reason: &str) -> AppResult<PathBuf> {
    let dir = paths.backups();
    std::fs::create_dir_all(&dir)?;
    let out = dir.join(format!("{reason}-{}.zip", stamp()));
    create_at(conn, paths, &out, reason)?;
    Ok(out)
}

/// Write a backup zip to an exact path (manual backups chosen in a save dialog).
pub fn create_at(conn: &Connection, paths: &Paths, out: &Path, reason: &str) -> AppResult<Manifest> {
    let parent = out.parent().ok_or_else(|| AppError::Other("Invalid backup path.".into()))?;
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".am-backup-tmp-{}", stamp()));
    std::fs::create_dir_all(&tmp)?;
    let result = (|| {
        let snap = tmp.join(DB_NAME);
        conn.execute("VACUUM INTO ?1", [snap.to_string_lossy()])?;
        let mut entries: Vec<(String, PathBuf)> = vec![(DB_NAME.into(), snap)];
        for (dir, path) in [("documents", paths.documents()), ("attachments", paths.attachments())] {
            if path.exists() {
                let mut names: Vec<_> = std::fs::read_dir(&path)?.collect::<Result<Vec<_>, _>>()?;
                names.sort_by_key(|e| e.file_name());
                for e in names {
                    if e.file_type()?.is_file() {
                        entries.push((format!("{dir}/{}", e.file_name().to_string_lossy()), e.path()));
                    }
                }
            }
        }
        let mut files = Vec::new();
        for (name, path) in &entries {
            let (sha256, size) = sha256_file(path)?;
            files.push(ManifestFile { path: name.clone(), sha256, size });
        }
        let manifest = Manifest {
            format: FORMAT.into(),
            format_version: FORMAT_VERSION,
            app_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: db::user_version(conn)?,
            created_at: db::now_utc(),
            reason: reason.into(),
            files,
        };
        let partial = out.with_extension("partial");
        {
            let f = std::fs::File::create(&partial)?;
            let mut z = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            z.start_file("manifest.json", opts).map_err(zip_err)?;
            z.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
            for (name, path) in &entries {
                z.start_file(name.as_str(), opts).map_err(zip_err)?;
                std::io::copy(&mut std::fs::File::open(path)?, &mut z)?;
            }
            z.finish().map_err(zip_err)?.sync_all()?;
        }
        std::fs::rename(&partial, out)?;
        Ok(manifest)
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

fn zip_err(e: zip::result::ZipError) -> AppError {
    AppError::Other(format!("Backup archive error: {e}"))
}

pub fn read_manifest(zip_path: &Path) -> AppResult<Manifest> {
    let mut z =
        zip::ZipArchive::new(std::fs::File::open(zip_path)?).map_err(|_| AppError::msg("This file is not a valid backup archive."))?;
    let mut s = String::new();
    z.by_name("manifest.json")
        .map_err(|_| AppError::msg("This archive has no backup manifest; it was not made by Auto Manager."))?
        .read_to_string(&mut s)?;
    let m: Manifest = serde_json::from_str(&s).map_err(|e| AppError::msg(format!("Backup manifest is unreadable: {e}")))?;
    if m.format != FORMAT {
        return Err(AppError::msg("This archive is not an Auto Manager backup."));
    }
    if m.format_version > FORMAT_VERSION {
        return Err(AppError::msg("This backup was made by a newer version of the app. Update the app to restore it."));
    }
    Ok(m)
}

/// Extract into `scratch`, verify everything, and upgrade the copy to the current schema.
/// Returns the manifest; `scratch` then holds the database, `documents/` and `attachments/`.
pub fn verify_and_extract(zip_path: &Path, scratch: &Path) -> AppResult<Manifest> {
    let m = read_manifest(zip_path)?;
    if !m.files.iter().any(|f| f.path == DB_NAME) {
        return Err(AppError::msg("Backup is missing its database."));
    }
    std::fs::create_dir_all(scratch.join("attachments"))?;
    std::fs::create_dir_all(scratch.join("documents"))?;
    let mut z = zip::ZipArchive::new(std::fs::File::open(zip_path)?).map_err(zip_err)?;
    for f in &m.files {
        let rel = Path::new(&f.path);
        let safe = f.path == DB_NAME
            || ((rel.parent() == Some(Path::new("attachments")) || rel.parent() == Some(Path::new("documents")))
                && rel.file_name().is_some_and(|n| !n.to_string_lossy().starts_with('.')));
        if !safe || f.path.contains("..") {
            return Err(AppError::msg(format!("Backup contains an unexpected path: {}", f.path)));
        }
        let dest = scratch.join(rel);
        {
            let mut entry = z.by_name(&f.path).map_err(|_| AppError::msg(format!("Backup is incomplete: {} is missing.", f.path)))?;
            let mut out = std::fs::File::create(&dest)?;
            std::io::copy(&mut entry, &mut out)?;
        }
        let (sha, size) = sha256_file(&dest)?;
        if sha != f.sha256 || size != f.size {
            return Err(AppError::msg(format!("Backup is damaged: checksum mismatch for {}.", f.path)));
        }
    }
    let mut conn = Connection::open(scratch.join(DB_NAME))?;
    let ok: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if ok != "ok" {
        return Err(AppError::msg(format!("Backup database failed its integrity check: {ok}")));
    }
    if db::user_version(&conn)? > db::schema_version() {
        return Err(AppError::msg("This backup was made by a newer version of the app. Update the app to restore it."));
    }
    db::configure(&conn)?;
    let fk: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?;
    if fk > 0 {
        return Err(AppError::msg("Backup database has broken references and cannot be restored safely."));
    }
    db::migrate(&mut conn, |_, _| Ok(()))?;
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    drop(conn);
    Ok(m)
}

pub fn list(dir: &Path) -> AppResult<Vec<BackupInfo>> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("zip") {
            continue;
        }
        let (manifest, error) = match read_manifest(&p) {
            Ok(m) => (Some(m), None),
            Err(e) => (None, Some(e.to_string())),
        };
        out.push(BackupInfo {
            path: p.to_string_lossy().into(),
            file_name: e.file_name().to_string_lossy().into(),
            size: e.metadata()?.len(),
            manifest,
            error,
        });
    }
    out.sort_by(|a, b| b.file_name.cmp(&a.file_name));
    out.sort_by(|a, b| {
        let ka = a.manifest.as_ref().map(|m| m.created_at.clone()).unwrap_or_default();
        let kb = b.manifest.as_ref().map(|m| m.created_at.clone()).unwrap_or_default();
        kb.cmp(&ka)
    });
    Ok(out)
}

/// Daily automatic backup on open; keeps the newest AUTO_KEEP automatic backups.
pub fn auto_backup_if_due(conn: &Connection, paths: &Paths) -> AppResult<()> {
    let dir = paths.backups();
    let dir = dir.as_path();
    let autos: Vec<BackupInfo> = list(dir)?.into_iter().filter(|b| b.file_name.starts_with("auto-")).collect();
    let due = match autos.first().and_then(|b| b.manifest.as_ref()) {
        None => true,
        Some(m) => chrono::DateTime::parse_from_rfc3339(&m.created_at)
            .map(|t| chrono::Utc::now().signed_duration_since(t).num_hours() >= AUTO_INTERVAL_HOURS)
            .unwrap_or(true),
    };
    // A brand-new empty database has nothing worth backing up yet.
    let empty: bool = conn.query_row(
        "SELECT NOT EXISTS (SELECT 1 FROM vehicles) AND NOT EXISTS (SELECT 1 FROM parts) AND NOT EXISTS (SELECT 1 FROM settings)",
        [],
        |r| r.get(0),
    )?;
    if due && !empty {
        create(conn, paths, "auto")?;
        let autos: Vec<BackupInfo> = list(dir)?.into_iter().filter(|b| b.file_name.starts_with("auto-")).collect();
        for old in autos.iter().skip(AUTO_KEEP) {
            let _ = std::fs::remove_file(&old.path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_tamper_detection() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::new(root.path().join("data"));
        paths.ensure().unwrap();
        let mut conn = Connection::open(paths.db()).unwrap();
        db::configure(&conn).unwrap();
        db::migrate(&mut conn, |_, _| Ok(())).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('k', '\"v\"')", []).unwrap();
        conn.execute("INSERT INTO vehicles (id, make, model, saved) VALUES ('v1', 'Honda', 'Accord', 1)", []).unwrap();
        std::fs::write(paths.attachments().join("abc.jpg"), b"photo").unwrap();
        std::fs::write(paths.documents().join("def.pdf"), b"%PDF-manual").unwrap();

        let zip_path = create(&conn, &paths, "manual").unwrap();
        let m = verify_and_extract(&zip_path, &root.path().join("restore1")).unwrap();
        assert_eq!(m.files.len(), 3);
        let restored = Connection::open(root.path().join("restore1").join(DB_NAME)).unwrap();
        let v: String = restored.query_row("SELECT model FROM vehicles WHERE id='v1'", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "Accord");
        assert_eq!(std::fs::read(root.path().join("restore1/attachments/abc.jpg")).unwrap(), b"photo");
        assert_eq!(std::fs::read(root.path().join("restore1/documents/def.pdf")).unwrap(), b"%PDF-manual");

        // Tamper: rewrite the archive with a modified document but the original manifest.
        let tampered = root.path().join("tampered.zip");
        {
            let mut src = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
            let mut z = zip::ZipWriter::new(std::fs::File::create(&tampered).unwrap());
            for i in 0..src.len() {
                let mut e = src.by_index(i).unwrap();
                let name = e.name().unwrap().to_string();
                let mut buf = Vec::new();
                e.read_to_end(&mut buf).unwrap();
                if name == "documents/def.pdf" {
                    buf = b"%PDF-forged".to_vec();
                }
                z.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
                z.write_all(&buf).unwrap();
            }
            z.finish().unwrap();
        }
        let err = verify_and_extract(&tampered, &root.path().join("restore2")).unwrap_err().to_string();
        assert!(err.contains("checksum"), "{err}");

        let bytes = std::fs::read(&zip_path).unwrap();
        std::fs::write(root.path().join("trunc.zip"), &bytes[..bytes.len() / 2]).unwrap();
        assert!(verify_and_extract(&root.path().join("trunc.zip"), &root.path().join("restore3")).is_err());
    }

    #[test]
    fn rejects_path_traversal() {
        let root = tempfile::tempdir().unwrap();
        let evil = root.path().join("evil.zip");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&evil).unwrap());
            let m = serde_json::json!({"format": FORMAT, "format_version": 1, "app_version": "0", "schema_version": 1,
                "created_at": "2026-01-01T00:00:00Z", "reason": "x",
                "files": [{"path": "documents/../../x", "sha256": "", "size": 0}, {"path": DB_NAME, "sha256": "", "size": 0}]});
            z.start_file("manifest.json", zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(m.to_string().as_bytes()).unwrap();
            z.finish().unwrap();
        }
        let err = verify_and_extract(&evil, &root.path().join("out")).unwrap_err().to_string();
        assert!(err.contains("unexpected path"), "{err}");
    }
}
