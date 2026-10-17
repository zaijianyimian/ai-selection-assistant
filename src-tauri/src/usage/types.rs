use serde::{Deserialize, Serialize};
use serde_json::Value;

// IPC uses decimal strings, including inside stored aggregate JSON. Never pass a
// Token counter through JavaScript Number (whose safe range is only 53 bits).
mod decimal {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(n: &Option<i64>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_some(&n.map(|n| n.to_string()))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
        Option::<String>::deserialize(d)?
            .map(|s| s.parse().map_err(serde::de::Error::custom))
            .transpose()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    #[serde(with = "decimal")]
    pub input: Option<i64>,
    #[serde(with = "decimal")]
    pub output: Option<i64>,
    #[serde(with = "decimal")]
    pub total: Option<i64>,
    #[serde(with = "decimal")]
    pub cached_input: Option<i64>,
    #[serde(with = "decimal")]
    pub reasoning_output: Option<i64>,
}
fn count(v: &Value) -> Option<i64> {
    v.as_i64().filter(|n| *n >= 0)
}

impl Counts {
    pub fn parse(envelope: &Value) -> Self {
        let v = &envelope["usage"];
        let mut n = Self {
            input: count(&v["prompt_tokens"]),
            output: count(&v["completion_tokens"]),
            total: count(&v["total_tokens"]),
            reasoning_output: count(&v["completion_tokens_details"]["reasoning_tokens"]),
            ..Self::default()
        };
        let nested = v
            .get("prompt_tokens_details")
            .and_then(|v| v.get("cached_tokens"));
        let alias = v.get("prompt_cache_hit_tokens");
        n.cached_input = match (nested, alias) {
            (Some(a), Some(b)) if count(a) == count(b) => count(a),
            (Some(_), Some(_)) => None,
            (Some(a), None) | (None, Some(a)) => count(a),
            _ => None,
        };
        if let (Some(i), Some(o), Some(t)) = (n.input, n.output, n.total) {
            if i.checked_add(o) != Some(t) {
                n.total = None;
            }
        }
        if let (Some(detail), Some(parent)) = (n.cached_input, n.input) {
            if detail > parent {
                n.cached_input = None;
            }
        }
        if let (Some(hit), Some(miss), Some(parent)) =
            (n.cached_input, v.get("prompt_cache_miss_tokens"), n.input)
        {
            if count(miss).and_then(|m| hit.checked_add(m)) != Some(parent) {
                n.cached_input = None;
            }
        }
        if let (Some(detail), Some(parent)) = (n.reasoning_output, n.output) {
            if detail > parent {
                n.reasoning_output = None;
            }
        }
        n
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    #[serde(with = "decimal")]
    pub tokens: Option<i64>,
    pub known: u64,
    pub unknown: u64,
    pub overflow: bool,
}
impl Metric {
    fn merge(&mut self, other: &Self) {
        self.known = self.known.saturating_add(other.known);
        self.unknown = self.unknown.saturating_add(other.unknown);
        self.overflow |= other.overflow;
        if let Some(n) = other.tokens {
            match self.tokens.unwrap_or(0).checked_add(n) {
                Some(sum) => self.tokens = Some(sum),
                None => self.overflow = true, // retain the observed lower bound
            }
        }
    }
    fn add(&mut self, n: Option<i64>) {
        self.merge(&Self {
            tokens: n,
            known: u64::from(n.is_some()),
            unknown: u64::from(n.is_none()),
            overflow: false,
        });
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Aggregate {
    pub attempts: u64,
    pub pending: u64,
    pub input: Metric,
    pub output: Metric,
    pub total: Metric,
    pub cached_input: Metric,
    pub reasoning_output: Metric,
}
impl Aggregate {
    pub fn add(&mut self, c: &Counts) {
        self.attempts += 1;
        self.input.add(c.input);
        self.output.add(c.output);
        self.total.add(c.total);
        self.cached_input.add(c.cached_input);
        self.reasoning_output.add(c.reasoning_output);
    }
    pub fn merge(&mut self, other: &Self) {
        self.attempts = self.attempts.saturating_add(other.attempts);
        self.pending = self.pending.saturating_add(other.pending);
        self.input.merge(&other.input);
        self.output.merge(&other.output);
        self.total.merge(&other.total);
        self.cached_input.merge(&other.cached_input);
        self.reasoning_output.merge(&other.reasoning_output);
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Purpose {
    Translation,
    ConnectionTest,
    ManualAnalysis,
    AutoAnalysis,
}
impl Purpose {
    pub fn label(self) -> &'static str {
        match self {
            Self::Translation => "translation",
            Self::ConnectionTest => "connection-test",
            Self::ManualAnalysis => "manual-analysis",
            Self::AutoAnalysis => "auto-analysis",
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub operation_id: String,
    pub generation: u64,
    pub source: &'static str,
    pub purpose: Purpose,
    pub usage: Aggregate,
    pub persistence_incomplete: bool,
    pub excluded_after_reset: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn validates_fields_without_inventing_or_adding_subsets() {
        let c = Counts::parse(
            &json!({"usage":{"prompt_tokens":100,"completion_tokens":30,"total_tokens":130,
            "prompt_tokens_details":{"cached_tokens":20},"prompt_cache_hit_tokens":20,"prompt_cache_miss_tokens":80,
            "completion_tokens_details":{"reasoning_tokens":10}}}),
        );
        assert_eq!(
            c,
            Counts {
                input: Some(100),
                output: Some(30),
                total: Some(130),
                cached_input: Some(20),
                reasoning_output: Some(10)
            }
        );
        for bad in [
            Value::Null,
            json!(-1),
            json!(1.5),
            json!(9223372036854775808u64),
            json!("10"),
        ] {
            let c = Counts::parse(&json!({"usage":{"prompt_tokens":0,"completion_tokens":bad}}));
            assert_eq!(c.input, Some(0));
            assert_eq!(c.output, None);
            assert_eq!(c.total, None);
        }
        let c = Counts::parse(
            &json!({"usage":{"prompt_tokens":100,"completion_tokens":30,"total_tokens":160,
            "prompt_tokens_details":{"cached_tokens":20},"prompt_cache_hit_tokens":21,"completion_tokens_details":{"reasoning_tokens":31}}}),
        );
        assert_eq!(c.total, None);
        assert_eq!(c.input, Some(100));
        assert_eq!(c.cached_input, None);
        assert_eq!(c.reasoning_output, None);
        assert_eq!(Counts::parse(&json!({})), Counts::default());
    }
    #[test]
    fn exact_strings_coverage_and_checked_sums() {
        let mut a = Aggregate::default();
        assert_eq!(a.total.tokens, None);
        assert_eq!(a.attempts, 0);
        a.add(&Counts::default());
        a.add(&Counts {
            total: Some(9007199254740993),
            ..Counts::default()
        });
        assert_eq!(a.total.unknown, 1);
        assert_eq!(a.total.known, 1);
        assert_eq!(
            serde_json::to_value(&a).unwrap()["total"]["tokens"],
            "9007199254740993"
        );
        a.add(&Counts {
            total: Some(i64::MAX),
            ..Counts::default()
        });
        assert!(a.total.overflow);
        assert_eq!(a.total.tokens, Some(9007199254740993));
        let mut zero = Aggregate::default();
        zero.add(&Counts {
            input: Some(0),
            output: Some(0),
            total: Some(0),
            ..Counts::default()
        });
        assert_eq!(zero.total.tokens, Some(0));
        assert_eq!(zero.total.known, 1);
        let json = serde_json::to_string(&zero).unwrap();
        assert_eq!(serde_json::from_str::<Aggregate>(&json).unwrap(), zero);
    }
}
