//! Actor-only sync metadata. All mutations share the transaction of the user data.
use super::*;
use rusqlite::{params, Connection};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    pub id: Option<i64>,
    pub inc: Option<String>,
    pub tomb: i64,
    pub feedback_rev: i64,
    pub initial_feedback: Option<Familiarity>,
    #[serde(default)]
    pub last_local_feedback: String,
    #[serde(default)]
    pub remote_feedback: Option<Familiarity>,
    #[serde(default)]
    pub deleted: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Pending {
    pub op: Operation,
    pub sent: bool,
    pub conflict: Option<Conflict>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Pref {
    pub revision: i64,
    pub value: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Data {
    #[serde(default)]
    pub session: i64,
    pub binding: String,
    pub url: String,
    pub device_name: String,
    pub device_id: String,
    pub context: Option<Context>,
    pub enabled: bool,
    pub background: bool,
    pub memory_allowed: bool,
    pub memory_cursor: i64,
    pub preference_cursor: i64,
    pub entities: BTreeMap<String, Entity>,
    pub events: BTreeSet<String>,
    pub queue: Vec<Pending>,
    pub prefs: BTreeMap<String, Pref>,
    pub inbox: BTreeMap<String, String>,
    pub intent: Option<BTreeMap<String, String>>,
    pub status: String,
    pub message: String,
    pub last_success: i64,
    pub discarded: usize,
    pub reset: Option<Reset>,
    pub next_attempt: i64,
    pub failures: u32,
}
pub fn err(_: impl std::fmt::Debug) -> AppError {
    AppError::new(
        "sync_storage",
        "同步数据库操作失败，原有翻译与记忆仍可使用。",
    )
}
pub fn load(db: &Connection) -> AppResult<Option<Data>> {
    let v: i64 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(err)?;
    if v < 2 {
        return Ok(None);
    }
    let s: String = db
        .query_row("SELECT body FROM sync_state WHERE id=1", [], |r| r.get(0))
        .map_err(err)?;
    Ok(Some(serde_json::from_str(&s).map_err(err)?))
}
pub fn save(db: &Connection, d: &Data) -> AppResult<()> {
    db.execute(
        "UPDATE sync_state SET body=? WHERE id=1",
        [serde_json::to_string(d).map_err(err)?],
    )
    .map_err(err)?;
    Ok(())
}
pub fn migrate(db: &mut Connection) -> AppResult<()> {
    if load(db)?.is_some() {
        return Ok(());
    }
    let path = db.path().ok_or_else(|| err("path"))?;
    let backup = std::path::PathBuf::from(format!("{path}.pre-sync-v1.sqlite3"));
    if !backup.exists() {
        let temp = backup.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&temp).map_err(err)?;
        db.backup("main", &temp, None).map_err(err)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600)).map_err(err)?;
        }
        std::fs::File::open(&temp)
            .and_then(|f| f.sync_all())
            .map_err(err)?;
        std::fs::rename(temp, &backup).map_err(err)?;
    }
    let backup_db =
        Connection::open_with_flags(&backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(err)?;
    let backup_version: i64 = backup_db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(err)?;
    let valid: String = backup_db
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(err)?;
    if backup_version != 1 || valid != "ok" {
        return Err(err("backup"));
    }
    drop(backup_db);
    let tx = db.transaction().map_err(err)?;
    tx.execute_batch("CREATE TABLE sync_state(id INTEGER PRIMARY KEY CHECK(id=1),body TEXT NOT NULL);PRAGMA user_version=2;").map_err(err)?;
    tx.execute(
        "INSERT INTO sync_state VALUES(1,?)",
        [serde_json::to_string(&Data::default()).map_err(err)?],
    )
    .map_err(err)?;
    tx.commit().map_err(err)
}
pub fn prefs(s: &crate::settings::Settings) -> BTreeMap<String, String> {
    [
        (Preference::Theme, s.theme.clone()),
        (Preference::TargetLanguage, s.target_language.clone()),
        (Preference::ExplanationStyle, s.explanation_style.clone()),
    ]
    .into_iter()
    .map(|(p, v)| (p.as_str().into(), v))
    .collect()
}
pub fn patch(
    s: &mut crate::settings::Settings,
    values: &BTreeMap<String, String>,
) -> AppResult<()> {
    for (k, v) in values {
        let p = Preference::ALL
            .into_iter()
            .find(|p| p.as_str() == k)
            .ok_or_else(|| err("field"))?;
        if !p.valid(v) {
            return Err(err("value"));
        }
        match p {
            Preference::Theme => s.theme = v.clone(),
            Preference::TargetLanguage => s.target_language = v.clone(),
            Preference::ExplanationStyle => s.explanation_style = v.clone(),
        }
    }
    Ok(())
}
fn generation(d: &Data) -> i64 {
    d.context.as_ref().map(|c| c.memory_generation).unwrap_or(0)
}
pub fn enqueue(d: &mut Data, action: Action) {
    // Only unsent edits are coalesced. Once sent the exact payload survives ambiguous failures.
    let same = |a: &Action, b: &Action| match (a, b) {
        (Action::Preference { field: a, .. }, Action::Preference { field: b, .. }) => a == b,
        (Action::Feedback { key: a, .. }, Action::Feedback { key: b, .. }) => a == b,
        _ => false,
    };
    d.queue
        .retain(|p| (p.sent && p.conflict.is_none()) || !same(&p.op.action, &action));
    d.queue.push(Pending {
        op: Operation {
            id: uuid::Uuid::new_v4().to_string(),
            generation: generation(d),
            action,
        },
        sent: false,
        conflict: None,
    });
}
pub fn term(db: &Connection, id: i64) -> AppResult<Term> {
    db.query_row(
        "SELECT original,translation,domain,sense,language FROM terms WHERE id=?",
        [id],
        |r| {
            Ok(Term {
                original: r.get(0)?,
                translation: r.get(1)?,
                domain: r.get(2)?,
                sense: r.get(3)?,
                language: r.get(4)?,
            })
        },
    )
    .map_err(err)
}
pub fn scan(db: &Connection, d: &mut Data) -> AppResult<()> {
    if !d.memory_allowed {
        return Ok(());
    }
    let ids: Vec<i64> = db
        .prepare("SELECT id FROM terms ORDER BY id")
        .map_err(err)?
        .query_map([], |r| r.get(0))
        .map_err(err)?
        .collect::<Result<_, _>>()
        .map_err(err)?;
    for id in ids {
        let t = term(db, id)?;
        let key = t.key();
        if !t.validate() {
            d.status = "error".into();
            d.message = "部分旧术语超出同步字段限制，已保留本机；请缩短字段后重试。".into();
            continue;
        }
        let fresh = !d.entities.contains_key(&key);
        let e = d.entities.entry(key.clone()).or_default();
        e.id = Some(id);
        if fresh {
            let f:String=db.query_row("SELECT COALESCE((SELECT value FROM events WHERE term_id=? AND kind='feedback' ORDER BY id DESC LIMIT 1),'unknown')",[id],|r|r.get(0)).map_err(err)?;
            e.initial_feedback = serde_json::from_value(serde_json::Value::String(f))
                .ok()
                .filter(|f| *f != Familiarity::Unknown);
        }
        let f:String=db.query_row("SELECT COALESCE((SELECT value FROM events WHERE term_id=? AND kind='feedback' ORDER BY id DESC LIMIT 1),'unknown')",[id],|r|r.get(0)).map_err(err)?;
        let feedback_changed = !fresh && f != e.last_local_feedback;
        e.last_local_feedback = f.clone();
        let (inc, tomb) = (e.inc.clone(), e.tomb);
        if feedback_changed {
            let value: Familiarity = serde_json::from_value(f.into()).map_err(err)?;
            if let Some(incarnation) = inc.clone() {
                let rev = e.feedback_rev;
                enqueue(
                    d,
                    Action::Feedback {
                        key: key.clone(),
                        incarnation,
                        base_revision: rev,
                        value,
                    },
                );
            } else {
                d.entities.get_mut(&key).unwrap().initial_feedback = Some(value);
            }
        }

        let rows:Vec<(String,i64,String)>=db.prepare("SELECT job,at,source FROM events WHERE term_id=? AND kind='translation' ORDER BY id").map_err(err)?.query_map([id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(err)?.collect::<Result<_,_>>().map_err(err)?;
        for (job, at, source) in rows {
            let eid = job
                .strip_prefix("sync:")
                .map(str::to_owned)
                .unwrap_or_else(|| legacy_event_id(&job, &key));
            if d.events.insert(eid.clone()) {
                let event = Occurrence {
                    id: eid,
                    at,
                    source,
                };
                if event.validate() {
                    enqueue(
                        d,
                        Action::Observe {
                            term: t.clone(),
                            event,
                            incarnation: inc.clone(),
                            tombstone_revision: tomb,
                        },
                    );
                } else {
                    d.message = "部分旧出现记录超出同步范围，已保留本机。".into();
                }
            }
        }
    }
    Ok(())
}
pub fn observed(db: &Connection) -> AppResult<()> {
    if let Some(mut d) = load(db)? {
        if !d.binding.is_empty() && d.memory_allowed {
            scan(db, &mut d)?;
            save(db, &d)?;
        }
    }
    Ok(())
}
pub fn feedback(db: &Connection, id: i64, value: &str) -> AppResult<()> {
    if let Some(mut d) = load(db)? {
        if !d.binding.is_empty() {
            let key = term(db, id)?.key();
            if let Some(e) = d.entities.get_mut(&key) {
                let f: Familiarity = serde_json::from_value(value.into()).map_err(err)?;
                e.last_local_feedback = value.into();
                if let Some(inc) = e.inc.clone() {
                    let rev = e.feedback_rev;
                    enqueue(
                        &mut d,
                        Action::Feedback {
                            key,
                            incarnation: inc,
                            base_revision: rev,
                            value: f,
                        },
                    );
                } else {
                    e.initial_feedback = Some(f);
                }
            }
            save(db, &d)?;
        }
    }
    Ok(())
}
pub fn deleted(db: &Connection, id: Option<i64>) -> AppResult<()> {
    if let Some(mut d) = load(db)? {
        if d.enabled {
            let Some(id) = id else {
                return Err(AppError::new(
                    "sync_clear_scope",
                    "同步已连接，请使用「清空所有设备」或先断开再清空本机。",
                ));
            };
            let key = term(db, id)?.key();
            d.queue
                .retain(|p| p.op.action.key().as_deref() != Some(&key));
            if let Some(e) = d.entities.get_mut(&key) {
                e.id = None;
                e.deleted = true;
                if let Some(inc) = e.inc.clone() {
                    enqueue(
                        &mut d,
                        Action::Delete {
                            key,
                            incarnation: inc,
                        },
                    );
                }
            }
        } else {
            // Local-only cleanup deliberately forgets the mirror baseline. Reconnect reimports it.
            d.memory_cursor = 0;
            d.entities.clear();
            d.events.clear();
            d.queue.retain(|p| p.op.action.stream() != Stream::Memory);
        }
        save(db, &d)?;
    }
    Ok(())
}
pub fn preferences(db: &Connection, values: &BTreeMap<String, String>) -> AppResult<()> {
    if let Some(mut d) = load(db)? {
        if !d.binding.is_empty() {
            for field in Preference::ALL {
                let key = field.as_str();
                let value = values.get(key).ok_or_else(|| err("pref"))?;
                let old = d.prefs.get(key).cloned().unwrap_or_default();
                let queued = d.queue.iter().rev().find_map(|p| match &p.op.action {
                    Action::Preference {
                        field: f, value, ..
                    } if *f == field => Some(value),
                    _ => None,
                });
                if queued.map(|v| v != value).unwrap_or(old.value != *value) {
                    enqueue(
                        &mut d,
                        Action::Preference {
                            field,
                            base_revision: old.revision,
                            value: value.clone(),
                        },
                    );
                }
            }
        }
        d.intent = None;
        save(db, &d)?;
    }
    Ok(())
}
pub fn ticket(d: &Data) -> String {
    format!("{}:{}", d.binding, d.session)
}
pub fn ensure(d: &Data, binding: &str) -> AppResult<()> {
    if !d.enabled || ticket(d) != binding {
        return Err(AppError::new("sync_cancelled", "同步已暂停或目标已改变。"));
    }
    Ok(())
}
pub fn check(d: &Data, c: &Context) -> AppResult<()> {
    if c.protocol != VERSION
        || c.revision < 0
        || c.memory_generation < 1
        || !valid_id(&c.workspace_id)
        || !valid_id(&c.history_id)
    {
        return Err(AppError::new("sync_protocol", "同步服务协议无效或不兼容。"));
    }
    let old = d.context.as_ref().ok_or_else(|| err("context"))?;
    if old.workspace_id != c.workspace_id || old.history_id != c.history_id {
        return Err(AppError::new(
            "sync_history",
            "服务历史已恢复或工作区改变。同步已暂停；请重新登录并明确选择新的同步基线。",
        ));
    }
    Ok(())
}
pub fn apply(db: &mut Connection, binding: &str, page: Page) -> AppResult<()> {
    let tx = db.transaction().map_err(err)?;
    let mut d = load(&tx)?.ok_or_else(|| err("schema"))?;
    ensure(&d, binding)?;
    check(&d, &page.context)?;
    if page.stream == Stream::Memory && !d.memory_allowed {
        return Err(AppError::new("sync_cancelled", "术语记忆已暂停。"));
    }
    let before = if page.stream == Stream::Memory {
        d.memory_cursor
    } else {
        d.preference_cursor
    };
    if page.changes.len() > MAX_PAGE
        || page.until < before
        || page.next_cursor < before
        || page.next_cursor > page.until
        || page.until > page.context.revision
        || page.has_more && page.next_cursor == before
    {
        return Err(err("cursor"));
    }
    if page.next_cursor <= before && page.until <= before {
        return Ok(());
    }
    let mut previous = before;
    let mut changed = false;
    for record in page.changes {
        if record.revision <= previous || record.revision > page.next_cursor {
            return Err(err("revision"));
        }
        previous = record.revision;
        let memory = !matches!(record.change, Change::Preference { .. } | Change::Redacted);
        if (memory && page.stream != Stream::Memory)
            || matches!(record.change, Change::Preference { .. })
                && page.stream != Stream::Preferences
        {
            return Err(err("stream"));
        }
        match record.change {
            Change::Term {
                key,
                term,
                incarnation,
                tombstone_revision,
            } => {
                if !term.validate()
                    || term.key() != key
                    || !valid_id(&incarnation)
                    || tombstone_revision < 0
                {
                    return Err(err("term"));
                }
                let mut e = d.entities.get(&key).cloned().unwrap_or_default();
                // A deliberate local deletion stays deleted while its tombstone is pending.
                let deleting = e.deleted
                    || d.queue
                        .iter()
                        .any(|p| matches!(&p.op.action,Action::Delete{key:k,..}if k==&key));
                if !deleting {
                    let normalized = term.normalized();
                    tx.execute("INSERT INTO terms(normalized,original,translation,domain,sense,language) VALUES(?,?,?,?,?,?) ON CONFLICT(normalized,domain,sense,language) DO UPDATE SET original=excluded.original,translation=excluded.translation",params![normalized,term.original,term.translation,term.domain,term.sense,term.language]).map_err(err)?;
                    e.id=Some(tx.query_row("SELECT id FROM terms WHERE normalized=? AND domain=? AND sense=? AND language=?",params![normalized,term.domain,term.sense,term.language],|r|r.get(0)).map_err(err)?);
                    changed = true;
                }
                e.inc = Some(incarnation.clone());
                e.tomb = tombstone_revision;
                let initial = e.initial_feedback.take();
                let must_delete = e.deleted;
                d.entities.insert(key.clone(), e);
                if must_delete
                    && !d
                        .queue
                        .iter()
                        .any(|p| matches!(&p.op.action,Action::Delete{key:k,..}if k==&key))
                {
                    enqueue(
                        &mut d,
                        Action::Delete {
                            key: key.clone(),
                            incarnation: incarnation.clone(),
                        },
                    );
                }
                if let Some(value) = initial {
                    enqueue(
                        &mut d,
                        Action::Feedback {
                            key,
                            incarnation,
                            base_revision: 0,
                            value,
                        },
                    );
                }
            }
            Change::Occurrence {
                key,
                incarnation,
                event,
            } => {
                if !event.validate() {
                    return Err(err("event"));
                }
                if let Some(e) = d.entities.get(&key) {
                    if e.inc.as_ref() == Some(&incarnation) {
                        if let Some(id) = e.id {
                            if d.events.insert(event.id.clone()) {
                                tx.execute("INSERT OR IGNORE INTO events(job,term_id,at,source,kind) VALUES(?,?,?,?,'translation')",params![format!("sync:{}",event.id),id,event.at,event.source]).map_err(err)?;
                                changed = true;
                            }
                        }
                    }
                }
            }
            Change::Feedback {
                key,
                incarnation,
                value,
            } => {
                if let Some(e) = d.entities.get_mut(&key) {
                    if e.inc.as_ref() == Some(&incarnation) && record.revision > e.feedback_rev {
                        e.feedback_rev = record.revision;
                        e.remote_feedback = Some(value);
                        for p in &mut d.queue {
                            if matches!(&p.op.action,Action::Feedback{key:k,..}if k==&key) {
                                if let Some(c) = &mut p.conflict {
                                    c.remote_revision = record.revision;
                                    c.remote_value = Some(value.as_str().into());
                                }
                            }
                        }
                        let pending = d
                            .queue
                            .iter()
                            .any(|p| matches!(&p.op.action,Action::Feedback{key:k,..}if k==&key));
                        if !pending {
                            if let Some(id) = e.id {
                                put_feedback(&tx, id, value, record.revision)?;
                                e.last_local_feedback = value.as_str().into();
                                changed = true;
                            }
                        }
                    }
                }
            }
            Change::Preference { field, value } => {
                if !field.valid(&value) {
                    return Err(err("pref"));
                }
                let old = d.prefs.entry(field.as_str().into()).or_default();
                if record.revision > old.revision {
                    *old = Pref {
                        revision: record.revision,
                        value: value.clone(),
                    };
                    for p in &mut d.queue {
                        if matches!(&p.op.action,Action::Preference{field:f,..}if *f==field) {
                            if let Some(c) = &mut p.conflict {
                                c.remote_revision = record.revision;
                                c.remote_value = Some(value.clone());
                            }
                        }
                    }
                    let pending = d
                        .queue
                        .iter()
                        .any(|p| matches!(&p.op.action,Action::Preference{field:f,..}if *f==field));
                    if !pending {
                        d.inbox.insert(field.as_str().into(), value);
                    }
                }
            }
            Change::Deleted { key, incarnation } => {
                let e = d.entities.entry(key.clone()).or_default();
                if let Some(id) = e.id.take() {
                    tx.execute("DELETE FROM terms WHERE id=?", [id])
                        .map_err(err)?;
                    changed = true;
                }
                e.inc = None;
                e.tomb = record.revision;
                e.feedback_rev = 0;
                e.initial_feedback = None;
                e.deleted = false;
                e.last_local_feedback = String::new();
                e.remote_feedback = None;
                let _ = incarnation;
                d.discarded += d
                    .queue
                    .iter()
                    .filter(|p| p.op.action.key().as_ref() == Some(&key))
                    .count();
                d.queue.retain(|p| p.op.action.key().as_ref() != Some(&key));
            }
            Change::Reset { generation } => {
                if generation < 1 {
                    return Err(err("generation"));
                }
                tx.execute("DELETE FROM terms", []).map_err(err)?;
                changed = true;
                d.discarded += d
                    .queue
                    .iter()
                    .filter(|p| p.op.action.stream() == Stream::Memory)
                    .count();
                d.queue.retain(|p| p.op.action.stream() != Stream::Memory);
                d.entities.clear();
                d.events.clear();
                d.context.as_mut().unwrap().memory_generation = generation;
                d.reset = None;
            }
            Change::Redacted => {}
        }
    }
    if changed {
        crate::memory::invalidate(&tx)?;
    }
    if page.stream == Stream::Memory {
        d.memory_cursor = page.next_cursor;
    } else {
        d.preference_cursor = page.next_cursor;
    }
    // Memory generation moves only with the memory stream. Paused memory retains its old baseline.
    d.context.as_mut().unwrap().revision = page.context.revision;
    save(&tx, &d)?;
    tx.commit().map_err(err)
}
pub fn put_feedback(db: &Connection, id: i64, value: Familiarity, rev: i64) -> AppResult<()> {
    db.execute("INSERT OR IGNORE INTO events(job,term_id,at,source,kind,value) VALUES(?,?,?,'sync','feedback',?)",params![format!("sync-feedback:{rev}"),id,crate::memory::now(),value.as_str()]).map_err(err)?;
    Ok(())
}

pub fn acknowledge(
    db: &mut Connection,
    binding: &str,
    ids: &[String],
    a: Accepted,
) -> AppResult<()> {
    let tx = db.transaction().map_err(err)?;
    let mut d = load(&tx)?.ok_or_else(|| err("schema"))?;
    ensure(&d, binding)?;
    check(&d, &a.context)?;
    if a.receipts.len() != ids.len()
        || a.receipts
            .iter()
            .any(|r| !ids.contains(&r.id) || r.revision < 0 || r.revision > a.context.revision)
        || a.receipts
            .iter()
            .map(|r| &r.id)
            .collect::<BTreeSet<_>>()
            .len()
            != ids.len()
    {
        return Err(err("receipts"));
    }
    let mut fields = BTreeSet::new();
    let mut feedback_keys = BTreeSet::new();
    for receipt in &a.receipts {
        let action = d
            .queue
            .iter()
            .find(|p| p.op.id == receipt.id)
            .map(|p| p.op.action.clone());
        let Some(action) = action else {
            continue;
        };
        for pending in &mut d.queue {
            if pending.sent {
                continue;
            }
            match (&action, &mut pending.op.action) {
                (
                    Action::Preference { field: a, .. },
                    Action::Preference {
                        field: b,
                        base_revision,
                        ..
                    },
                ) if a == b => *base_revision = receipt.revision,
                (
                    Action::Feedback { key: a, .. },
                    Action::Feedback {
                        key: b,
                        base_revision,
                        ..
                    },
                ) if a == b => *base_revision = receipt.revision,
                _ => {}
            }
        }
        match action {
            Action::Preference { field, value, .. } => {
                let old = d.prefs.entry(field.as_str().into()).or_default();
                if receipt.revision >= old.revision {
                    *old = Pref {
                        revision: receipt.revision,
                        value,
                    };
                }
                fields.insert(field.as_str().to_string());
            }
            Action::Feedback { key, value, .. } => {
                if let Some(e) = d.entities.get_mut(&key) {
                    if receipt.revision >= e.feedback_rev {
                        e.feedback_rev = receipt.revision;
                        e.remote_feedback = Some(value);
                    }
                    feedback_keys.insert(key);
                }
            }
            _ => {}
        }
    }
    d.queue.retain(|p| !ids.contains(&p.op.id));
    for field in fields {
        if !d
            .queue
            .iter()
            .any(|p| matches!(&p.op.action,Action::Preference{field:f,..}if f.as_str()==field))
        {
            if let Some(pref) = d.prefs.get(&field) {
                d.inbox.insert(field, pref.value.clone());
            }
        }
    }
    let mut changed = false;
    for key in feedback_keys {
        if d.queue
            .iter()
            .any(|p| matches!(&p.op.action,Action::Feedback{key:k,..}if k==&key))
        {
            continue;
        }
        if let Some(e) = d.entities.get_mut(&key) {
            if let (Some(id), Some(value)) = (e.id, e.remote_feedback) {
                let old:String=tx.query_row("SELECT COALESCE((SELECT value FROM events WHERE term_id=? AND kind='feedback' ORDER BY id DESC LIMIT 1),'unknown')",[id],|r|r.get(0)).map_err(err)?;
                if old != value.as_str() {
                    tx.execute("INSERT INTO events(job,term_id,at,source,kind,value) VALUES(?,?,?,'sync','feedback',?)",params![uuid::Uuid::new_v4().to_string(),id,crate::memory::now(),value.as_str()]).map_err(err)?;
                    changed = true;
                }
                e.last_local_feedback = value.as_str().into();
            }
        }
    }
    if changed {
        crate::memory::invalidate(&tx)?;
    }
    save(&tx, &d)?;
    tx.commit().map_err(err)
}
