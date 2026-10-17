use super::*;
use crate::memory::{Observation, Profile, Topic};
use std::collections::BTreeMap;
use std::{net::SocketAddr, path::PathBuf};
fn obs() -> Observation {
    Observation {
        original: "Agent".into(),
        translation: "智能体".into(),
        domain: "Agent".into(),
        sense_summary: "自主执行任务的系统".into(),
        language: "English".into(),
    }
}
async fn configured(path: PathBuf, c: Context) -> Memory {
    let m = Memory::open(path).unwrap();
    m.call(move |db| {
        storage::migrate(db)?;
        let mut d = load(db)?.unwrap();
        d.context = Some(c);
        d.binding = "fixture-binding".into();
        d.enabled = true;
        d.memory_allowed = true;
        save(db, &d)
    })
    .await
    .unwrap();
    m
}
fn ctx() -> Context {
    Context {
        protocol: 1,
        workspace_id: "fixture-workspace".into(),
        history_id: "fixture-history".into(),
        memory_generation: 1,
        revision: 0,
    }
}
async fn record(m: &Memory, job: &str) -> i64 {
    m.record(
        m.epoch().await.unwrap(),
        job.into(),
        "manual".into(),
        vec![obs()],
    )
    .await
    .unwrap()[0]
}
async fn scan(m: &Memory) {
    m.call(|db| {
        let tx = db.transaction().map_err(err)?;
        let mut d = load(&tx)?.unwrap();
        storage::scan(&tx, &mut d)?;
        save(&tx, &d)?;
        tx.commit().map_err(err)
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn v2_migration_keeps_ids_counts_feedback_profiles_and_private_v1_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("memory.db");
    let m = Memory::open(path.clone()).unwrap();
    let id = record(&m, "old").await;
    m.feedback(id, "known".into()).await.unwrap();
    let s = m.snapshot(false).await.unwrap();
    m.finish(
        s,
        Profile {
            topics: vec![Topic {
                domain: "Agent".into(),
                evidence_ids: vec![id],
            }],
        },
    )
    .await
    .unwrap();
    m.call(|db| {
        storage::migrate(db)?;
        storage::migrate(db)
    })
    .await
    .unwrap();
    let v = m.view("Agent".into(), 0).await.unwrap();
    assert_eq!(v.terms[0].id, id);
    assert_eq!(v.terms[0].count, 1);
    assert_eq!(v.terms[0].familiarity, "known");
    assert_eq!(v.versions.len(), 1);
    let old =
        rusqlite::Connection::open(format!("{}.pre-sync-v1.sqlite3", path.display())).unwrap();
    assert_eq!(
        old.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(format!("{}.pre-sync-v1.sqlite3", path.display()))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let reopened = Memory::open(path).unwrap();
    assert_eq!(
        reopened
            .call(|db| db
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .map_err(err))
            .await
            .unwrap(),
        2
    );
}
#[tokio::test]
async fn bootstrap_clones_have_same_event_identity_and_only_current_feedback() {
    let d = tempfile::tempdir().unwrap();
    let a = configured(d.path().join("a.db"), ctx()).await;
    let id = record(&a, "cloned-job").await;
    a.feedback(id, "learning".into()).await.unwrap();
    a.feedback(id, "known".into()).await.unwrap();
    let b = configured(d.path().join("b.db"), ctx()).await;
    record(&b, "cloned-job").await;
    scan(&a).await;
    let da = snapshot(&a).await.unwrap();
    let db = snapshot(&b).await.unwrap();
    let eid = |d: &Data| {
        d.queue
            .iter()
            .find_map(|p| {
                if let Action::Observe { event, .. } = &p.op.action {
                    Some(event.id.clone())
                } else {
                    None
                }
            })
            .unwrap()
    };
    assert_eq!(eid(&da), eid(&db));
    assert_eq!(da.queue.len(), 1);
    assert_eq!(
        da.entities.values().next().unwrap().initial_feedback,
        Some(Familiarity::Known)
    );
    scan(&a).await;
    assert_eq!(snapshot(&a).await.unwrap().queue.len(), 1);
}
#[tokio::test]
async fn atomic_hooks_reject_partial_business_and_outbox_writes() {
    let d = tempfile::tempdir().unwrap();
    let m = configured(d.path().join("a.db"), ctx()).await;
    m.call(|db|{db.execute_batch("CREATE TRIGGER fail_sync BEFORE UPDATE ON sync_state BEGIN SELECT RAISE(ABORT,'fixture'); END;").map_err(err)}).await.unwrap();
    assert!(m
        .record(
            m.epoch().await.unwrap(),
            "rollback".into(),
            "manual".into(),
            vec![obs()]
        )
        .await
        .is_err());
    assert_eq!(m.view("".into(), 0).await.unwrap().total, 0);
}
#[tokio::test]
async fn page_is_atomic_and_disable_or_privacy_pause_rejects_late_import() {
    let d = tempfile::tempdir().unwrap();
    let m = configured(d.path().join("a.db"), ctx()).await;
    let term = Term {
        original: "Agent".into(),
        translation: "智能体".into(),
        domain: "Agent".into(),
        sense: "自主执行任务的系统".into(),
        language: "English".into(),
    };
    let mut c = ctx();
    c.revision = 2;
    let page = Page {
        context: c,
        stream: Stream::Memory,
        until: 2,
        next_cursor: 2,
        has_more: false,
        changes: vec![
            Record {
                revision: 1,
                change: Change::Term {
                    key: term.key(),
                    term,
                    incarnation: "inc".into(),
                    tombstone_revision: 0,
                },
            },
            Record {
                revision: 2,
                change: Change::Occurrence {
                    key: "key".into(),
                    incarnation: "inc".into(),
                    event: Occurrence {
                        id: "bad/id".into(),
                        at: 0,
                        source: "manual".into(),
                    },
                },
            },
        ],
    };
    let p = page.clone();
    assert!(m
        .call(move |db| storage::apply(db, "fixture-binding:0", p))
        .await
        .is_err());
    assert_eq!(m.view("".into(), 0).await.unwrap().total, 0);
    assert_eq!(snapshot(&m).await.unwrap().memory_cursor, 0);
    m.call(|db| {
        let mut d = load(db)?.unwrap();
        d.enabled = false;
        save(db, &d)
    })
    .await
    .unwrap();
    assert!(m
        .call(move |db| storage::apply(db, "fixture-binding:0", page))
        .await
        .is_err());
}
async fn service() -> (
    tempfile::TempDir,
    Arc<asa_sync_server::store::Store>,
    Http,
    Context,
    tokio::task::JoinHandle<()>,
) {
    let d = tempfile::tempdir().unwrap();
    let s = Arc::new(
        asa_sync_server::store::Store::open(
            &d.path().join("server.db"),
            asa_sync_server::store::Limits {
                requests: 10000,
                logins: 100,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let hash = asa_sync_server::web::hash_password("fixture-sync-password-only").unwrap();
    s.set_password(&hash, true).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let r = asa_sync_server::web::router(s.clone());
    let task = tokio::spawn(async move {
        axum_serve(listener, r).await;
    });
    let url = format!("http://{addr}/");
    let h = Http::new(&url, None).unwrap();
    let enrollment:Enrollment=transport::unwrap(h.request(Method::POST,"v1/devices/enroll",Some(serde_json::json!({"protocol":1,"password":"fixture-sync-password-only","deviceName":"integration"}))).await.unwrap()).unwrap();
    let context = enrollment.context.clone();
    (
        d,
        s,
        Http::new(&url, Some(Zeroizing::new(enrollment.token))).unwrap(),
        context,
        task,
    )
}
async fn axum_serve(listener: tokio::net::TcpListener, r: axum::Router) {
    axum::serve(
        listener,
        r.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
async fn pump(m: &Memory, h: &Http) {
    pull(m, h, "fixture-binding:0", Stream::Preferences)
        .await
        .unwrap();
    pull(m, h, "fixture-binding:0", Stream::Memory)
        .await
        .unwrap();
    scan(m).await;
    push(m, h, "fixture-binding:0").await.unwrap();
    pull(m, h, "fixture-binding:0", Stream::Preferences)
        .await
        .unwrap();
    pull(m, h, "fixture-binding:0", Stream::Memory)
        .await
        .unwrap();
}
#[tokio::test]
async fn real_http_two_clients_retries_conflicts_delete_relearn_reset_and_history() {
    let (_d, s, h, c, task) = service().await;
    let tmp = tempfile::tempdir().unwrap();
    let a = configured(tmp.path().join("a.db"), c.clone()).await;
    let b = configured(tmp.path().join("b.db"), c.clone()).await;
    let id = record(&a, "cloned-event").await;
    record(&b, "cloned-event").await;
    record(&b, "unique-event").await;
    pump(&a, &h).await;
    pump(&b, &h).await;
    pump(&a, &h).await;
    assert_eq!(a.view("".into(), 0).await.unwrap().terms[0].count, 2);
    assert_eq!(b.view("".into(), 0).await.unwrap().terms[0].count, 2);
    assert!(snapshot(&a).await.unwrap().queue.is_empty());
    // Accepted response is lost; retain the exact sent operation and retry after actor restart.
    record(&a, "lost-response").await;
    let mut da = snapshot(&a).await.unwrap();
    let ops = da.queue.iter().map(|p| p.op.clone()).collect::<Vec<_>>();
    let push_request = Push {
        protocol: 1,
        workspace_id: c.workspace_id.clone(),
        history_id: c.history_id.clone(),
        operations: ops,
    };
    let _: Accepted = transport::unwrap(
        h.request(
            Method::POST,
            "v1/push",
            Some(serde_json::to_value(push_request).unwrap()),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    for p in &mut da.queue {
        p.sent = true;
    }
    a.call(move |db| save(db, &da)).await.unwrap();
    let reopened = Memory::open(tmp.path().join("a.db")).unwrap();
    pump(&reopened, &h).await;
    assert_eq!(reopened.view("".into(), 0).await.unwrap().terms[0].count, 3);
    let bid = b.view("".into(), 0).await.unwrap().terms[0].id;
    a.feedback(id, "known".into()).await.unwrap();
    b.feedback(bid, "learning".into()).await.unwrap();
    pump(&a, &h).await;
    pump(&b, &h).await;
    let pending = snapshot(&b).await.unwrap();
    assert_eq!(
        pending
            .queue
            .iter()
            .filter(|p| p.conflict.is_some())
            .count(),
        1
    );
    assert_eq!(
        b.view("".into(), 0).await.unwrap().terms[0].familiarity,
        "learning"
    );
    a.delete(Some(id)).await.unwrap();
    pump(&a, &h).await;
    pump(&b, &h).await;
    assert_eq!(b.view("".into(), 0).await.unwrap().total, 0);
    assert!(snapshot(&b).await.unwrap().queue.is_empty());
    record(&b, "new-learning").await;
    pump(&b, &h).await;
    pump(&a, &h).await;
    assert_eq!(a.view("".into(), 0).await.unwrap().terms[0].count, 1);
    // Preferences progress independently while memory is paused.
    b.call(|db| {
        let mut d = load(db)?.unwrap();
        d.memory_allowed = false;
        save(db, &d)
    })
    .await
    .unwrap();
    let memory_cursor = snapshot(&b).await.unwrap().memory_cursor;
    let cc = s.context().unwrap();
    let reset = Reset {
        protocol: 1,
        workspace_id: cc.workspace_id,
        history_id: cc.history_id,
        id: "global-clear-fixture".into(),
        generation: cc.memory_generation,
    };
    s.reset(&reset).unwrap();
    a.call(|db| {
        storage::preferences(
            db,
            &storage::prefs(&crate::settings::Settings {
                theme: "dark".into(),
                ..Default::default()
            }),
        )
    })
    .await
    .unwrap();
    pump(&a, &h).await;
    pump(&b, &h).await;
    assert_eq!(snapshot(&b).await.unwrap().memory_cursor, memory_cursor);
    assert_eq!(
        snapshot(&b)
            .await
            .unwrap()
            .inbox
            .get("theme")
            .map(String::as_str),
        Some("dark")
    );
    assert_eq!(b.view("".into(), 0).await.unwrap().total, 1);
    b.call(|db| {
        let mut d = load(db)?.unwrap();
        d.memory_allowed = true;
        save(db, &d)
    })
    .await
    .unwrap();
    pump(&b, &h).await;
    assert_eq!(b.view("".into(), 0).await.unwrap().total, 0);
    s.restored().unwrap();
    assert!(pull(&a, &h, "fixture-binding:0", Stream::Memory)
        .await
        .is_err());
    let mut other = s.context().unwrap();
    other.history_id = "changed-history".into();
    assert_eq!(
        storage::check(&snapshot(&a).await.unwrap(), &other)
            .unwrap_err()
            .code,
        "sync_history"
    );
    task.abort();
}
#[test]
fn endpoint_and_preference_allowlist_reject_credentials_and_unauthorized_fields() {
    for u in [
        "http://example.com",
        "https://user:pass@example.com",
        "https://example.com/?x=1",
        "https://example.com/#x",
        "file:///tmp/x",
    ] {
        assert!(transport::endpoint(u).is_err());
    }
    assert_eq!(
        transport::endpoint("http://127.0.0.1:1234").unwrap(),
        "http://127.0.0.1:1234/"
    );
    let mut settings = crate::settings::Settings::default();
    let before = settings.clone();
    let mut values = BTreeMap::new();
    values.insert("rememberTerms".into(), "true".into());
    assert!(storage::patch(&mut settings, &values).is_err());
    assert_eq!(settings, before);
    values.clear();
    values.insert("targetLanguage".into(), "日本語".into());
    storage::patch(&mut settings, &values).unwrap();
    assert!(!settings.remember_terms);
    assert_eq!(settings.base_url, before.base_url);
}

#[tokio::test]
async fn settings_intent_recovery_inbox_and_cancel_generation_are_durable() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.db");
    let m = configured(path.clone(), ctx()).await;
    let mut config = crate::settings::Settings::default();
    config.theme = "dark".into();
    let prefs = storage::prefs(&config);
    m.prepare_sync_preferences(prefs.clone()).await.unwrap();
    let reopened = Memory::open(path).unwrap();
    let expected = prefs.clone();
    assert_eq!(snapshot(&reopened).await.unwrap().intent, Some(expected));
    reopened
        .call(move |db| storage::preferences(db, &prefs))
        .await
        .unwrap();
    let d = snapshot(&reopened).await.unwrap();
    assert!(d.intent.is_none());
    assert_eq!(d.queue.len(), 3);
    assert!(!serde_json::to_string(&d).unwrap().contains("apiKey"));
    let old_ticket = storage::ticket(&d);
    reopened
        .call(|db| {
            let mut d = load(db)?.unwrap();
            d.enabled = false;
            d.session += 1;
            save(db, &d)?;
            d.enabled = true;
            d.session += 1;
            save(db, &d)
        })
        .await
        .unwrap();
    assert!(storage::ensure(&snapshot(&reopened).await.unwrap(), &old_ticket).is_err());
}

#[tokio::test]
async fn http_transport_rejects_redirects_oversized_and_extra_secret_fields() {
    use axum::{response::Redirect, routing::get, Json, Router};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let routes = Router::new()
        .route("/large", get(|| async { vec![b'x'; MAX_BYTES + 1] }))
        .route(
            "/extra",
            get(|| async {
                let mut c = serde_json::to_value(ctx()).unwrap();
                c["apiKey"] = "secret-marker".into();
                Json(c)
            }),
        )
        .route(
            "/redirect",
            get(|| async { Redirect::temporary("https://unapproved.invalid") }),
        );
    let task = tokio::spawn(axum_serve(listener, routes));
    let h = Http::new(
        &format!("http://{addr}"),
        Some(Zeroizing::new("fixture-device-token".into())),
    )
    .unwrap();
    for (path, code) in [
        ("large", "sync_response_size"),
        ("extra", "sync_protocol"),
        ("redirect", "sync_redirect"),
    ] {
        match h.request::<Context>(Method::GET, path, None).await {
            Err(e) => {
                assert_eq!(e.code, code);
                assert!(!e.message.contains("secret-marker"));
                assert!(!e.message.contains("fixture-device-token"));
            }
            _ => panic!("unsafe response accepted"),
        }
    }
    task.abort();
}

#[tokio::test]
async fn conflict_choices_use_latest_versions_and_allow_independent_progress() {
    let (_d, _s, h, c, task) = service().await;
    let tmp = tempfile::tempdir().unwrap();
    let a = configured(tmp.path().join("a.db"), c.clone()).await;
    let b = configured(tmp.path().join("b.db"), c).await;
    let aid = record(&a, "one").await;
    pump(&a, &h).await;
    pump(&b, &h).await;
    let bid = b.view("".into(), 0).await.unwrap().terms[0].id;
    a.feedback(aid, "known".into()).await.unwrap();
    b.feedback(bid, "learning".into()).await.unwrap();
    pump(&a, &h).await;
    pump(&b, &h).await;
    let id = snapshot(&b)
        .await
        .unwrap()
        .queue
        .iter()
        .find(|p| p.conflict.is_some())
        .unwrap()
        .op
        .id
        .clone();
    b.call(move |db| resolve_value(db, id, false))
        .await
        .unwrap();
    assert_eq!(
        b.view("".into(), 0).await.unwrap().terms[0].familiarity,
        "known"
    );
    pump(&b, &h).await;
    assert!(snapshot(&b).await.unwrap().queue.is_empty());
    a.feedback(aid, "learning".into()).await.unwrap();
    b.feedback(bid, "unknown".into()).await.unwrap();
    pump(&a, &h).await;
    pump(&b, &h).await;
    let id = snapshot(&b)
        .await
        .unwrap()
        .queue
        .iter()
        .find(|p| p.conflict.is_some())
        .unwrap()
        .op
        .id
        .clone();
    b.call(move |db| resolve_value(db, id, true)).await.unwrap();
    pump(&b, &h).await;
    pump(&a, &h).await;
    assert_eq!(
        a.view("".into(), 0).await.unwrap().terms[0].familiarity,
        "unknown"
    );
    task.abort();
}
#[tokio::test]
async fn failed_backup_or_migration_keeps_v1_and_existing_data() {
    for migration in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("memory.db");
        let m = Memory::open(path.clone()).unwrap();
        record(&m, "one").await;
        if migration {
            m.call(|db| {
                db.execute_batch("CREATE TABLE sync_state(already TEXT)")
                    .map_err(err)
            })
            .await
            .unwrap();
        } else {
            std::fs::create_dir(format!("{}.pre-sync-v1.sqlite3", path.display())).unwrap();
        }
        assert!(m.call(storage::migrate).await.is_err());
        assert_eq!(
            m.call(|db| db
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .map_err(err))
                .await
                .unwrap(),
            1
        );
        assert_eq!(m.view("".into(), 0).await.unwrap().terms[0].count, 1);
    }
}

#[test]
fn partial_credential_writes_compensate_without_touching_existing_binding() {
    use std::sync::Mutex;
    struct Fake {
        values: Mutex<BTreeMap<String, String>>,
        write_fails: bool,
        remove_fails: bool,
    }
    impl Credentials for Fake {
        fn write(&self, b: &str, v: &str) -> AppResult<()> {
            self.values.lock().unwrap().insert(b.into(), v.into());
            if self.write_fails {
                Err(vault_error())
            } else {
                Ok(())
            }
        }
        fn remove(&self, b: &str) -> AppResult<()> {
            if self.remove_fails {
                return Err(vault_error());
            }
            self.values.lock().unwrap().remove(b);
            Ok(())
        }
    }
    for failed_cleanup in [false, true] {
        let vault = Fake {
            values: Mutex::new(BTreeMap::from([("old-binding".into(), "old-token".into())])),
            write_fails: true,
            remove_fails: failed_cleanup,
        };
        let failure = stage_credential(&vault, "provisional-binding", "fixture-token").unwrap_err();
        assert!(!failure.message.contains("fixture-token"));
        assert_eq!(
            vault.values.lock().unwrap().get("old-binding").unwrap(),
            "old-token"
        );
        assert_eq!(
            failure.code,
            if failed_cleanup {
                "sync_keyring_recovery"
            } else {
                "sync_keyring"
            }
        );
        if !failed_cleanup {
            assert!(!vault
                .values
                .lock()
                .unwrap()
                .contains_key("provisional-binding"));
        }
    }
    let vault = Fake {
        values: Mutex::new(BTreeMap::new()),
        write_fails: false,
        remove_fails: false,
    };
    stage_credential(&vault, "unpublished-binding", "fixture-token").unwrap();
    undo_credential(&vault, "unpublished-binding").unwrap();
    assert!(vault.values.lock().unwrap().is_empty());
}
#[tokio::test]
async fn scheduler_fake_clock_preserves_backoff_and_stops_revoked_devices_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.db");
    let m = configured(path.clone(), ctx()).await;
    m.call(|db| {
        let mut d = load(db)?.unwrap();
        d.background = true;
        failed(&mut d, transport::rejection_after(429, 120), 100, 0);
        save(db, &d)
    })
    .await
    .unwrap();
    let reopened = Memory::open(path).unwrap();
    let d = snapshot(&reopened).await.unwrap();
    assert!(!scheduled(&d, 219));
    assert!(scheduled(&d, 220));
    let mut d = d;
    failed(&mut d, transport::rejection(401), 220, 0);
    assert!(!scheduled(&d, 1_000_000));
    assert_eq!(d.status, "needs-auth");
    d.status = "idle".into();
    d.failures = 100;
    failed(&mut d, transport::network(), 300, 4);
    assert!(d.next_attempt <= 600);
    d.enabled = false;
    assert!(!scheduled(&d, 1_000_000));
}

#[tokio::test]
async fn local_only_clear_reimports_cloud_without_stale_ids_or_double_count() {
    let (_d, _s, h, c, task) = service().await;
    let dir = tempfile::tempdir().unwrap();
    let m = configured(dir.path().join("a.db"), c).await;
    record(&m, "one").await;
    pump(&m, &h).await;
    m.call(|db| {
        let mut d = load(db)?.unwrap();
        d.enabled = false;
        save(db, &d)
    })
    .await
    .unwrap();
    m.delete(None).await.unwrap();
    assert_eq!(m.view("".into(), 0).await.unwrap().total, 0);
    m.call(|db| {
        let mut d = load(db)?.unwrap();
        d.enabled = true;
        storage::scan(db, &mut d)?;
        save(db, &d)
    })
    .await
    .unwrap();
    pump(&m, &h).await;
    assert_eq!(m.view("".into(), 0).await.unwrap().terms[0].count, 1);
    assert!(snapshot(&m).await.unwrap().queue.is_empty());
    task.abort();
}
#[tokio::test]
async fn delayed_receipt_never_overrides_newer_accepted_feedback() {
    let (_d, _s, h, c, task) = service().await;
    let dir = tempfile::tempdir().unwrap();
    let a = configured(dir.path().join("a.db"), c.clone()).await;
    let b = configured(dir.path().join("b.db"), c.clone()).await;
    let aid = record(&a, "one").await;
    pump(&a, &h).await;
    pump(&b, &h).await;
    a.feedback(aid, "learning".into()).await.unwrap();
    let da = snapshot(&a).await.unwrap();
    let request = Push {
        protocol: 1,
        workspace_id: c.workspace_id,
        history_id: c.history_id,
        operations: da.queue.iter().map(|p| p.op.clone()).collect(),
    };
    let _: Accepted = transport::unwrap(
        h.request(
            Method::POST,
            "v1/push",
            Some(serde_json::to_value(request).unwrap()),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    pump(&b, &h).await;
    let bid = b.view("".into(), 0).await.unwrap().terms[0].id;
    b.feedback(bid, "known".into()).await.unwrap();
    pump(&b, &h).await;
    pump(&a, &h).await;
    assert_eq!(
        a.view("".into(), 0).await.unwrap().terms[0].familiarity,
        "known"
    );
    assert!(snapshot(&a).await.unwrap().queue.is_empty());
    task.abort();
}

#[tokio::test]
async fn preference_fields_merge_and_target_language_conflicts_resolve_without_echo() {
    let (_d, _s, h, c, task) = service().await;
    let dir = tempfile::tempdir().unwrap();
    let a = configured(dir.path().join("a.db"), c.clone()).await;
    let b = configured(dir.path().join("b.db"), c).await;
    let mut sa = crate::settings::Settings::default();
    let mut sb = sa.clone();
    let values = storage::prefs(&sa);
    a.call(move |db| storage::preferences(db, &values))
        .await
        .unwrap();
    pump(&a, &h).await;
    pump(&b, &h).await;
    sa.theme = "dark".into();
    sb.explanation_style = "practical".into();
    let va = storage::prefs(&sa);
    let vb = storage::prefs(&sb);
    a.call(move |db| storage::preferences(db, &va))
        .await
        .unwrap();
    b.call(move |db| storage::preferences(db, &vb))
        .await
        .unwrap();
    pump(&a, &h).await;
    pump(&b, &h).await;
    assert!(snapshot(&b).await.unwrap().queue.is_empty());
    pump(&a, &h).await;
    assert_eq!(
        snapshot(&a).await.unwrap().prefs["explanationStyle"].value,
        "practical"
    );
    sa.explanation_style = "practical".into();
    sb.theme = "dark".into();
    sa.target_language = "English".into();
    sb.target_language = "日本語".into();
    let va = storage::prefs(&sa);
    let vb = storage::prefs(&sb);
    a.call(move |db| storage::preferences(db, &va))
        .await
        .unwrap();
    b.call(move |db| storage::preferences(db, &vb))
        .await
        .unwrap();
    pump(&a, &h).await;
    pump(&b, &h).await;
    let id = snapshot(&b)
        .await
        .unwrap()
        .queue
        .iter()
        .find(|p| p.conflict.is_some())
        .unwrap()
        .op
        .id
        .clone();
    b.call(move |db| resolve_value(db, id, true)).await.unwrap();
    pump(&b, &h).await;
    pump(&a, &h).await;
    assert_eq!(
        snapshot(&a).await.unwrap().prefs["targetLanguage"].value,
        "日本語"
    );
    let values = storage::prefs(&sb);
    b.call(move |db| storage::preferences(db, &values))
        .await
        .unwrap();
    assert!(snapshot(&b).await.unwrap().queue.is_empty());
    task.abort();
}
