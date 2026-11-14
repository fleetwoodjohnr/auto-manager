# Development

## Requirements

- Rust (stable, 1.90+), Node 24 and npm.
- Linux: `webkit2gtk4.1-devel libsoup3-devel openssl-devel gtk3-devel` (Fedora) or `libwebkit2gtk-4.1-dev librsvg2-dev patchelf` (Debian/Ubuntu).
- For end-to-end tests: `tauri-driver` (`cargo install tauri-driver`), `WebKitWebDriver` (Fedora `webkit2gtk4.1`; Debian `webkit2gtk-driver`) and `Xvfb`.

```sh
npm ci
npm run tauri dev          # app with hot reload (data in the normal app-data folder)
AUTO_MANAGER_DATA_DIR=/tmp/am npm run tauri dev   # throwaway data folder
```

## Layout

See [ARCHITECTURE.md](ARCHITECTURE.md). In short:

- `crates/core` holds the logic and its tests;
- `src-tauri` is the thin command layer (`build.rs` generates the IPC allow-list);
- `src` is the React UI.

## Checks (what CI runs)

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked            # 226 tests: core, OBD, migrations, providers (mocked)
npx tsc --noEmit && npm test && npm run build
node --test scripts/release-manifest.test.mjs
node scripts/check-version.mjs
cargo audit && npm audit --omit=dev
```

After changing a Rust type used by the UI, regenerate the TypeScript types:

```sh
npm run bindings      # cargo test export_bindings + scripts/types-index.mjs
```

## End-to-end tests (real app)

```sh
npx tauri build --debug --no-bundle        # target/debug/auto-manager with the UI embedded
node e2e/smoke.mjs                          # every page, light and dark, screenshots in e2e/screenshots
node e2e/workflows.mjs                      # acceptance workflows A–D + accessibility of data pages
node e2e/a11y.mjs                           # axe-core WCAG 2.0–2.2 A/AA on every page, both themes
```

- `e2e/harness.mjs` starts the app through `tauri-driver` on a private Xvfb display, with a fresh data folder each run. Set `AM_USE_DISPLAY=1` to watch it on your own display.
- `e2e/fixture-server.mjs` serves recorded NHTSA and EPA responses from `crates/core/tests/fixtures/`. The app is pointed at it with `AUTO_MANAGER_VPIC_BASE`, `AUTO_MANAGER_NHTSA_BASE`, `AUTO_MANAGER_FUELECONOMY_BASE` and `AUTO_MANAGER_ALLOW_HTTP_LOCALHOST=1`. These variables are for tests only.

**Update test (Workflow E, Linux):**

```sh
e2e/build-update-images.sh /tmp/am-update            # two AppImages signed with a throwaway key
APPIMAGE_EXTRACT_AND_RUN=1 node e2e/update-appimage.mjs /tmp/am-update
```

**Live provider tests** (network; they prove the integrations, which mocks can't):

```sh
AUTO_MANAGER_LIVE=1 cargo test -p auto-manager-core --test live -- --ignored --test-threads=1
```

**Self-test of a built binary** (used by release CI on every platform):

```sh
AUTO_MANAGER_DATA_DIR=$(mktemp -d) target/release/auto-manager --self-test
```

## Environment variables

| Variable | Use |
|---|---|
| `AUTO_MANAGER_DATA_DIR` | Data folder (portable installs, tests) |
| `AUTO_MANAGER_LOG` | Log filter, e.g. `debug` or `auto_manager_core=trace` (default `info`) |
| `AUTO_MANAGER_VPIC_BASE`, `AUTO_MANAGER_NHTSA_BASE`, `AUTO_MANAGER_FUELECONOMY_BASE` | Provider base URLs (tests) |
| `AUTO_MANAGER_ALLOW_HTTP_LOCALHOST` | Allow plain HTTP to 127.0.0.1 (tests) |
| `AUTO_MANAGER_UPDATE_ENDPOINT` | Update manifest URL instead of GitHub (update tests) |
| `AUTO_MANAGER_LIVE` | Enable the live provider tests |

## Conventions

- **Never invent data.** Missing values are shown as missing, and every stored value records its source.
- **Money** is `rust_decimal` in Rust and decimal strings in TypeScript; never floats.
- **Stock** changes only through `inventory.rs`, which writes ledger rows; the database `CHECK`s back it up.
- **Errors** reach the UI as `AppError { kind, message, field }`. Messages are written for the person using the app; details go to the log.
- **UI** uses Simple Suite tokens (`src/styles.css`). Use components from `src/ui`, keep every control keyboard-reachable, keep 24 px minimum targets, and leave no button that doesn't work.
