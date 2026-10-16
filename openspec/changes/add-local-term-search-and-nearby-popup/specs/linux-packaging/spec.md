# Spec Delta

## Purpose

Keep the delivered Linux application lightweight and self-contained for local terminology retrieval, enforcing the user's explicit installation-package size budget against the actual release artifact.

## ADDED Requirements

### Requirement: Model-free DEB stays below thirty megabytes
The final DEB MUST be smaller than 30,000,000 bytes. Search MUST require no additional embedding model, model download, vector service or new runtime interpreter. Build verification and release verification MUST fail for an artifact at or above the limit while retaining the existing runtime dependency checks.

#### Scenario: Final package is within budget
- **WHEN** the release candidate DEB is verified
- **THEN** its actual byte size is recorded and verification passes only if it is below 30,000,000 bytes and runtime dependencies are valid

#### Scenario: Candidate exceeds the budget
- **WHEN** a DEB is 30,000,000 bytes or larger
- **THEN** verification fails before it can be marked ready for release

#### Scenario: Search is used on a fresh installation
- **WHEN** the application is installed and the user searches locally retained terms
- **THEN** no model is downloaded and no model service or Python runtime is needed by the desktop application
