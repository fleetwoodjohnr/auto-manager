//! Encrypted sync bundles: a full backup, encrypted with a passphrase, written to a folder the
//! person chooses (a USB stick, or a folder another service syncs). Another copy of Auto
//! Manager can then restore it. This moves a complete copy of the data; it doesn't merge edits
//! made on two computers (see docs/ARCHITECTURE.md, "Sync").
//!
//! Format: "AMSYNC1\n" ‖ salt (16) ‖ nonce (12) ‖ ChaCha20-Poly1305 ciphertext of the backup zip.
//! The key is Argon2id(passphrase, salt).

use crate::backup;
use crate::error::{AppError, AppResult};
use crate::state::{Core, Paths};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use std::path::Path;

const MAGIC: &[u8] = b"AMSYNC1\n";
const MIN_PASSPHRASE: usize = 10;

fn key(passphrase: &str, salt: &[u8]) -> AppResult<Key> {
    let mut out = [0u8; 32];
    argon2::Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut out)
        .map_err(|e| AppError::Other(format!("Key derivation failed: {e}")))?;
    Ok(Key::from(out))
}

fn random<const N: usize>() -> AppResult<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|e| AppError::Other(format!("No secure random numbers available: {e}")))?;
    Ok(b)
}

pub fn encrypt(plain: &[u8], passphrase: &str) -> AppResult<Vec<u8>> {
    if passphrase.chars().count() < MIN_PASSPHRASE {
        return Err(AppError::invalid("passphrase", format!("Use a passphrase of at least {MIN_PASSPHRASE} characters.")));
    }
    let salt = random::<16>()?;
    let nonce = random::<12>()?;
    let ct = ChaCha20Poly1305::new(&key(passphrase, &salt)?)
        .encrypt(Nonce::from_slice(&nonce), plain)
        .map_err(|_| AppError::Other("Encryption failed.".into()))?;
    Ok([MAGIC, &salt, &nonce, &ct].concat())
}

pub fn decrypt(data: &[u8], passphrase: &str) -> AppResult<Vec<u8>> {
    if data.len() < MAGIC.len() + 28 || &data[..MAGIC.len()] != MAGIC {
        return Err(AppError::msg("That file isn't an Auto Manager sync bundle."));
    }
    let rest = &data[MAGIC.len()..];
    let (salt, rest) = rest.split_at(16);
    let (nonce, ct) = rest.split_at(12);
    ChaCha20Poly1305::new(&key(passphrase, salt)?)
        .decrypt(Nonce::from_slice(nonce), ct)
        .map_err(|_| AppError::msg("Wrong passphrase, or the bundle is damaged."))
}

/// Write an encrypted bundle of everything to `out`.
// ponytail: the whole backup is held in memory while encrypting; stream it in chunks if
// libraries of manuals grow past a few hundred MB.
pub fn export(core: &Core, paths: &Paths, out: &Path, passphrase: &str) -> AppResult<()> {
    let tmp = paths.root.join(format!(".sync-{}.zip", crate::db::new_id()));
    let result = (|| {
        core.read(|c| backup::create_at(c, paths, &tmp, "sync-export").map(|_| ()))?;
        let sealed = encrypt(&std::fs::read(&tmp)?, passphrase)?;
        let partial = out.with_extension("partial");
        std::fs::write(&partial, sealed)?;
        std::fs::rename(partial, out)?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&tmp);
    result
}

/// Replace this copy's data with a bundle (after the usual restore checks and a backup).
pub fn import(core: &Core, bundle: &Path, passphrase: &str) -> AppResult<backup::Manifest> {
    let data = std::fs::read(bundle)?;
    let zip = decrypt(&data, passphrase)?;
    let tmp = core.paths.root.join(format!(".sync-in-{}.zip", crate::db::new_id()));
    std::fs::write(&tmp, zip)?;
    let r = core.restore(&tmp);
    let _ = std::fs::remove_file(&tmp);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_wrong_passphrase() {
        let sealed = encrypt(b"hello inventory", "correct horse battery").unwrap();
        assert_eq!(decrypt(&sealed, "correct horse battery").unwrap(), b"hello inventory");
        assert!(decrypt(&sealed, "wrong passphrase!!").is_err());
        let mut tampered = sealed.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert!(decrypt(&tampered, "correct horse battery").is_err());
        assert!(encrypt(b"x", "short").is_err());
    }

    #[test]
    fn export_and_import_between_copies() {
        let a_dir = tempfile::tempdir().unwrap();
        let a = Core::open(Paths::new(a_dir.path().join("a"))).unwrap();
        a.tx(|c| Ok(c.execute("INSERT INTO vehicles (id, make, model, saved) VALUES ('v1', 'Honda', 'Accord', 1)", [])?)).unwrap();
        let bundle = a_dir.path().join("bundle.amsync");
        export(&a, &a.paths.clone(), &bundle, "a long passphrase").unwrap();
        let b = Core::open(Paths::new(a_dir.path().join("b"))).unwrap();
        import(&b, &bundle, "a long passphrase").unwrap();
        let model: String = b.read(|c| Ok(c.query_row("SELECT model FROM vehicles WHERE id = 'v1'", [], |r| r.get(0))?)).unwrap();
        assert_eq!(model, "Accord");
    }
}
