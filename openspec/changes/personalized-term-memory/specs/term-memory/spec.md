## Purpose

Provide optional local memory of technical terminology and explicit familiarity feedback without retaining source passages or complete translation history.

## ADDED Requirements

### Requirement: Optional minimal term retention
The application SHALL default memory off and retain only technical terms, language, domain, standalone meaning, translation, occurrence time/source and explicit feedback when enabled. It MUST NOT persist source passages, full translations, examples or API credentials in memory.

#### Scenario: Translation succeeds with memory enabled
- **WHEN** a user translation returns technical terms
- **THEN** terms and occurrences are persisted before result publication, including when term display is disabled

#### Scenario: Connection test or memory disabled
- **WHEN** a connection test runs or memory is disabled
- **THEN** no occurrences are written

### Requirement: Reliable occurrence and meaning identity
The application SHALL count a completed task at most once per term, count distinct repeated translations separately, and keep different domains or standalone meanings separate. Storage failure SHALL preserve the translation with a clear warning.

#### Scenario: Retry and repeated use
- **WHEN** the same task is recorded twice and a second task encounters the same term
- **THEN** the term has two occurrences

### Requirement: User control and deletion
Users SHALL browse and search saved terms, set familiarity explicitly, delete a term and clear all memory. Deletion or disabling memory SHALL invalidate in-flight writes and generated profiles.

#### Scenario: Delete during analysis
- **WHEN** data is deleted while translation or analysis is running
- **THEN** stale results cannot repopulate deleted records or activate a stale profile
