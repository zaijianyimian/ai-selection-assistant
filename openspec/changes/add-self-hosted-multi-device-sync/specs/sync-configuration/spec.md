# Spec Delta

## Purpose

Let a user deliberately connect desktop devices to a personal synchronization service, with independent credentials, explicit scheduling consent and recoverable connection state that never prevents local translation.

## ADDED Requirements

### Requirement: Synchronization requires explicit opt-in
Synchronization and background scheduling MUST default off. Connection testing MUST NOT upload retained data or enable synchronization. The first connection SHALL explain the exact data scope and require consent before importing existing local data.

#### Scenario: Existing installation is upgraded
- **WHEN** an existing user opens the new client without enabling synchronization
- **THEN** no synchronization requests or remote data uploads occur and local translation and memory continue to work

#### Scenario: User tests connection credentials
- **WHEN** the user tests a service address and dedicated synchronization password
- **THEN** the client reports authentication and protocol compatibility without uploading memory, changing model settings or registering a persistent device

### Requirement: Sync credentials remain independent and protected
The client MUST store persistent device credentials only in the operating-system credential vault, independently of model credentials. It MUST expose only credential presence to the UI and MUST NOT persist the login password, log secrets or use a plaintext fallback. Failed connection saves MUST preserve the previously accepted connection or expose an explicit recovery state.

#### Scenario: Credential vault cannot be written
- **WHEN** a new connection authenticates but its device token cannot be saved securely
- **THEN** the client does not report a fully enabled connection, preserves the prior usable configuration and leaves the model API Key unchanged

#### Scenario: User finishes testing or enrollment
- **WHEN** a password test or enrollment succeeds or fails
- **THEN** the temporary password field is cleared and saved device tokens are not returned to the frontend

### Requirement: Connection identity prevents unintended workspace transfer
The client SHALL bind saved credentials, cursors and pending operations to the accepted service and workspace. Changing the destination or detecting a different workspace or restored history MUST pause synchronization until the user explicitly chooses how to connect existing local data. Tokens MUST NOT be sent to an unapproved destination.

#### Scenario: User changes the service address
- **WHEN** a device has pending operations for its previous workspace and the user configures another service
- **THEN** those operations are not automatically uploaded to the new destination and the user must explicitly confirm any local-data import

#### Scenario: Server history changes after restore
- **WHEN** a reauthenticated device detects a restored service history
- **THEN** it stops automatic replay, preserves recoverable local state and asks the user to choose a recovery baseline before continuing

### Requirement: Local privacy controls remain authoritative
Enabling synchronization MUST NOT enable memory retention, personalization or automatic analysis. When local memory retention is disabled, memory synchronization SHALL pause without applying remote memory or uploading pending memory operations; permitted preference synchronization can continue. Disabling synchronization MUST prevent late responses from changing local state.

#### Scenario: Memory retention is disabled on one device
- **WHEN** another device adds terms while this device has memory retention disabled
- **THEN** this device reports paused memory synchronization, retains its local switch and does not import the terms or upload its queued memory changes

#### Scenario: Synchronization is disabled during a request
- **WHEN** the user disables synchronization before a request completes
- **THEN** local data remains usable and the late response cannot alter it; an operation already committed remotely is not falsely reported as rolled back

#### Scenario: Preference synchronization continues while memory is paused
- **WHEN** preferences synchronize repeatedly while memory retention is disabled and memory is enabled again later
- **THEN** memory resumes from its own preserved progress, applies missed deletions or reset before old writes and does not lose terms by having advanced a preference cursor

### Requirement: Scheduling and status reflect durable progress
Users SHALL have an immediate sync action and separately opt-in background synchronization that runs only while the app is open. One synchronization cycle per connection SHALL run at a time. Offline changes MUST survive restart. Status MUST distinguish pending work, authentication failure, conflicts and successful durable progress without blocking translation.

#### Scenario: Device reconnects after an offline restart
- **WHEN** a device edits data offline, exits and later reconnects
- **THEN** pending changes remain queued, synchronization resumes with bounded retries and local translation stays available

#### Scenario: Some changes require user conflict resolution
- **WHEN** independent changes have synchronized but a conflicting value remains unresolved
- **THEN** the UI reports partial completion and the remaining conflict count rather than claiming all data is synchronized

#### Scenario: Authentication is revoked
- **WHEN** the service rejects the saved device credential
- **THEN** background authentication retries stop, pending data is preserved and the UI requests login again
