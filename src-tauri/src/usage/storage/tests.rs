use super::*;
fn stamp(utc: i64, date: &str, offset: i32) -> ClockStamp {
    ClockStamp {
        utc,
        date: date.into(),
        offset,
    }
}
fn attempt(id: &str, stamp: ClockStamp) -> Attempt {
    Attempt {
        id: id.into(),
        operation: "operation".into(),
        generation: 1,
        service: "service".into(),
        label: "example.com".into(),
        model: "model".into(),
        purpose: "translation".into(),
        stamp,
    }
}
fn counts() -> Counts {
    Counts {
        input: Some(100),
        output: Some(30),
        total: Some(130),
        ..Counts::default()
    }
}

#[test]
fn persistence_idempotence_recovery_retention_and_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("usage.sqlite3");
    let mut db = Ledger::open(&path).unwrap();
    let a = attempt("first", stamp(1, "2026-09-30", 28800));
    db.begin(&a).unwrap();
    db.finish(&a.id, 1, &counts()).unwrap();
    db.finish(&a.id, 1, &counts()).unwrap();
    db.begin(&attempt("pending", stamp(2, "2026-10-01", 28800)))
        .unwrap();
    db.prune(9 * 86400).unwrap();
    assert_eq!(
        db.db
            .query_row("SELECT count(*) FROM attempts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(db);
    let mut db = Ledger::open(&path).unwrap();
    db.recover(9 * 86400).unwrap();
    db.recover(9 * 86400).unwrap();
    let v = db
        .query(
            &Query {
                limit: 20,
                ..Query::default()
            },
            &stamp(3, "2026-10-01", 28800),
        )
        .unwrap();
    assert_eq!(v.cumulative.attempts, 2);
    assert_eq!(v.cumulative.total.tokens, Some(130));
    assert_eq!(v.cumulative.total.unknown, 1);
    assert_eq!(v.today.attempts, 1);
    assert_eq!(v.month.attempts, 1);
    // Late finalization of a pruned receipt cannot add to a retained daily total.
    db.finish("first", 1, &counts()).unwrap();
    assert_eq!(
        db.query(&Query::default(), &stamp(3, "2026-10-01", 0))
            .unwrap()
            .cumulative
            .attempts,
        2
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn captured_dates_filters_paging_and_generation_reset() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Ledger::open(&dir.path().join("usage.db")).unwrap();
    for (id, date, offset, purpose) in [
        ("a", "2026-10-31", 3600, "translation"),
        ("b", "2026-11-01", 0, "manual-analysis"),
        ("c", "2026-11-01", -18000, "auto-analysis"),
    ] {
        let mut a = attempt(id, stamp(100, date, offset));
        a.purpose = purpose.into();
        db.begin(&a).unwrap();
        db.finish(id, 1, &counts()).unwrap();
    }
    let now = stamp(200, "2026-11-01", 0);
    let view = db
        .query(
            &Query {
                limit: 1,
                offset: 1,
                ..Query::default()
            },
            &now,
        )
        .unwrap();
    assert_eq!(view.cumulative.total.tokens, Some(390));
    assert_eq!(view.today.total.tokens, Some(260));
    assert_eq!(view.month.total.tokens, Some(260));
    assert_eq!(view.group_count, 3);
    assert_eq!(view.groups.len(), 1);
    let filtered = db
        .query(
            &Query {
                purpose: Some("translation".into()),
                ..Query::default()
            },
            &now,
        )
        .unwrap();
    assert_eq!(filtered.cumulative.total.tokens, Some(130));
    let before = db
        .query(&Query::default(), &stamp(0, "2026-10-31", 28800))
        .unwrap();
    assert_eq!(before.today.total.tokens, Some(130));
    assert_eq!(before.cumulative.total.tokens, Some(390));
    let old = attempt("late", now.clone());
    db.begin(&old).unwrap();
    let gen = db.clear(201).unwrap();
    assert_eq!(gen, 2);
    db.finish("late", 1, &counts()).unwrap();
    db.begin(&old).unwrap();
    let mut new = attempt("new", now.clone());
    new.generation = 2;
    db.begin(&new).unwrap();
    db.finish("new", 2, &counts()).unwrap();
    let after = db.query(&Query::default(), &now).unwrap();
    assert_eq!(after.cumulative.total.tokens, Some(130));
    assert_eq!(after.cumulative.attempts, 1);
    assert_eq!(after.since, 201);
}

#[test]
fn future_schema_corruption_write_failure_and_privacy() {
    let dir = tempfile::tempdir().unwrap();
    let future = dir.path().join("future.db");
    let raw = Connection::open(&future).unwrap();
    raw.pragma_update(None, "user_version", 99).unwrap();
    drop(raw);
    assert!(Ledger::open(&future).is_err());
    let raw = Connection::open(&future).unwrap();
    assert_eq!(
        raw.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        99
    );
    drop(raw);
    let corrupt = dir.path().join("corrupt.db");
    std::fs::write(&corrupt, b"do-not-erase-corrupt").unwrap();
    assert!(Ledger::open(&corrupt).is_err());
    assert_eq!(std::fs::read(&corrupt).unwrap(), b"do-not-erase-corrupt");
    let path = dir.path().join("normal.db");
    let mut db = Ledger::open(&path).unwrap();
    let a = attempt("safe-id", stamp(100, "2026-10-17", 0));
    db.begin(&a).unwrap();
    db.finish(&a.id, 1, &counts()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    for marker in [
        "fixture-key",
        "private-source-marker",
        "messages",
        "choices",
        "apiKey",
        "translationText",
    ] {
        assert!(!text.contains(marker));
    }
    db.db.pragma_update(None, "query_only", true).unwrap();
    assert!(db.begin(&attempt("blocked", a.stamp)).is_err());
    assert_eq!(
        db.query(&Query::default(), &stamp(100, "2026-10-17", 0))
            .unwrap()
            .cumulative
            .total
            .tokens,
        Some(130)
    );
}
