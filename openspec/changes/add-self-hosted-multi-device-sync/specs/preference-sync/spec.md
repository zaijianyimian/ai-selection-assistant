# Spec Delta

## Purpose

Share a deliberately small set of reading preferences across personal devices without transferring provider credentials, changing device-specific controls or losing existing settings drafts and recoverable save behavior.

## ADDED Requirements

### Requirement: Only confirmed preferences synchronize
Preference synchronization MUST allow exactly theme, target language and explanation style, using their existing validated choices. Both client and service MUST reject additional settings fields. Provider addresses, model credentials, authorization switches, shortcuts and window placement MUST remain local.

#### Scenario: A device changes a permitted preference
- **WHEN** a device saves a valid theme, target language or explanation style while synchronization is enabled
- **THEN** that field can synchronize without replacing other settings on another device

#### Scenario: Remote payload attempts to change model or authorization settings
- **WHEN** a preference payload contains a model endpoint, API Key, memory switch, automatic-analysis switch or any other non-allowlisted field
- **THEN** it is rejected without changing local settings or exposing local credentials

### Requirement: Preference merge uses field versions
Distinct preference fields SHALL merge independently. Conflicting edits to the same field MUST retain both the pending local choice and accepted remote choice until the user resolves them. Device wall-clock differences MUST NOT silently select a winner, and retries MUST NOT generate repeated edits.

#### Scenario: Devices change different preferences
- **WHEN** one device changes theme and another changes target language from the same synchronization state
- **THEN** both changes converge without asking the user to discard either field

#### Scenario: Devices concurrently choose different target languages
- **WHEN** two devices edit target language against the same accepted version and submit incompatible values
- **THEN** a conflict is displayed with both choices, and resolution is checked against the latest accepted version

### Requirement: Imported preferences preserve safe local settings behavior
Remote preference application SHALL use recoverable local settings commits. Unsaved drafts MUST be retained with an external-update notice, existing results MUST retain their request options, and imported settings MUST NOT echo as new outbound changes. Failed application MUST remain retryable rather than being reported as synchronized.

#### Scenario: Preference arrives while a settings draft is open
- **WHEN** a remote preference is committed while the user has unsaved local edits
- **THEN** accepted settings update both windows, the form keeps its draft with a reload notice and existing translation labels remain tied to their original requests

#### Scenario: Local settings file cannot be published
- **WHEN** a received preference cannot be committed locally or a local save cannot durably record its sync intent
- **THEN** prior usable settings are preserved or explicit recovery is shown, durable pending work remains recoverable across restart and model credentials are unchanged

#### Scenario: Imported preference notification is processed twice
- **WHEN** duplicate remote changes or settings events are received
- **THEN** the preference is applied idempotently and does not cause an upload loop or falsely advance successful sync state
