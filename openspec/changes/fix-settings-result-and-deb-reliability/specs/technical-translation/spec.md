# Spec Delta

## Purpose

Preserve the meaning of an already produced translation when user preferences change, ensuring each displayed result remains associated with the options actually used by its request across application windows.

## ADDED Requirements

### Requirement: Translation results retain request display options
Each translation task MUST retain its effective target language and term-display option from the configuration used to execute that request. Existing results MUST display those options independently of later settings changes. This metadata MUST remain in memory and MUST NOT introduce translation-history persistence or change the model response schema.

#### Scenario: Target language changes after completion
- **WHEN** a Chinese translation has completed and the user subsequently saves Japanese as the target language
- **THEN** the existing translation remains labelled as Chinese and the next request uses Japanese

#### Scenario: Settings change while a request is running
- **WHEN** a request starts with one target language and settings change before its response arrives
- **THEN** its completed result uses the target-language label from that request and is not relabelled with the newer setting

#### Scenario: Term-display preference changes
- **WHEN** an existing result contains term cards and the user later changes term-display settings
- **THEN** the existing result's cards and explanatory state remain consistent with its request options, while subsequent requests use the new preference

### Requirement: Snapshot metadata is synchronized with the task
Task events and task snapshots MUST expose the same request display metadata to the main and translation windows. Refreshing or reopening a window MUST NOT relabel a recovered result or allow an older task revision to replace a newer one. Idle views MAY display the current defaults because no translation result exists.

#### Scenario: Popup opens after preferences changed
- **WHEN** a completed Chinese result exists, Japanese is now the default, and the popup recovers the current task
- **THEN** both windows label the recovered result as Chinese

#### Scenario: Older task snapshot arrives after a newer event
- **WHEN** an older snapshot arrives after a newer task event
- **THEN** the newer revision's translation and display metadata remain visible together
