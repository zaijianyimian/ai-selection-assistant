## Purpose

Keep translation available alongside the user's current work through a reusable independent desktop window and accessible controls.

## ADDED Requirements

### Requirement: Translation interface
The application SHALL provide manual input, loading and failure states, separate original and translated text, term cards, copy translation, dark mode and provider settings.

#### Scenario: Request lifecycle
- **WHEN** text is translated
- **THEN** a loading state precedes a result or actionable error and duplicate jobs are rejected

### Requirement: Offline Chinese glyph coverage
The application SHALL bundle a licensed local Chinese font so that interface labels and translated text render without external font requests, even when the desktop has no Chinese fonts installed.

#### Scenario: Minimal Linux desktop
- **WHEN** the application runs on a Linux desktop without a Chinese system font
- **THEN** webview text uses the bundled Chinese font and native-menu font prerequisites are documented

### Requirement: Background lifecycle
Closing a window MUST hide it without exiting. Users SHALL be able to reopen through a tray menu and quit explicitly.

#### Scenario: Popup dismissed
- **WHEN** the translation popup is closed
- **THEN** the app remains available to the global shortcut

#### Scenario: Tray quit
- **WHEN** the user chooses quit from the tray
- **THEN** the application exits
