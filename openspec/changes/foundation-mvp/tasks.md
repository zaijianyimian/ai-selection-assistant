## 1. Foundation
- [x] 1.1 Inspect workspace and install/initialize project-local OpenSpec; write proposal, design and capability specs.
- [x] 1.2 Scaffold Tauri 2 and Vue 3 with TypeScript, Pinia and build scripts.

## 2. Native modules
- [x] 2.1 Implement validated settings, secure key storage and transactional shortcut updates; test validation and secret-free serialization.
- [x] 2.2 Implement endpoint normalization, model HTTP requests, structured output and compatibility parsing; test with local HTTP fixtures.
- [x] 2.3 Implement Windows capture, Linux PRIMARY/clipboard fallback, shortcut handling and bounded error paths; test available platform paths.
- [x] 2.4 Implement shared job state, independent popup, tray and hide-on-close lifecycle.

## 3. Frontend
- [x] 3.1 Implement translation/input/results/term cards and Pinia job synchronization.
- [x] 3.2 Implement settings, connection test, shortcut editing, key replacement/removal and dark mode.
- [x] 3.3 Verify frontend request/error synchronization and rendered states.
- [x] 3.4 Fix missing Chinese glyphs with a bundled offline font, system font prerequisites and a packaged font license.

## 4. Acceptance
- [x] 4.1 Run frontend tests and npm run build; resolve all failures.
- [x] 4.2 Run Rust tests and cargo check on available native environments; report unavailable verification explicitly.
- [x] 4.3 Attempt npm run tauri dev and verify desktop startup where native prerequisites permit.
- [x] 4.4 Write Windows/Ubuntu setup, security limitations and manual acceptance guide; validate OpenSpec.

## Verification notes

All implementation tasks are complete. Environment-dependent manual acceptance remains explicitly listed in docs/verification.md and docs/manual-acceptance.md; Windows native compilation is blocked by the absent MSVC linker. No real provider key has been supplied. Linux check and the isolated X11 test suite passed.
