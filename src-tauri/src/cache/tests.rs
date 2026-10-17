use super::*;

fn result(text: &str) -> Translation {
    Translation {
        translation: text.into(),
        source_language: "English".into(),
        terms: vec![],
        compatibility_fallback: false,
    }
}
fn key(cache: &Cache, text: &str) -> Key {
    cache
        .key(&Settings::default(), "fixture-key", text, "", None)
        .unwrap()
}
#[test]
fn exact_text_hit_clones_output_and_preserves_internal_whitespace() {
    let mut cache = Cache::default();
    let now = Instant::now();
    cache.insert(key(&cache, "Code A\nB"), result("代码"), now);
    let mut hit = cache.get(&key(&cache, "  Code A\nB  "), now).unwrap();
    hit.translation = "edited".into();
    assert_eq!(
        cache
            .get(&key(&cache, "Code A\nB"), now)
            .unwrap()
            .translation,
        "代码"
    );
    for text in ["Code A B", "code A\nB", "Code A\nC"] {
        assert!(cache.get(&key(&cache, text), now).is_none());
    }
}
#[test]
fn request_settings_credentials_context_and_memory_isolate_entries() {
    let mut cache = Cache::default();
    let now = Instant::now();
    let config = Settings {
        personalize_explanations: true,
        ..Settings::default()
    };
    let make = |c: &Cache, s: &Settings, auth: &str, context: &str, memory| {
        c.key(s, auth, "Agent", context, memory).unwrap()
    };
    cache.insert(
        make(&cache, &config, "first", "context", Some((1, 1))),
        result("one"),
        now,
    );
    for change in 0..9 {
        let mut s = config.clone();
        match change {
            0 => s.base_url = "https://other.example/v1".into(),
            1 => s.model = "other-model".into(),
            2 => s.target_language = "English".into(),
            3 => s.temperature = 0.8,
            4 => s.output_mode = "prompt".into(),
            5 => s.explain_terms = false,
            6 => s.remember_terms = true,
            7 => s.explanation_style = "concise".into(),
            _ => s.background_domain = "backend".into(),
        }
        assert!(cache
            .get(&make(&cache, &s, "first", "context", Some((1, 1))), now)
            .is_none());
    }
    for (auth, context, memory) in [
        ("second", "context", Some((1, 1))),
        ("first", "changed", Some((1, 1))),
        ("first", "context", Some((2, 1))),
        ("first", "context", Some((1, 2))),
    ] {
        assert!(cache
            .get(&make(&cache, &config, auth, context, memory), now)
            .is_none());
    }
    // Display settings do not affect a request; equivalent API URLs share a key.
    let mut s = config.clone();
    s.theme = "dark".into();
    s.shortcut = "Alt+Shift+T".into();
    s.base_url = "https://api.openai.com/v1/chat/completions".into();
    assert!(cache
        .get(&make(&cache, &s, "first", "context", Some((1, 1))), now)
        .is_some());
}
#[test]
fn expiration_is_absolute_and_hits_only_update_lru_order() {
    let mut cache = Cache::default();
    let now = Instant::now();
    for i in 0..MAX_ENTRIES {
        cache.insert(key(&cache, &i.to_string()), result("value"), now);
    }
    assert!(cache.get(&key(&cache, "0"), now + TTL / 2).is_some());
    cache.insert(key(&cache, "new"), result("new"), now + TTL / 2);
    assert!(cache.get(&key(&cache, "1"), now + TTL / 2).is_none());
    assert!(cache.get(&key(&cache, "0"), now + TTL).is_none());
    assert!(cache.get(&key(&cache, "new"), now + TTL).is_some());
    assert_eq!(cache.entries.len(), 1);
}
#[test]
fn byte_budget_evicts_and_oversized_results_are_not_cached() {
    let mut cache = Cache::default();
    let now = Instant::now();
    for i in 0..12 {
        cache.insert(
            key(&cache, &i.to_string()),
            result(&"a".repeat(1_000_000)),
            now,
        );
    }
    assert!(cache.bytes <= MAX_BYTES);
    assert!(cache.entries.len() < 12);
    assert!(cache.get(&key(&cache, "0"), now).is_none());
    cache.insert(key(&cache, "large"), result(&"x".repeat(MAX_BYTES)), now);
    assert!(cache.get(&key(&cache, "large"), now).is_none());
}
#[test]
fn clear_blocks_inflight_repopulation_and_new_instances_start_empty() {
    let mut cache = Cache::default();
    let now = Instant::now();
    let inflight = key(&cache, "late");
    cache.insert(key(&cache, "old"), result("old"), now);
    cache.clear();
    cache.insert(inflight, result("late"), now);
    assert!(cache.entries.is_empty());
    assert_eq!(cache.bytes, 0);
    cache.insert(key(&cache, "fresh"), result("fresh"), now);
    assert!(Cache::default().get(&key(&cache, "fresh"), now).is_none());
}
