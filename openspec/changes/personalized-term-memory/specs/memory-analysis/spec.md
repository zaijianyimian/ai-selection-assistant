## Purpose

Summarize saved terminology into inspectable topic evidence through the configured model, with bounded input, explicit scheduling consent and recoverable versions.

## ADDED Requirements

### Requirement: Bounded manual analysis
Users SHALL start analysis manually using the configured model and credential vault. Input SHALL contain only bounded term aggregates and feedback. Validated output SHALL reference input term IDs. Only one analysis SHALL run at a time; translation SHALL remain available.

#### Scenario: Invalid response or request failure
- **WHEN** a response contains invalid fields, unknown evidence IDs or the request fails
- **THEN** an actionable error is shown and the previous successful profile remains active

### Requirement: Opt-in application scheduling
Automatic analysis SHALL default off, require memory enabled, and run only while the application is open and translation is idle. It SHALL require 20 new occurrences or feedback records and wait at least 24 hours between automatic attempts, including failed attempts.

#### Scenario: Restart after a failed attempt
- **WHEN** the application restarts within 24 hours of an automatic attempt
- **THEN** it does not repeat that automatic request

### Requirement: Snapshot progress and profile versions
Successful analysis SHALL persist a version and advance only the analyzed snapshot watermarks. Users SHALL inspect and select retained versions. Interrupted or stale work SHALL not replace a valid profile.

#### Scenario: New translation during analysis
- **WHEN** occurrences arrive after the analysis snapshot
- **THEN** they remain pending for the next analysis
