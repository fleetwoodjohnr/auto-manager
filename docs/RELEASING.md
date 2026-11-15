# Releasing and updates

## One-time setup

1. **Create the repository** `fleetwoodjohnr/auto-manager` on GitHub and push `main`.

2. **Updater signing key.** A key was generated on the development machine at `~/.tauri/auto-manager.key`, with the public key in `~/.tauri/auto-manager.key.pub`. That public key is already in `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`. The key has **no password**: it is protected only by the file's permissions (0600) and by GitHub's secret storage.
   - To use a password instead, regenerate the key with `npx tauri signer generate -w ~/.tauri/auto-manager.key -f`, put the new public key in `tauri.conf.json`, and set the password secret below.
   - **Do this before the first public release.** Changing the key later means installed copies can't verify new updates.
   - **Back the key up.** If it's lost, existing installations can never be updated again.

3. **Repository secrets** (Settings → Secrets and variables → Actions):

   | Secret | Required | Value |
   |---|---|---|
   | `TAURI_SIGNING_PRIVATE_KEY` | **yes** | Contents of `~/.tauri/auto-manager.key` |
   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | if the key has one | Its password |
   | `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | optional | Developer ID certificate (base64 .p12) and notarization credentials. Without them macOS builds are ad-hoc signed and users must allow the app in Privacy & Security |
   | `WINDOWS_SIGN_COMMAND` | optional | A signing command that takes the file path as `%1` (e.g. Azure Trusted Signing or `signtool`). Without it Windows shows SmartScreen on first run |

## Cutting a release

```sh
node scripts/bump-version.mjs 0.2.0
cargo check                              # refreshes Cargo.lock
# add "## [0.2.0] - YYYY-MM-DD" to CHANGELOG.md: it becomes the release and update notes
git commit -am "release: v0.2.0" && git tag v0.2.0 && git push && git push --tags
```

`.github/workflows/release.yml` then runs these steps:

1. **build**: six native runners: Linux x64/ARM64 (deb, rpm, AppImage), Windows x64/ARM64 (NSIS), macOS Intel/Apple silicon (app, dmg). On each, it:
   - checks that the tag matches every version field (`scripts/check-version.mjs`);
   - runs the tests;
   - builds and signs the updater packages;
   - installs, runs, updates and uninstalls every package (`scripts/package-smoke.py`, adapted from Simple Connect):
     - **Windows**: the NSIS installer silently, the binary's architecture, the bundled text-recognition models, `--self-test`, a 10-second launch, the update rehearsal, then the uninstaller (every file must go);
     - **macOS**: the DMG mounted and the app copied out, `codesign --verify --deep --strict`, the architecture, the models, `--self-test`, a launch, the update rehearsal (signature verified again afterwards);
     - **Linux**: the .deb installed, self-tested and removed on a clean `ubuntu:24.04` container, the .rpm the same on `fedora:latest` (so the declared dependencies really install), and the AppImage launched under Xvfb and updated.
   - The **update rehearsal** runs the real updater (`src-tauri/examples/update_rehearsal.rs`, CI-only, never shipped) as an installed "0.0.9" against a local copy of `latest.json`. The first download is corrupted and must be refused with nothing changed; the second, genuine one must install, and the data must be untouched.

   Run the workflow by hand (Actions → Release → Run workflow) to test every installer without publishing anything.
2. **publish**: `scripts/release-manifest.mjs`:
   - verifies **every** updater signature against the public key in `tauri.conf.json`, and that each signature's version matches the tag;
   - requires all 10 platform/bundle entries and both DMGs;
   - writes `latest.json`, `SHA256SUMS` and the release notes (from CHANGELOG.md);
   - creates the GitHub release (a `-beta.N` tag becomes a pre-release, which the app doesn't offer; it's only for downloading by hand).

## How updates reach users

Updates work the same way as in the other Simple Suite apps (Simple Connect, Simple Share). The app reads `https://github.com/fleetwoodjohnr/auto-manager/releases/latest/download/latest.json` (GitHub's "latest" never points at a pre-release) through the Tauri updater plugin:

- with **Check for updates automatically** on, it checks at start-up and every 24 hours while it runs (never in development builds);
- a newer version shows a notice in the corner, **Review update**, which opens Settings → Updates with the release notes;
- **Install and restart** downloads the package, verifies its signature against the public key built into the app, backs up the data (`pre-update-…zip`), installs and relaunches. It refuses while there are unsaved changes.

Only a version **newer** than the installed one is offered, so there are no downgrades.

| Installed as | Update route |
|---|---|
| Windows NSIS installer | In-app. Installs per user in passive mode; the installer closes and restarts the app |
| macOS .app / .dmg | In-app. Replaces the .app bundle |
| Linux AppImage | In-app. Replaces the AppImage file (it must be writable by you) |
| Linux .deb | In-app. Runs `dpkg -i` through a system password prompt (pkexec) |
| Linux .rpm | In-app. Runs `rpm -U` through a system password prompt (pkexec) |
| Flatpak, distribution repositories, builds from source | **Not from inside the app.** Update the way it was installed. No Flatpak is published at present. Settings → Updates says so when the app wasn't installed from one of the packages above |

A .deb doesn't work on RPM-based distributions, and an .rpm doesn't work on Debian-based ones. The AppImage works on both.

## Checking a release by hand (Workflow E)

The update path was tested locally with real AppImages. `e2e/build-update-images.sh` builds two AppImages signed with a throwaway key, and `APPIMAGE_EXTRACT_AND_RUN=1 node e2e/update-appimage.mjs <dir>` checks four things:

1. an interrupted download is refused and the installed AppImage doesn't change;
2. a package that doesn't match its signature is refused and the AppImage doesn't change;
3. a good package installs after "Install and restart", with one pre-update backup;
4. the relaunched app reports the new version and still has the saved data.

To check the real GitHub path after the first two releases:

1. Install **v0.1.0** from its release page and start it. Add a vehicle so there's data to keep.
2. Publish **v0.1.1** with the workflow above.
3. v0.1.0 still has the older Updates screen: open **Settings → Updates → Check for Updates**, expect "What's new in 0.1.1", then **Download update** and **Restart and install**.
4. Expect the app to come back as 0.1.1, with the vehicle still there and a `pre-update-…zip` in the backups folder.
5. From 0.1.1 on, the next release appears as a notice: **Review update → Install and restart**.
6. Repeat on Windows, macOS and one deb and one rpm system, so each package type has been exercised.
