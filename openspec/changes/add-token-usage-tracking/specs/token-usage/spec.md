# Spec Delta

## Purpose

为使用自有模型服务的用户提供每次请求及跨重启累计的 Token 用量和清晰说明，覆盖翻译、测试、分析与缓存场景；保留未知数据、隐私边界和统计完整性提示，使本机统计不会被误认为账户余额或服务商账单。

## ADDED Requirements

### Requirement: Validated provider usage
The application SHALL display service-reported input, output and total Token counts independently. Missing, null, negative, non-integer, overflowing or inconsistent values MUST be marked unknown rather than fabricated or interpreted as zero. Malformed usage MUST NOT invalidate an otherwise valid translation or analysis.

#### Scenario: Complete or explicit zero usage
- **WHEN** a response supplies valid non-negative integer counts, including explicit zero
- **THEN** the reported counts are displayed and included once in known usage

#### Scenario: Partial or invalid usage
- **WHEN** only input usage is valid, or total conflicts with the reported input plus output
- **THEN** independently valid fields remain usable, missing/invalid fields are unknown, and incomplete coverage is visible without failing the result

### Requirement: Non-additive detail breakdown
The application SHALL distinguish service-side cached input and reasoning output from local result caching. Cached-input and reasoning counts MUST be treated as subsets of their parent counts, never added again to total. Duplicate field aliases MUST NOT be counted twice.

#### Scenario: Provider reports overlapping details
- **WHEN** input is 100 including 20 cached tokens and output is 30 including 10 reasoning tokens
- **THEN** the detail counts are shown separately and do not turn a reported total of 130 into 160

#### Scenario: Invalid detail or duplicate aliases
- **WHEN** cached-input aliases disagree, or a detail exceeds its reported parent
- **THEN** the affected detail is unknown, valid parent counts remain available, and no duplicate amount is added

### Requirement: All model request purposes
The application SHALL capture usage for translation, connection tests, manual topic analysis and automatic topic analysis, attributed to the service/model configuration and purpose used when each request started. The feature MUST NOT initiate extra model or balance requests to obtain statistics.

#### Scenario: Analysis and translation overlap
- **WHEN** analysis and translation complete concurrently using their captured settings
- **THEN** both contributions are retained under their own purpose and captured service/model without overwriting one another

#### Scenario: Unsaved connection configuration
- **WHEN** a connection test uses an unsaved model or service
- **THEN** its usage is classified under that tested configuration, without saving the configuration or temporary API Key

#### Scenario: Non-model network traffic
- **WHEN** devices perform terminology synchronization or the user views statistics
- **THEN** these actions do not create model Token consumption or call a provider for statistics

### Requirement: Actual attempts and exactly-once accounting
The application SHALL distinguish a logical operation from its actual HTTP attempts. Each observed attempt MUST contribute at most once even when notifications or views repeat. Compatibility retries MUST use separate attempt identities, and operation totals MUST summarize all reported attempt usage with unknown coverage retained.

#### Scenario: Compatibility retry
- **WHEN** an initial attempt is rejected with no usage and a fallback reports 120 tokens
- **THEN** known operation usage is 120, one attempt has unknown usage, and the operation is not presented as completely measured

#### Scenario: Repeated events and changed settings
- **WHEN** the same attempt completion or window update is processed twice after settings change
- **THEN** it contributes once under the original request's service/model, without reclassifying earlier usage

#### Scenario: Request blocked before sending
- **WHEN** input validation, recovery checks, credential access or a busy gate prevents any HTTP attempt
- **THEN** no provider attempt or consumption is added; the UI indicates that no model request was sent

### Requirement: Usage survives result failures
Valid usage in an observed response SHALL be retained before translation/analysis output validation, whether the application accepts, rejects or discards the generated result. HTTP errors, timeouts, interrupted reads and process interruption without usage MUST be identified as unmeasured attempts, not evidence of free requests.

#### Scenario: Truncated or malformed model content
- **WHEN** a response reports valid usage but generated JSON is invalid, truncated or refused
- **THEN** the operation still displays/records that usage while preserving the existing actionable result error

#### Scenario: Failed or interrupted transport
- **WHEN** an HTTP attempt ends without observable valid usage, including timeout or oversized response rejection
- **THEN** the unknown attempt is retained in coverage indicators without guessing Token counts or exposing response content

#### Scenario: Restart after interruption
- **WHEN** the application restarts with an unfinished recorded attempt
- **THEN** it marks that attempt unknown once, preserves previous totals, and does not resend the provider request to reconstruct usage

### Requirement: Local cache produces no new consumption
A local translation-cache hit SHALL display zero new input/output/total Tokens for that reuse and MUST NOT replay original request usage into cumulative totals. A forced refresh SHALL be tracked as a new model operation. Clearing translation cache MUST NOT clear usage statistics.

#### Scenario: Reuse from either window
- **WHEN** input, clipboard or shortcut translation returns a local cached result and the main window and popup both display it
- **THEN** the reuse shows zero new usage and no provider attempt or prior Token amount is added

#### Scenario: Force refresh
- **WHEN** the user explicitly requests the model again
- **THEN** any actual HTTP attempts are measured as new usage even if the text matches a cached result

### Requirement: Persistent scoped aggregates
The application SHALL retain known usage, unknown-coverage counts and observation start time on this device across restarts. It SHALL show today, current month and cumulative values with service/model/purpose breakdowns. Historical usage before tracking began MUST NOT be backfilled, and changing an API Key MUST NOT imply account-level accounting.

#### Scenario: Restart, filtering and key change
- **WHEN** the user restarts, filters a model or replaces a key for the same service/model
- **THEN** persisted local totals remain consistent, filtering does not erase other categories, and the screen states that totals concern this application/device rather than the whole account

#### Scenario: Known zero versus empty history
- **WHEN** tracking has no requests, or some recorded attempts omit usage
- **THEN** the UI distinguishes no history, explicitly reported zero and unknown coverage rather than showing an apparently complete zero bill

### Requirement: Stable local date attribution
Today and month statistics SHALL use the device-local calendar date captured at HTTP attempt start, with that rule visible to users. Crossing midnight, daylight-saving transitions or later time-zone changes MUST NOT move an existing attempt or count it twice. Cumulative counts MUST remain unaffected by calendar regrouping.

#### Scenario: Midnight and month boundary
- **WHEN** an attempt starts before local midnight/month end and finishes afterward
- **THEN** its usage belongs to the start date/month and appears once in cumulative statistics

#### Scenario: Time-zone or clock change
- **WHEN** the device time zone or clock changes after an attempt starts
- **THEN** the existing attempt retains its recorded date; later attempts use their own captured date and the display explains this convention

### Requirement: Independent confirmed reset
Clearing usage SHALL require an explicit confirmation and start a fresh statistics generation. Earlier in-flight attempts and stale window events MUST NOT repopulate it. The operation MUST preserve translations, translation cache, terminology, sync configuration, credentials and model settings.

#### Scenario: Cancel or confirm reset
- **WHEN** the user cancels confirmation, or confirms clearing usage
- **THEN** cancellation leaves all counts intact; confirmation clears only usage and updates every open window

#### Scenario: Completion after reset
- **WHEN** a request started before reset returns afterward
- **THEN** it cannot enter the new cumulative statistics, its current result may still show observed per-operation usage, and the UI explains that reset does not cancel already sent requests

### Requirement: Private local statistics
Statistics SHALL persist only necessary numeric counts and bounded classification metadata. They MUST NOT persist source text, translations, prompts, credentials, credential hashes, raw provider bodies or arbitrary provider error strings. Usage MUST NOT be synchronized or included in terminology backups, and clearing terminology MUST NOT erase usage.

#### Scenario: Inspect storage and sync traffic
- **WHEN** statistics are recorded and the device synchronizes or backs up terminology
- **THEN** neither usage records nor source/translation/secret content appears in the sync payload or terminology backup, and the usage store contains no such content

### Requirement: Non-blocking statistics failures
Statistics read/write or schema failures SHALL preserve valid translation/analysis results and existing protected data. The UI MUST disclose unavailable/incomplete persistence rather than claim complete totals. Statistics failures MUST NOT trigger provider retries, plaintext secret fallback or destructive automatic recreation.

#### Scenario: Full disk or unsupported statistics schema
- **WHEN** local usage storage cannot be updated or opened safely
- **THEN** valid results and observable per-operation usage remain readable, cumulative statistics show a warning, and terminology/settings remain untouched

### Requirement: Accessible usage presentation and explanation
The main window and popup SHALL show per-operation usage on success and failure; connection tests and topic analysis SHALL expose their own usage. Settings SHALL provide aggregates, coverage, reset and explanations. The UI MUST explain Token versus characters, hidden prompt/context contributions, server-cache versus local-cache semantics, and the distinction between this device's known usage, request/context limits, account quota and monetary billing.

#### Scenario: Review a result or analysis
- **WHEN** a user opens the popup, main window, connection-test outcome or latest analysis status
- **THEN** usage belongs to that operation, remains available when the model result fails, and is readable without obscuring primary controls in narrow or dark layouts

#### Scenario: Learn the scope of statistics
- **WHEN** the user reads the usage explanation
- **THEN** it states that missing reports and other applications/devices are outside these totals, provider context-cache hits can still consume reported tokens, and no remaining balance, monetary estimate or exact saved-token amount is promised

### Requirement: Preserve lightweight offline statistics
Usage viewing and explanation SHALL work offline from local observations, without a tokenizer, downloaded model or balance integration. Existing translation compatibility and response/input limits MUST remain effective. Implementation release acceptance MUST retain the existing installer budget of less than 30,000,000 bytes on supported Windows/Linux package formats.

#### Scenario: Provider lacks usage support
- **WHEN** an otherwise compatible provider omits usage
- **THEN** translation and analysis remain usable, the UI reports unknown usage, and no extra request or model download is made to estimate it
