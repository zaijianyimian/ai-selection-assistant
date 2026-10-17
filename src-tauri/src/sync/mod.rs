//! Self-hosted sync. No calls into model/network analysis originate from this module.
pub mod storage;
mod transport;
use crate::{
    error::{AppError, AppResult},
    memory::Memory,
};
use asa_sync_protocol::*;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use storage::{err, load, save, Data};
use tauri::{Emitter, Manager};
use transport::{Http, Reply};
use zeroize::Zeroizing;
pub struct Runtime {
    pub gate: Arc<tokio::sync::Mutex<()>>,
    pub notify: Arc<tokio::sync::Notify>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            gate: Arc::new(tokio::sync::Mutex::new(())),
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }
}
fn database(app: &tauri::AppHandle) -> AppResult<Memory> {
    app.state::<crate::analysis::Runtime>().database()
}
fn entry(binding: &str) -> AppResult<keyring::Entry> {
    keyring::Entry::new("io.github.ai-selection-assistant.sync", binding).map_err(|_| vault_error())
}
fn vault_error() -> AppError {
    AppError::new(
        "sync_keyring",
        "系统凭据存储不可用；同步暂停，模型密钥不受影响。",
    )
}
fn token(binding: &str) -> AppResult<Zeroizing<String>> {
    entry(binding)?
        .get_password()
        .map(Zeroizing::new)
        .map_err(|_| vault_error())
}
fn write_token(binding: &str, value: &str) -> AppResult<()> {
    entry(binding)?
        .set_password(value)
        .map_err(|_| vault_error())
}
fn erase_token(binding: &str) -> AppResult<()> {
    match entry(binding)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(vault_error()),
    }
}
trait Credentials {
    fn write(&self, binding: &str, value: &str) -> AppResult<()>;
    fn remove(&self, binding: &str) -> AppResult<()>;
}
struct Vault;
impl Credentials for Vault {
    fn write(&self, binding: &str, value: &str) -> AppResult<()> {
        write_token(binding, value)
    }
    fn remove(&self, binding: &str) -> AppResult<()> {
        erase_token(binding)
    }
}
fn stage_credential(vault: &impl Credentials, binding: &str, value: &str) -> AppResult<()> {
    if let Err(e) = vault.write(binding, value) {
        undo_credential(vault, binding)?;
        return Err(e);
    }
    Ok(())
}
fn undo_credential(vault: &impl Credentials, binding: &str) -> AppResult<()> {
    vault.remove(binding).map_err(|_| {
        AppError::new(
            "sync_keyring_recovery",
            "新连接未生效，但临时凭据清理失败。旧连接保留，请检查系统凭据存储后重新登录。",
        )
    })
}
fn retry_at(now: i64, failures: u32, jitter: u8) -> i64 {
    now + ((5i64 * 2i64.saturating_pow(failures.saturating_sub(1).min(6))).min(300)
        + (jitter as i64) % 5)
        .min(300)
}
fn scheduled(d: &Data, now: i64) -> bool {
    d.enabled && d.background && d.status != "needs-auth" && d.next_attempt <= now
}
fn failed(d: &mut Data, e: AppError, now: i64, jitter: u8) {
    d.status = match e.code.as_str() {
        "sync_auth" | "sync_keyring" => "needs-auth",
        "sync_offline" => "offline",
        "sync_history" | "sync_cancelled" => "paused",
        _ => "error",
    }
    .into();
    if e.code == "sync_history" {
        d.enabled = false;
    }
    d.message = e.message;
    d.failures = d.failures.saturating_add(1);
    d.next_attempt = if let Some(retry) = e.retry_after {
        now + retry as i64
    } else {
        retry_at(now, d.failures, jitter)
    };
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub url: String,
    pub device_name: String,
    pub device_id: String,
    pub enabled: bool,
    pub background: bool,
    pub status: String,
    pub message: String,
    pub pending: usize,
    pub conflicts: Vec<ConflictView>,
    pub last_success: i64,
    pub discarded: usize,
    pub memory_paused: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictView {
    pub id: String,
    pub field: String,
    pub local: String,
    pub remote: Option<String>,
    pub revision: i64,
    pub code: String,
}
fn view(d: Data) -> View {
    let conflicts = d
        .queue
        .iter()
        .filter_map(|p| {
            p.conflict.as_ref().map(|c| {
                let (field, local) = match &p.op.action {
                    Action::Preference { field, value, .. } => {
                        (field.as_str().into(), value.clone())
                    }
                    Action::Feedback { key, value, .. } => (key.clone(), value.as_str().into()),
                    _ => ("术语生命周期".into(), "本机待提交记录".into()),
                };
                ConflictView {
                    id: p.op.id.clone(),
                    field,
                    local,
                    remote: c.remote_value.clone(),
                    revision: c.remote_revision,
                    code: c.code.clone(),
                }
            })
        })
        .collect();
    View {
        url: d.url,
        device_name: d.device_name,
        device_id: d.device_id,
        enabled: d.enabled,
        background: d.background,
        status: if d.status.is_empty() {
            "disabled".into()
        } else {
            d.status
        },
        message: d.message,
        pending: d.queue.len() + usize::from(d.reset.is_some()),
        conflicts,
        last_success: d.last_success,
        discarded: d.discarded,
        memory_paused: !d.memory_allowed,
    }
}
#[tauri::command]
pub async fn get_sync(app: tauri::AppHandle) -> AppResult<View> {
    database(&app)?
        .call(|db| Ok(view(load(db)?.unwrap_or_default())))
        .await
}
#[tauri::command]
pub async fn test_sync_connection(
    url: String,
    password: String,
    device_name: String,
) -> AppResult<String> {
    let secret = Zeroizing::new(password);
    validate_login(&secret, &device_name)?;
    let h = Http::new(&url, None)?;
    let c:Context=transport::unwrap(h.request(Method::POST,"v1/auth/check",Some(serde_json::json!({"protocol":VERSION,"password":secret.as_str(),"deviceName":device_name}))).await?)?;
    validate_context(&c)?;
    Ok("连接与密码验证成功；未注册设备、未上传数据。".into())
}
fn validate_login(p: &str, n: &str) -> AppResult<()> {
    if p.len() < 12
        || p.len() > 8192
        || p.chars().any(char::is_control)
        || n.trim().is_empty()
        || n.chars().count() > 80
        || n.chars().any(char::is_control)
    {
        return Err(AppError::new(
            "sync_login",
            "请输入至少 12 字节的应用同步密码和 1–80 字设备名称。",
        ));
    }
    Ok(())
}
fn validate_context(c: &Context) -> AppResult<()> {
    if c.protocol != VERSION
        || !valid_id(&c.workspace_id)
        || !valid_id(&c.history_id)
        || c.memory_generation < 1
        || c.revision < 0
    {
        return Err(AppError::new("sync_protocol", "同步协议无效或版本不兼容。"));
    }
    Ok(())
}
#[tauri::command]
pub async fn connect_sync(
    app: tauri::AppHandle,
    url: String,
    password: String,
    device_name: String,
    consent: bool,
    replace_target: bool,
    import_local: bool,
) -> AppResult<View> {
    let secret = Zeroizing::new(password);
    validate_login(&secret, &device_name)?;
    if !consent {
        return Err(AppError::new(
            "sync_consent",
            "请先确认同步范围和服务端可读取的数据。",
        ));
    }
    let _guard = app.state::<Runtime>().gate.clone().lock_owned().await;
    let url = transport::endpoint(&url)?;
    let db = database(&app)?;
    let old = db.call(|db| Ok(load(db)?.unwrap_or_default())).await?;
    if !old.url.is_empty() && old.url != url && !replace_target {
        return Err(AppError::new(
            "sync_target",
            "更换服务需要明确确认新的同步基线。",
        ));
    }
    let h = Http::new(&url, None)?;
    let enrolled:Enrollment=transport::unwrap(h.request(Method::POST,"v1/devices/enroll",Some(serde_json::json!({"protocol":VERSION,"password":secret.as_str(),"deviceName":device_name}))).await?)?;
    let enrolled_token = Zeroizing::new(enrolled.token);
    validate_context(&enrolled.context)?;
    if !valid_id(&enrolled.device_id)
        || enrolled_token.len() != 64
        || !enrolled_token.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(AppError::new("sync_protocol", "服务端返回的设备凭据无效。"));
    }
    let changed = old.context.as_ref().is_some_and(|c| {
        c.workspace_id != enrolled.context.workspace_id
            || c.history_id != enrolled.context.history_id
    }) || (!old.url.is_empty() && old.url != url);
    if changed && !replace_target {
        let _ = Http::new(&url, Some(enrolled_token))?
            .request::<serde_json::Value>(
                Method::DELETE,
                &format!("v1/devices/{}", enrolled.device_id),
                None,
            )
            .await;
        return Err(AppError::new(
            "sync_history",
            "工作区或备份历史改变。请确认新基线后重新登录，选择是否导入本机数据。",
        ));
    }
    let binding = uuid::Uuid::new_v4().to_string();
    if let Err(e) = stage_credential(&Vault, &binding, &enrolled_token) {
        let _ = erase_token(&binding);
        let _ = Http::new(&url, Some(enrolled_token))?
            .request::<serde_json::Value>(
                Method::DELETE,
                &format!("v1/devices/{}", enrolled.device_id),
                None,
            )
            .await;
        return Err(e);
    }
    let app_state = app.state::<crate::AppState>();
    let settings_guard = app_state.settings.lock().await;
    let config = settings_guard.clone();
    let name = device_name.trim().to_string();
    let ctx = enrolled.context.clone();
    let b = binding.clone();
    let provisional_device = enrolled.device_id.clone();
    let provisional_url = url.clone();
    let result = db
        .call(move |db| {
            storage::migrate(db)?;
            let tx = db.transaction().map_err(err)?;
            let mut d = if changed {
                Data::default()
            } else {
                load(&tx)?.unwrap_or_default()
            };
            if changed && !import_local {
                tx.execute("DELETE FROM terms", []).map_err(err)?;
                crate::memory::invalidate(&tx)?;
            }
            d.session += 1;
            d.binding = b;
            d.url = url;
            d.device_id = enrolled.device_id;
            d.device_name = name;
            d.context = Some(ctx);
            d.enabled = true;
            d.background = false;
            d.memory_allowed = config.remember_terms;
            d.status = "idle".into();
            d.message = String::new();
            d.next_attempt = 0;
            d.failures = 0;
            // Prepare local identities now; initial pull applies tombstones/reset before any push.
            storage::scan(&tx, &mut d)?;
            save(&tx, &d)?;
            storage::preferences(&tx, &storage::prefs(&config))?;
            tx.commit().map_err(err)?;
            Ok(())
        })
        .await;
    if let Err(e) = result {
        let cleanup = undo_credential(&Vault, &binding);
        let _ = Http::new(&provisional_url, Some(enrolled_token))?
            .request::<serde_json::Value>(
                Method::DELETE,
                &format!("v1/devices/{provisional_device}"),
                None,
            )
            .await;
        cleanup?;
        return Err(e);
    }
    drop(settings_guard);
    if !old.binding.is_empty() {
        if let Ok(old_token) = token(&old.binding) {
            if let Ok(old_http) = Http::new(&old.url, Some(old_token)) {
                let _ = old_http
                    .request::<serde_json::Value>(
                        Method::DELETE,
                        &format!("v1/devices/{}", old.device_id),
                        None,
                    )
                    .await;
            }
        }
        let _ = erase_token(&old.binding);
    }
    changed_event(&app);
    get_sync(app).await
}
#[tauri::command]
pub async fn configure_sync(
    app: tauri::AppHandle,
    enabled: bool,
    background: bool,
) -> AppResult<View> {
    let db = database(&app)?;
    let app_state = app.state::<crate::AppState>();
    let settings_guard = app_state.settings.lock().await;
    let config = settings_guard.clone();
    db.call(move |db| {
        let tx = db.transaction().map_err(err)?;
        let mut d = load(&tx)?.ok_or_else(|| AppError::new("sync_login", "请先登录同步服务。"))?;
        if enabled && d.binding.is_empty() {
            return Err(AppError::new("sync_login", "请先登录同步服务。"));
        }
        d.session += 1;
        d.enabled = enabled;
        d.background = background;
        d.memory_allowed = config.remember_terms;
        d.status = if enabled { "idle" } else { "disabled" }.into();
        d.next_attempt = 0;
        storage::scan(&tx, &mut d)?;
        save(&tx, &d)?;
        crate::memory::invalidate(&tx)?;
        tx.commit().map_err(err)
    })
    .await?;
    drop(settings_guard);
    changed_event(&app);
    get_sync(app).await
}
fn changed_event(app: &tauri::AppHandle) {
    let _ = app.emit("sync-changed", ());
}
pub fn changed(app: &tauri::AppHandle) {
    if let Some(r) = app.try_state::<Runtime>() {
        r.notify.notify_one();
    }
    changed_event(app);
}
async fn update(
    db: &Memory,
    binding: String,
    f: impl FnOnce(&mut Data) -> AppResult<()> + Send + 'static,
) -> AppResult<()> {
    db.call(move |db| {
        let tx = db.transaction().map_err(err)?;
        let mut d = load(&tx)?.ok_or_else(|| err("schema"))?;
        storage::ensure(&d, &binding)?;
        f(&mut d)?;
        save(&tx, &d)?;
        tx.commit().map_err(err)
    })
    .await
}
async fn snapshot(db: &Memory) -> AppResult<Data> {
    db.call(|db| load(db)?.ok_or_else(|| err("schema"))).await
}
async fn pull(db: &Memory, h: &Http, b: &str, stream: Stream) -> AppResult<()> {
    let mut until = None;
    loop {
        let d = snapshot(db).await?;
        storage::ensure(&d, b)?;
        if stream == Stream::Memory && !d.memory_allowed {
            return Ok(());
        }
        let after = if stream == Stream::Memory {
            d.memory_cursor
        } else {
            d.preference_cursor
        };
        let path = format!(
            "v1/changes?stream={}&after={after}&limit=100{}",
            stream.as_str(),
            until.map(|v| format!("&until={v}")).unwrap_or_default()
        );
        let p: Page = transport::unwrap(h.request(Method::GET, &path, None).await?)?;
        if p.stream != stream || until.is_some_and(|u| u != p.until) {
            return Err(err("stream"));
        }
        until = Some(p.until);
        let more = p.has_more;
        let b = b.to_string();
        db.call(move |db| storage::apply(db, &b, p)).await?;
        if !more {
            break;
        }
    }
    Ok(())
}
async fn push(db: &Memory, h: &Http, b: &str) -> AppResult<()> {
    loop {
        let b2 = b.to_string();
        let request = db
            .call(move |db| {
                let tx = db.transaction().map_err(err)?;
                let mut d = load(&tx)?.ok_or_else(|| err("schema"))?;
                storage::ensure(&d, &b2)?;
                let c = d.context.clone().unwrap();
                let memory = d.memory_allowed;
                let mut fields = std::collections::BTreeSet::new();
                let ops: Vec<_> = d
                    .queue
                    .iter_mut()
                    .filter(|p| {
                        p.conflict.is_none()
                            && (memory || p.op.action.stream() == Stream::Preferences)
                    })
                    .filter(|p| {
                        let field = match &p.op.action {
                            Action::Preference { field, .. } => {
                                Some(format!("pref:{}", field.as_str()))
                            }
                            Action::Feedback { key, .. } => Some(format!("feedback:{key}")),
                            _ => None,
                        };
                        field.is_none_or(|f| fields.insert(f))
                    })
                    .take(MAX_PUSH)
                    .map(|p| {
                        p.sent = true;
                        p.op.clone()
                    })
                    .collect();
                save(&tx, &d)?;
                tx.commit().map_err(err)?;
                Ok(Push {
                    protocol: VERSION,
                    workspace_id: c.workspace_id,
                    history_id: c.history_id,
                    operations: ops,
                })
            })
            .await?;
        if request.operations.is_empty() {
            break;
        }
        match h
            .request::<Accepted>(
                Method::POST,
                "v1/push",
                Some(serde_json::to_value(&request).map_err(err)?),
            )
            .await?
        {
            Reply::Ok(a) => {
                let ids = request
                    .operations
                    .iter()
                    .map(|o| o.id.clone())
                    .collect::<Vec<_>>();
                let binding = b.to_string();
                db.call(move |db| storage::acknowledge(db, &binding, &ids, a))
                    .await?;
            }
            Reply::Rejected(f, 409, _) if !f.conflicts.is_empty() => {
                update(db, b.into(), move |d| {
                    if let Some(c) = &f.context {
                        storage::check(d, c)?;
                    }
                    for c in f.conflicts {
                        let Some(p) = d.queue.iter_mut().find(|p| p.op.id == c.op_id) else {
                            return Err(err("conflict"));
                        };
                        p.conflict = Some(c);
                    }
                    Ok(())
                })
                .await?;
            }
            Reply::Rejected(_, status, retry) => {
                if status == 429 {
                    update(db, b.into(), move |d| {
                        d.next_attempt = crate::memory::now() + retry as i64;
                        Ok(())
                    })
                    .await?;
                }
                return Err(transport::rejection_after(status, retry));
            }
        }
    }
    Ok(())
}
async fn inbox(app: &tauri::AppHandle, db: &Memory) -> AppResult<()> {
    let state = app.state::<crate::AppState>();
    let mut current = state.settings.lock().await;
    crate::settings_commit::ensure_ready(&state.path, &state.recovering)?;
    let d = snapshot(db).await?;
    if d.inbox.is_empty() {
        return Ok(());
    }
    let ticket = storage::ticket(&d);
    let mut next = current.clone();
    storage::patch(&mut next, &d.inbox)?;
    next.validate()?;
    let path = state.path.clone();
    let published = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = published.clone();
    let config = next.clone();
    let values = d.inbox;
    let result = db
        .call(move |db| {
            let tx = db.transaction().map_err(err)?;
            let mut d = load(&tx)?.ok_or_else(|| err("schema"))?;
            storage::ensure(&d, &ticket)?;
            crate::settings::persist(&path, &config)?;
            flag.store(true, std::sync::atomic::Ordering::Release);
            for (k, v) in values {
                if d.inbox.get(&k) == Some(&v) {
                    d.inbox.remove(&k);
                }
            }
            save(&tx, &d)?;
            tx.commit().map_err(err)
        })
        .await;
    if published.load(std::sync::atomic::Ordering::Acquire) {
        *current = next;
        let _ = app.emit("settings-changed", ());
    }
    result
}
async fn cycle(app: &tauri::AppHandle, db: &Memory) -> AppResult<()> {
    let state = app.state::<crate::AppState>();
    crate::settings_commit::ensure_ready(&state.path, &state.recovering)?;
    let settings = state.settings.lock().await;
    let config = settings.clone();
    let d = snapshot(db).await?;
    let b = storage::ticket(&d);
    storage::ensure(&d, &b)?;
    update(db, b.clone(), move |d| {
        d.memory_allowed = config.remember_terms;
        d.status = "syncing".into();
        d.message.clear();
        Ok(())
    })
    .await?;
    drop(settings);
    changed_event(app);
    let secret = token(&d.binding)?;
    let h = Http::new(&d.url, Some(secret))?;
    pull(db, &h, &b, Stream::Preferences).await?;
    pull(db, &h, &b, Stream::Memory).await?;
    let b2 = b.clone();
    db.call(move |db| {
        let tx = db.transaction().map_err(err)?;
        let mut d = load(&tx)?.unwrap();
        storage::ensure(&d, &b2)?;
        storage::scan(&tx, &mut d)?;
        let intent = d.intent.take();
        save(&tx, &d)?;
        if let Some(prefs) = intent {
            storage::preferences(&tx, &prefs)?;
        }
        tx.commit().map_err(err)
    })
    .await?;
    let latest = snapshot(db).await?;
    if latest.memory_allowed {
        if let Some(reset) = latest.reset {
            let _: Accepted = transport::unwrap(
                h.request(
                    Method::POST,
                    "v1/memory/reset",
                    Some(serde_json::to_value(reset).map_err(err)?),
                )
                .await?,
            )?;
            pull(db, &h, &b, Stream::Memory).await?;
        }
    }
    push(db, &h, &b).await?;
    pull(db, &h, &b, Stream::Preferences).await?;
    pull(db, &h, &b, Stream::Memory).await?;
    inbox(app, db).await?;
    update(db, b, |d| {
        d.status = if d.queue.iter().any(|p| p.conflict.is_some()) {
            "conflict"
        } else if !d.memory_allowed {
            "paused"
        } else {
            "idle"
        }
        .into();
        d.last_success = crate::memory::now();
        d.failures = 0;
        d.next_attempt = 0;
        Ok(())
    })
    .await?;
    Ok(())
}
#[tauri::command]
pub async fn sync_now(app: tauri::AppHandle) -> AppResult<View> {
    let _guard = app
        .state::<Runtime>()
        .gate
        .clone()
        .try_lock_owned()
        .map_err(|_| AppError::new("sync_busy", "已有同步正在进行。"))?;
    let db = database(&app)?;
    let d = snapshot(&db).await?;
    storage::ensure(&d, &storage::ticket(&d))?;
    if d.status == "needs-auth" {
        return Err(transport::rejection(401));
    }
    if d.next_attempt > crate::memory::now() {
        return Err(AppError::new("sync_backoff", "同步正在等待重试，请稍后。"));
    }
    changed_event(&app);
    let result = tokio::time::timeout(std::time::Duration::from_secs(30), cycle(&app, &db))
        .await
        .unwrap_or_else(|_| Err(transport::network()));
    if let Err(ref e) = result {
        let e = e.clone();
        let _ = update(&db, storage::ticket(&d), move |d| {
            failed(
                d,
                e,
                crate::memory::now(),
                uuid::Uuid::new_v4().as_bytes()[0],
            );
            Ok(())
        })
        .await;
    }
    let _ = app.emit("memory-changed", ());
    changed_event(&app);
    result?;
    get_sync(app).await
}
#[tauri::command]
pub async fn sync_devices(app: tauri::AppHandle) -> AppResult<Vec<Device>> {
    let d = snapshot(&database(&app)?).await?;
    storage::ensure(&d, &storage::ticket(&d))?;
    transport::unwrap(
        Http::new(&d.url, Some(token(&d.binding)?))?
            .request(Method::GET, "v1/devices", None)
            .await?,
    )
}
#[tauri::command]
pub async fn revoke_sync_device(app: tauri::AppHandle, id: String) -> AppResult<()> {
    if !valid_id(&id) {
        return Err(err("id"));
    }
    let db = database(&app)?;
    let d = snapshot(&db).await?;
    storage::ensure(&d, &storage::ticket(&d))?;
    let _: serde_json::Value = transport::unwrap(
        Http::new(&d.url, Some(token(&d.binding)?))?
            .request(Method::DELETE, &format!("v1/devices/{id}"), None)
            .await?,
    )?;
    if id == d.device_id {
        let _ = erase_token(&d.binding);
        update(&db, storage::ticket(&d), |d| {
            d.enabled = false;
            d.status = "needs-auth".into();
            Ok(())
        })
        .await?;
    }
    changed_event(&app);
    Ok(())
}
#[tauri::command]
pub async fn resolve_sync_conflict(
    app: tauri::AppHandle,
    id: String,
    keep_local: bool,
) -> AppResult<View> {
    let db = database(&app)?;
    db.call(move |db| resolve_value(db, id, keep_local)).await?;
    inbox(&app, &db).await?;
    crate::analysis::changed(&app);
    get_sync(app).await
}
#[tauri::command]
pub async fn clear_sync_memory(
    app: tauri::AppHandle,
    confirm_all_devices: bool,
) -> AppResult<View> {
    if !confirm_all_devices {
        return Err(AppError::new("sync_confirm", "清空所有设备需要明确确认。"));
    }
    let db = database(&app)?;
    db.call(|db| {
        let tx = db.transaction().map_err(err)?;
        let mut d = load(&tx)?.ok_or_else(|| err("schema"))?;
        storage::ensure(&d, &storage::ticket(&d))?;
        let c = d.context.as_ref().unwrap();
        if d.reset.is_none() {
            d.reset = Some(Reset {
                protocol: VERSION,
                workspace_id: c.workspace_id.clone(),
                history_id: c.history_id.clone(),
                id: uuid::Uuid::new_v4().to_string(),
                generation: c.memory_generation,
            });
        }
        d.session += 1;
        d.status = "idle".into();
        d.queue.retain(|p| p.op.action.stream() != Stream::Memory);
        d.entities.clear();
        d.events.clear();
        tx.execute("DELETE FROM terms", []).map_err(err)?;
        crate::memory::invalidate(&tx)?;
        save(&tx, &d)?;
        tx.commit().map_err(err)
    })
    .await?;
    crate::analysis::changed(&app);
    changed(&app);
    get_sync(app).await
}
pub fn schedule(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Ok(db) = database(&app) else {
            return;
        };
        let config = app.state::<crate::AppState>().settings.lock().await.clone();
        let p = storage::prefs(&config);
        let _ = db
            .call(move |db| {
                if let Some(mut d) = load(db)? {
                    if d.intent.as_ref() == Some(&p) {
                        storage::preferences(db, &p)?;
                    } else {
                        d.intent = None;
                        save(db, &d)?;
                    }
                }
                Ok(())
            })
            .await;
        loop {
            let notify = app.state::<Runtime>().notify.clone();
            let waiting = snapshot(&db).await.ok();
            let now = crate::memory::now();
            let interval = waiting
                .as_ref()
                .filter(|d| d.enabled && d.background && d.status != "needs-auth" && d.failures > 0)
                .map(|d| (d.next_attempt - now).clamp(1, 60) as u64)
                .unwrap_or(60);
            let debounce = tokio::select! {_=tokio::time::sleep(std::time::Duration::from_secs(interval))=>false,_=notify.notified()=>true};
            if debounce {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            let Ok(d) = snapshot(&db).await else {
                continue;
            };
            if scheduled(&d, crate::memory::now()) {
                let _ = sync_now(app.clone()).await;
            }
        }
    });
}
#[cfg(test)]
mod tests;

fn resolve_value(db: &mut rusqlite::Connection, id: String, keep_local: bool) -> AppResult<()> {
    let tx = db.transaction().map_err(err)?;
    let mut d = load(&tx)?.ok_or_else(|| err("schema"))?;
    let index = d
        .queue
        .iter()
        .position(|p| p.op.id == id && p.conflict.is_some())
        .ok_or_else(|| err("conflict"))?;
    let p = d.queue.remove(index);
    let c = p.conflict.unwrap();
    if keep_local {
        let action = match p.op.action {
            Action::Preference { field, value, .. } => Action::Preference {
                field,
                value,
                base_revision: c.remote_revision,
            },
            Action::Feedback {
                key,
                incarnation,
                value,
                ..
            } if c.code == "field_changed" => Action::Feedback {
                key,
                incarnation,
                value,
                base_revision: c.remote_revision,
            },
            _ => {
                return Err(AppError::new(
                    "sync_deleted",
                    "远端已删除或清空；请接受远端结果。之后的新翻译可重新积累。",
                ))
            }
        };
        storage::enqueue(&mut d, action);
    } else {
        match p.op.action {
            Action::Preference { field, .. } => {
                if let Some(v) = c.remote_value {
                    if !field.valid(&v) {
                        return Err(err("pref"));
                    }
                    d.inbox.insert(field.as_str().into(), v);
                }
            }
            Action::Feedback { key, .. } => {
                if let Some(id) = d.entities.get(&key).and_then(|e| e.id) {
                    if let Some(v) = c.remote_value {
                        let f = serde_json::from_value(v.into()).map_err(err)?;
                        tx.execute("INSERT INTO events(job,term_id,at,source,kind,value) VALUES(?,?,?,'sync','feedback',?)",rusqlite::params![uuid::Uuid::new_v4().to_string(),id,crate::memory::now(),asa_sync_protocol::Familiarity::as_str(f)]).map_err(err)?;
                        if let Some(e) = d.entities.get_mut(&key) {
                            e.last_local_feedback = f.as_str().into();
                        }
                    }
                }
            }
            _ => {}
        }
    }
    d.next_attempt = 0;
    save(&tx, &d)?;
    tx.commit().map_err(err)
}
