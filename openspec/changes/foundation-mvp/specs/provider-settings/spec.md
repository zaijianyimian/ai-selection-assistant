## Purpose

Let users configure their own model provider and preserve credentials securely between desktop application sessions.

## ADDED Requirements

### Requirement: Secure persistence
The application MUST persist non-secret provider settings locally and API keys only in the OS credential vault. Persisted keys MUST NOT be returned to the frontend or logged.

#### Scenario: Save and restart
- **WHEN** a user saves valid settings and a new API key and restarts
- **THEN** the settings are restored and only key presence is exposed

#### Scenario: Vault unavailable
- **WHEN** the credential vault is unavailable or locked
- **THEN** a useful error is displayed without writing plaintext credentials

### Requirement: Provider validation and test
The application SHALL validate model, URL, temperature and timeout and allow testing saved or unsaved provider configuration.

#### Scenario: Full endpoint supplied
- **WHEN** a URL ending in /chat/completions is configured
- **THEN** no extra /v1 or /chat/completions is added

#### Scenario: Connection failure
- **WHEN** authentication, networking, timeout or rate limiting fails
- **THEN** the UI displays a relevant error without exposing secrets
