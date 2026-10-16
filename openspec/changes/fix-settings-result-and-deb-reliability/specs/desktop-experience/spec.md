# Spec Delta

## Purpose

Keep editable desktop settings predictable when application windows synchronize their saved configuration, preserving deliberate user edits and making external updates visible without silently discarding drafts.

## ADDED Requirements

### Requirement: External refresh preserves a dirty settings draft
The settings interface MUST distinguish the saved configuration from an edited draft. An external settings refresh MUST update an unedited form automatically, but MUST preserve edited fields and temporary credential intent in a dirty form and display an external-update notice. Temporary credentials MUST remain absent from shared store state and logs.

#### Scenario: Another window updates settings during editing
- **WHEN** the user edits Model without saving and another window publishes saved settings
- **THEN** the edited Model and other local draft fields remain unchanged and an external-update notice is displayed

#### Scenario: An unedited form receives an update
- **WHEN** saved settings refresh while the form has no local edits or credential intent
- **THEN** the form reflects the latest saved settings without showing a false conflict

### Requirement: Explicit reload and local save establish a new baseline
The interface SHALL provide an explicit action to replace a draft with the latest saved settings. A successful local save MUST establish the accepted configuration as the new clean baseline, including when its settings event arrives before the save response. A failed save MUST preserve non-secret edits. Temporary credentials MUST still be cleared after save or connection-test completion and on unmount.

#### Scenario: User chooses to reload external settings
- **WHEN** the user invokes the reload action after an external update
- **THEN** the latest saved configuration replaces the draft, temporary key and deletion intent are cleared, and the update notice is dismissed

#### Scenario: Local save event precedes its response
- **WHEN** a local save succeeds and its settings event is received before the command response
- **THEN** the final accepted settings become the form baseline and no external-conflict notice remains

#### Scenario: Local save fails
- **WHEN** saving a modified Model fails
- **THEN** the modified Model remains editable, the failure is displayed, the API Key input is cleared, and the form is not marked as successfully saved

#### Scenario: User saves a retained draft after an external update
- **WHEN** a user explicitly saves the retained draft after seeing the external-update notice
- **THEN** normal validation applies and a successful save uses that draft as the new baseline
