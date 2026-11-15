//! Secrets kept in the operating system's keyring (Secret Service on Linux, Credential Manager
//! on Windows, Keychain on macOS), adapted from Simple Connect. Never in the database, backups,
//! the sync folder or the logs.
//!
//! `AUTO_MANAGER_SECRET_STORE=memory` keeps them in memory only, for tests and machines with
//! no keyring (they're then gone when the app closes).

use crate::error::{AppError, AppResult};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const SERVICE: &str = "dev.automanager.desktop";

pub trait Secrets: Send + Sync {
    fn get(&self, id: &str) -> AppResult<Option<String>>;
    fn set(&self, id: &str, secret: &str) -> AppResult<()>;
    fn delete(&self, id: &str) -> AppResult<()>;
}

fn keyring_error(e: impl std::fmt::Display) -> AppError {
    AppError::Other(format!("The system keyring isn't available ({e}). On Linux, GNOME Keyring or KWallet must be running."))
}

pub struct Keyring;

impl Keyring {
    fn connect() -> AppResult<()> {
        static READY: OnceLock<Result<(), String>> = OnceLock::new();
        READY
            .get_or_init(|| {
                #[cfg(target_os = "linux")]
                let store = zbus_secret_service_keyring_store::Store::new().map_err(|e| e.to_string())?;
                #[cfg(target_os = "windows")]
                let store = windows_native_keyring_store::Store::new().map_err(|e| e.to_string())?;
                #[cfg(target_os = "macos")]
                let store = apple_native_keyring_store::keychain::Store::new().map_err(|e| e.to_string())?;
                keyring_core::set_default_store(store);
                Ok(())
            })
            .clone()
            .map_err(keyring_error)
    }

    fn entry(id: &str) -> AppResult<keyring_core::Entry> {
        Self::connect()?;
        // Keep Windows credentials on this computer instead of roaming with the profile.
        #[cfg(target_os = "windows")]
        let entry = keyring_core::Entry::new_with_modifiers(SERVICE, id, &HashMap::from([("persistence", "Local")]));
        #[cfg(not(target_os = "windows"))]
        let entry = keyring_core::Entry::new(SERVICE, id);
        entry.map_err(keyring_error)
    }
}

impl Secrets for Keyring {
    fn get(&self, id: &str) -> AppResult<Option<String>> {
        match Self::entry(id)?.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(keyring_error(e)),
        }
    }
    fn set(&self, id: &str, secret: &str) -> AppResult<()> {
        Self::entry(id)?.set_password(secret).map_err(keyring_error)
    }
    fn delete(&self, id: &str) -> AppResult<()> {
        match Self::entry(id)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(keyring_error(e)),
        }
    }
}

#[derive(Default)]
pub struct Memory(Mutex<HashMap<String, String>>);

impl Secrets for Memory {
    fn get(&self, id: &str) -> AppResult<Option<String>> {
        Ok(self.0.lock().unwrap_or_else(|e| e.into_inner()).get(id).cloned())
    }
    fn set(&self, id: &str, secret: &str) -> AppResult<()> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(id.into(), secret.into());
        Ok(())
    }
    fn delete(&self, id: &str) -> AppResult<()> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
        Ok(())
    }
}

/// The store this app uses: the OS keyring unless told to keep secrets in memory.
pub fn store() -> Box<dyn Secrets> {
    if std::env::var("AUTO_MANAGER_SECRET_STORE").as_deref() == Ok("memory") { Box::new(Memory::default()) } else { Box::new(Keyring) }
}

impl<T: Secrets> Secrets for std::sync::Arc<T> {
    fn get(&self, id: &str) -> AppResult<Option<String>> {
        (**self).get(id)
    }
    fn set(&self, id: &str, secret: &str) -> AppResult<()> {
        (**self).set(id, secret)
    }
    fn delete(&self, id: &str) -> AppResult<()> {
        (**self).delete(id)
    }
}
