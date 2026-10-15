## Purpose

Capture selected text from other desktop applications reliably while providing explicit alternatives for restricted environments.

## ADDED Requirements

### Requirement: Configurable global trigger
The application SHALL register Ctrl+Shift+Q by default, persist a configurable replacement and show shortcut registration failures.

#### Scenario: Conflicting shortcut
- **WHEN** a replacement cannot be registered
- **THEN** an error is shown and the previous shortcut remains usable

### Requirement: Platform selection and fallback
The application SHALL capture Windows text by simulated copy, prefer PRIMARY on X11 and provide clipboard-only translation on Wayland and Windows. Capture failures MUST NOT crash or silently translate stale Windows clipboard text.

#### Scenario: Windows selection
- **WHEN** the global shortcut is pressed over selected text
- **THEN** capture precedes popup focus and prior clipboard text is restored when safe

#### Scenario: No selection
- **WHEN** simulated copy does not change the Windows clipboard
- **THEN** an empty-selection or permission error is displayed

#### Scenario: Wayland fallback
- **WHEN** a user explicitly copies text and invokes clipboard translation
- **THEN** the clipboard text is translated without injecting key events
