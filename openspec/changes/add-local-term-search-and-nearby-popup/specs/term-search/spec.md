# Spec Delta

## Purpose

Let users retrieve their locally retained terminology by words, definitions and approximate spelling without downloading an embedding model or sending search queries and term contents to an external service.

## ADDED Requirements

### Requirement: Local keyword and fuzzy-related modes
The application SHALL expose keyword and fuzzy-related modes over term originals, translations, domains and standalone meanings. Keyword mode MUST match all entered keywords across those fields. Fuzzy-related mode SHALL rank exact and substring matches ahead of approximate word matches. It MUST be described as lexical matching, not vector semantic inference.

#### Scenario: Multiple keywords span different fields
- **WHEN** keyword mode searches for a term name together with a word in its standalone meaning
- **THEN** matching terms satisfy both keywords and are displayed with stable pagination

#### Scenario: Minor English spelling error
- **WHEN** fuzzy-related mode searches a technical term with a small spelling error
- **THEN** a matching local term can be retrieved by spelling similarity, and an exact match ranks ahead of that approximate match

#### Scenario: Chinese query and literal punctuation
- **WHEN** a Chinese query or a query containing percent, underscore, quotes or punctuation is submitted
- **THEN** the query is treated as literal user text, does not broaden into an SQL wildcard or executable search expression, and does not fail because of query syntax

#### Scenario: Empty or overlong query
- **WHEN** an empty query or a query longer than 200 characters is submitted
- **THEN** empty input lists recent terms and overlong input produces a recoverable validation error

### Requirement: Search index follows retained terminology
The application MUST automatically index existing retained terms without changing their IDs, counts, familiarity or analysis state, and MUST update the index on insert, update, delete and clear. The index MUST contain only existing retained term fields and MUST NOT add source passages, full translations, examples or credentials.

#### Scenario: Existing database is opened
- **WHEN** an existing terminology database without a search index is opened
- **THEN** existing terms become searchable and their prior memory and profile data remain intact

#### Scenario: A term is deleted or all memory is cleared
- **WHEN** a saved term is deleted or memory is cleared
- **THEN** the deleted entries disappear from both search modes, including after restarting

### Requirement: Search is offline and computationally bounded
Search MUST issue no model or network requests. Fuzzy candidate ranking MUST be bounded and its candidate limit documented. Switching query or mode MUST reset pagination, and late responses MUST NOT overwrite newer search state. Invalid modes MUST produce a useful error.

#### Scenario: Network is unavailable
- **WHEN** a user searches retained terminology without network access or an API Key
- **THEN** both search modes remain usable locally

#### Scenario: Search changes during a pending response
- **WHEN** a new query or mode is submitted before an older search returns
- **THEN** the newer results and page remain authoritative
