//! Minimal, strict wire data shared by desktop and service. No credentials in Debug output.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const VERSION: u32 = 1;
pub const MAX_BYTES: usize = 1_048_576;
pub const MAX_PUSH: usize = 100;
pub const MAX_PAGE: usize = 500;
pub const DOMAINS: &[&str] = &[
    "Java",
    "JVM",
    "数据库",
    "分布式系统",
    "AI / LLM",
    "Agent",
    "机器学习",
    "软件工程",
];
pub fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 160
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_:".contains(&c))
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.chars().count() <= max && !s.chars().any(char::is_control)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Term {
    pub original: String,
    pub translation: String,
    pub domain: String,
    pub sense: String,
    pub language: String,
}
impl Term {
    pub fn normalized(&self) -> String {
        self.original
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }
    pub fn key(&self) -> String {
        format!(
            "t1:{}",
            digest(
                &serde_json::to_vec(&[
                    self.normalized(),
                    self.domain.trim().into(),
                    self.sense.trim().into(),
                    self.language.trim().into()
                ])
                .expect("strings serialize")
            )
        )
    }
    pub fn validate(&self) -> bool {
        text(&self.original, 120)
            && text(&self.translation, 200)
            && DOMAINS.contains(&self.domain.as_str())
            && text(&self.sense, 120)
            && text(&self.language, 40)
    }
}
pub fn legacy_event_id(job: &str, term_key: &str) -> String {
    format!(
        "e1:{}",
        digest(&serde_json::to_vec(&["occurrence1", job, term_key]).expect("strings serialize"))
    )
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Familiarity {
    Unknown,
    Learning,
    Known,
}
impl Familiarity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Learning => "learning",
            Self::Known => "known",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Preference {
    Theme,
    TargetLanguage,
    ExplanationStyle,
}
impl Preference {
    pub const ALL: [Self; 3] = [Self::Theme, Self::TargetLanguage, Self::ExplanationStyle];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::TargetLanguage => "targetLanguage",
            Self::ExplanationStyle => "explanationStyle",
        }
    }
    pub fn valid(self, value: &str) -> bool {
        match self {
            Self::Theme => ["system", "light", "dark"].contains(&value),
            Self::TargetLanguage => ["简体中文", "English", "日本語"].contains(&value),
            Self::ExplanationStyle => {
                ["balanced", "introductory", "concise", "practical"].contains(&value)
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Memory,
    Preferences,
}
impl Stream {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Preferences => "preferences",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Occurrence {
    pub id: String,
    pub at: i64,
    pub source: String,
}
impl Occurrence {
    pub fn validate(&self) -> bool {
        valid_id(&self.id) && self.at >= 0 && text(&self.source, 40)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Observe {
        term: Term,
        event: Occurrence,
        incarnation: Option<String>,
        #[serde(rename = "tombstoneRevision")]
        tombstone_revision: i64,
    },
    Feedback {
        key: String,
        incarnation: String,
        #[serde(rename = "baseRevision")]
        base_revision: i64,
        value: Familiarity,
    },
    Delete {
        key: String,
        incarnation: String,
    },
    Preference {
        field: Preference,
        #[serde(rename = "baseRevision")]
        base_revision: i64,
        value: String,
    },
}
impl Action {
    pub fn stream(&self) -> Stream {
        if matches!(self, Self::Preference { .. }) {
            Stream::Preferences
        } else {
            Stream::Memory
        }
    }
    pub fn key(&self) -> Option<String> {
        match self {
            Self::Observe { term, .. } => Some(term.key()),
            Self::Feedback { key, .. } | Self::Delete { key, .. } => Some(key.clone()),
            _ => None,
        }
    }
    pub fn validate(&self) -> bool {
        match self {
            Self::Observe {
                term,
                event,
                incarnation,
                tombstone_revision,
            } => {
                *tombstone_revision >= 0
                    && term.validate()
                    && event.validate()
                    && incarnation.as_deref().is_none_or(valid_id)
            }
            Self::Feedback {
                key,
                incarnation,
                base_revision,
                ..
            } => *base_revision >= 0 && valid_id(key) && valid_id(incarnation),
            Self::Delete { key, incarnation } => valid_id(key) && valid_id(incarnation),
            Self::Preference {
                field,
                value,
                base_revision,
            } => *base_revision >= 0 && field.valid(value),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Operation {
    pub id: String,
    pub generation: i64,
    pub action: Action,
}
impl Operation {
    pub fn validate(&self) -> bool {
        valid_id(&self.id) && self.generation >= 0 && self.action.validate()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Context {
    pub protocol: u32,
    pub workspace_id: String,
    pub history_id: String,
    pub memory_generation: i64,
    pub revision: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Push {
    pub protocol: u32,
    pub workspace_id: String,
    pub history_id: String,
    pub operations: Vec<Operation>,
}
impl Push {
    pub fn validate(&self) -> bool {
        self.protocol == VERSION
            && valid_id(&self.workspace_id)
            && valid_id(&self.history_id)
            && !self.operations.is_empty()
            && self.operations.len() <= MAX_PUSH
            && self.operations.iter().all(Operation::validate)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reset {
    pub protocol: u32,
    pub workspace_id: String,
    pub history_id: String,
    pub id: String,
    pub generation: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    Term {
        key: String,
        term: Term,
        incarnation: String,
        #[serde(rename = "tombstoneRevision")]
        tombstone_revision: i64,
    },
    Occurrence {
        key: String,
        incarnation: String,
        event: Occurrence,
    },
    Feedback {
        key: String,
        incarnation: String,
        value: Familiarity,
    },
    Preference {
        field: Preference,
        value: String,
    },
    Deleted {
        key: String,
        incarnation: String,
    },
    Reset {
        generation: i64,
    },
    Redacted,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    pub revision: i64,
    pub change: Change,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Page {
    pub context: Context,
    pub stream: Stream,
    pub until: i64,
    pub next_cursor: i64,
    pub has_more: bool,
    pub changes: Vec<Record>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receipt {
    pub id: String,
    pub revision: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Accepted {
    pub context: Context,
    pub receipts: Vec<Receipt>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Conflict {
    pub op_id: String,
    pub code: String,
    pub remote_value: Option<String>,
    pub remote_revision: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Failure {
    pub code: String,
    #[serde(default)]
    pub conflicts: Vec<Conflict>,
    pub context: Option<Context>,
}
// Intentionally no Debug: passwords/tokens must not appear in diagnostics.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Login {
    pub protocol: u32,
    pub password: String,
    pub device_name: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Enrollment {
    pub context: Context,
    pub device_id: String,
    pub token: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub last_seen: i64,
}
#[cfg(test)]
mod tests;
