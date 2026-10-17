# Spec Delta

## Purpose

Converge minimal terminology, occurrence records and explicit familiarity across a user's desktop devices while preserving offline edits, distinct meanings, deletion intent and the existing local privacy guarantees.

## ADDED Requirements

### Requirement: Existing local memory migrates without loss
The first sync enablement SHALL create a consistent private backup before transactional migration. Migration MUST preserve existing term identity, counts, familiarity and profiles until an actual imported change invalidates them. Backup or migration failure MUST leave usable original data and MUST NOT start an upload. Downgrade requirements SHALL be documented.

#### Scenario: Populated legacy database enables synchronization
- **WHEN** a database containing terms, repeated occurrences, feedback and a profile first enables synchronization
- **THEN** a valid backup exists, original local IDs and values are preserved, and repeated migration or bootstrap does not duplicate records

#### Scenario: Backup or migration is interrupted
- **WHEN** backup creation fails or migration cannot commit
- **THEN** synchronization remains disabled, the legacy database stays readable and no partial migration is treated as complete

### Requirement: Meaning identity and occurrences merge idempotently
The system SHALL identify terms consistently across devices using normalized name, domain, standalone meaning and language, preserving distinct meanings. Occurrences MUST have stable global identity and merge as a set. Counts MUST derive from unique retained occurrences rather than summed client aggregates.

#### Scenario: Two devices add the same meaning independently
- **WHEN** two devices retain the same term identity with different local integer IDs
- **THEN** they converge on one shared term identity, keep both distinct occurrences and continue using valid local IDs

#### Scenario: Request succeeds remotely but its response is lost
- **WHEN** the device retries the same submitted occurrence after a timeout or restart
- **THEN** the occurrence is counted once and replaying the operation does not allocate another logical change

#### Scenario: Devices begin with copies of one database
- **WHEN** both devices import the same legacy occurrence records
- **THEN** shared records deduplicate while genuinely new translations on either device count separately

#### Scenario: Similar names have different meanings
- **WHEN** terms differ in domain, standalone meaning or language
- **THEN** synchronization preserves them as separate entries without model-based semantic merging

### Requirement: Feedback conflicts preserve user intent
Familiarity SHALL represent explicit user feedback and MUST NOT be inferred from counts. Concurrent incompatible feedback MUST preserve the local pending value and accepted remote value for resolution. Older imported feedback or delivery order MUST NOT silently override the accepted current state.

#### Scenario: Devices submit different familiarity values offline
- **WHEN** two devices edit one term from the same accepted feedback version and later reconnect
- **THEN** the incompatible update is shown as a conflict and the user can choose a value against the latest accepted version

#### Scenario: Historical feedback arrives after a newer value
- **WHEN** an old or duplicate feedback update is delivered after a more recent accepted version
- **THEN** current familiarity does not regress because of local insertion order and the historical update does not become a new user action

### Requirement: Durable transfers survive interruption
Local changes SHALL be durably queued with their originating workspace. Imported pages and progress SHALL commit atomically; retries MUST NOT create upload echoes. Failed or incompatible transfers MUST preserve local data and pending work, and imported memory MUST remain searchable.

#### Scenario: Process exits while applying a page
- **WHEN** a client restarts after an interrupted page import
- **THEN** it resumes from committed progress, safely replays any incomplete page and produces no duplicate occurrences or outbound echoes

#### Scenario: User adds data during a pull
- **WHEN** local or remote records are added while an incrementally paged sync is running
- **THEN** the current bounded pass terminates and later passes retain the new records without skipping changes

### Requirement: Deletion propagates without stale resurrection
Deleting a synchronized term SHALL clearly state its cross-device effect and remove its active content and occurrences. Non-content deletion metadata MUST reject stale writes and survive long device absences. Applying deletion MUST invalidate stale local model writes, related pending content and obsolete profiles. Fresh deliberate learning after observing deletion SHALL remain possible.

#### Scenario: An offline device reconnects with an old occurrence
- **WHEN** a term is deleted elsewhere and a device later submits an occurrence from the deleted lifetime
- **THEN** the old occurrence cannot recreate the term and the device removes the obsolete local term when applying the deletion

#### Scenario: Deletion occurs during translation or analysis
- **WHEN** a synchronized deletion is accepted locally before an older translation or analysis completes
- **THEN** the old result cannot repopulate the term or activate an obsolete profile, while a valid translation can still be displayed

#### Scenario: User deliberately encounters the term again
- **WHEN** a device has observed a deletion and a new user translation retains the same meaning afterward
- **THEN** a new term lifetime can be created without accepting old occurrences from the deleted lifetime

### Requirement: Global clear is explicit and generation-aware
Clearing synchronized memory on all devices MUST require confirmation of its scope. It SHALL invalidate all prior memory-generation uploads, remove retained memory content on reconnecting devices and report discarded offline changes. It MUST preserve permitted preferences, credentials and unrelated local settings. Local-only cleanup MUST NOT silently perform a global clear.

#### Scenario: Old device reconnects after global clear
- **WHEN** all synchronized memory has been cleared and an offline device returns with old data and pending changes
- **THEN** the old generation is not reimported, its pending memory is invalidated with a visible count, and only new learning after reset can populate the new generation

#### Scenario: User chooses local-only cleanup
- **WHEN** the user explicitly disconnects synchronization and clears only local memory
- **THEN** no global deletion is submitted and the UI explains that reconnecting can restore retained server data

### Requirement: Synchronization preserves minimal retention and model boundaries
Sync payloads MUST contain only retained term fields, occurrence metadata and explicit feedback. Source passages, full translation results, examples, secrets, profiles and search-index internals MUST NOT be transferred. Synchronization itself MUST NOT invoke a model; analysis remains subject to unchanged local authorization and limits.

#### Scenario: Memory is synchronized with all model authorizations off
- **WHEN** two clients exchange memory without a configured model API Key
- **THEN** synchronization and local search work, captured payloads contain no prohibited fields and no model request occurs

#### Scenario: Imported data changes analysis input
- **WHEN** accepted remote memory changes the local analysis basis
- **THEN** obsolete profiles and in-flight analysis writes are invalidated without automatic model calls initiated by synchronization
