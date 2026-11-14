//! The open application: data folder, database connection and HTTP client.
//!
//! Layout under the data root (platform app-data dir, or $AUTO_MANAGER_DATA_DIR):
//!   auto-manager.db        database
//!   documents/             imported manuals, named by content hash
//!   attachments/           photos, receipts, named by content hash
//!   backups/               automatic, pre-migration, pre-update and pre-restore backups
//!   logs/                  rotating log files

use crate::backup;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::providers::{Endpoints, http::Http};
use rusqlite::{Connection, TransactionBehavior};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn new(root: PathBuf) -> Self {
        Paths { root }
    }
    pub fn db(&self) -> PathBuf {
        self.root.join("auto-manager.db")
    }
    pub fn documents(&self) -> PathBuf {
        self.root.join("documents")
    }
    pub fn attachments(&self) -> PathBuf {
        self.root.join("attachments")
    }
    pub fn backups(&self) -> PathBuf {
        self.root.join("backups")
    }
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn ensure(&self) -> AppResult<()> {
        for d in [self.root.clone(), self.documents(), self.attachments(), self.backups(), self.logs()] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }
}

/// Resolve the data root: $AUTO_MANAGER_DATA_DIR wins (tests, portable installs), else the platform dir.
pub fn data_root(platform_dir: &Path) -> PathBuf {
    std::env::var_os("AUTO_MANAGER_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| platform_dir.to_path_buf())
}

pub struct Core {
    pub paths: Paths,
    pub http: Http,
    pub endpoints: Endpoints,
    db: Mutex<Option<Connection>>,
}

impl Core {
    /// Open (creating or migrating) the database. A backup is taken before any migration and
    /// once a day automatically.
    pub fn open(paths: Paths) -> AppResult<Core> {
        paths.ensure()?;
        let conn = Self::open_db(&paths)?;
        Ok(Core { paths, http: Http::new(), endpoints: Endpoints::from_env(), db: Mutex::new(Some(conn)) })
    }

    fn open_db(paths: &Paths) -> AppResult<Connection> {
        let conn = db::open(&paths.db(), |conn, from| backup::create(conn, paths, &format!("pre-migration-v{from}")).map(|_| ()))?;
        if crate::settings::load(&conn).backup_auto {
            backup::auto_backup_if_due(&conn, paths)?;
        }
        Ok(conn)
    }

    /// An in-memory core for tests.
    pub fn memory(root: PathBuf) -> AppResult<Core> {
        let paths = Paths::new(root);
        paths.ensure()?;
        Ok(Core { paths, http: Http::new(), endpoints: Endpoints::from_env(), db: Mutex::new(Some(db::open_memory()?)) })
    }

    fn lock(&self) -> MutexGuard<'_, Option<Connection>> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let guard = self.lock();
        let conn =
            guard.as_ref().ok_or_else(|| AppError::Other("The database is closed while a restore runs. Try again in a moment.".into()))?;
        f(conn)
    }

    /// Run `f` in an IMMEDIATE transaction (takes the write lock up front); commits on Ok,
    /// rolls back on Err.
    pub fn tx<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let mut guard = self.lock();
        let conn =
            guard.as_mut().ok_or_else(|| AppError::Other("The database is closed while a restore runs. Try again in a moment.".into()))?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    /// Replace the live data with a verified backup. Takes a pre-restore backup first and keeps
    /// the previous database beside the new one.
    pub fn restore(&self, zip: &Path) -> AppResult<backup::Manifest> {
        let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S%3f").to_string();
        let scratch = self.paths.root.join(format!(".restore-{ts}"));
        let manifest = match backup::verify_and_extract(zip, &scratch) {
            Ok(m) => m,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&scratch);
                return Err(e);
            }
        };
        let mut guard = self.lock();
        if let Some(conn) = guard.as_ref() {
            backup::create(conn, &self.paths, "pre-restore")?;
        }
        *guard = None; // close before swapping files
        let keep = self.paths.root.join(format!("previous-{ts}"));
        std::fs::create_dir_all(&keep)?;
        let swap = || -> AppResult<()> {
            for name in ["auto-manager.db", "auto-manager.db-wal", "auto-manager.db-shm"] {
                let p = self.paths.root.join(name);
                if p.exists() {
                    std::fs::rename(&p, keep.join(name))?;
                }
            }
            for dir in ["documents", "attachments"] {
                let p = self.paths.root.join(dir);
                if p.exists() {
                    std::fs::rename(&p, keep.join(dir))?;
                }
                std::fs::rename(scratch.join(dir), &p)?;
            }
            std::fs::rename(scratch.join("auto-manager.db"), self.paths.db())?;
            Ok(())
        };
        let swapped = swap();
        let _ = std::fs::remove_dir_all(&scratch);
        if let Err(e) = swapped {
            // Put the previous data back so the app keeps working.
            for name in ["auto-manager.db", "documents", "attachments"] {
                let from = keep.join(name);
                if from.exists() {
                    let to = self.paths.root.join(name);
                    let _ = if to.is_dir() { std::fs::remove_dir_all(&to) } else { std::fs::remove_file(&to) };
                    let _ = std::fs::rename(from, to);
                }
            }
            *guard = Some(Self::open_db(&self.paths)?);
            return Err(e);
        }
        *guard = Some(Self::open_db(&self.paths)?);
        Ok(manifest)
    }

    /// Check a path chosen in a save dialog before writing to it: right file type, an existing
    /// folder, and never inside the app's own data folder.
    pub fn output_path(&self, path: &str, ext: &str) -> AppResult<PathBuf> {
        let p = PathBuf::from(path);
        if p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) != Some(ext.to_string()) {
            return Err(AppError::msg(format!("Choose a .{ext} file name.")));
        }
        let parent = p.parent().filter(|d| d.is_dir()).ok_or_else(|| AppError::msg("That folder doesn't exist."))?;
        let parent = parent.canonicalize()?;
        let root = self.paths.root.canonicalize().unwrap_or_else(|_| self.paths.root.clone());
        if parent.starts_with(&root) {
            return Err(AppError::msg("Save exports somewhere outside Auto Manager's data folder."));
        }
        let name = p.file_name().ok_or_else(|| AppError::msg("Choose a file name."))?;
        Ok(parent.join(name))
    }
}
