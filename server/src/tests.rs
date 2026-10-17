use super::{
    store::{Limits, Store},
    web::{hash_password, router},
};
use asa_sync_protocol::*;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use std::{
    net::SocketAddr,
    sync::{Arc, OnceLock},
};
use tower::ServiceExt;
const PASSWORD: &str = "fixture-sync-password-only";
fn hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| hash_password(PASSWORD).unwrap())
}
fn store() -> (tempfile::TempDir, Store) {
    let d = tempfile::tempdir().unwrap();
    let s = Store::open(&d.path().join("server.sqlite3"), Limits::default()).unwrap();
    s.set_password(hash(), true).unwrap();
    (d, s)
}
fn term() -> Term {
    Term {
        original: "Agent".into(),
        translation: "智能体".into(),
        domain: "Agent".into(),
        sense: "自主执行任务的系统".into(),
        language: "English".into(),
    }
}
fn observe(id: &str) -> Action {
    Action::Observe {
        term: term(),
        event: Occurrence {
            id: id.into(),
            at: 1,
            source: "manual".into(),
        },
        incarnation: None,
        tombstone_revision: 0,
    }
}
fn request(s: &Store, actions: Vec<Action>) -> Push {
    let c = s.context().unwrap();
    Push {
        protocol: VERSION,
        workspace_id: c.workspace_id,
        history_id: c.history_id,
        operations: actions
            .into_iter()
            .enumerate()
            .map(|(i, action)| Operation {
                id: format!("op-{}-{i}", uuid::Uuid::new_v4()),
                generation: c.memory_generation,
                action,
            })
            .collect(),
    }
}
fn push(s: &Store, actions: Vec<Action>) -> Accepted {
    s.push(&request(s, actions)).unwrap()
}
fn incarnation(s: &Store) -> String {
    s.page(Stream::Memory, 0, None, MAX_PAGE)
        .unwrap()
        .changes
        .iter()
        .find_map(|r| {
            if let Change::Term { incarnation, .. } = &r.change {
                Some(incarnation.clone())
            } else {
                None
            }
        })
        .unwrap()
}
fn count(s: &Store, table: &str) -> i64 {
    s.db.lock()
        .unwrap()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
#[test]
fn durable_receipts_deduplicate_retries_and_cloned_events() {
    let (d, s) = store();
    let request = request(&s, vec![observe("shared-event")]);
    let accepted = s.push(&request).unwrap();
    assert_eq!(count(&s, "events"), 1);
    assert_eq!(
        s.push(&request).unwrap().context.revision,
        accepted.context.revision
    );
    push(&s, vec![observe("shared-event"), observe("distinct-event")]);
    assert_eq!(count(&s, "events"), 2);
    let mut reused = request.clone();
    reused.operations[0].action = observe("other");
    assert_eq!(
        s.push(&reused).unwrap_err().body.code,
        "operation_identity_reused"
    );
    drop(s);
    let reopened = Store::open(&d.path().join("server.sqlite3"), Limits::default()).unwrap();
    let before = reopened.context().unwrap().revision;
    reopened.push(&request).unwrap();
    assert_eq!(reopened.context().unwrap().revision, before);
    assert_eq!(count(&reopened, "events"), 2);
}
#[test]
fn conflicting_batch_rolls_back_all_new_operations() {
    let (_d, s) = store();
    push(
        &s,
        vec![Action::Preference {
            field: Preference::Theme,
            base_revision: 0,
            value: "light".into(),
        }],
    );
    let before = s.context().unwrap().revision;
    let bad = request(
        &s,
        vec![
            observe("must-rollback"),
            Action::Preference {
                field: Preference::Theme,
                base_revision: 0,
                value: "dark".into(),
            },
        ],
    );
    let failure = s.push(&bad).unwrap_err();
    assert_eq!(failure.status, StatusCode::CONFLICT);
    assert_eq!(
        failure.body.conflicts[0].remote_value.as_deref(),
        Some("light")
    );
    assert_eq!(count(&s, "events"), 0);
    assert_eq!(s.context().unwrap().revision, before);
}
#[test]
fn field_versions_merge_independent_fields_and_refuse_stale_feedback() {
    let (_d, s) = store();
    push(&s, vec![observe("one")]);
    let inc = incarnation(&s);
    let first = push(
        &s,
        vec![Action::Feedback {
            key: term().key(),
            incarnation: inc.clone(),
            base_revision: 0,
            value: Familiarity::Known,
        }],
    );
    let failure = s
        .push(&request(
            &s,
            vec![Action::Feedback {
                key: term().key(),
                incarnation: inc.clone(),
                base_revision: 0,
                value: Familiarity::Learning,
            }],
        ))
        .unwrap_err();
    assert_eq!(
        failure.body.conflicts[0].remote_value.as_deref(),
        Some("known")
    );
    push(
        &s,
        vec![Action::Feedback {
            key: term().key(),
            incarnation: inc,
            base_revision: first.context.revision,
            value: Familiarity::Learning,
        }],
    );
    push(
        &s,
        vec![
            Action::Preference {
                field: Preference::Theme,
                base_revision: 0,
                value: "dark".into(),
            },
            Action::Preference {
                field: Preference::TargetLanguage,
                base_revision: 0,
                value: "日本語".into(),
            },
        ],
    );
    assert_eq!(count(&s, "preferences"), 2);
}
#[test]
fn deletion_redacts_history_blocks_old_lifetimes_and_allows_relearning() {
    let (_d, s) = store();
    let original = request(&s, vec![observe("old-event")]);
    s.push(&original).unwrap();
    let old_inc = incarnation(&s);
    let before = s.context().unwrap().revision;
    let deleted = push(
        &s,
        vec![Action::Delete {
            key: term().key(),
            incarnation: old_inc.clone(),
        }],
    );
    let page = s.page(Stream::Memory, 0, Some(before), MAX_PAGE).unwrap();
    assert!(page
        .changes
        .iter()
        .all(|r| matches!(r.change, Change::Redacted)));
    assert_eq!(page.next_cursor, before);
    s.push(&original).unwrap();
    assert_eq!(count(&s, "events"), 0);
    assert!(s.push(&request(&s, vec![observe("stale-new-id")])).is_err());
    let mut fresh = observe("fresh");
    if let Action::Observe {
        tombstone_revision, ..
    } = &mut fresh
    {
        *tombstone_revision = deleted.context.revision;
    }
    push(&s, vec![fresh.clone()]);
    let new_inc = incarnation(&s);
    assert_ne!(old_inc, new_inc);
    let mut second = fresh;
    if let Action::Observe { event, .. } = &mut second {
        event.id = "fresh-on-other-device".into();
    }
    push(&s, vec![second]);
    assert_eq!(count(&s, "events"), 2);
    let mut stale = observe("old-lifetime");
    if let Action::Observe { incarnation, .. } = &mut stale {
        *incarnation = Some(old_inc);
    }
    assert!(s.push(&request(&s, vec![stale])).is_err());
}
#[test]
fn reset_is_idempotent_and_preserves_preferences() {
    let (_d, s) = store();
    push(
        &s,
        vec![
            observe("one"),
            Action::Preference {
                field: Preference::Theme,
                base_revision: 0,
                value: "dark".into(),
            },
        ],
    );
    let old = request(&s, vec![observe("offline")]);
    let ctx = s.context().unwrap();
    let reset = Reset {
        protocol: 1,
        workspace_id: ctx.workspace_id,
        history_id: ctx.history_id,
        id: "reset-request".into(),
        generation: ctx.memory_generation,
    };
    let accepted = s.reset(&reset).unwrap();
    let revision = accepted.context.revision;
    assert_eq!(accepted.context.memory_generation, 2);
    assert_eq!(s.reset(&reset).unwrap().context.revision, revision);
    assert_eq!(count(&s, "events"), 0);
    assert_eq!(count(&s, "preferences"), 1);
    assert!(s.push(&old).is_err());
    push(&s, vec![observe("new-generation")]);
    assert_eq!(count(&s, "events"), 1);
}
#[test]
fn bounded_stream_pagination_preserves_later_changes() {
    let (_d, s) = store();
    push(
        &s,
        vec![
            observe("first"),
            Action::Preference {
                field: Preference::Theme,
                base_revision: 0,
                value: "dark".into(),
            },
        ],
    );
    let page = s.page(Stream::Memory, 0, None, 1).unwrap();
    assert!(page.has_more);
    let until = page.until;
    push(&s, vec![observe("later")]);
    let second = s
        .page(Stream::Memory, page.next_cursor, Some(until), 1)
        .unwrap();
    assert!(!second.has_more);
    assert_eq!(second.next_cursor, until);
    let later = s.page(Stream::Memory, until, None, 500).unwrap();
    assert_eq!(later.changes.len(), 1);
    let prefs = s.page(Stream::Preferences, 0, None, 500).unwrap();
    assert_eq!(prefs.changes.len(), 1);
    assert!(serde_json::to_vec(&prefs).unwrap().len() < MAX_BYTES);
    assert!(s.page(Stream::Memory, -1, None, 1).is_err());
    assert!(s.page(Stream::Memory, 0, None, 501).is_err());
}
#[test]
fn quota_rolls_back_additions_but_allows_delete() {
    let (_d, mut s) = store();
    push(&s, vec![observe("one")]);
    let inc = incarnation(&s);
    s.limits.storage = 1;
    let before = s.context().unwrap().revision;
    assert_eq!(
        s.push(&request(&s, vec![observe("more")]))
            .unwrap_err()
            .body
            .code,
        "storage_quota"
    );
    assert_eq!(s.context().unwrap().revision, before);
    push(
        &s,
        vec![Action::Delete {
            key: term().key(),
            incarnation: inc,
        }],
    );
    assert_eq!(count(&s, "events"), 0);
}
#[test]
fn token_digests_rotation_and_backup_restore_keep_data() {
    let (d, s) = store();
    let token = "fixture-token-unique";
    let login = s.enroll(hash(), "desktop", token).unwrap();
    assert_eq!(s.authenticate(token).unwrap(), login.device_id);
    let stored: String =
        s.db.lock()
            .unwrap()
            .query_row("SELECT token_digest FROM devices", [], |r| r.get(0))
            .unwrap();
    assert_ne!(stored, token);
    push(&s, vec![observe("one")]);
    let ctx = s.context().unwrap();
    let backup = d.path().join("backup.sqlite3");
    s.backup(&backup).unwrap();
    let restored = Store::open(&backup, Limits::default()).unwrap();
    restored.restored().unwrap();
    assert_eq!(restored.context().unwrap().workspace_id, ctx.workspace_id);
    assert_ne!(restored.context().unwrap().history_id, ctx.history_id);
    assert!(restored.authenticate(token).is_err());
    assert_eq!(count(&restored, "events"), 1);
    s.set_password(hash(), false).unwrap();
    assert!(s.authenticate(token).is_err());
    assert_eq!(count(&s, "events"), 1);
}
async fn call(
    app: axum::Router,
    method: &str,
    path: &str,
    body: serde_json::Value,
    token: Option<&str>,
) -> (StatusCode, String) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    let mut req = req
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    req.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
    ));
    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let data = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(data.to_vec()).unwrap())
}
#[tokio::test]
async fn http_auth_checks_do_not_enroll_or_leak_and_revoke_works() {
    let (_d, s) = store();
    let s = Arc::new(s);
    let app = router(s.clone());
    let login = serde_json::json!({"protocol":1,"password":PASSWORD,"deviceName":"test"});
    let (status, _) = call(app.clone(), "POST", "/v1/auth/check", login.clone(), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(count(&s, "devices"), 0);
    let (status, body) = call(app.clone(), "POST", "/v1/devices/enroll", login, None).await;
    assert_eq!(status, StatusCode::OK);
    let enrolled: Enrollment = serde_json::from_str(&body).unwrap();
    let (status, _) = call(
        app.clone(),
        "GET",
        "/v1/devices",
        serde_json::Value::Null,
        Some(&enrolled.token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(
        app.clone(),
        "DELETE",
        &format!("/v1/devices/{}", enrolled.device_id),
        serde_json::Value::Null,
        Some(&enrolled.token),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body) = call(
        app.clone(),
        "GET",
        "/v1/devices",
        serde_json::Value::Null,
        Some(&enrolled.token),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!body.contains(&enrolled.token));
    let(status,body)=call(app,"POST","/v1/auth/check",serde_json::json!({"protocol":1,"password":PASSWORD,"deviceName":"d","apiKey":"secret-marker"}),None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body.contains("secret-marker"));
}
#[tokio::test]
async fn http_actual_body_limits_and_rate_limits_fail_closed() {
    let (_d, mut s) = store();
    s.limits.logins = 1;
    let app = router(Arc::new(s));
    let data = serde_json::json!({"protocol":1,"password":PASSWORD,"deviceName":"d"});
    assert_eq!(
        call(app.clone(), "POST", "/v1/auth/check", data.clone(), None)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(app.clone(), "POST", "/v1/auth/check", data, None)
            .await
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    let oversized = serde_json::json!({"password":"x".repeat(MAX_BYTES)});
    assert_eq!(
        call(app, "POST", "/v1/auth/check", oversized, None).await.0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

#[tokio::test]
async fn device_and_business_limits_reject_without_data_changes() {
    let (_d, mut s) = store();
    s.limits.devices = 1;
    s.limits.requests = 1;
    let enrolled = s.enroll(hash(), "one", "fixture-token").unwrap();
    assert_eq!(
        s.enroll(hash(), "two", "other-token")
            .err()
            .unwrap()
            .body
            .code,
        "device_limit"
    );
    let s = Arc::new(s);
    let app = router(s.clone());
    let (status, body) = call(
        app.clone(),
        "POST",
        "/v1/auth/check",
        serde_json::json!({"protocol":1,"password":"wrong-password-long","deviceName":"bad"}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!body.contains("wrong-password"));
    assert_eq!(
        call(
            app.clone(),
            "GET",
            "/v1/devices",
            serde_json::Value::Null,
            Some(&enrolled.token)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            app,
            "POST",
            "/v1/push",
            serde_json::to_value(request(&s, vec![observe("blocked")])).unwrap(),
            Some(&enrolled.token)
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(count(&s, "events"), 0);
}

#[tokio::test]
async fn concurrent_password_work_is_limited_before_argon2() {
    let (_d, mut s) = store();
    s.limits.logins = 100;
    let app = router(Arc::new(s));
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..5 {
        let app = app.clone();
        tasks.spawn(async move {
            call(
                app,
                "POST",
                "/v1/auth/check",
                serde_json::json!({"protocol":1,"password":PASSWORD,"deviceName":"d"}),
                None,
            )
            .await
            .0
        });
    }
    let mut accepted = 0;
    let mut limited = 0;
    while let Some(status) = tasks.join_next().await {
        match status.unwrap() {
            StatusCode::OK => accepted += 1,
            StatusCode::TOO_MANY_REQUESTS => limited += 1,
            _ => panic!("unexpected authentication status"),
        }
    }
    assert_eq!(accepted, 2);
    assert_eq!(limited, 3);
    assert!(hash().contains("$argon2id$v=19$m=65536,t=3,p=1$"));
}
