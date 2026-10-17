use super::*;
use serde_json::json;

#[tokio::test]
async fn operation_states_reset_and_unavailable_storage_are_independent() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Runtime::open(dir.path().join("usage.sqlite3"));
    let config = Settings::default();
    let op = runtime.operation(&config, Purpose::Translation);
    assert_eq!(op.snapshot().usage.attempts, 0);
    let cache = op.cache_hit();
    assert_eq!(cache.source, "local-cache");
    assert_eq!(cache.usage.total.tokens, Some(0));
    assert_eq!(cache.usage.attempts, 0);
    let mut first = op.begin();
    first.finish(Counts::parse(
        &json!({"usage":{"prompt_tokens":100,"completion_tokens":30,"total_tokens":130}}),
    ));
    first.finish(Counts {
        total: Some(999),
        ..Counts::default()
    });
    drop(op.begin()); // timeout/drop keeps coverage unknown
    let view = runtime.query(Query::default()).await.unwrap();
    assert_eq!(view.cumulative.total.tokens, Some(130));
    assert_eq!(view.cumulative.attempts, 2);
    assert_eq!(view.cumulative.total.unknown, 1);
    let old = op.begin();
    runtime.clear().await.unwrap();
    drop(old);
    let mut late = op.begin();
    late.finish(Counts {
        total: Some(42),
        ..Counts::default()
    });
    assert!(op.snapshot().excluded_after_reset);
    assert_eq!(
        runtime
            .query(Query::default())
            .await
            .unwrap()
            .cumulative
            .attempts,
        0
    );
    let new = runtime.operation(&config, Purpose::ManualAnalysis);
    drop(new.begin());
    assert_eq!(
        runtime
            .query(Query::default())
            .await
            .unwrap()
            .cumulative
            .attempts,
        1
    );
    let inaccessible = Runtime::open(dir.path().to_path_buf());
    let operation = inaccessible.operation(&config, Purpose::AutoAnalysis);
    let mut ticket = operation.begin();
    ticket.finish(Counts {
        total: Some(7),
        ..Counts::default()
    });
    assert_eq!(operation.snapshot().usage.total.tokens, Some(7));
    assert!(operation.snapshot().persistence_incomplete);
    assert!(inaccessible.query(Query::default()).await.is_err());
}

#[tokio::test]
async fn bounded_queue_fails_fast_and_preserves_in_memory_observations() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Runtime::open(dir.path().join("usage.db"));
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = mpsc::channel();
    runtime.submit(Box::new(move |_| {
        let _ = started_tx.send(());
        let _ = release_rx.recv();
    }));
    started_rx.await.unwrap();
    for _ in 0..128 {
        assert!(runtime.submit(Box::new(|_| {})));
    }
    let op = runtime.operation(&Settings::default(), Purpose::Translation);
    let start = std::time::Instant::now();
    let mut t = op.begin();
    t.finish(Counts {
        input: Some(10),
        ..Counts::default()
    });
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(op.snapshot().persistence_incomplete);
    assert_eq!(op.snapshot().usage.input.tokens, Some(10));
    release_tx.send(()).unwrap();
    // A barrier may race a full queue; it must fail rather than hang.
    let _ = runtime.query(Query::default()).await;
}

#[test]
fn grouping_uses_captured_configuration_and_never_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Runtime::open(dir.path().join("usage.db"));
    let mut config = Settings::default();
    let first = runtime.operation(&config, Purpose::ConnectionTest);
    config.model = "another-model".into();
    config.base_url = "https://example.com/other".into();
    let other = runtime.operation(&config, Purpose::Translation);
    assert_ne!(first.service, other.service);
    assert_ne!(first.model, other.model);
    assert_eq!(first.label, "api.openai.com");
    assert!(!first.service.contains("https"));
}

#[tokio::test]
async fn injected_clock_captures_each_attempt_across_dst_midnight_and_month() {
    struct Fixed(Mutex<ClockStamp>);
    impl Clock for Fixed {
        fn stamp(&self) -> ClockStamp {
            self.0.lock().unwrap().clone()
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(Fixed(Mutex::new(ClockStamp {
        utc: 100,
        date: "2026-10-31".into(),
        offset: 3600,
    })));
    let runtime = Runtime::with_clock(dir.path().join("usage.db"), clock.clone());
    let op = runtime.operation(&Settings::default(), Purpose::Translation);
    let mut a = op.begin();
    *clock.0.lock().unwrap() = ClockStamp {
        utc: 200,
        date: "2026-11-01".into(),
        offset: 0,
    };
    a.finish(Counts {
        total: Some(100),
        ..Counts::default()
    });
    let mut b = op.begin();
    b.finish(Counts {
        total: Some(30),
        ..Counts::default()
    });
    let view = runtime.query(Query::default()).await.unwrap();
    assert_eq!(view.month.total.tokens, Some(30));
    assert_eq!(view.today.total.tokens, Some(30));
    assert_eq!(view.cumulative.total.tokens, Some(130));
    *clock.0.lock().unwrap() = ClockStamp {
        utc: 50,
        date: "2026-10-31".into(),
        offset: -18000,
    };
    let view = runtime.query(Query::default()).await.unwrap();
    assert_eq!(view.today.total.tokens, Some(100));
    assert_eq!(view.cumulative.total.tokens, Some(130));
}

#[tokio::test]
async fn concurrent_operations_never_overwrite_each_other() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Runtime::open(dir.path().join("usage.db"));
    let mut handles = vec![];
    for purpose in [
        Purpose::Translation,
        Purpose::ConnectionTest,
        Purpose::ManualAnalysis,
        Purpose::AutoAnalysis,
    ] {
        for _ in 0..4 {
            let r = runtime.clone();
            handles.push(std::thread::spawn(move || {
                let op = r.operation(&Settings::default(), purpose);
                let mut t = op.begin();
                t.finish(Counts {
                    total: Some(10),
                    ..Counts::default()
                });
                assert_eq!(op.snapshot().usage.total.tokens, Some(10));
            }));
        }
    }
    for handle in handles {
        handle.join().unwrap();
    }
    let view = runtime.query(Query::default()).await.unwrap();
    assert_eq!(view.cumulative.total.tokens, Some(160));
    assert_eq!(view.cumulative.attempts, 16);
    assert_eq!(view.group_count, 4);
}
