//! Coordinates recoverable saves. No credential is ever written to a recovery file.
use crate::{
    error::{AppError, AppResult},
    memory::Memory,
    settings::{self, Settings},
    shortcut,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use zeroize::Zeroizing;

pub trait Services: Send + Sync {
    fn read_key(&self) -> AppResult<Option<Zeroizing<String>>>;
    fn write_key(&self, key: Option<&str>) -> AppResult<()>;
    fn register(&self, shortcut: &str) -> AppResult<()>;
    fn unregister(&self, shortcut: &str) -> AppResult<()>;
    fn prepare_shortcut(
        &self,
        _previous: &str,
        _next: &str,
    ) -> AppResult<Option<serde_json::Value>> {
        Ok(None)
    }
    fn prepare_recovered_shortcut(
        &self,
        previous: &str,
        next: &str,
    ) -> AppResult<Option<serde_json::Value>> {
        self.prepare_shortcut(previous, next)
    }
    fn apply_shortcut(&self, value: &serde_json::Value, rollback: bool) -> AppResult<()> {
        shortcut::apply_change(value, rollback)
    }
    // Fault-injection seam; native services never override this.
    fn checkpoint(&self, _point: &str) -> AppResult<()> {
        Ok(())
    }
}

pub struct NativeServices(pub tauri::AppHandle);
impl Services for NativeServices {
    fn read_key(&self) -> AppResult<Option<Zeroizing<String>>> {
        settings::read_key()
    }
    fn write_key(&self, key: Option<&str>) -> AppResult<()> {
        settings::write_key(key)
    }
    fn register(&self, value: &str) -> AppResult<()> {
        shortcut::register(&self.0, value)
    }
    fn prepare_shortcut(&self, previous: &str, next: &str) -> AppResult<Option<serde_json::Value>> {
        use tauri::Manager;
        let state = self.0.state::<crate::AppState>();
        shortcut::prepare_change(&self.0, &state.path, previous, next)
    }
    fn prepare_recovered_shortcut(
        &self,
        previous: &str,
        next: &str,
    ) -> AppResult<Option<serde_json::Value>> {
        #[cfg(target_os = "linux")]
        {
            use tauri::Manager;
            if self.0.state::<shortcut::Runtime>().backend == shortcut::Backend::Gnome {
                return serde_json::to_value(
                    shortcut::gnome::Desktop(&shortcut::gnome::System).prepare(Some(next))?,
                )
                .map(Some)
                .map_err(|_| recovery_error());
            }
        }
        self.prepare_shortcut(previous, next)
    }
    fn unregister(&self, value: &str) -> AppResult<()> {
        use tauri::Manager;
        if !self
            .0
            .state::<shortcut::Runtime>()
            .native_capable
            .load(Ordering::Acquire)
        {
            // Native grabs from a previous process/session no longer exist.
            return Ok(());
        }
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        if self
            .0
            .global_shortcut()
            .is_registered(shortcut::parse(value)?)
        {
            shortcut::unregister(&self.0, value)
        } else {
            Ok(())
        }
    }
}

pub fn marker(path: &Path) -> PathBuf {
    path.with_extension("recovery")
}
pub fn recovery_error() -> AppError {
    AppError::new(
        "settings_recovery",
        "设置恢复尚未完成，模型请求已暂停。请重新输入或明确删除密钥后保存设置。",
    )
}
pub fn ensure_ready(path: &Path, recovering: &AtomicBool) -> AppResult<()> {
    if recovering.load(Ordering::Acquire)
        || marker(path).exists()
        || shortcut::recovery_path(path).exists()
    {
        Err(recovery_error())
    } else {
        Ok(())
    }
}
fn file_error() -> AppError {
    AppError::new(
        "settings_write",
        "保存配置失败，请检查目录写入权限和磁盘空间。",
    )
}

struct Staged {
    path: PathBuf,
    tmp: PathBuf,
    previous: Option<Vec<u8>>,
    published: AtomicBool,
}
impl Staged {
    fn prepare(path: &Path, config: &Settings) -> AppResult<Self> {
        let parent = path.parent().ok_or_else(file_error)?;
        fs::create_dir_all(parent).map_err(|_| file_error())?;
        let previous = match fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(file_error()),
        };
        let tmp = parent.join(format!(".settings-{}.tmp", uuid::Uuid::new_v4()));
        let bytes = serde_json::to_vec_pretty(config).map_err(|_| file_error())?;
        fs::write(&tmp, bytes).map_err(|_| file_error())?;
        Ok(Self {
            path: path.into(),
            tmp,
            previous,
            published: AtomicBool::new(false),
        })
    }
    fn publish(&self) -> AppResult<()> {
        fs::rename(&self.tmp, &self.path).map_err(|_| file_error())?;
        self.published.store(true, Ordering::Release);
        Ok(())
    }
    fn restore(&self) -> AppResult<()> {
        if !self.published.load(Ordering::Acquire) {
            return Ok(());
        }
        match &self.previous {
            Some(bytes) => {
                fs::write(&self.tmp, bytes).map_err(|_| file_error())?;
                fs::rename(&self.tmp, &self.path).map_err(|_| file_error())
            }
            None => fs::remove_file(&self.path).map_err(|_| file_error()),
        }
    }
}
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.tmp);
    }
}

pub struct Outcome {
    pub has_api_key: bool,
    pub warnings: Vec<String>,
    pub invalidated: bool,
}

/// Caller holds the settings lock for the entire operation, including publication.
pub async fn commit(
    path: &Path,
    previous: &Settings,
    config: &Settings,
    replacement: Option<Zeroizing<String>>,
    remove_key: bool,
    db: Option<Memory>,
    services: Arc<dyn Services>,
    recovering: &AtomicBool,
) -> AppResult<Outcome> {
    config.validate()?;
    if shortcut::recovery_path(path).exists() {
        return Err(AppError::new(
            "desktop_shortcut",
            "请先在快捷键面板恢复桌面操作，再保存设置。",
        ));
    }
    if replacement.is_some() && remove_key {
        return Err(AppError::new("api_key", "不能同时设置和删除 API Key。"));
    }
    if let Some(key) = &replacement {
        settings::validate_key(key)?;
    }
    let was_recovering = ensure_ready(path, recovering).is_err();
    let recovery_path = marker(path);
    let old_marker = if was_recovering {
        fs::read(&recovery_path).unwrap_or_default()
    } else {
        vec![]
    };
    let undo_state: serde_json::Value =
        serde_json::from_slice(&old_marker).unwrap_or(serde_json::Value::Null);
    let prior_remember = undo_state["rememberTerms"]
        .as_bool()
        .unwrap_or(previous.remember_terms);
    let prior_auto = undo_state["autoAnalyze"]
        .as_bool()
        .unwrap_or(previous.auto_analyze);
    let invalidated = prior_remember != config.remember_terms || prior_auto != config.auto_analyze;
    if invalidated && db.is_none() {
        return Err(AppError::new(
            "memory_storage",
            "术语数据库不可用，无法安全更改记忆开关；其他设置仍可保存。",
        ));
    }
    if was_recovering && replacement.is_none() && !remove_key {
        return Err(recovery_error());
    }
    let shortcut_changed = undo_state["desktopShortcut"].is_object()
        || shortcut::parse(&previous.shortcut)? != shortcut::parse(&config.shortcut)?;
    let key_changed = replacement.is_some() || remove_key;
    services.checkpoint("stage")?;
    let stage = Arc::new(Staged::prepare(path, config)?);
    let old_key = if key_changed {
        services.read_key()?
    } else {
        None
    };
    if was_recovering {
        if let Some(token) = undo_state.get("desktopShortcut") {
            services
                .apply_shortcut(token, true)
                .map_err(|_| recovery_error())?;
        }
        // Preserve enough non-secret undo information to remove a reserved shortcut
        // whose earlier compensation failed. Across restart it may already be absent.
        {
            let value = &undo_state;
            if let Some(orphan) = value["reservedShortcut"].as_str() {
                if orphan != previous.shortcut {
                    services.unregister(orphan).map_err(|_| recovery_error())?;
                }
            }
        }
    }
    // A durable, non-secret gate also survives a compensation failure and restart.
    let desktop_change = if shortcut_changed {
        if was_recovering && undo_state["desktopShortcut"].is_object() {
            services.prepare_recovered_shortcut(&previous.shortcut, &config.shortcut)?
        } else {
            services.prepare_shortcut(&previous.shortcut, &config.shortcut)?
        }
    } else {
        None
    };
    let undo = serde_json::json!({"rememberTerms": prior_remember, "autoAnalyze": prior_auto, "reservedShortcut": if shortcut_changed && desktop_change.is_none() { Some(&config.shortcut) } else { None }, "desktopShortcut":desktop_change});
    shortcut::atomic_json(&recovery_path, &undo)?;
    let mut reserved = false;
    let mut desktop_attempted = false;
    let mut key_attempted = false;
    let result = async {
        if shortcut_changed {
            if let Some(token) = &desktop_change {
                desktop_attempted = true;
                services.apply_shortcut(token, false)?;
            } else {
                services.register(&config.shortcut)?;
                reserved = true;
            }
        }
        if key_changed {
            key_attempted = true; // A provider can fail after partially mutating its vault.
            services.write_key(replacement.as_ref().map(|k| k.as_str()))?;
        }
        if let Some(db) = &db {
            db.prepare_sync_preferences(crate::sync::storage::prefs(config))
                .await?;
        }
        let stage = stage.clone();
        let publisher = services.clone();
        let publish = move || {
            publisher.checkpoint("publish")?;
            stage.publish()?;
            publisher.checkpoint("memory_commit")
        };
        if let Some(db) = &db {
            if invalidated {
                services.checkpoint("memory_invalidate")?;
            }
            db.commit_synced_settings(
                publish,
                crate::sync::storage::prefs(config),
                invalidated,
                config.remember_terms,
            )
            .await?;
        } else {
            publish()?;
        }
        Ok::<_, AppError>(())
    }
    .await;
    if let Err(error) = result {
        let mut failed = services
            .checkpoint("restore_file")
            .and_then(|_| stage.restore())
            .is_err();
        if key_attempted
            && services
                .write_key(old_key.as_ref().map(|k| k.as_str()))
                .is_err()
        {
            failed = true;
        }
        if reserved && services.unregister(&config.shortcut).is_err() {
            failed = true;
        }
        if desktop_attempted
            && services
                .apply_shortcut(desktop_change.as_ref().unwrap(), true)
                .is_err()
        {
            failed = true;
        }
        if !failed {
            if let Some(db) = &db {
                if db.abandon_sync_preferences().await.is_err() {
                    failed = true;
                }
            }
        }
        if !failed && !was_recovering && fs::remove_file(&recovery_path).is_err() {
            failed = true;
        }
        if failed || was_recovering {
            recovering.store(true, Ordering::Release);
            return Err(recovery_error());
        }
        return Err(error);
    }
    // No fallible operation after here is allowed to report an uncommitted save.
    let mut warnings = vec![];
    if fs::remove_file(&recovery_path).is_err() {
        recovering.store(true, Ordering::Release);
        warnings.push("设置已提交，但恢复标记无法清除；请检查配置目录权限并重新保存设置。".into());
    } else {
        recovering.store(false, Ordering::Release);
    }
    if shortcut_changed && desktop_change.is_none() {
        if let Err(e) = services.unregister(&previous.shortcut) {
            warnings.push(e.message);
        }
    }
    let has_api_key = if key_changed {
        replacement.as_ref().is_some_and(|k| !k.is_empty())
    } else {
        match services.read_key() {
            Ok(key) => key.is_some_and(|k| !k.is_empty()),
            Err(e) => {
                warnings.push(e.message);
                false
            }
        }
    };
    Ok(Outcome {
        has_api_key,
        warnings,
        invalidated,
    })
}

#[cfg(test)]
mod tests;
