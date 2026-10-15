## Purpose

Provide accurate translations and contextual technical explanations using a user-chosen compatible language model service.

## ADDED Requirements

### Requirement: Combined structured result
The application SHALL request translation, technical term translations, Chinese explanations and optional examples in one successful model completion. Ordinary vocabulary MUST NOT be forcibly explained.

#### Scenario: Technical prose
- **WHEN** text about Java, JVM, databases, distributed systems, AI, LLMs, agents, machine learning or software engineering is submitted
- **THEN** translation and relevant terms are displayed together

#### Scenario: Ordinary prose
- **WHEN** no specialist terms are present
- **THEN** the model is instructed to return an empty terms array

### Requirement: Compatible output and validation
The application SHALL prefer structured output and offer JSON and prompt-only compatibility modes. Invalid, empty or oversized results MUST produce recoverable errors.

#### Scenario: Schema unsupported
- **WHEN** a provider explicitly rejects response_format in automatic mode
- **THEN** one prompt-only retry is made

#### Scenario: Invalid response
- **WHEN** a completion contains malformed or incomplete JSON
- **THEN** a parsing error suggests a compatibility mode without fabricating output
