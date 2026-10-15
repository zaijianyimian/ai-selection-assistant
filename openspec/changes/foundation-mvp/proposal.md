# Proposal

## Why

Developers need to translate selected text and understand technical terms without leaving their current application. A local desktop client can use their own model provider without accounts or an application server.

## What Changes

- Create a working Tauri 2, Rust, Vue 3, TypeScript, Vite and Pinia desktop application.
- Store public model settings locally and API keys only in the operating system credential vault.
- Translate text and explain relevant technical terms in one model response, with explicit compatibility modes.
- Capture Windows selections and X11 PRIMARY selections; provide clipboard and manual input fallbacks.
- Provide a reusable translation popup, settings, dark mode, tray actions and clear errors.
- Add automated verification and Windows/Ubuntu build and manual acceptance instructions.

## Capabilities

### New Capabilities

- `provider-settings`: Validated persistent settings, secure credentials and connection testing.
- `technical-translation`: Translation and contextual technical term explanations using compatible model APIs.
- `selection-capture`: Native selection capture, configurable global shortcut and clipboard fallback.
- `desktop-experience`: Independent popup, input, results, status, theme and background lifecycle.

### Modified Capabilities

None; the project directory was empty.

## Impact

New frontend and native application. No self-hosted server, account, database, telemetry, updater or remote code upload. Local OS credential storage and selected provider are the only external integrations.
