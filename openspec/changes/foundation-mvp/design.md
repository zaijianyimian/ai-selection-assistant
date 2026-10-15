# Design

## Context

The initial workspace is empty. Node 24 is available; Rust and native build prerequisites need verification. See proposal.md for motivation. Windows 11 and Ubuntu desktop sessions are the target environments.

## Goals / Non-Goals

Goals: a working local MVP with native capture, secure configurable provider access, useful term explanations and a recoverable desktop workflow.

Non-goals: accounts, cloud sync, OCR, screenshots, RAG, vector storage, MCP, automatic updates and persistent translation history. SQLite is unnecessary for a single settings document.

## Decisions

1. Rust owns selection, shortcuts, HTTP and keyring. Vue invokes narrow commands; the vault never returns its key to the frontend. The password field holds only a newly typed replacement and clears after save/test. Credentials in URLs are rejected. HTTP redirects are disabled to avoid forwarding credentials.
2. Persist validated non-secret JSON in Tauri's app config directory through a same-directory temporary file. Keep a memory snapshot for reliable reads. Serialize settings changes and restore prior shortcut/credential if persistence fails.
3. Use OpenAI-compatible Chat Completions. A bare origin appends `/v1/chat/completions`; an existing path appends `/chat/completions`; a full endpoint stays unchanged. Do not insert `/v1` into arbitrary provider paths.
4. Auto mode first requests strict JSON Schema and retries once without response_format only when a provider explicitly rejects structured output. JSON and prompt-only settings avoid failed probe calls on known incompatible providers. Translation and terms always share one successful completion. Invalid model output is an error, not invented content.
5. Windows waits for shortcut modifiers to be released before SendInput Ctrl+C, detects clipboard sequence changes and restores previous text only if no later clipboard write occurred. Rich clipboard formats cannot be fully preserved by the text MVP; explicit clipboard mode avoids all writes. X11 uses arboard PRIMARY; Wayland uses wl-paste with a deadline. No shell interpolation is used for selections.
6. Create both windows at startup; popup starts hidden. Store job snapshots in Rust and emit update notifications, so late webviews can load the current job. Capture finishes before popup focus changes. A busy guard prevents overlapping capture/translation. Closing hides windows; the tray provides reopen and explicit quit. On Wayland shortcut registration is best effort; in-app clipboard translation remains available.
7. Validate text length, timeouts, temperature and model settings before requests. Bound response size and redact provider/network errors; never log prompts or secrets. Rendering uses escaped Vue text, never model-generated HTML.
8. Bundle local variable Noto Sans SC font subsets under OFL-1.1. UTF-8 source correctness does not guarantee glyph availability on minimal Linux installations; desktop native menus additionally use the documented system CJK font prerequisite.

## Risks / Trade-offs

- Windows elevated applications can reject synthetic input → show permission guidance and offer explicit copy or manual input.
- Clipboard restoration preserves text but may lose rich formatting → document the limitation and expose clipboard-only mode.
- Wayland compositors can deny global shortcuts → allow compositor custom shortcut to launch the single-instance application after copying.
- Secret Service may be locked or unavailable → report vault failure without plaintext fallback.
- No real provider key or second physical OS is supplied → use local HTTP fixtures and provide manual checks without claiming live provider success.

## Migration Plan

No existing data. `npm ci`, install native prerequisites, then `npm run tauri dev`. Remove the application to roll back; delete the key through settings before uninstalling if desired.
