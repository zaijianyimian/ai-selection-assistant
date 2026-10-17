use crate::error::{Error, Result};
use asa_sync_protocol::*;
use axum::http::StatusCode;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::{
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
#[derive(Clone)]
pub struct Limits {
    pub storage: i64,
    pub devices: usize,
    pub logins: u32,
    pub requests: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            storage: 1 << 30,
            devices: 20,
            logins: 5,
            requests: 60,
        }
    }
}
pub struct Store {
    pub(crate) db: Mutex<Connection>,
    pub limits: Limits,
}
fn get(db: &Connection, key: &str) -> Result<String> {
    db.query_row("SELECT value FROM meta WHERE key=?", [key], |r| r.get(0))
        .map_err(Error::storage)
}
fn set(db: &Connection, key: &str, value: &str) -> Result<()> {
    db.execute("INSERT INTO meta(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value]).map_err(Error::storage)?;
    Ok(())
}
pub fn context(db: &Connection) -> Result<Context> {
    Ok(Context {
        protocol: VERSION,
        workspace_id: get(db, "workspace")?,
        history_id: get(db, "history")?,
        memory_generation: get(db, "generation")?.parse().map_err(Error::storage)?,
        revision: db
            .query_row("SELECT COALESCE(MAX(revision),0) FROM changes", [], |r| {
                r.get(0)
            })
            .map_err(Error::storage)?,
    })
}
fn append(
    db: &Connection,
    stream: Stream,
    key: &str,
    generation: i64,
    change: &Change,
) -> Result<i64> {
    db.execute(
        "INSERT INTO changes(stream,entity,generation,body) VALUES(?,?,?,?)",
        params![
            stream.as_str(),
            key,
            generation,
            serde_json::to_string(change).map_err(Error::storage)?
        ],
    )
    .map_err(Error::storage)?;
    Ok(db.last_insert_rowid() as i64)
}
fn field_conflict(op: &Operation, code: &str, value: Option<String>, rev: i64) -> Conflict {
    Conflict {
        op_id: op.id.clone(),
        code: code.into(),
        remote_value: value,
        remote_revision: rev,
    }
}
struct Entity {
    inc: String,
    body: Option<String>,
    tomb: i64,
    feedback: String,
    feedback_rev: i64,
}
fn entity(db: &Connection, key: &str) -> Result<Option<Entity>> {
    db.query_row(
        "SELECT incarnation,body,tombstone,feedback,feedback_rev FROM terms WHERE key=?",
        [key],
        |r| {
            Ok(Entity {
                inc: r.get(0)?,
                body: r.get(1)?,
                tomb: r.get(2)?,
                feedback: r.get(3)?,
                feedback_rev: r.get(4)?,
            })
        },
    )
    .optional()
    .map_err(Error::storage)
}
impl Store {
    pub fn open(path: &Path, limits: Limits) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(Error::storage)?;
        }
        let db = Connection::open(path).map_err(Error::storage)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(Error::storage)?;
        }
        let version: i64 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(Error::storage)?;
        if version > 1 {
            return Err(Error::bad("server_schema_newer"));
        }
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(Error::storage)?;
        db.execute_batch("PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL;PRAGMA secure_delete=ON;BEGIN IMMEDIATE;
  CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
  CREATE TABLE IF NOT EXISTS devices(id TEXT PRIMARY KEY,name TEXT NOT NULL,token_digest TEXT UNIQUE NOT NULL,created INTEGER NOT NULL,last_seen INTEGER NOT NULL,revoked INTEGER NOT NULL DEFAULT 0);
  CREATE TABLE IF NOT EXISTS terms(key TEXT PRIMARY KEY,incarnation TEXT NOT NULL,body TEXT,tombstone INTEGER NOT NULL DEFAULT 0,feedback TEXT NOT NULL DEFAULT 'unknown',feedback_rev INTEGER NOT NULL DEFAULT 0);
  CREATE TABLE IF NOT EXISTS events(id TEXT PRIMARY KEY,entity TEXT NOT NULL,incarnation TEXT NOT NULL,body TEXT NOT NULL);
  CREATE INDEX IF NOT EXISTS events_entity ON events(entity);
  CREATE TABLE IF NOT EXISTS preferences(field TEXT PRIMARY KEY,value TEXT NOT NULL,revision INTEGER NOT NULL);
  CREATE TABLE IF NOT EXISTS changes(revision INTEGER PRIMARY KEY AUTOINCREMENT,stream TEXT NOT NULL,entity TEXT NOT NULL,generation INTEGER NOT NULL,body TEXT);
  CREATE INDEX IF NOT EXISTS changes_stream ON changes(stream,revision);
  CREATE INDEX IF NOT EXISTS changes_entity ON changes(entity);
  CREATE TABLE IF NOT EXISTS receipts(id TEXT PRIMARY KEY,hash TEXT NOT NULL,revision INTEGER NOT NULL);
  PRAGMA user_version=1;").map_err(Error::storage)?;
        for (key, value) in [
            ("workspace", Uuid::new_v4().to_string()),
            ("history", Uuid::new_v4().to_string()),
            ("generation", "1".into()),
        ] {
            db.execute(
                "INSERT OR IGNORE INTO meta VALUES(?,?)",
                params![key, value],
            )
            .map_err(Error::storage)?;
        }
        db.execute_batch("COMMIT").map_err(Error::storage)?;
        Ok(Self {
            db: Mutex::new(db),
            limits,
        })
    }
    pub fn context(&self) -> Result<Context> {
        context(&*self.db.lock().map_err(Error::storage)?)
    }
    pub fn password_hash(&self) -> Result<String> {
        get(&*self.db.lock().map_err(Error::storage)?, "password")
    }
    pub fn set_password(&self, hash: &str, initialize: bool) -> Result<()> {
        let mut db = self.db.lock().map_err(Error::storage)?;
        let tx = db.transaction().map_err(Error::storage)?;
        if initialize
            && tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM meta WHERE key='password')",
                    [],
                    |r| r.get::<_, bool>(0),
                )
                .map_err(Error::storage)?
        {
            return Err(Error::bad("already_initialized"));
        }
        set(&tx, "password", hash)?;
        tx.execute("UPDATE devices SET revoked=1", [])
            .map_err(Error::storage)?;
        tx.commit().map_err(Error::storage)
    }
    pub fn enroll(&self, verified_hash: &str, name: &str, token: &str) -> Result<Enrollment> {
        if name.trim().is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control)
        {
            return Err(Error::bad("device_name"));
        }
        let mut db = self.db.lock().map_err(Error::storage)?;
        let tx = db.transaction().map_err(Error::storage)?;
        if get(&tx, "password")? != verified_hash {
            return Err(Error::unauthorized());
        }
        let count: i64 = tx
            .query_row("SELECT COUNT(*) FROM devices WHERE revoked=0", [], |r| {
                r.get(0)
            })
            .map_err(Error::storage)?;
        if count >= self.limits.devices as i64 {
            return Err(Error::new(StatusCode::CONFLICT, "device_limit"));
        }
        let id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO devices(id,name,token_digest,created,last_seen) VALUES(?,?,?,?,?)",
            params![id, name.trim(), digest(token.as_bytes()), now(), now()],
        )
        .map_err(Error::storage)?;
        let ctx = context(&tx)?;
        tx.commit().map_err(Error::storage)?;
        Ok(Enrollment {
            context: ctx,
            device_id: id,
            token: token.into(),
        })
    }
    pub fn authenticate(&self, token: &str) -> Result<String> {
        if token.len() > 256 {
            return Err(Error::unauthorized());
        }
        let db = self.db.lock().map_err(Error::storage)?;
        let id: String = db
            .query_row(
                "SELECT id FROM devices WHERE token_digest=? AND revoked=0",
                [digest(token.as_bytes())],
                |r| r.get(0),
            )
            .optional()
            .map_err(Error::storage)?
            .ok_or_else(Error::unauthorized)?;
        db.execute(
            "UPDATE devices SET last_seen=? WHERE id=? AND last_seen<?",
            params![now(), id, now() - 60],
        )
        .map_err(Error::storage)?;
        Ok(id)
    }
    pub fn devices(&self) -> Result<Vec<Device>> {
        let db = self.db.lock().map_err(Error::storage)?;
        let mut s = db
            .prepare(
                "SELECT id,name,created,last_seen FROM devices WHERE revoked=0 ORDER BY created,id",
            )
            .map_err(Error::storage)?;
        let rows = s
            .query_map([], |r| {
                Ok(Device {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created_at: r.get(2)?,
                    last_seen: r.get(3)?,
                })
            })
            .map_err(Error::storage)?
            .collect::<std::result::Result<_, _>>()
            .map_err(Error::storage);
        rows
    }
    pub fn revoke(&self, id: &str) -> Result<()> {
        self.db
            .lock()
            .map_err(Error::storage)?
            .execute("UPDATE devices SET revoked=1 WHERE id=?", [id])
            .map_err(Error::storage)?;
        Ok(())
    }
    pub fn restored(&self) -> Result<()> {
        let mut db = self.db.lock().map_err(Error::storage)?;
        let tx = db.transaction().map_err(Error::storage)?;
        set(&tx, "history", &Uuid::new_v4().to_string())?;
        tx.execute("UPDATE devices SET revoked=1", [])
            .map_err(Error::storage)?;
        tx.commit().map_err(Error::storage)
    }
    pub fn backup(&self, path: &Path) -> Result<()> {
        let db = self.db.lock().map_err(Error::storage)?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path).map_err(Error::storage)?;
        db.backup("main", path, None).map_err(Error::storage)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(Error::storage)?;
        }
        Ok(())
    }
    pub fn push(&self, request: &Push) -> Result<Accepted> {
        if !request.validate() {
            return Err(Error::bad("invalid_push"));
        }
        let mut db = self.db.lock().map_err(Error::storage)?;
        let tx = db.transaction().map_err(Error::storage)?;
        let ctx = context(&tx)?;
        check_context(
            &ctx,
            request.protocol,
            &request.workspace_id,
            &request.history_id,
        )?;
        let mut receipts = vec![];
        let mut conflicts = vec![];
        let mut additions = false;
        for op in &request.operations {
            let hash = digest(&serde_json::to_vec(op).map_err(Error::storage)?);
            if let Some((old, rev)) = tx
                .query_row(
                    "SELECT hash,revision FROM receipts WHERE id=?",
                    [&op.id],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
                )
                .optional()
                .map_err(Error::storage)?
            {
                if old != hash {
                    return Err(Error::bad("operation_identity_reused"));
                }
                receipts.push(Receipt {
                    id: op.id.clone(),
                    revision: rev,
                });
                continue;
            }
            if op.action.stream() == Stream::Memory && op.generation != ctx.memory_generation {
                conflicts.push(field_conflict(
                    op,
                    "generation_changed",
                    None,
                    ctx.memory_generation,
                ));
                continue;
            }
            let revision = match apply(&tx, op, &ctx, &mut conflicts)? {
                Some(r) => r,
                None => continue,
            };
            additions |= !matches!(op.action, Action::Delete { .. });
            tx.execute(
                "INSERT INTO receipts VALUES(?,?,?)",
                params![op.id, hash, revision],
            )
            .map_err(Error::storage)?;
            receipts.push(Receipt {
                id: op.id.clone(),
                revision,
            });
        }
        if !conflicts.is_empty() {
            return Err(Error::conflict("conflict", ctx, conflicts));
        }
        if additions && storage_bytes(&tx)? > self.limits.storage {
            return Err(Error::new(StatusCode::CONFLICT, "storage_quota"));
        }
        let ctx = context(&tx)?;
        tx.commit().map_err(Error::storage)?;
        Ok(Accepted {
            context: ctx,
            receipts,
        })
    }
    pub fn reset(&self, request: &Reset) -> Result<Accepted> {
        if !valid_id(&request.id) {
            return Err(Error::bad("operation_id"));
        }
        let mut db = self.db.lock().map_err(Error::storage)?;
        let tx = db.transaction().map_err(Error::storage)?;
        let ctx = context(&tx)?;
        check_context(
            &ctx,
            request.protocol,
            &request.workspace_id,
            &request.history_id,
        )?;
        let hash = digest(&serde_json::to_vec(request).map_err(Error::storage)?);
        if let Some((old, rev)) = tx
            .query_row(
                "SELECT hash,revision FROM receipts WHERE id=?",
                [&request.id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(Error::storage)?
        {
            if old != hash {
                return Err(Error::bad("operation_identity_reused"));
            }
            return Ok(Accepted {
                context: ctx,
                receipts: vec![Receipt {
                    id: request.id.clone(),
                    revision: rev,
                }],
            });
        }
        if request.generation != ctx.memory_generation {
            return Err(Error::conflict("generation_changed", ctx, vec![]));
        }
        let next = ctx
            .memory_generation
            .checked_add(1)
            .ok_or_else(|| Error::bad("generation_overflow"))?;
        tx.execute_batch("DELETE FROM events;DELETE FROM terms;UPDATE changes SET body=NULL WHERE stream='memory';").map_err(Error::storage)?;
        set(&tx, "generation", &next.to_string())?;
        let revision = append(
            &tx,
            Stream::Memory,
            "",
            next,
            &Change::Reset { generation: next },
        )?;
        tx.execute(
            "INSERT INTO receipts VALUES(?,?,?)",
            params![request.id, hash, revision],
        )
        .map_err(Error::storage)?;
        let ctx = context(&tx)?;
        tx.commit().map_err(Error::storage)?;
        Ok(Accepted {
            context: ctx,
            receipts: vec![Receipt {
                id: request.id.clone(),
                revision,
            }],
        })
    }
    pub fn page(
        &self,
        stream: Stream,
        after: i64,
        until: Option<i64>,
        limit: usize,
    ) -> Result<Page> {
        let db = self.db.lock().map_err(Error::storage)?;
        let ctx = context(&db)?;
        let until = until.unwrap_or(ctx.revision);
        if after < 0 || after > until || until > ctx.revision || limit == 0 || limit > MAX_PAGE {
            return Err(Error::bad("invalid_cursor"));
        }
        let mut page = Page {
            context: ctx,
            stream,
            until,
            next_cursor: after,
            has_more: false,
            changes: vec![],
        };
        let mut stmt=db.prepare("SELECT revision,body FROM changes WHERE stream=? AND revision>? AND revision<=? ORDER BY revision LIMIT ?").map_err(Error::storage)?;
        let rows = stmt
            .query_map(params![stream.as_str(), after, until, limit as i64], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
            })
            .map_err(Error::storage)?;
        let mut bytes = serde_json::to_vec(&page).map_err(Error::storage)?.len() + 64;
        for row in rows {
            let (revision, body) = row.map_err(Error::storage)?;
            let change = body
                .map(|s| serde_json::from_str::<Change>(&s))
                .transpose()
                .map_err(Error::storage)?
                .unwrap_or(Change::Redacted);
            let record = Record { revision, change };
            let size = serde_json::to_vec(&record).map_err(Error::storage)?.len() + 1;
            if bytes + size > MAX_BYTES {
                break;
            }
            bytes += size;
            page.changes.push(record);
            page.next_cursor = revision;
        }
        page.has_more=db.query_row("SELECT EXISTS(SELECT 1 FROM changes WHERE stream=? AND revision>? AND revision<=?)",params![stream.as_str(),page.next_cursor,until],|r|r.get(0)).map_err(Error::storage)?;
        if !page.has_more {
            page.next_cursor = until;
        }
        Ok(page)
    }
}
fn check_context(ctx: &Context, version: u32, workspace: &str, history: &str) -> Result<()> {
    if version != VERSION {
        return Err(Error::bad("protocol_version"));
    }
    if ctx.workspace_id != workspace || ctx.history_id != history {
        return Err(Error::conflict("history_changed", ctx.clone(), vec![]));
    }
    Ok(())
}
fn storage_bytes(db: &Connection) -> Result<i64> {
    let pages: i64 = db
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .map_err(Error::storage)?;
    let free: i64 = db
        .pragma_query_value(None, "freelist_count", |r| r.get(0))
        .map_err(Error::storage)?;
    let size: i64 = db
        .pragma_query_value(None, "page_size", |r| r.get(0))
        .map_err(Error::storage)?;
    Ok((pages - free) * size)
}

fn apply(
    tx: &Transaction,
    op: &Operation,
    ctx: &Context,
    conflicts: &mut Vec<Conflict>,
) -> Result<Option<i64>> {
    let gen = ctx.memory_generation;
    match &op.action {
        Action::Observe {
            term,
            event,
            incarnation,
            tombstone_revision,
        } => {
            let key = term.key();
            let old = entity(tx, &key)?;
            let inc = match old {
                Some(old) => {
                    if incarnation.as_ref().is_some_and(|v| v != &old.inc)
                        || *tombstone_revision != old.tomb
                    {
                        conflicts.push(field_conflict(op, "term_deleted", None, old.tomb));
                        return Ok(None);
                    }
                    if old.body.is_some() {
                        old.inc
                    } else {
                        if incarnation.is_some() {
                            conflicts.push(field_conflict(op, "term_deleted", None, old.tomb));
                            return Ok(None);
                        }
                        let inc = Uuid::new_v4().to_string();
                        tx.execute("UPDATE terms SET incarnation=?,body=?,feedback='unknown',feedback_rev=0 WHERE key=?",params![inc,serde_json::to_string(term).map_err(Error::storage)?,key]).map_err(Error::storage)?;
                        append(
                            tx,
                            Stream::Memory,
                            &key,
                            gen,
                            &Change::Term {
                                key: key.clone(),
                                term: term.clone(),
                                incarnation: inc.clone(),
                                tombstone_revision: old.tomb,
                            },
                        )?;
                        inc
                    }
                }
                None => {
                    if incarnation.is_some() || *tombstone_revision != 0 {
                        conflicts.push(field_conflict(op, "term_deleted", None, 0));
                        return Ok(None);
                    }
                    let inc = Uuid::new_v4().to_string();
                    tx.execute(
                        "INSERT INTO terms(key,incarnation,body) VALUES(?,?,?)",
                        params![
                            key,
                            inc,
                            serde_json::to_string(term).map_err(Error::storage)?
                        ],
                    )
                    .map_err(Error::storage)?;
                    append(
                        tx,
                        Stream::Memory,
                        &key,
                        gen,
                        &Change::Term {
                            key: key.clone(),
                            term: term.clone(),
                            incarnation: inc.clone(),
                            tombstone_revision: 0,
                        },
                    )?;
                    inc
                }
            };
            let encoded = serde_json::to_string(event).map_err(Error::storage)?;
            if let Some((old_key, old_inc, body)) = tx
                .query_row(
                    "SELECT entity,incarnation,body FROM events WHERE id=?",
                    [&event.id],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(Error::storage)?
            {
                if old_key != key || old_inc != inc || body != encoded {
                    return Err(Error::bad("event_identity_reused"));
                }
                return Ok(Some(context(tx)?.revision));
            }
            tx.execute(
                "INSERT INTO events VALUES(?,?,?,?)",
                params![event.id, key, inc, encoded],
            )
            .map_err(Error::storage)?;
            Ok(Some(append(
                tx,
                Stream::Memory,
                &key,
                gen,
                &Change::Occurrence {
                    key: key.clone(),
                    incarnation: inc,
                    event: event.clone(),
                },
            )?))
        }
        Action::Feedback {
            key,
            incarnation,
            base_revision,
            value,
        } => {
            let Some(old) = entity(tx, key)? else {
                conflicts.push(field_conflict(op, "term_deleted", None, 0));
                return Ok(None);
            };
            if old.body.is_none() || old.inc != *incarnation {
                conflicts.push(field_conflict(op, "term_deleted", None, old.tomb));
                return Ok(None);
            }
            if old.feedback == value.as_str() {
                return Ok(Some(old.feedback_rev));
            }
            if old.feedback_rev != *base_revision {
                conflicts.push(field_conflict(
                    op,
                    "field_changed",
                    Some(old.feedback),
                    old.feedback_rev,
                ));
                return Ok(None);
            }
            let rev = append(
                tx,
                Stream::Memory,
                key,
                gen,
                &Change::Feedback {
                    key: key.clone(),
                    incarnation: incarnation.clone(),
                    value: *value,
                },
            )?;
            tx.execute(
                "UPDATE terms SET feedback=?,feedback_rev=? WHERE key=?",
                params![value.as_str(), rev, key],
            )
            .map_err(Error::storage)?;
            Ok(Some(rev))
        }
        Action::Delete { key, incarnation } => {
            let Some(old) = entity(tx, key)? else {
                return Ok(Some(context(tx)?.revision));
            };
            if old.inc != *incarnation {
                conflicts.push(field_conflict(op, "term_deleted", None, old.tomb));
                return Ok(None);
            }
            if old.body.is_none() {
                return Ok(Some(old.tomb));
            }
            tx.execute(
                "UPDATE changes SET body=NULL WHERE stream='memory' AND entity=?",
                [key],
            )
            .map_err(Error::storage)?;
            tx.execute("DELETE FROM events WHERE entity=?", [key])
                .map_err(Error::storage)?;
            let rev = append(
                tx,
                Stream::Memory,
                key,
                gen,
                &Change::Deleted {
                    key: key.clone(),
                    incarnation: incarnation.clone(),
                },
            )?;
            tx.execute("UPDATE terms SET body=NULL,tombstone=?,feedback='unknown',feedback_rev=0 WHERE key=?",params![rev,key]).map_err(Error::storage)?;
            Ok(Some(rev))
        }
        Action::Preference {
            field,
            base_revision,
            value,
        } => {
            let old = tx
                .query_row(
                    "SELECT value,revision FROM preferences WHERE field=?",
                    [field.as_str()],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
                )
                .optional()
                .map_err(Error::storage)?;
            if let Some((old, rev)) = &old {
                if old == value {
                    return Ok(Some(*rev));
                }
            }
            let rev = old.as_ref().map(|(_, r)| *r).unwrap_or(0);
            if rev != *base_revision {
                conflicts.push(field_conflict(
                    op,
                    "field_changed",
                    old.map(|(v, _)| v),
                    rev,
                ));
                return Ok(None);
            }
            let rev = append(
                tx,
                Stream::Preferences,
                field.as_str(),
                0,
                &Change::Preference {
                    field: *field,
                    value: value.clone(),
                },
            )?;
            tx.execute("INSERT INTO preferences VALUES(?,?,?) ON CONFLICT(field) DO UPDATE SET value=excluded.value,revision=excluded.revision",params![field.as_str(),value,rev]).map_err(Error::storage)?;
            Ok(Some(rev))
        }
    }
}
