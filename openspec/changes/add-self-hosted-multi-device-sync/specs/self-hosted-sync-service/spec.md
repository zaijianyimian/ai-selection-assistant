# Spec Delta

## Purpose

Provide a deployable personal VPS service that authenticates individual devices, durably orders synchronization changes and exposes bounded recoverable operations independently of the desktop application's runtime and model provider.

## ADDED Requirements

### Requirement: Personal workspace and device access are authenticated
The service SHALL provide one owner workspace per deployment with a dedicated synchronization password and separately revocable device tokens. Passwords MUST be stored as salted resistant hashes and tokens as digests. Every data operation MUST authenticate the caller; rotating the password MUST revoke existing tokens without deleting synchronized data.

#### Scenario: Unauthorized client accesses memory
- **WHEN** a request has missing, incorrect or revoked credentials
- **THEN** access is denied without disclosing workspace data, logging credentials or changing retained data

#### Scenario: Owner revokes a device or rotates the password
- **WHEN** a device is revoked or the synchronization password is rotated
- **THEN** affected credentials no longer authorize new operations, accepted data remains intact and clients preserve pending changes for reauthentication

### Requirement: Accepted operations have durable identity and order
The service MUST assign ordered revisions transactionally and deduplicate operations by stable operation identity and payload. A repeated accepted operation MUST return its original acceptance without creating new changes. Reusing an operation identity for different content MUST fail. Rejected batches MUST NOT partially change business data.

#### Scenario: Service restarts after accepting a push
- **WHEN** the same operation is retried after a lost response and service restart
- **THEN** the original receipt remains available and no occurrence or logical update is duplicated

#### Scenario: A batch contains a stale conflicting field update
- **WHEN** one new operation in a submitted batch has an incompatible base version
- **THEN** the service reports a structured conflict and does not partially commit the batch's new business operations

### Requirement: Incremental protocol is bounded and versioned
The service SHALL expose protocol compatibility, workspace identity, history identity, memory generation and ordered page cursors. A pull pass MUST support a fixed revision upper bound. Requests SHALL be limited to 1 MiB and 100 pushed operations; responses to 1 MiB and 500 changes per page. Incompatible versions or invalid fields MUST fail explicitly without data mutation.

#### Scenario: Concurrent writes occur during pagination
- **WHEN** a client pulls multiple pages while other devices write
- **THEN** the bounded pass can finish at its chosen upper revision and subsequent passes include later accepted changes without gaps

#### Scenario: A request is oversized or has an unsupported protocol
- **WHEN** a client exceeds supported limits or sends an incompatible protocol version
- **THEN** the service rejects it with a bounded actionable response and preserves existing state

### Requirement: Service retention honors deletion and minimal data scope
The service MUST reject non-allowlisted data and MUST NOT contact model providers. Deletion SHALL remove affected active content and historical content payloads while retaining only non-content metadata needed to reject stale replays. Operational backups SHALL have a documented independent retention policy and MUST NOT be represented as immediately remotely erased.

#### Scenario: Deleted data remains in an earlier change entry
- **WHEN** a synchronized term is deleted
- **THEN** its text and occurrence content are removed or redacted from historical change payloads and receipts as well as active records, and stale operation rejection remains possible

#### Scenario: A synchronization payload includes prohibited content
- **WHEN** a client submits source text, full translation output, credentials, profiles or unsupported fields
- **THEN** the service rejects the payload, does not retain or log that content and issues no model requests

### Requirement: Service operation has controlled resource use
The service SHALL enforce authentication and device request limits, bounded concurrent password verification, device count and storage quotas. It MUST expose actionable retry or quota errors, omit secrets and payloads from logs, and preserve read/delete/revocation access when additions exceed storage limits.

#### Scenario: A caller repeatedly submits invalid passwords
- **WHEN** login attempts exceed configured bounds
- **THEN** password verification remains resource-bounded and further attempts receive rate-limit responses without revealing password details

#### Scenario: Workspace reaches its storage quota
- **WHEN** new data would exceed the configured quota
- **THEN** additions fail without discarding existing or client-pending data, while reading, deletion and credential revocation remain possible

### Requirement: VPS deployment preserves state and transport trust
The service SHALL ship independently with reproducible container configuration, persistent storage, HTTPS reverse-proxy instructions and initialization/backup/restore procedures. Production clients MUST validate certificates and MUST NOT send credentials over public plaintext HTTP or authentication-bearing redirects. The desktop MUST require no server runtime installation and its final DEB MUST remain below 30,000,000 bytes.

#### Scenario: Containers restart or are replaced
- **WHEN** the documented deployment is restarted or upgraded with its persistent volume
- **THEN** workspace identity, accepted data, revisions and idempotency receipts remain available

#### Scenario: User configures an insecure remote address
- **WHEN** a non-loopback service uses plaintext HTTP, an invalid certificate or an unapproved redirect
- **THEN** the client rejects the connection without sending credentials to the insecure or redirected destination

#### Scenario: Owner restores an older backup
- **WHEN** the documented restore procedure installs an older valid backup
- **THEN** it establishes a new history identity and revokes tokens so clients must reauthenticate and explicitly reconcile a recovery baseline instead of replaying obsolete cursors

#### Scenario: Desktop release is verified
- **WHEN** implementation is complete and the desktop DEB is built for delivery
- **THEN** its actual size is below 30,000,000 bytes and it contains neither the server runtime nor additional models
