# Spec Delta

## Purpose

Protect a user's provider configuration and learned terminology state while settings are being saved, so recoverable failures do not discard previously usable data or silently change active behavior.

## ADDED Requirements

### Requirement: Recoverable settings commit preserves memory state
The application MUST preserve previously committed settings, profiles, active profile, analysis progress and stale-write protection when a settings save fails and compensation succeeds. Profile invalidation MUST become effective only with a successful relevant settings change. Failure messages MUST NOT expose credentials.

#### Scenario: Credential operation fails after toggling memory
- **WHEN** a user changes a memory or automatic-analysis switch and the requested credential operation fails
- **THEN** save reports failure, the prior switches and profiles remain active, and prior analysis progress is preserved

#### Scenario: Shortcut reservation or configuration publication fails
- **WHEN** a relevant switch change is submitted and shortcut reservation or configuration publication fails
- **THEN** the prior settings and memory state remain effective after compensation, and any newly reserved shortcut or changed credential is restored

#### Scenario: Memory invalidation cannot commit
- **WHEN** the memory state cannot be invalidated while committing a relevant switch change
- **THEN** the application rolls back staged configuration and reversible external changes, reports failure and preserves existing profiles and progress

### Requirement: Successful commits retain stale task protection
The application MUST invalidate old translation and analysis writes when a relevant memory-setting change commits successfully. It MUST preserve retained terms, keep normal translation usable, and publish the accepted settings consistently to both windows.

#### Scenario: Memory is disabled during a translation
- **WHEN** disabling memory is saved successfully before an earlier translation finishes
- **THEN** that translation may still be displayed but cannot repopulate term occurrences or activate an old profile

#### Scenario: Change succeeds and application restarts
- **WHEN** a relevant settings change succeeds and the application is restarted
- **THEN** the accepted settings and invalidated profile state agree, and retained terms remain available

### Requirement: Compensation failure has explicit recovery state
If restoring a configuration, credential or shortcut fails, the application MUST report incomplete recovery, MUST NOT report a successful save, and MUST prevent new translation and analysis requests until consistent settings are recovered. It MUST preserve recoverable prior memory data and MUST NOT store plaintext credentials as a recovery mechanism.

#### Scenario: Restoring a credential fails
- **WHEN** save fails and the prior credential cannot be restored
- **THEN** the user receives a recovery error, existing memory data remains preserved, and model requests remain blocked until a subsequent successful settings recovery
