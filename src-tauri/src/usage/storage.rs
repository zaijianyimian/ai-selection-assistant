use super::{Aggregate, Attempt, ClockStamp, Counts, Group, Query, Summary};
use crate::error::{AppError, AppResult};
use rusqlite::{params, Connection, OptionalExtension};
use std::{path::Path, time::Duration};

pub fn storage_error(_: impl std::fmt::Debug) -> AppError {
    AppError::new(
        "usage_storage",
        "用量统计暂不可用或未完整保存；请检查磁盘空间和数据目录。翻译仍可使用，未重发模型请求。",
    )
}
fn counter(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let n: i64 = row.get(index)?;
    u64::try_from(n).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, n))
}
pub struct Ledger {
    db: Connection,
}
impl Ledger {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(storage_error)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                    .map_err(storage_error)?;
            }
        }
        // Explicit create permissions prevent a world-readable window before SQLite opens it.
        let mut options = std::fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path).map_err(storage_error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(storage_error)?;
        }
        let db = Connection::open(path).map_err(storage_error)?;
        db.busy_timeout(Duration::from_millis(100))
            .map_err(storage_error)?;
        let version: i64 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(storage_error)?;
        if version > 1 {
            return Err(storage_error(()));
        }
        if version == 0 {
            db.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE metadata (id INTEGER PRIMARY KEY CHECK(id=1), generation INTEGER NOT NULL, since INTEGER NOT NULL, revision INTEGER NOT NULL, incomplete INTEGER NOT NULL);
                INSERT INTO metadata VALUES(1,1,strftime('%s','now'),0,0);
                CREATE TABLE attempts (id TEXT PRIMARY KEY, operation TEXT NOT NULL, generation INTEGER NOT NULL, service TEXT NOT NULL, label TEXT NOT NULL, model TEXT NOT NULL, purpose TEXT NOT NULL, utc INTEGER NOT NULL, date TEXT NOT NULL, offset INTEGER NOT NULL, final INTEGER NOT NULL DEFAULT 0, counts TEXT);
                CREATE TABLE daily_usage (date TEXT NOT NULL, service TEXT NOT NULL, label TEXT NOT NULL, model TEXT NOT NULL, purpose TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(date,service,model,purpose));
                PRAGMA user_version=1; COMMIT;").map_err(storage_error)?;
        }
        Ok(Self { db })
    }
    pub fn metadata(&self) -> AppResult<(u64, i64, u64, bool)> {
        self.db
            .query_row(
                "SELECT generation,since,revision,incomplete FROM metadata WHERE id=1",
                [],
                |r| Ok((counter(r, 0)?, r.get(1)?, counter(r, 2)?, r.get(3)?)),
            )
            .map_err(storage_error)
    }
    pub fn begin(&self, a: &Attempt) -> AppResult<()> {
        if a.generation != self.metadata()?.0 {
            return Ok(());
        }
        self.db.execute("INSERT OR IGNORE INTO attempts(id,operation,generation,service,label,model,purpose,utc,date,offset) VALUES(?,?,?,?,?,?,?,?,?,?)",
            params![a.id,a.operation,a.generation as i64,a.service,a.label,a.model,a.purpose,a.stamp.utc,a.stamp.date,a.stamp.offset]).map_err(storage_error)?;
        self.db
            .execute("UPDATE metadata SET revision=revision+1 WHERE id=1", [])
            .map_err(storage_error)?;
        Ok(())
    }
    pub fn finish(&mut self, id: &str, generation: u64, counts: &Counts) -> AppResult<()> {
        let tx = self.db.transaction().map_err(storage_error)?;
        let current: u64 = tx
            .query_row("SELECT generation FROM metadata WHERE id=1", [], |r| {
                counter(r, 0)
            })
            .map_err(storage_error)?;
        if current != generation {
            return Ok(());
        }
        let row: Option<(String,String,String,String,String)> = tx.query_row("SELECT date,service,label,model,purpose FROM attempts WHERE id=? AND generation=? AND final=0",params![id,generation as i64],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(storage_error)?;
        let Some((date, service, label, model, purpose)) = row else {
            return Ok(());
        };
        let old: Option<String> = tx
            .query_row(
                "SELECT data FROM daily_usage WHERE date=? AND service=? AND model=? AND purpose=?",
                params![date, service, model, purpose],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        let mut sum: Aggregate = match old {
            Some(s) => serde_json::from_str(&s).map_err(storage_error)?,
            None => Aggregate::default(),
        };
        sum.add(counts);
        tx.execute("INSERT INTO daily_usage VALUES(?,?,?,?,?,?) ON CONFLICT(date,service,model,purpose) DO UPDATE SET data=excluded.data",params![date,service,label,model,purpose,serde_json::to_string(&sum).map_err(storage_error)?]).map_err(storage_error)?;
        tx.execute(
            "UPDATE attempts SET final=1,counts=? WHERE id=?",
            params![serde_json::to_string(counts).map_err(storage_error)?, id],
        )
        .map_err(storage_error)?;
        tx.execute("UPDATE metadata SET revision=revision+1 WHERE id=1", [])
            .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }
    pub fn recover(&mut self, now: i64) -> AppResult<()> {
        let rows = {
            let mut s = self
                .db
                .prepare("SELECT id,generation FROM attempts WHERE final=0")
                .map_err(storage_error)?;
            let rows = s
                .query_map([], |r| Ok((r.get::<_, String>(0)?, counter(r, 1)?)))
                .map_err(storage_error)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(storage_error)?
        };
        for (id, generation) in rows {
            self.finish(&id, generation, &Counts::default())?;
        }
        self.prune(now)
    }
    pub fn prune(&self, now: i64) -> AppResult<()> {
        self.db
            .execute(
                "DELETE FROM attempts WHERE final=1 AND utc < ?",
                [now.saturating_sub(7 * 86400)],
            )
            .map_err(storage_error)?;
        Ok(())
    }
    pub fn mark_incomplete(&self) -> AppResult<()> {
        self.db
            .execute(
                "UPDATE metadata SET incomplete=1,revision=revision+1 WHERE id=1 AND incomplete=0",
                [],
            )
            .map_err(storage_error)?;
        Ok(())
    }
    pub fn clear(&mut self, now: i64) -> AppResult<u64> {
        let tx = self.db.transaction().map_err(storage_error)?;
        tx.execute("UPDATE metadata SET generation=generation+1,since=?,revision=revision+1,incomplete=0 WHERE id=1",[now]).map_err(storage_error)?;
        tx.execute("DELETE FROM daily_usage", [])
            .map_err(storage_error)?;
        tx.execute("DELETE FROM attempts", [])
            .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(self.metadata()?.0)
    }
    pub fn query(&self, q: &Query, stamp: &ClockStamp) -> AppResult<Summary> {
        let (generation, since, revision, incomplete) = self.metadata()?;
        let mut view = Summary {
            generation,
            since,
            revision,
            incomplete,
            today: Aggregate::default(),
            month: Aggregate::default(),
            cumulative: Aggregate::default(),
            groups: vec![],
            group_count: 0,
            date: stamp.date.clone(),
        };
        // Aggregate one bounded DB row at a time; no permanent per-request scan.
        let mut statement = self.db.prepare("SELECT date,service,label,model,purpose,data FROM daily_usage WHERE (? IS NULL OR service=?) AND (? IS NULL OR model=?) AND (? IS NULL OR purpose=?) ORDER BY service,model,purpose,date").map_err(storage_error)?;
        let rows = statement
            .query_map(
                params![q.service, q.service, q.model, q.model, q.purpose, q.purpose],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                },
            )
            .map_err(storage_error)?;
        let mut current: Option<Group> = None;
        for row in rows {
            let (date, service, label, model, purpose, data) = row.map_err(storage_error)?;
            let a: Aggregate = serde_json::from_str(&data).map_err(storage_error)?;
            view.cumulative.merge(&a);
            if date == stamp.date {
                view.today.merge(&a);
            }
            if date.get(..7) == stamp.date.get(..7) {
                view.month.merge(&a);
            }
            if current
                .as_ref()
                .is_some_and(|g| g.service != service || g.model != model || g.purpose != purpose)
            {
                Self::page_group(&mut view, q, current.take().unwrap());
            }
            let group = current.get_or_insert_with(|| Group {
                service,
                label,
                model,
                purpose,
                usage: Aggregate::default(),
            });
            group.usage.merge(&a);
        }
        if let Some(group) = current {
            Self::page_group(&mut view, q, group);
        }
        let pending: u64 = self.db.query_row("SELECT count(*) FROM attempts WHERE final=0 AND (? IS NULL OR service=?) AND (? IS NULL OR model=?) AND (? IS NULL OR purpose=?)",params![q.service,q.service,q.model,q.model,q.purpose,q.purpose],|r|counter(r,0)).map_err(storage_error)?;
        view.cumulative.pending = pending;
        for (target, condition) in [
            (&mut view.today, "date=?"),
            (&mut view.month, "substr(date,1,7)=substr(?,1,7)"),
        ] {
            target.pending = self.db.query_row(&format!("SELECT count(*) FROM attempts WHERE final=0 AND {condition} AND (? IS NULL OR service=?) AND (? IS NULL OR model=?) AND (? IS NULL OR purpose=?)"),params![stamp.date,q.service,q.service,q.model,q.model,q.purpose,q.purpose],|r|counter(r,0)).map_err(storage_error)?;
        }
        Ok(view)
    }
    fn page_group(view: &mut Summary, q: &Query, group: Group) {
        let n = view.group_count;
        view.group_count += 1;
        if n >= q.offset && view.groups.len() < q.limit.clamp(1, 100) as usize {
            view.groups.push(group);
        }
    }
}

#[cfg(test)]
mod tests;
