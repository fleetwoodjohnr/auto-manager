# Test report

These results come from running the checks on the development machine: Fedora 44, Linux 7.3 x86-64, WebKitGTK 2.54, Rust 1.99, Node 24.

Mocked responses are never counted as proof that a provider works. The live results below are the evidence for that.

## Automated suites

| Suite | Command | Result (2026-11-14) |
|---|---|---|
| Rust: core, OBD, migrations, providers, sync, scanning (recorded responses + local mock server) | `cargo test --workspace --locked` | **258 passed**, 0 failed (255 core, 3 OBD) |
| Lint | `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --check`, `npx tsc --noEmit` (strict) | clean |
| Frontend unit | `npm test` | **14 passed** (formatting, units, theme, UI kit, every text translated to Spanish and French with its slots) |
| Release script | `node --test scripts/release-manifest.test.mjs` | 3 passed (signature verification, tamper and version checks, complete-platform manifest, changelog notes) |
| Dependencies | `cargo audit`, `npm audit --omit=dev` | 0 vulnerabilities. Warnings only for unmaintained/unsound GTK and D-Bus crates used by Tauri on Linux (see SECURITY.md). One moderate advisory in a development-only test dependency (`minimatch` under webdriverio) |

Notable tests:

- **VIN**: check digit, I/O/Q, partial, invalid input, routing.
- **Fitment**: confirmed needs a verified full match; make and model alone give nothing; unknown year; conditional on missing detail, resolved when supplied; engine and trim variants; displacement tolerance; ambiguous 4x2 vs FWD; exclusions; conflicting records.
- **Inventory**:
  - receiving and weighted average cost; never negative;
  - reservations protect stock; consuming once; releasing;
  - transfers, damage, counts, returns; undo as a compensating movement, once;
  - duplicate SKU; stale edit forms;
  - **8 concurrent writers racing for the last units** (exactly two win);
  - rollback leaves no trace.
- **Pricing**: markup vs margin ($50 → $70.00 vs $83.33), precedence, rounding modes, minimum price, fixed price, quantity breaks, decimal precision, profile validation.
- **Jobs**: totals with taxable and non-taxable lines, never deducting twice, failed completion rolls back, cancelling releases reservations.
- **Database**:
  - migrates from every earlier version with a backup hook, refuses newer schemas;
  - search triggers;
  - stock `CHECK`s hold even against raw SQL.
- **Backup**: round trip, tampering, truncated archive, path traversal.
- **Sync bundle**: copy between two data folders, wrong passphrase, tampering.
- **Providers**:
  - every vPIC fixture (clean, check-digit error, partial, unknown);
  - POST batch decode; cache, then offline fallback marked stale;
  - 429 retry with `Retry-After`, giving up, no retry on 404, offline detection, refusing plain HTTP;
  - recalls with day-first dates; ratings; Canadian specs; EPA one-or-many options.
- **Imports**: inventory spreadsheet (mapping, duplicates, errors), fitment spreadsheet with conditions, PIES then ACES with vehicle and attribute maps, XXE refusal, XLSX formula and leading-zero handling.
- **Fractional quantities**: 4.5 L received, 0.25 issued, 1.75 reserved, all exact; whole-unit items refuse 1.5; a version-4 database's stock migrates intact; exports in units.
- **Sync** (two simulated computers through one folder): edits both ways with the later change winning; deletions; stock taken twice corrected once and reported; the same SKU on both merged with both stocks; a job completed on both deducted once (confirmed to fail if the stable movement id is removed); references to a deleted customer cleared; invoice numbers used twice renumbered; photos travel and nothing in the folder is readable; wrong passphrase refused.
- **Scanning**: a Code 39 door-jamb label decoded; a rendered VIN-plate photo read by the bundled OCR models; candidates only with a correct check digit; single misreads corrected and labelled (all candidates listed when more than one fits); nothing found inside longer character runs; non-pictures refused.
- **Values from manuals**: a capacities page (oil 4.4 qt, 0W-20, coolant, DOT 3, engine torque only when quoted with rpm, wheel-nut torque, battery group, plug gap, front/rear pressures); intervals with km converted; nothing from unrelated text; no failure on any Unicode text (this test found and fixed two byte-offset bugs).
- **Bulletins**: 123 communications parsed for a 2019 Sentra; document links only from static.nhtsa.gov.
- **OBD**: scripted CAN adapter (codes, VIN, live data, freeze frame, never sends mode 04), refusal of 8 kinds of write or unsafe requests, older protocols.

## Live provider tests

`AUTO_MANAGER_LIVE=1 cargo test -p auto-manager-core --test live -- --ignored`, run 2026-11-14 against the real services: **2 passed**.

- **NHTSA manufacturer communications**: list for a 2019 Nissan Sentra (over 50), a bulletin's documents, and its PDF downloaded from static.nhtsa.gov.

- **vPIC**: decode of `1HGCM82633A004352` (2003 Honda Accord), batch decode of two VINs by POST, makes, models.
- **NHTSA**: recalls for a 2019 Nissan Sentra (all dates parsed); 5-Star ratings.
- **vPIC Canadian specifications**: wheelbase present.
- **EPA**: options and one vehicle record with combined MPG.

Also found while recording fixtures: vPIC's `DecodeVINValuesBatch` rejects GET with "does not support http method 'GET'". The client uses POST.

## End-to-end, real app (tauri-driver + WebKitWebDriver, Xvfb)

| Run | Result |
|---|---|
| `e2e/smoke.mjs`: all 10 sections, light and dark, screenshots | passed |
| `e2e/a11y.mjs`: axe-core WCAG 2.0/2.1/2.2 A+AA, all sections, both themes | **0 violations** (after fixing 22 px tree buttons to 24 px) |
| `e2e/workflows.mjs`: acceptance workflows | **A, B, C, D passed**; then **0 violations** on all 8 vehicle tabs and the job page with data (after fixing an ARIA reference on the closed search box) |
| `e2e/update-appimage.mjs`: **Workflow E** with two locally built signed AppImages | **passed** |

What each workflow checked:

- **A**: VIN decoded through the fixture provider. Unavailable details named (drivetrain, oil). Saved. The app was restarted **with no provider at all** and reopened the vehicle with sources and gaps shown.
- **D**: 40% markup rule, with Try it showing $70.00. Part and item created through the UI with cost $50 and 4 received; price $70.00. Fitment for 2016–2019 Nissan Sentra with 1.8 L, verified. Found by part number through global search.
- **B**: VIN decoded and its Parts in Stock opened. Confirmed and conditional matches were in separate groups, and the deciding detail (brake package) was asked for. Quantities and prices were shown. Reserving 1 took available from 4 to 3 while on hand stayed 4.
- **C**: A PDF was imported and indexed. Where Is It? found "starter motor" on page 1 of the vehicle's manual, and the viewer opened it. The part's fitment was viewed, and the part was saved to a list and added to an estimate at $70.00 with its price explanation.
- **E**:
  - interrupted download: refused, AppImage unchanged (SHA-256);
  - wrong signature: refused, unchanged;
  - good package: verified and installed after "Restart and install", one pre-update backup;
  - the relaunched app reported **0.1.1** and still had the saved vehicle.

## Cross-platform CI

GitHub Actions (`ci.yml`) on the pushed code: format, lint, all tests and the frontend build pass natively on **Linux x64 and ARM64, Windows x64 and ARM64, and macOS Intel and Apple silicon**, plus the security audit and the Linux end-to-end job. The first Windows run found a real bug (the pdf.js asset path used `URL.pathname`, which gives `/D:/…` on Windows), now fixed.

## End-to-end additions

| Run | Result |
|---|---|
| `e2e/smoke.mjs` | Every page in light and dark, **and in Spanish and French** (no untranslated slot or error on any page) |
| `e2e/workflows.mjs` | Also: the model's 123 service bulletins listed and one added to the library; the oil capacity found in an imported owner's manual and saved with "page 1" cited |
| `e2e/sync.mjs` | Two app instances (separate data folders) set up sync to one folder through the UI; a vehicle saved on the first appears on the second, a customer added on the second appears on the first; nothing readable in the folder |
| `e2e/scan.mjs` | The Scan dialog opens the camera (WebKit's mock device), a frame is read and no VIN is invented from the test pattern, the dialog passes the accessibility audit, and the running app's bundled OCR models read a VIN-plate photo |

Picture reading takes about 1.2 s for a VIN photo and 1.9 s for a full-HD camera frame (optimized build).

## New in the final pass

`e2e/workflows.mjs` also checks attachments and saved searches in the real app (both passed):

- **Attachments**: a PNG attached to the job showed as a thumbnail loaded from the app and was then removed. The file goes in through the same command the native file picker feeds, because WebDriver can't drive the native picker.
- **Saved searches**: an inventory search was saved under a name, cleared, and restored with one click.

## Packaging (Linux x86-64)

The release build was signed with the project's updater key (`npx tauri build`, 2026-11-14):

| Package | Size | Checked |
|---|---|---|
| `Auto Manager_0.1.0_amd64.AppImage` + `.sig` | 121 MiB | `--self-test` run from the AppImage: `{"version":"0.1.0","schema":4,"integrity":"ok"}` |
| `Auto Manager_0.1.0_amd64.deb` + `.sig` | 15 MiB | Control file: `auto-manager` 0.1.0 amd64, depends on `libwebkit2gtk-4.1-0, libgtk-3-0` |
| `Auto Manager-0.1.0-1.x86_64.rpm` + `.sig` | 15 MiB | `rpm -qip`: MIT, x86_64. Contents are the binary, `.desktop` entry and icons. Requires `libgtk-3.so.0` and `libwebkit2gtk-4.1.so.0` |

The release binary `target/release/auto-manager --self-test`, run on a fresh data folder, printed `{"version":"0.1.0","schema":4,"integrity":"ok"}`.

The .deb and .rpm weren't installed system-wide on this machine. The update route for those formats is in RELEASING.md.

## Not verified here

- Windows and macOS installers and in-app updates: the release workflow builds them on a version tag; not run yet (no release tagged). Their tests and builds pass in CI.
- Real GitHub release updates (stable and beta): see the checklist in RELEASING.md.
- OBD-II with real adapters, or with an independent ELM327 emulator: installing the third-party `ELM327-emulator` package wasn't permitted in this session, so the client is tested only against its scripted adapter.
- The camera on Windows and macOS (Linux was exercised through WebKit's mock camera).
- Code-signed or notarized builds.
