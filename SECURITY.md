# Security and privacy

## Reporting a problem

Please report vulnerabilities privately through GitHub's **Report a vulnerability** (Security tab) rather than a public issue.

## Privacy by default

- **No account and no telemetry.** Nothing is sent anywhere except the lookups you ask for:
  - VIN and year/make/model to NHTSA and the EPA;
  - release checks to GitHub, when update checks are on;
  - a service bulletin PDF from static.nhtsa.gov, when you choose *Add to library*.
- **Your records stay local:** vehicles, customers, inventory, prices, documents. They're never uploaded. Encrypted transfer bundles are written only where you choose.
- **Logs** (in the data folder's `logs/`, 7 days) contain no customer details or request bodies. A redaction filter removes secret-looking values, bearer tokens and email addresses as a backstop.

## How the app is built to be safe

| Area | Measure |
|---|---|
| IPC boundary | The webview is treated as untrusted. Only the app's own commands are callable; `src-tauri/build.rs` generates the allow-list from the `#[tauri::command]` functions. No shell, file-system or HTTP plugin permissions are granted. Every argument is validated in the core (ids, lengths, enumerations, decimals) |
| Content Security Policy | `default-src 'self'`. No remote scripts, styles or frames. `connect-src` is limited to IPC. Workers come only from the app. PDF scripting is off |
| Files | Files are read only from paths you pick in a native dialog. PDFs are checked by their `%PDF-` signature and a 512 MB cap, then stored by SHA-256. Spreadsheets are capped at 50 MB and 50,000 rows. XML imports refuse documents that declare entities (no XXE). Exports can't be written into the app's data folder, and CSV cells that look like formulas are neutralized |
| Backups | Restores check every file's SHA-256, refuse unexpected paths (no zip-slip), run SQLite `integrity_check` and `foreign_key_check`, and refuse newer schemas. All of this happens in a scratch folder, before anything is replaced. A backup of the current data is taken first |
| Data integrity | SQLite with WAL, `synchronous=FULL` and foreign keys. Migrations are append-only and run in a transaction, with a backup first. Stock can't go negative or over-reserved, enforced by `CHECK` constraints. Money uses decimal arithmetic |
| Network | HTTPS only (plain HTTP only to `127.0.0.1` when a test sets an environment variable). Bounded timeouts, response size limit, at most 3 redirects |
| Updates | Packages are signed with minisign (`TAURI_SIGNING_PRIVATE_KEY`, kept in GitHub secrets) and checked against the public key built into the app before installing. Releases are verified again in CI before publishing. Only newer versions are offered. A backup is taken before installing |
| Encrypted bundles and sync | Argon2id key derivation from your passphrase (at least 10 characters), with a random salt, and ChaCha20-Poly1305 authenticated encryption. Wrong passphrases and tampering are detected. In a sync folder every file is encrypted (state, documents, photos); only a salt and a check value are readable. The derived key is kept in the data folder's `sync.json`, next to the (unencrypted) database it protects copies of |
| Camera | Used only while the Scan dialog is open. On Linux the app grants video (never microphone) access to its own pages only; Windows and macOS ask you. Pictures are decoded on this computer and not kept |
| Bundled models | The text-recognition models are downloaded at build time and checked against pinned SHA-256s; a changed file stops the build |
| QuickBooks | Read only: Auto Manager sends only GET requests to the QuickBooks API (checked in tests). Sign-in is OAuth 2.0 in your browser; Intuit's answer comes back through a static HTTPS page to a one-time listener on `127.0.0.1` that accepts one answer for 5 minutes, and must carry the random `state` value (48 hex characters) it was started with. The company id must be a number. Intuit app keys entered in Settings are kept in the OS keyring too; the Client Secret is never sent back to the window. The refresh token is kept in the OS keyring and replaced on every refresh; the access token is only in memory; neither goes into the database, backups, the sync folder or logs. Disconnecting revokes the sign-in at Intuit |
| OBD-II | Read-only allow-list: modes 01, 02, 03, 07 and 09 02, plus adapter setup. Clearing codes and any write to a control module are refused in code |
| Dependencies | `cargo audit` and `npm audit` run in CI |

## Known limits

- Unsigned builds (no Apple or Windows code-signing certificate configured) trigger Gatekeeper and SmartScreen warnings. The updater's own signature check is separate, and always required.
- **The Intuit app's client secret is built into release builds.** Intuit's OAuth needs one, and a desktop app can't keep it hidden: someone could extract it and present themselves as "Auto Manager" in an Intuit sign-in. They still couldn't reach anyone's QuickBooks without that person signing in and approving them, and every sign-in shows Intuit's own page. If the secret is misused, it's replaced in the Intuit developer portal and the next release carries the new one (older copies then can't connect until updated). The secret isn't in the repository; CI injects it from GitHub secrets.
- The database isn't encrypted at rest. Use your operating system's disk encryption. Encrypted bundles are for moving data, not for everyday storage.
- `cargo audit` reports "unmaintained" and "unsound" warnings in the GTK and D-Bus crates that Tauri uses on Linux (`glib`, `zvariant` and related). They have no fix within Tauri 2.x and aren't vulnerabilities in Auto Manager's use of them.
