use super::*;
use crate::memory::{Memory, Observation, Profile, Topic};
fn database() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE metadata(key TEXT PRIMARY KEY,value INTEGER NOT NULL); CREATE TABLE terms(id INTEGER PRIMARY KEY,original TEXT,translation TEXT,domain TEXT,sense TEXT);").unwrap();
    init(&db).unwrap();
    db
}
fn add(db: &Connection, name: &str, translated: &str, sense: &str) -> i64 {
    db.execute(
        "INSERT INTO terms(original,translation,domain,sense) VALUES(?1,?2,'Agent',?3)",
        [name, translated, sense],
    )
    .unwrap();
    db.last_insert_rowid()
}
#[test]
fn keyword_searches_all_fields_literals_and_stable_pages() {
    let db = database();
    let agent = add(&db, "Agent", "智能体", "自主执行任务的软件系统");
    add(&db, "Other", "其他", "普通内容");
    let literal = add(&db, "100%_\"quoted", "特殊符号", "literal");
    for query in ["Agent 自主", "agent 软件", "智能", "智", "智能体"] {
        assert_eq!(
            page(&db, query, "keyword", 0).unwrap().0,
            vec![agent],
            "{query}"
        );
    }
    assert_eq!(page(&db, "%_\"", "keyword", 0).unwrap().0, vec![literal]);
    assert!(page(&db, "' OR 1=1 --", "keyword", 0).unwrap().0.is_empty());
    assert_eq!(page(&db, " ", "keyword", 0).unwrap().1, 3);
    assert!(page(&db, &"中".repeat(201), "keyword", 0).is_err());
    assert!(page(&db, &"中".repeat(200), "keyword", 0).is_ok());
    assert!(page(&db, "a b c d e f g h i", "keyword", 0).is_err());
    assert!(page(&db, "missing", "invalid", 0).is_err());
    for i in 0..35 {
        add(&db, &format!("Agent-{i}"), "智能体", "自主");
    }
    let first = page(&db, "Agent", "keyword", 0).unwrap();
    let second = page(&db, "Agent", "keyword", 30).unwrap();
    assert_eq!(first.1, 38);
    assert_eq!(first.0.len(), 30);
    assert_eq!(second.0.len(), 8);
    assert!(first.0.iter().all(|id| !second.0.contains(id)));
    assert_eq!(page(&db, "Agent", "keyword", 0).unwrap().0, first.0);
}
#[test]
fn fuzzy_spelling_exact_priority_dedup_and_punctuation() {
    let db = database();
    let garbage = add(&db, "Garbage Collection", "垃圾回收", "释放不可达对象内存");
    assert_eq!(page(&db, "garbge", "fuzzy", 0).unwrap().0, vec![garbage]);
    let exact = add(&db, "garbge", "测试", "精确测试词");
    let (ids, total) = page(&db, "garbge", "fuzzy", 0).unwrap();
    assert_eq!(total, 2);
    assert_eq!(ids, vec![exact, garbage]);
    assert!(page(&db, "zzzzzzzzzzz", "fuzzy", 0).unwrap().0.is_empty());
    let special = add(&db, "100%_\"quoted", "特殊", "特殊符号");
    assert!(page(&db, "%_\"", "fuzzy", 0).unwrap().0.contains(&special));
    assert!(page(&db, "中 OR \"*", "fuzzy", 0).is_ok());
    assert!(near("garbge", "garbage"));
    assert!(!near("gc", "gg"));
}
#[test]
fn index_tracks_update_delete_clear_and_rebuild() {
    let db = database();
    let id = add(&db, "Agent", "智能体", "自主系统");
    db.execute("UPDATE terms SET sense='独立规划执行' WHERE id=?", [id])
        .unwrap();
    assert_eq!(page(&db, "规划执行", "fuzzy", 0).unwrap().0, vec![id]);
    db.execute("DELETE FROM terms WHERE id=?", [id]).unwrap();
    assert!(page(&db, "规划执行", "fuzzy", 0).unwrap().0.is_empty());
    add(&db, "Garbage Collection", "垃圾回收", "回收内存");
    db.execute("DELETE FROM terms", []).unwrap();
    assert_eq!(page(&db, "garbage", "fuzzy", 0).unwrap().1, 0);
    db.execute(
        "INSERT INTO terms_fts(terms_fts,rank) VALUES('integrity-check',1)",
        [],
    )
    .unwrap();
    let fields = db
        .prepare("SELECT name FROM pragma_table_info('terms_fts')")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields, vec!["original", "translation", "domain", "sense"]);
}
#[test]
fn fuzzy_candidates_are_bounded_while_keyword_total_covers_the_library() {
    let db = database();
    for i in 0..1700 {
        add(&db, &format!("Agent{i}"), "智能体", "自主系统");
    }
    let fuzzy = page(&db, "Agent", "fuzzy", 0).unwrap();
    assert!(fuzzy.1 <= 1500);
    assert_eq!(fuzzy.0.len(), 30);
    assert_eq!(page(&db, "Agent", "keyword", 0).unwrap().1, 1700);
}
#[tokio::test]
async fn legacy_memory_migrates_without_changing_profile_feedback_or_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("memory.sqlite3");
    let db = Memory::open(path.clone()).unwrap();
    let epoch = db.epoch().await.unwrap();
    let id = db
        .record(
            epoch,
            "one".into(),
            "manual".into(),
            vec![Observation {
                original: "Agent".into(),
                translation: "智能体".into(),
                domain: "Agent".into(),
                sense_summary: "自主规划系统".into(),
                language: "English".into(),
            }],
        )
        .await
        .unwrap()[0];
    db.feedback(id, "known".into()).await.unwrap();
    db.finish(
        db.snapshot(false).await.unwrap(),
        Profile {
            topics: vec![Topic {
                domain: "Agent".into(),
                evidence_ids: vec![id],
            }],
        },
    )
    .await
    .unwrap();
    let before = serde_json::to_value(db.view("".into(), 0).await.unwrap()).unwrap();
    db.call(|db| {db.execute_batch("DROP TRIGGER terms_search_insert;DROP TRIGGER terms_search_update;DROP TRIGGER terms_search_delete;DROP TABLE terms_fts;DELETE FROM metadata WHERE key='search_index_version';").map_err(error)?;Ok(())}).await.unwrap();
    drop(db);
    let reopened = Memory::open(path.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(reopened.view("".into(), 0).await.unwrap()).unwrap(),
        before
    );
    assert_eq!(
        reopened
            .view_with_mode("规划".into(), 0, "fuzzy".into())
            .await
            .unwrap()
            .terms[0]
            .id,
        id
    );
    reopened.delete(None).await.unwrap();
    drop(reopened);
    let cleared = Memory::open(path).unwrap();
    assert_eq!(
        cleared
            .view_with_mode("Agent".into(), 0, "fuzzy".into())
            .await
            .unwrap()
            .total,
        0
    );
}
