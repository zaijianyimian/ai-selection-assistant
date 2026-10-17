//! Local numeric accounting only. This module never receives prompts or keys.
mod storage;
mod types;
use crate::{error::AppResult, settings::Settings};
use chrono::{Local, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::Emitter;
pub use types::*;

#[derive(Clone)]
pub struct ClockStamp {
    pub utc: i64,
    pub date: String,
    pub offset: i32,
}
pub trait Clock: Send + Sync {
    fn stamp(&self) -> ClockStamp;
}
struct SystemClock;
impl Clock for SystemClock {
    fn stamp(&self) -> ClockStamp {
        let now = Local::now();
        ClockStamp {
            utc: now.timestamp(),
            date: now.format("%Y-%m-%d").to_string(),
            offset: now.offset().local_minus_utc(),
        }
    }
}
#[derive(Clone)]
pub(crate) struct Attempt {
    id: String,
    operation: String,
    generation: u64,
    service: String,
    label: String,
    model: String,
    purpose: String,
    stamp: ClockStamp,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    pub service: Option<String>,
    pub model: Option<String>,
    pub purpose: Option<String>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "page_size")]
    pub limit: u32,
}
impl Default for Query {
    fn default() -> Self {
        Self {
            service: None,
            model: None,
            purpose: None,
            offset: 0,
            limit: 20,
        }
    }
}
fn page_size() -> u32 {
    20
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub service: String,
    pub label: String,
    pub model: String,
    pub purpose: String,
    pub usage: Aggregate,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub generation: u64,
    pub revision: u64,
    pub since: i64,
    pub date: String,
    pub incomplete: bool,
    pub today: Aggregate,
    pub month: Aggregate,
    pub cumulative: Aggregate,
    pub groups: Vec<Group>,
    pub group_count: u32,
}
type Work = Box<dyn FnOnce(&mut storage::Ledger) + Send>;
struct Shared {
    generation: AtomicU64,
    incomplete: AtomicBool,
    notify: Mutex<Option<tauri::AppHandle>>,
}
impl Shared {
    fn changed(&self) {
        if let Ok(app) = self.notify.lock() {
            if let Some(app) = app.as_ref() {
                let _ = app.emit("usage-changed", ());
            }
        }
    }
}
#[derive(Clone)]
pub struct Runtime {
    sender: Option<SyncSender<Work>>,
    shared: Arc<Shared>,
    clock: Arc<dyn Clock>,
}
impl Runtime {
    pub fn open(path: PathBuf) -> Self {
        Self::with_clock(path, Arc::new(SystemClock))
    }
    fn with_clock(path: PathBuf, clock: Arc<dyn Clock>) -> Self {
        let ledger = storage::Ledger::open(&path).and_then(|mut db| {
            db.recover(clock.stamp().utc)?;
            Ok(db)
        });
        let (generation, incomplete) = ledger
            .as_ref()
            .ok()
            .and_then(|d| d.metadata().ok())
            .map(|m| (m.0, m.3))
            .unwrap_or((1, true));
        let shared = Arc::new(Shared {
            generation: AtomicU64::new(generation),
            incomplete: AtomicBool::new(incomplete),
            notify: Mutex::new(None),
        });
        let sender = ledger.ok().map(|mut db| {
            let (tx, rx) = mpsc::sync_channel::<Work>(128);
            let shared = shared.clone();
            std::thread::spawn(move || {
                while let Ok(work) = rx.recv() {
                    work(&mut db);
                    if shared.incomplete.load(Ordering::Acquire) {
                        let _ = db.mark_incomplete();
                    }
                }
            });
            tx
        });
        Self {
            sender,
            shared,
            clock,
        }
    }
    pub fn attach(&self, app: tauri::AppHandle) {
        *self.shared.notify.lock().unwrap_or_else(|e| e.into_inner()) = Some(app);
    }
    pub fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::Acquire)
    }
    fn submit(&self, work: Work) -> bool {
        if self
            .sender
            .as_ref()
            .is_some_and(|s| s.try_send(work).is_ok())
        {
            true
        } else {
            self.shared.incomplete.store(true, Ordering::Release);
            false
        }
    }
    pub fn operation(&self, config: &Settings, purpose: Purpose) -> Operation {
        let endpoint = crate::llm::normalize_endpoint(&config.base_url).ok();
        let service = endpoint
            .as_ref()
            .map(|u| format!("{:x}", Sha256::digest(u.as_str().as_bytes())))
            .unwrap_or_default();
        let label = endpoint
            .as_ref()
            .map(|u| {
                format!(
                    "{}{}",
                    u.host_str().unwrap_or(""),
                    u.port().map(|p| format!(":{p}")).unwrap_or_default()
                )
            })
            .unwrap_or_else(|| "未发送".into());
        Operation {
            runtime: self.clone(),
            service,
            label,
            model: config.model.trim().chars().take(200).collect(),
            inner: Arc::new(Mutex::new(Snapshot {
                operation_id: uuid::Uuid::new_v4().to_string(),
                generation: self.generation(),
                source: "provider",
                purpose,
                usage: Aggregate::default(),
                persistence_incomplete: self.shared.incomplete.load(Ordering::Acquire),
                excluded_after_reset: false,
            })),
        }
    }
    pub async fn query(&self, q: Query) -> AppResult<Summary> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let stamp = self.clock.stamp();
        if !self.submit(Box::new(move |db| {
            let _ = tx.send(db.query(&q, &stamp));
        })) {
            return Err(storage::storage_error(()));
        }
        let mut view = tokio::time::timeout(Duration::from_secs(2), rx)
            .await
            .map_err(storage::storage_error)?
            .map_err(storage::storage_error)??;
        view.incomplete |= self.shared.incomplete.load(Ordering::Acquire);
        Ok(view)
    }
    pub async fn clear(&self) -> AppResult<Summary> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let now = self.clock.stamp().utc;
        let shared = self.shared.clone();
        if !self.submit(Box::new(move |db| {
            let result = db.clear(now).map(|g| {
                shared.incomplete.store(false, Ordering::Release);
                shared.generation.store(g, Ordering::Release);
                shared.changed();
            });
            let _ = tx.send(result);
        })) {
            return Err(storage::storage_error(()));
        }
        tokio::time::timeout(Duration::from_secs(2), rx)
            .await
            .map_err(storage::storage_error)?
            .map_err(storage::storage_error)??;
        self.query(Query {
            limit: 20,
            ..Query::default()
        })
        .await
    }
    #[cfg(test)]
    pub(crate) async fn settled(&self) -> Summary {
        // Model completion only enqueues receipts. Stress tests must await the
        // worker, including slow Windows fsyncs, without changing the UI's 2s cap.
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Ok(summary) = self.query(Query::default()).await {
                    return summary;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("statistics did not settle within the bounded test deadline")
    }
}
#[derive(Clone)]
pub struct Operation {
    runtime: Runtime,
    service: String,
    label: String,
    model: String,
    inner: Arc<Mutex<Snapshot>>,
}
impl Operation {
    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = self.inner.lock().unwrap_or_else(|e| e.into_inner()).clone();
        snapshot.excluded_after_reset = snapshot.generation != self.runtime.generation();
        snapshot.persistence_incomplete |= self.runtime.shared.incomplete.load(Ordering::Acquire);
        snapshot
    }
    pub fn cache_hit(&self) -> Snapshot {
        let mut snapshot = self.snapshot();
        snapshot.source = "local-cache";
        for metric in [
            &mut snapshot.usage.input,
            &mut snapshot.usage.output,
            &mut snapshot.usage.total,
        ] {
            metric.tokens = Some(0);
        }
        snapshot
    }
    pub fn begin(&self) -> Ticket {
        let attempt = {
            let mut snapshot = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            snapshot.usage.pending += 1;
            Attempt {
                id: uuid::Uuid::new_v4().to_string(),
                operation: snapshot.operation_id.clone(),
                generation: snapshot.generation,
                service: self.service.clone(),
                label: self.label.clone(),
                model: self.model.clone(),
                purpose: snapshot.purpose.label().into(),
                stamp: self.runtime.clock.stamp(),
            }
        };
        let a = attempt.clone();
        let shared = self.runtime.shared.clone();
        self.runtime.submit(Box::new(move |db| {
            if db.begin(&a).is_err() {
                shared.incomplete.store(true, Ordering::Release);
            }
            shared.changed();
        }));
        Ticket {
            operation: self.clone(),
            attempt: Some(attempt),
        }
    }
}
pub struct Ticket {
    operation: Operation,
    attempt: Option<Attempt>,
}
impl Ticket {
    pub fn finish(&mut self, counts: Counts) {
        let Some(a) = self.attempt.take() else {
            return;
        };
        {
            let mut snapshot = self
                .operation
                .inner
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            snapshot.usage.pending = snapshot.usage.pending.saturating_sub(1);
            snapshot.usage.add(&counts);
        }
        let shared = self.operation.runtime.shared.clone();
        self.operation.runtime.submit(Box::new(move |db| {
            if db.finish(&a.id, a.generation, &counts).is_err() {
                shared.incomplete.store(true, Ordering::Release);
            }
            let _ = db.prune(Utc::now().timestamp());
            shared.changed();
        }));
    }
}
impl Drop for Ticket {
    fn drop(&mut self) {
        self.finish(Counts::default());
    }
}

#[tauri::command]
pub async fn get_usage(
    state: tauri::State<'_, Runtime>,
    query: Option<Query>,
) -> AppResult<Summary> {
    state
        .query(query.unwrap_or(Query {
            limit: 20,
            ..Query::default()
        }))
        .await
}
#[tauri::command]
pub async fn clear_usage(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    confirm: bool,
) -> AppResult<Summary> {
    if !confirm {
        return Err(crate::error::AppError::new(
            "usage_confirmation",
            "请先确认仅清空本机用量统计。",
        ));
    }
    let summary = state.clear().await?;
    let _ = app.emit("usage-changed", ());
    Ok(summary)
}

#[cfg(test)]
mod tests;
