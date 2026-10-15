## Purpose

Adapt technical explanations using user-controlled preferences and limited relevant local terminology while keeping current source meaning authoritative and inferred knowledge inspectable.

## ADDED Requirements

### Requirement: Independent personalization controls
The application SHALL offer independent term display, memory, personalization and automatic analysis controls. Explicit background and explanation style SHALL be editable. Personalization SHALL default off.

#### Scenario: Memory without personalization
- **WHEN** memory is enabled and personalization is disabled
- **THEN** translation records terms without injecting a generated profile into requests

### Requirement: Relevant bounded context
Personalized requests SHALL inject at most eight matching terms and 2000 characters of memory data. They SHALL prioritize current source meaning, explicit preferences and explicit familiarity over inferred interests. Generated data MUST NOT act as executable instructions or infer identity or mastery from frequency.

#### Scenario: Ambiguous familiar word
- **WHEN** saved terminology does not match the current meaning
- **THEN** the model is instructed to use current context rather than force the saved sense
