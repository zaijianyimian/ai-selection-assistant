# Spec Delta

## Purpose

让用户在支持的桌面环境中通过可配置的全局快捷键启动翻译，准确了解当前后端与实际绑定状态，并在应用重启、软件升级或设置失败时保留可恢复、不会干扰其他应用的快捷键配置。

## ADDED Requirements

### Requirement: Session-aware shortcut backend
The application MUST select shortcut behavior from the active desktop session. Windows and X11 SHALL retain native registration. GNOME Wayland SHALL offer managed system shortcuts when its prerequisites are present. Other Wayland sessions MUST show manual system-shortcut guidance instead of treating an X11 registration as Wayland support.

#### Scenario: GNOME running a Wayland session
- **WHEN** the session is GNOME Wayland and an X11 display is also exposed
- **THEN** the application selects the GNOME system-shortcut path and does not additionally grab the same action through X11

#### Scenario: Native supported desktop
- **WHEN** the application starts on Windows or X11
- **THEN** it registers the configured native shortcut and reports registration failure separately from backend initialization

#### Scenario: Unsupported desktop or missing prerequisites
- **WHEN** automatic integration is unavailable or required desktop tools or schemas are missing
- **THEN** the application explains the limitation, displays the clipboard command for manual setup and retains in-app clipboard translation

### Requirement: Explicit per-user shortcut setup
GNOME system-shortcut creation MUST follow an explicit user enable action in that user's desktop session. Installing or upgrading the DEB MUST NOT alter any user's desktop shortcuts. The application MUST provide enable, disable, refresh and explicit reapply controls without showing a configuration dialog on every launch.

#### Scenario: Fresh installation and first launch
- **WHEN** the package is installed and the application first starts without a managed binding
- **THEN** no system shortcut has been created automatically and the user can choose to enable it after seeing the command and copy-first behavior

#### Scenario: Normal relaunch
- **WHEN** the user starts the application again with an unchanged managed binding
- **THEN** startup only inspects the binding and neither creates a duplicate nor prompts for configuration

### Requirement: Persistent fixed command binding
A managed GNOME shortcut SHALL invoke the installed application's stable clipboard-translation command, without process identifiers or temporary bus addresses. It MUST remain configured when the application quits and after an upgrade that retains the installation path. Activating it SHALL start the application when absent or forward to its existing instance when present.

#### Scenario: Application already running
- **WHEN** the user copies text and activates the managed shortcut
- **THEN** the existing instance receives one translation request without a second persistent instance or duplicate trigger

#### Scenario: Application has exited
- **WHEN** the user copies text and activates the managed shortcut after the application has fully exited
- **THEN** the application starts and presents the translation result or an actionable capture or provider error

#### Scenario: Upgrade and login again
- **WHEN** the installed application is upgraded at the same path or the user logs out and logs back in
- **THEN** the binding is retained and usable without having to add it again

### Requirement: Accurate shortcut status
The application MUST distinguish backend capability, saved desired shortcut, observed system binding and operation errors. Plugin initialization alone MUST NOT imply successful registration. For GNOME, configuration readback SHALL be labelled as configured rather than proof of successful key delivery, and the observed trigger SHALL be displayed when it differs from the saved preference.

#### Scenario: Native registration rejected
- **WHEN** backend initialization succeeds but native key registration fails
- **THEN** the UI reports the failure and does not present the shortcut as ready

#### Scenario: System binding exists
- **WHEN** the application's managed command and nonempty key binding are present in GNOME configuration
- **THEN** the UI shows the observed configured shortcut and explains that actual delivery can be checked by copying text and pressing the shortcut

#### Scenario: Other preferences are saved
- **WHEN** shortcut integration is unavailable and the user saves settings without changing the shortcut
- **THEN** shortcut unavailability alone does not reject that save or clear the unrelated preferences

### Requirement: Respect externally managed configuration
Inspection and ordinary startup MUST NOT overwrite or recreate bindings changed or removed in system settings. A managed binding MUST have a stable application-specific identity. Repeated enable or reapply operations MUST be idempotent. Disabling MUST remove only an entry still identifiable as application-owned and MUST preserve unrelated shortcuts.

#### Scenario: User changes or deletes the binding externally
- **WHEN** refresh or startup discovers an external change or removal
- **THEN** the application shows the observed state without restoring its saved preference until the user explicitly reapplies it

#### Scenario: Stable path contains another command
- **WHEN** the application's expected path has been repurposed for a different command
- **THEN** normal enable, disable and reapply operations refuse to overwrite or remove that command and offer manual resolution

#### Scenario: Enable is repeated or shortcut is disabled
- **WHEN** a user repeats enable with matching configuration or disables an application-owned binding
- **THEN** repeated enable creates no duplicate, and disable preserves all other applications' bindings

### Requirement: Conflict handling does not claim exhaustive detection
Before publishing a managed binding, the application MUST reject invalid or unsupported trigger syntax and conflicts detectable in the system shortcuts it can inspect. It MUST NOT overwrite a competing binding or claim all compositor reservations are detectable. If an apparently configured shortcut does not trigger, the UI SHALL offer changing the key and checking system shortcuts.

#### Scenario: Observable conflict
- **WHEN** the requested combination conflicts with an inspectable system binding
- **THEN** the operation reports a conflict, preserves the prior configuration and lets the user choose another combination

#### Scenario: Unobservable compositor reservation
- **WHEN** readback succeeds but the desktop does not deliver the requested key
- **THEN** the configured state does not claim end-to-end verification and guidance directs the user to change the combination or inspect system shortcuts

### Requirement: Recoverable shortcut mutations
Enable, key changes and disable MUST either commit their local and desktop state or report failure and compensate reversible mutations. Failed settings saves MUST preserve prior settings and memory when compensation succeeds. Incomplete compensation MUST be reported, survive restart and retain the existing recovery gate on model requests. Recovery data MUST contain no credentials or copied text.

#### Scenario: Desktop update or settings publication fails
- **WHEN** an operation fails after partially changing a shortcut
- **THEN** compensation restores the previous application-owned binding and local state without replacing unrelated desktop entries, and no success is reported

#### Scenario: Compensation fails
- **WHEN** the previous consistent state cannot be restored
- **THEN** the application exposes a recovery error, preserves non-secret recovery information across restart and blocks model requests until consistency is recovered

#### Scenario: Another actor edits configuration during an operation
- **WHEN** a concurrent desktop edit is detected during apply or rollback
- **THEN** the application does not blindly publish an old complete shortcut list and reports unresolved ownership or consistency instead of claiming success

### Requirement: Wayland clipboard and placement boundaries
The Wayland shortcut MUST translate explicitly copied text through the same validation and error handling as in-app clipboard translation. It MUST NOT inject copy keystrokes or imply automatic access to another application's selection or its coordinates. Existing compositor-managed popup placement behavior SHALL remain intact.

#### Scenario: Clipboard is empty or contains an image
- **WHEN** the user triggers translation without readable text in the clipboard
- **THEN** an actionable error is displayed and no translation model request is sent

#### Scenario: User reopens a moved popup
- **WHEN** the user moves the popup and triggers another translation within the same Wayland application run
- **THEN** the existing popup is reused with its compositor-managed position and the UI continues to describe copy-first operation

### Requirement: Local and lightweight integration
Shortcut ownership, enablement and recovery state MUST remain local to each device. The integration MUST require no additional model, runtime interpreter or background daemon. A delivered DEB MUST retain dependency checks and remain smaller than 30,000,000 bytes.

#### Scenario: Preferences are synchronized
- **WHEN** another device receives synchronized preferences
- **THEN** its shortcut, enablement, desktop ownership and recovery state remain unchanged

#### Scenario: Release candidate is checked
- **WHEN** the final DEB is verified
- **THEN** required command and desktop-tool dependencies are checked and the package is rejected if its actual size is at least 30,000,000 bytes
