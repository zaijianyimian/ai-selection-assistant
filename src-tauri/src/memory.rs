//! Only minimal term data crosses this boundary; never source passages or full results.
use crate::error::{AppError, AppResult};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::mpsc,
    time::{SystemTime, UNIX_EPOCH},
};

fn db_error(_: impl std::fmt::Debug) -> AppError {
    AppError::new(
        "memory_storage",
        "术语数据库不可用，请检查数据目录权限和磁盘空间；翻译仍可使用。",
    )
}
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
type Work = Box<dyn FnOnce(&mut Connection) + Send>;
#[derive(Clone)]
pub struct Memory {
    sender: mpsc::Sender<Work>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredTerm {
    pub id: i64,
    pub original: String,
    pub translation: String,
    pub domain: String,
    pub sense_summary: String,
    pub language: String,
    pub count: i64,
    pub last_seen: i64,
    pub familiarity: String,
}
#[derive(Debug, Clone)]
pub struct Observation {
    pub original: String,
    pub translation: String,
    pub domain: String,
    pub sense_summary: String,
    pub language: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Topic {
    pub domain: String,
    pub evidence_ids: Vec<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub topics: Vec<Topic>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: i64,
    pub created_at: i64,
    pub profile: Profile,
}
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub epoch: i64,
    pub cursor: i64,
    pub terms: Vec<StoredTerm>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryView {
    pub terms: Vec<StoredTerm>,
    pub total: i64,
    pub pending: i64,
    pub versions: Vec<Version>,
    pub active_version: i64,
    pub last_auto_attempt: i64,
}

fn meta(db: &Connection, key: &str) -> AppResult<i64> {
    db.query_row("SELECT value FROM metadata WHERE key=?", [key], |r| {
        r.get(0)
    })
    .map_err(db_error)
}
fn set_meta(db: &Connection, key: &str, value: i64) -> AppResult<()> {
    db.execute(
        "UPDATE metadata SET value=? WHERE key=?",
        params![value, key],
    )
    .map_err(db_error)?;
    Ok(())
}
fn invalidate(db: &Connection) -> AppResult<()> {
    set_meta(db, "epoch", meta(db, "epoch")? + 1)?;
    db.execute("DELETE FROM profiles", []).map_err(db_error)?;
    set_meta(db, "active", 0)?;
    set_meta(db, "cursor", 0)
}
fn check_epoch(db: &Connection, epoch: i64) -> AppResult<()> {
    if meta(db, "epoch")? != epoch {
        return Err(AppError::new(
            "memory_stale",
            "记忆设置或数据已变化，本次旧任务未写入。",
        ));
    }
    Ok(())
}
fn init(db: &Connection) -> AppResult<()> {
    let version: i64 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(db_error)?;
    if version > 1 {
        return Err(AppError::new(
            "memory_version",
            "术语数据库版本较新，请使用新版应用打开。",
        ));
    }
    db.busy_timeout(std::time::Duration::from_secs(2))
        .map_err(db_error)?;
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA secure_delete=ON;
      CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value INTEGER NOT NULL);
      INSERT OR IGNORE INTO metadata VALUES ('epoch',1),('cursor',0),('active',0),('last_auto',0);
      CREATE TABLE IF NOT EXISTS terms(
        id INTEGER PRIMARY KEY AUTOINCREMENT, normalized TEXT NOT NULL, original TEXT NOT NULL,
        translation TEXT NOT NULL, domain TEXT NOT NULL, sense TEXT NOT NULL, language TEXT NOT NULL,
        UNIQUE(normalized,domain,sense,language));
      CREATE TABLE IF NOT EXISTS events(
        id INTEGER PRIMARY KEY AUTOINCREMENT, job TEXT NOT NULL, term_id INTEGER NOT NULL REFERENCES terms(id) ON DELETE CASCADE,
        at INTEGER NOT NULL, source TEXT NOT NULL, kind TEXT NOT NULL, value TEXT NOT NULL DEFAULT '',
        UNIQUE(job,term_id,kind));
      CREATE INDEX IF NOT EXISTS events_term ON events(term_id,id);
      CREATE TABLE IF NOT EXISTS profiles(id INTEGER PRIMARY KEY AUTOINCREMENT,at INTEGER NOT NULL,content TEXT NOT NULL);
      PRAGMA user_version=1;").map_err(db_error)?;
    crate::search::init(db)
}
fn read_term(db: &Connection, id: i64, cursor: i64) -> AppResult<StoredTerm> {
    db.query_row("SELECT t.id,t.original,t.translation,t.domain,t.sense,t.language,
      (SELECT COUNT(*) FROM events WHERE term_id=t.id AND kind='translation' AND id<=?2),
      COALESCE((SELECT MAX(at) FROM events WHERE term_id=t.id AND kind='translation' AND id<=?2),0),
      COALESCE((SELECT value FROM events WHERE term_id=t.id AND kind='feedback' AND id<=?2 ORDER BY id DESC LIMIT 1),'unknown')
      FROM terms t WHERE t.id=?1", params![id,cursor], |r| Ok(StoredTerm {
        id:r.get(0)?,original:r.get(1)?,translation:r.get(2)?,domain:r.get(3)?,sense_summary:r.get(4)?,
        language:r.get(5)?,count:r.get(6)?,last_seen:r.get(7)?,familiarity:r.get(8)?
      })).map_err(db_error)
}
impl Memory {
    pub fn open(path: PathBuf) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(db_error)?;
        }
        let mut db = Connection::open(path).map_err(db_error)?;
        init(&db)?;
        let (sender, receiver) = mpsc::channel::<Work>();
        std::thread::Builder::new()
            .name("term-memory".into())
            .spawn(move || {
                for work in receiver {
                    work(&mut db);
                }
            })
            .map_err(db_error)?;
        Ok(Self { sender })
    }
    pub async fn call<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Connection) -> AppResult<T> + Send + 'static,
    ) -> AppResult<T> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.sender
            .send(Box::new(move |db| {
                let _ = sender.send(work(db));
            }))
            .map_err(db_error)?;
        receiver.await.map_err(db_error)?
    }
    pub async fn epoch(&self) -> AppResult<i64> {
        self.call(|db| meta(db, "epoch")).await
    }
    pub async fn commit_settings(
        &self,
        publish: impl FnOnce() -> AppResult<()> + Send + 'static,
    ) -> AppResult<()> {
        self.call(move |db| {
            let tx = db.transaction().map_err(db_error)?;
            invalidate(&tx)?;
            publish()?;
            tx.commit().map_err(db_error)
        })
        .await
    }
    #[cfg(test)]
    pub async fn invalidate(&self) -> AppResult<()> {
        self.call(|db| {
            let tx = db.transaction().map_err(db_error)?;
            invalidate(&tx)?;
            tx.commit().map_err(db_error)
        })
        .await
    }
    pub async fn record(
        &self,
        epoch: i64,
        job: String,
        source: String,
        terms: Vec<Observation>,
    ) -> AppResult<Vec<i64>> {
        self.call(move |db| {
            let tx=db.transaction().map_err(db_error)?; check_epoch(&tx,epoch)?;
            let mut ids=vec![];
            for t in terms {
                let normalized=t.original.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
                tx.execute("INSERT OR IGNORE INTO terms(normalized,original,translation,domain,sense,language) VALUES(?,?,?,?,?,?)",
                    params![normalized,t.original.trim(),t.translation.trim(),t.domain,t.sense_summary.trim(),t.language]).map_err(db_error)?;
                let id:i64=tx.query_row("SELECT id FROM terms WHERE normalized=? AND domain=? AND sense=? AND language=?",
                    params![normalized,t.domain,t.sense_summary.trim(),t.language],|r|r.get(0)).map_err(db_error)?;
                tx.execute("INSERT OR IGNORE INTO events(job,term_id,at,source,kind) VALUES(?,?,?,?,'translation')",params![job,id,now(),source]).map_err(db_error)?;
                ids.push(id);
            }
            tx.commit().map_err(db_error)?; Ok(ids)
        }).await
    }
    pub async fn view(&self, query: String, offset: u32) -> AppResult<MemoryView> {
        self.view_with_mode(query, offset, "keyword".into()).await
    }
    pub async fn view_with_mode(
        &self,
        query: String,
        offset: u32,
        mode: String,
    ) -> AppResult<MemoryView> {
        self.call(move |db| {
            let (ids, total) = crate::search::page(db, &query, &mode, offset)?;
            let terms = ids
                .into_iter()
                .map(|id| read_term(db, id, i64::MAX))
                .collect::<AppResult<_>>()?;
            let versions = db
                .prepare("SELECT id,at,content FROM profiles ORDER BY id DESC LIMIT 10")
                .map_err(db_error)?
                .query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })
                .map_err(db_error)?
                .map(|row| {
                    let (id, created_at, content) = row.map_err(db_error)?;
                    Ok(Version {
                        id,
                        created_at,
                        profile: serde_json::from_str(&content).map_err(db_error)?,
                    })
                })
                .collect::<AppResult<_>>()?;
            let pending = db
                .query_row(
                    "SELECT COUNT(*) FROM events WHERE id>?",
                    [meta(db, "cursor")?],
                    |r| r.get(0),
                )
                .map_err(db_error)?;
            Ok(MemoryView {
                terms,
                total,
                pending,
                versions,
                active_version: meta(db, "active")?,
                last_auto_attempt: meta(db, "last_auto")?,
            })
        })
        .await
    }
    pub async fn feedback(&self, id: i64, value: String) -> AppResult<()> {
        if !["unknown", "learning", "known"].contains(&value.as_str()) {
            return Err(AppError::new("feedback", "熟悉程度无效。"));
        }
        self.call(move|db| {
            db.execute("INSERT INTO events(job,term_id,at,source,kind,value) VALUES(?,?,?,'manual','feedback',?)",params![uuid::Uuid::new_v4().to_string(),id,now(),value]).map_err(db_error)?;Ok(())
        }).await
    }
    pub async fn delete(&self, id: Option<i64>) -> AppResult<()> {
        self.call(move |db| {
            let tx = db.transaction().map_err(db_error)?;
            invalidate(&tx)?;
            match id {
                Some(id) => {
                    tx.execute("DELETE FROM terms WHERE id=?", [id])
                        .map_err(db_error)?;
                }
                None => {
                    tx.execute("DELETE FROM terms", []).map_err(db_error)?;
                }
            }
            tx.commit().map_err(db_error)
        })
        .await
    }
    pub async fn snapshot(&self, automatic: bool) -> AppResult<Snapshot> {
        self.call(move |db| {
            let tx = db.transaction().map_err(db_error)?;
            let cursor = meta(&tx, "cursor")?;
            let count: i64 = tx
                .query_row("SELECT COUNT(*) FROM events WHERE id>?", [cursor], |r| {
                    r.get(0)
                })
                .map_err(db_error)?;
            if automatic && !auto_due(now(), meta(&tx, "last_auto")?, count) {
                return Err(AppError::new("analysis_not_due", "未达到自动分析条件。"));
            }
            if count == 0 {
                return Err(AppError::new("memory_empty", "暂无待分析的新术语或反馈。"));
            }
            let rows: Vec<(i64, i64)> = tx
                .prepare("SELECT id,term_id FROM events WHERE id>? ORDER BY id LIMIT 500")
                .map_err(db_error)?
                .query_map([cursor], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(db_error)?
                .collect::<Result<_, _>>()
                .map_err(db_error)?;
            let mut ids = HashSet::new();
            let mut end = cursor;
            for (event, id) in rows {
                if !ids.contains(&id) && ids.len() == 100 {
                    break;
                }
                ids.insert(id);
                end = event;
            }
            let mut terms = ids
                .into_iter()
                .map(|id| read_term(&tx, id, end))
                .collect::<AppResult<Vec<_>>>()?;
            terms.sort_by_key(|t| t.id);
            let snapshot = Snapshot {
                epoch: meta(&tx, "epoch")?,
                cursor: end,
                terms,
            };
            // Persist attempts before network access; crashes/restarts cannot bypass the daily interval.
            if automatic {
                set_meta(&tx, "last_auto", now())?;
            }
            tx.commit().map_err(db_error)?;
            Ok(snapshot)
        })
        .await
    }
    pub async fn finish(&self, snapshot: Snapshot, profile: Profile) -> AppResult<()> {
        validate_profile(&profile, &snapshot)?;
        self.call(move|db| {
            let tx=db.transaction().map_err(db_error)?;check_epoch(&tx,snapshot.epoch)?;
            tx.execute("INSERT INTO profiles(at,content) VALUES(?,?)",params![now(),serde_json::to_string(&profile).map_err(db_error)?]).map_err(db_error)?;
            set_meta(&tx,"active",tx.last_insert_rowid())?;set_meta(&tx,"cursor",snapshot.cursor)?;
            tx.execute("DELETE FROM profiles WHERE id NOT IN (SELECT id FROM profiles ORDER BY id DESC LIMIT 10)",[]).map_err(db_error)?;
            tx.commit().map_err(db_error)
        }).await
    }
    pub async fn select_version(&self, id: i64) -> AppResult<()> {
        self.call(move |db| {
            let exists: bool = db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM profiles WHERE id=?)",
                    [id],
                    |r| r.get(0),
                )
                .map_err(db_error)?;
            if !exists {
                return Err(AppError::new("profile_version", "该画像版本已失效。"));
            }
            set_meta(db, "active", id)
        })
        .await
    }
    pub async fn context(&self, text: String) -> AppResult<String> {
        self.call(move|db| {
            let mut context=String::new();let mut matched=0;
            let mut stmt=db.prepare("SELECT id,original FROM terms ORDER BY id DESC").map_err(db_error)?;
            let rows=stmt.query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?))).map_err(db_error)?;
            for row in rows {
                let(id,term)=row.map_err(db_error)?;if !matches_term(&text,&term) {continue;}
                let t=read_term(db,id,i64::MAX)?;
                let line=serde_json::to_string(&serde_json::json!({"term":t.original,"domain":t.domain,"meaning":t.sense_summary,"familiarity":t.familiarity})).map_err(db_error)?;
                if context.chars().count()+line.chars().count()+1>1700 {continue;}context.push_str(&line);context.push('\n');matched+=1;if matched==8{break;}
            }
            let profile:Option<String>=db.query_row("SELECT content FROM profiles WHERE id=?",[meta(db,"active")?],|r|r.get(0)).optional().map_err(db_error)?;
            if let Some(p)=profile {let p:Profile=serde_json::from_str(&p).map_err(db_error)?;let domains:Vec<_>=p.topics.iter().map(|t|t.domain.as_str()).collect();context.push_str(&format!("Recent topic interests (not mastery): {}",domains.join(", ")));}
            Ok(context.chars().take(2000).collect())
        }).await
    }
}
pub fn auto_due(at: i64, last: i64, count: i64) -> bool {
    count >= 20 && (last == 0 || at.saturating_sub(last) >= 86400)
}
pub const DOMAINS: [&str; 8] = [
    "Java",
    "JVM",
    "数据库",
    "分布式系统",
    "AI / LLM",
    "Agent",
    "机器学习",
    "软件工程",
];
pub fn validate_profile(p: &Profile, s: &Snapshot) -> AppResult<()> {
    let ids: HashSet<_> = s.terms.iter().map(|t| t.id).collect();
    let mut domains = HashSet::new();
    if p.topics.len() > 8
        || p.topics.iter().any(|t| {
            !DOMAINS.contains(&t.domain.as_str())
                || !domains.insert(&t.domain)
                || t.evidence_ids.is_empty()
                || t.evidence_ids.len() > 12
                || t.evidence_ids.iter().any(|id| {
                    !ids.contains(id)
                        || !s
                            .terms
                            .iter()
                            .any(|term| term.id == *id && term.domain == t.domain)
                })
        })
    {
        return Err(AppError::new(
            "invalid_analysis",
            "模型分析格式或证据无效；已保留原有画像。",
        ));
    }
    Ok(())
}
fn matches_term(text: &str, term: &str) -> bool {
    let text = text.to_lowercase();
    let term = term.to_lowercase();
    !term.is_empty()
        && text.match_indices(&term).any(|(start, _)| {
            if !term.is_ascii() {
                return true;
            }
            let end = start + term.len();
            !text[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
                && !text[end..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observation(sense: &str) -> Observation {
        Observation {
            original: "Agent".into(),
            translation: "智能体".into(),
            domain: "Agent".into(),
            sense_summary: sense.into(),
            language: "English".into(),
        }
    }
    #[tokio::test]
    async fn persistence_idempotence_senses_feedback_and_epoch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.sqlite3");
        let db = Memory::open(path.clone()).unwrap();
        let epoch = db.epoch().await.unwrap();
        let record = |job: &str| {
            db.record(
                epoch,
                job.into(),
                "manual".into(),
                vec![observation("自主执行任务的软件系统")],
            )
        };
        let id = record("one").await.unwrap()[0];
        record("one").await.unwrap();
        record("two").await.unwrap();
        db.record(
            epoch,
            "three".into(),
            "manual".into(),
            vec![observation("代表其他程序通信的组件")],
        )
        .await
        .unwrap();
        let view = db.view("".into(), 0).await.unwrap();
        assert_eq!(view.total, 2);
        assert_eq!(view.terms.iter().find(|t| t.id == id).unwrap().count, 2);
        db.feedback(id, "known".into()).await.unwrap();
        let snapshot = db.snapshot(false).await.unwrap();
        let p = Profile {
            topics: vec![Topic {
                domain: "Agent".into(),
                evidence_ids: vec![id],
            }],
        };
        db.finish(snapshot.clone(), p.clone()).await.unwrap();
        let reopened = Memory::open(path).unwrap();
        assert_eq!(reopened.view("Agent".into(), 0).await.unwrap().total, 2);
        assert!(db
            .context("reagent".into())
            .await
            .unwrap()
            .starts_with("Recent"));
        assert!(db
            .context("An Agent works".into())
            .await
            .unwrap()
            .contains("known"));
        db.delete(Some(id)).await.unwrap();
        assert!(db.finish(snapshot, p).await.is_err());
        assert!(record("late").await.is_err());
        assert!(db.view("".into(), 0).await.unwrap().versions.is_empty());
        db.delete(None).await.unwrap();
        assert_eq!(db.view("".into(), 0).await.unwrap().total, 0);
    }
    #[tokio::test]
    async fn analysis_cursor_only_covers_snapshot_and_invalid_profile_preserves_old() {
        let dir = tempfile::tempdir().unwrap();
        let db = Memory::open(dir.path().join("test.db")).unwrap();
        let epoch = db.epoch().await.unwrap();
        let id = db
            .record(
                epoch,
                "one".into(),
                "clipboard".into(),
                vec![observation("自主系统")],
            )
            .await
            .unwrap()[0];
        let s = db.snapshot(false).await.unwrap();
        db.record(
            epoch,
            "late".into(),
            "manual".into(),
            vec![observation("自主系统")],
        )
        .await
        .unwrap();
        let p = Profile {
            topics: vec![Topic {
                domain: "Agent".into(),
                evidence_ids: vec![id],
            }],
        };
        db.finish(s.clone(), p).await.unwrap();
        assert_eq!(db.view("".into(), 0).await.unwrap().pending, 1);
        assert!(db
            .finish(
                s,
                Profile {
                    topics: vec![Topic {
                        domain: "Agent".into(),
                        evidence_ids: vec![999]
                    }]
                }
            )
            .await
            .is_err());
        assert_eq!(db.view("".into(), 0).await.unwrap().versions.len(), 1);
        let cols = db
            .call(|db| {
                let names = db
                    .prepare("SELECT name FROM pragma_table_info('terms')")
                    .map_err(db_error)?
                    .query_map([], |r| r.get::<_, String>(0))
                    .map_err(db_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(db_error)?;
                Ok(names)
            })
            .await
            .unwrap();
        assert!(!cols
            .iter()
            .any(|n| ["text", "explanation", "example", "api_key"].contains(&n.as_str())));
    }
    #[test]
    fn scheduling_and_word_boundaries() {
        assert!(!auto_due(100, 0, 19));
        assert!(auto_due(100, 0, 20));
        assert!(!auto_due(200, 100, 20));
        assert!(auto_due(86500, 100, 20));
        assert!(!auto_due(99, 100, 20));
        assert!(!matches_term("reagent and agentic", "Agent"));
        assert!(matches_term("An AGENT.", "Agent"));
    }
    #[tokio::test]
    async fn bounded_batches_and_auto_attempt_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = Memory::open(path.clone()).unwrap();
        let epoch = db.epoch().await.unwrap();
        for i in 0..101 {
            let mut t = observation("独立软件系统");
            t.original = format!("Agent{i}");
            db.record(epoch, format!("job{i}"), "manual".into(), vec![t])
                .await
                .unwrap();
        }
        let snapshot = db.snapshot(true).await.unwrap();
        assert_eq!(snapshot.terms.len(), 100);
        let reopened = Memory::open(path).unwrap();
        assert_eq!(
            reopened.snapshot(true).await.unwrap_err().code,
            "analysis_not_due"
        );
        db.finish(snapshot, Profile { topics: vec![] })
            .await
            .unwrap();
        assert_eq!(db.view("".into(), 0).await.unwrap().pending, 1);
        let context = db
            .context(
                (0..101)
                    .map(|i| format!("Agent{i}"))
                    .collect::<Vec<_>>()
                    .join(" "),
            )
            .await
            .unwrap();
        assert!(context.chars().count() <= 2000);
        assert!(context.lines().count() <= 9);
    }
}
