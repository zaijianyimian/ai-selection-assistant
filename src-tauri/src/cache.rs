//! Bounded, process-local translation results. Never persisted or synchronized.
use crate::{error::AppResult, llm, settings::Settings, translation::Translation};
use serde::Serialize;
use std::{
    collections::{hash_map::RandomState, VecDeque},
    hash::{BuildHasher, Hash, Hasher},
    time::{Duration, Instant},
};

const MAX_ENTRIES: usize = 100;
const MAX_BYTES: usize = 8 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(30 * 60);

// No Debug implementation: request text and credential identity are private.
#[derive(PartialEq, Serialize)]
pub struct Key {
    revision: u64,
    endpoint: String,
    model: String,
    temperature: u64,
    output_mode: String,
    prompt: String,
    text: String,
    credential: u64,
    explain_terms: bool,
    remember_terms: bool,
    memory: Option<(u64, i64)>,
}
struct Entry {
    key: Key,
    result: Translation,
    inserted: Instant,
    bytes: usize,
}
pub struct Cache {
    entries: VecDeque<Entry>,
    bytes: usize,
    revision: u64,
    credentials: RandomState,
}
impl Default for Cache {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            revision: 0,
            credentials: RandomState::new(),
        }
    }
}
impl Cache {
    pub fn key(
        &self,
        settings: &Settings,
        credential: &str,
        text: &str,
        context: &str,
        memory: Option<(u64, i64)>,
    ) -> AppResult<Key> {
        let mut auth = self.credentials.build_hasher();
        credential.hash(&mut auth);
        Ok(Key {
            revision: self.revision,
            endpoint: llm::normalize_endpoint(&settings.base_url)?.to_string(),
            model: settings.model.trim().into(),
            temperature: settings.temperature.to_bits(),
            output_mode: settings.output_mode.clone(),
            prompt: llm::translation_prompt(settings, context),
            text: text.trim().into(),
            credential: auth.finish(),
            explain_terms: settings.explain_terms,
            remember_terms: settings.remember_terms,
            memory,
        })
    }
    fn expire(&mut self, now: Instant) {
        self.entries
            .retain(|e| now.duration_since(e.inserted) < TTL);
        self.bytes = self.entries.iter().map(|e| e.bytes).sum();
    }
    pub fn get(&mut self, key: &Key, now: Instant) -> Option<Translation> {
        self.expire(now);
        let index = self.entries.iter().position(|e| &e.key == key)?;
        let entry = self.entries.remove(index)?;
        let result = entry.result.clone();
        self.entries.push_back(entry);
        Some(result)
    }
    pub fn insert(&mut self, key: Key, result: Translation, now: Instant) {
        self.expire(now);
        // A clear or credential change during an in-flight request must stay cleared.
        if key.revision != self.revision {
            return;
        }
        self.entries.retain(|e| e.key != key);
        self.bytes = self.entries.iter().map(|e| e.bytes).sum();
        let bytes = serde_json::to_vec(&key).map_or(MAX_BYTES + 1, |v| v.len())
            + serde_json::to_vec(&result).map_or(MAX_BYTES + 1, |v| v.len());
        if bytes > MAX_BYTES {
            return;
        }
        while self.entries.len() >= MAX_ENTRIES || self.bytes + bytes > MAX_BYTES {
            if let Some(entry) = self.entries.pop_front() {
                self.bytes -= entry.bytes;
            }
        }
        self.bytes += bytes;
        self.entries.push_back(Entry {
            key,
            result,
            inserted: now,
            bytes,
        });
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.revision = self.revision.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests;
