mod analysis;
mod cache;
mod error;
mod llm;
mod memory;
mod search;
mod selection;
mod settings;
mod settings_commit;
mod shortcut;
mod sync;
mod translation;
mod window;

use error::{AppError, AppResult};
use serde::Serialize;
use settings::{Settings, SettingsView};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::{Emitter, Manager, State};
use zeroize::Zeroizing;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestOptions {
    pub target_language: String,
    pub explain_terms: bool,
}
impl From<&Settings> for RequestOptions {
    fn from(config: &Settings) -> Self {
        Self {
            target_language: config.target_language.clone(),
            explain_terms: config.explain_terms,
        }
    }
}

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub revision: u64,
    pub request_options: Option<RequestOptions>,
    pub status: String,
    pub original: String,
    pub source: String,
    pub warning: Option<String>,
    pub result: Option<translation::Translation>,
    pub error: Option<AppError>,
    pub cache_hit: bool,
}

pub struct AppState {
    settings: tokio::sync::Mutex<Settings>,
    path: PathBuf,
    warnings: Mutex<Vec<String>>,
    job: Mutex<Job>,
    busy: AtomicBool,
    recovering: std::sync::Arc<AtomicBool>,
    cache: Mutex<cache::Cache>,
}

fn lock_error() -> AppError {
    AppError::new("state", "应用状态不可用，请重启应用。")
}

async fn vault_key() -> AppResult<Option<Zeroizing<String>>> {
    tauri::async_runtime::spawn_blocking(settings::read_key)
        .await
        .map_err(|_| lock_error())?
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> AppResult<SettingsView> {
    let settings = state.settings.lock().await.clone();
    let mut warnings = state.warnings.lock().map_err(|_| lock_error())?.clone();
    let has_api_key = match vault_key().await {
        Ok(key) => key.is_some_and(|k| !k.is_empty()),
        Err(error) => {
            warnings.push(error.message);
            false
        }
    };
    if settings_commit::ensure_ready(&state.path, &state.recovering).is_err() {
        warnings.push(settings_commit::recovery_error().message);
    }
    Ok(SettingsView {
        settings,
        has_api_key,
        warnings,
    })
}

#[tauri::command]
async fn save_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    mut config: Settings,
    api_key: Option<String>,
    remove_api_key: bool,
) -> AppResult<SettingsView> {
    let replacement = api_key.map(Zeroizing::new);
    if replacement.is_some() && remove_api_key {
        return Err(AppError::new("api_key", "不能同时设置和删除 API Key。"));
    }
    config.base_url = config.base_url.trim().to_owned();
    config.model = config.model.trim().to_owned();
    config.shortcut = config.shortcut.trim().to_owned();
    config.validate()?;
    if let Some(key) = &replacement {
        settings::validate_key(key)?;
    }
    let mut current = state.settings.lock().await;
    let shortcut_changed =
        shortcut::parse(&config.shortcut)? != shortcut::parse(&current.shortcut)?;
    let old = current.clone();
    let next = config.clone();
    let path = state.path.clone();
    let db = app.state::<analysis::Runtime>().db.clone();
    let recovering = state.recovering.clone();
    let services = std::sync::Arc::new(settings_commit::NativeServices(app.clone()));
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        tauri::async_runtime::block_on(settings_commit::commit(
            &path,
            &old,
            &next,
            replacement,
            remove_api_key,
            db,
            services,
            &recovering,
        ))
    })
    .await
    .map_err(|_| lock_error())??;
    *current = config.clone();
    // Also discard results from requests that started before this settings save.
    state
        .cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    if outcome.invalidated {
        app.state::<analysis::Runtime>()
            .generation
            .fetch_add(1, Ordering::AcqRel);
    }
    let mut warnings = state.warnings.lock().unwrap_or_else(|e| e.into_inner());
    warnings.retain(|s| !s.contains("恢复") && !(shortcut_changed && s.contains("快捷键")));
    warnings.extend(outcome.warnings);
    if app.emit("settings-changed", ()).is_err() {
        warnings.push("设置已提交，但窗口同步通知失败；重新打开窗口可恢复。".into());
    }
    let warnings = warnings.clone();
    drop(current);
    analysis::changed(&app);
    sync::changed(&app);
    Ok(SettingsView {
        settings: config,
        has_api_key: outcome.has_api_key,
        warnings,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionTest {
    message: String,
    elapsed_ms: u128,
    compatibility_fallback: bool,
}

#[tauri::command]
async fn test_connection(
    state: State<'_, AppState>,
    config: Settings,
    api_key: Option<String>,
) -> AppResult<ConnectionTest> {
    settings_commit::ensure_ready(&state.path, &state.recovering)?;
    config.validate()?;
    let key = match api_key {
        Some(key) => Zeroizing::new(key),
        None => vault_key().await?.unwrap_or_default(),
    };
    let start = std::time::Instant::now();
    let result = llm::translate(&config, &key, "Hello, world.").await?;
    Ok(ConnectionTest {
        message: "连接成功，模型已返回有效翻译。".into(),
        elapsed_ms: start.elapsed().as_millis(),
        compatibility_fallback: result.compatibility_fallback,
    })
}

#[tauri::command]
fn get_job(state: State<'_, AppState>) -> AppResult<Job> {
    Ok(state.job.lock().map_err(|_| lock_error())?.clone())
}

fn update_job(app: &tauri::AppHandle, update: impl FnOnce(&mut Job)) {
    let state = app.state::<AppState>();
    if let Ok(mut job) = state.job.lock() {
        update(&mut job);
        job.revision += 1;
        let _ = app.emit("job-changed", job.clone());
    };
}

struct BusyGuard(tauri::AppHandle);
impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0
            .state::<AppState>()
            .busy
            .store(false, Ordering::Release);
    }
}

pub fn start_job(
    app: tauri::AppHandle,
    input: Option<String>,
    clipboard_only: bool,
    popup: bool,
) -> AppResult<()> {
    start_job_with_cache(app, input, clipboard_only, popup, false)
}

fn start_job_with_cache(
    app: tauri::AppHandle,
    input: Option<String>,
    clipboard_only: bool,
    popup: bool,
    bypass_cache: bool,
) -> AppResult<()> {
    if let Some(text) = &input {
        translation::validate_input(text)?;
    }
    let state = app.state::<AppState>();
    settings_commit::ensure_ready(&state.path, &state.recovering)?;
    let memory_generation = app
        .state::<analysis::Runtime>()
        .generation
        .load(Ordering::Acquire);
    state
        .busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| AppError::new("busy", "正在处理上一段文字，请稍候。"))?;
    let anchor = if popup {
        window::selection_anchor(&app)
    } else {
        None
    };
    update_job(&app, |job| {
        let revision = job.revision;
        *job = Job {
            revision,
            status: if input.is_some() {
                "translating"
            } else {
                "capturing"
            }
            .into(),
            original: input.clone().unwrap_or_default(),
            ..Job::default()
        };
    });
    tauri::async_runtime::spawn(async move {
        let _guard = BusyGuard(app.clone());
        let outcome = run_job(
            &app,
            input,
            clipboard_only,
            popup,
            memory_generation,
            anchor,
            bypass_cache,
        )
        .await;
        match outcome {
            Ok((result, cache_hit)) => update_job(&app, |job| {
                job.status = "success".into();
                job.result = Some(result);
                job.cache_hit = cache_hit;
            }),
            Err(error) => {
                update_job(&app, |job| {
                    job.status = "error".into();
                    job.error = Some(error);
                });
                if popup {
                    let _ = window::show_translation(&app, anchor).await;
                }
            }
        }
    });
    Ok(())
}

// A desktop command has no caller webview to display a rejected request.
// Keep the current in-flight job for busy triggers; surface other gate errors.
fn dispatch_clipboard(app: &tauri::AppHandle) {
    if let Err(error) = start_job(app.clone(), None, true, true) {
        if error.code == "busy" {
            return;
        }
        update_job(app, |job| {
            *job = Job {
                revision: job.revision,
                status: "error".into(),
                error: Some(error),
                ..Job::default()
            };
        });
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = window::show_translation(&app, None).await;
        });
    }
}

async fn run_job(
    app: &tauri::AppHandle,
    input: Option<String>,
    clipboard_only: bool,
    popup: bool,
    memory_generation: u64,
    anchor: Option<tauri::PhysicalPosition<f64>>,
    bypass_cache: bool,
) -> AppResult<(translation::Translation, bool)> {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().await;
    settings_commit::ensure_ready(&state.path, &state.recovering)?;
    let config = settings.clone();
    let db = app.state::<analysis::Runtime>().db.clone();
    let epoch = if config.remember_terms || config.personalize_explanations {
        match &db {
            Some(db) => db.epoch().await.ok(),
            None => None,
        }
    } else {
        None
    };
    drop(settings);
    update_job(app, |job| {
        job.request_options = Some(RequestOptions::from(&config))
    });
    let job_id = uuid::Uuid::new_v4().to_string();
    let selection = match input {
        Some(text) => selection::Selection {
            text: translation::validate_input(&text)?,
            source: "manual".into(),
            warning: None,
        },
        None => selection::capture(clipboard_only || config.capture_mode == "clipboard").await?,
    };
    update_job(app, |job| {
        job.status = "translating".into();
        job.original = selection.text.clone();
        job.source = selection.source.clone();
        job.warning = selection.warning.clone();
    });
    // Capture always completes before any window steals focus from the source app.
    if popup {
        if let Some(warning) = window::show_translation(app, anchor).await? {
            update_job(app, |job| {
                job.warning = Some(match job.warning.take() {
                    Some(old) => format!("{old} {warning}"),
                    None => warning,
                });
            });
        }
    }
    settings_commit::ensure_ready(&state.path, &state.recovering)?;
    let key = vault_key().await?.unwrap_or_default();
    settings_commit::ensure_ready(&state.path, &state.recovering)?;
    let (context, context_available) = if config.personalize_explanations {
        match &db {
            Some(db) => match db.context(selection.text.clone()).await {
                Ok(context) => (context, true),
                Err(_) => (String::new(), false),
            },
            None => (String::new(), false),
        }
    } else {
        (String::new(), true)
    };
    let cache_key = state.cache.lock().map_err(|_| lock_error())?.key(
        &config,
        &key,
        &selection.text,
        &context,
        epoch.map(|epoch| (memory_generation, epoch)),
    )?;
    // When memory is unavailable, avoid cached memory IDs or incomplete context.
    let cache_allowed = context_available
        && (!(config.remember_terms || config.personalize_explanations) || epoch.is_some());
    if cache_allowed && !bypass_cache {
        let cached = state
            .cache
            .lock()
            .map_err(|_| lock_error())?
            .get(&cache_key, std::time::Instant::now());
        if let Some(result) = cached {
            // A cache hit is a reuse, not a new term observation or analysis event.
            return Ok((result, true));
        }
    }
    let mut result = llm::translate_with_context(&config, &key, &selection.text, &context).await?;
    let mut cacheable = cache_allowed;
    if config.remember_terms {
        let indices: Vec<_> = result
            .terms
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                !t.sense_summary.trim().is_empty()
                    && t.original.chars().count() <= 120
                    && selection
                        .text
                        .to_lowercase()
                        .contains(&t.original.to_lowercase())
            })
            .map(|(i, _)| i)
            .collect();
        let observations = indices
            .iter()
            .map(|&i| {
                let t = &result.terms[i];
                memory::Observation {
                    original: t.original.clone(),
                    translation: t.translation.clone(),
                    domain: t.domain.clone(),
                    sense_summary: t.sense_summary.clone(),
                    language: result.source_language.clone(),
                }
            })
            .collect();
        let recorded = if app
            .state::<analysis::Runtime>()
            .generation
            .load(Ordering::Acquire)
            != memory_generation
        {
            Err(AppError::new(
                "memory_stale",
                "记忆数据或设置已变化，本次旧任务未保存术语。",
            ))
        } else {
            match (&db, epoch) {
                (Some(db), Some(epoch)) => {
                    db.record(epoch, job_id, selection.source, observations)
                        .await
                }
                _ => Err(AppError::new(
                    "memory_storage",
                    "术语记忆不可用，本次译文仍可阅读。",
                )),
            }
        };
        let warning = match recorded {
            Ok(ids) => {
                for (i, id) in indices.iter().zip(ids) {
                    result.terms[*i].memory_id = Some(id);
                }
                if indices.len() != result.terms.len() {
                    Some("部分术语缺少独立词义或与原文不匹配，未保存这些术语。".to_string())
                } else {
                    None
                }
            }
            Err(e) => {
                cacheable = false;
                Some(e.message)
            }
        };
        if let Some(warning) = warning {
            cacheable = false;
            update_job(app, |job| {
                job.warning = Some(match job.warning.take() {
                    Some(old) => format!("{old} {warning}"),
                    None => warning,
                });
            });
        }
        analysis::changed(app);
    }
    if !config.explain_terms {
        result.terms.clear();
    }
    if cacheable {
        state.cache.lock().map_err(|_| lock_error())?.insert(
            cache_key,
            result.clone(),
            std::time::Instant::now(),
        );
    }
    Ok((result, false))
}

#[tauri::command]
fn translate_text(
    app: tauri::AppHandle,
    text: String,
    bypass_cache: Option<bool>,
) -> AppResult<()> {
    start_job_with_cache(app, Some(text), false, false, bypass_cache.unwrap_or(false))
}

#[tauri::command]
fn clear_translation_cache(state: State<'_, AppState>) -> AppResult<()> {
    state.cache.lock().map_err(|_| lock_error())?.clear();
    Ok(())
}

#[tauri::command]
fn translate_clipboard(app: tauri::AppHandle) -> AppResult<()> {
    start_job(app, None, true, false)
}

#[tauri::command]
async fn copy_translation(state: State<'_, AppState>) -> AppResult<()> {
    let text = state
        .job
        .lock()
        .map_err(|_| lock_error())?
        .result
        .as_ref()
        .map(|r| r.translation.clone())
        .ok_or_else(|| AppError::new("copy", "暂无可复制的译文。"))?;
    selection::copy_text(text).await
}

#[tauri::command]
fn show_settings(app: tauri::AppHandle) -> AppResult<()> {
    window::show(&app, "main")?;
    app.emit_to("main", "open-settings", ())
        .map_err(|_| AppError::new("window", "无法打开设置页面。"))
}

#[tauri::command]
fn show_popup(app: tauri::AppHandle) -> AppResult<()> {
    window::show(&app, "translation")
}

#[tauri::command]
async fn get_shortcut_status(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<shortcut::Status> {
    let settings = state.settings.lock().await;
    let desired = settings.shortcut.clone();
    let path = state.path.clone();
    tauri::async_runtime::spawn_blocking(move || shortcut::inspect(&app, &path, &desired))
        .await
        .map_err(|_| lock_error())
}
#[tauri::command]
async fn manage_shortcut(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    action: String,
    expected_shortcut: String,
) -> AppResult<shortcut::Status> {
    let settings = state.settings.lock().await;
    if expected_shortcut != settings.shortcut {
        return Err(shortcut::desktop_error(
            "已保存快捷键已更新，请刷新后重试。",
        ));
    }
    if app.state::<shortcut::Runtime>().backend != shortcut::Backend::Gnome {
        return Err(shortcut::desktop_error(
            "当前会话不支持自动管理；请使用系统手动配置。",
        ));
    }
    let path = state.path.clone();
    let desired = settings.shortcut.clone();
    tauri::async_runtime::spawn_blocking(move || {
        shortcut::manage(&path, &desired, &action)?;
        let _ = app.emit("shortcuts-changed", ());
        Ok(shortcut::inspect(&app, &path, &desired))
    })
    .await
    .map_err(|_| lock_error())?
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn platform_info() -> String {
    #[cfg(windows)]
    {
        "Windows · 自动选区 / 剪贴板".into()
    }
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s == "wayland")
        {
            "Wayland · 先复制；悬浮窗保留本次运行手动位置".into()
        } else {
            "X11 · PRIMARY 选区 / 剪贴板".into()
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        "当前系统尚未支持".into()
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if args.iter().any(|a| a == "--translate-clipboard") {
                dispatch_clipboard(app);
            } else {
                let _ = window::show(app, "main");
            }
        }))
        .setup(|app| {
            let path = app.path().app_config_dir()?.join("settings.json");
            let mut warnings = vec![];
            let config = settings::load(&path).unwrap_or_else(|error| {
                warnings.push(error.message);
                Settings::default()
            });
            let db = memory::Memory::open(app.path().app_data_dir()?.join("memory.sqlite3"))
                .map_err(|e| warnings.push(e.message))
                .ok();
            app.manage(analysis::Runtime::new(db));
            app.manage(sync::Runtime::default());
            app.manage(AppState {
                settings: tokio::sync::Mutex::new(config.clone()),
                recovering: std::sync::Arc::new(AtomicBool::new(
                    settings_commit::marker(&path).exists(),
                )),
                path,
                warnings: Mutex::new(warnings),
                job: Mutex::new(Job {
                    status: "idle".into(),
                    ..Job::default()
                }),
                busy: AtomicBool::new(false),
                cache: Mutex::new(cache::Cache::default()),
            });
            let shortcuts = shortcut::Runtime::detect();
            let native = shortcuts.backend == shortcut::Backend::Native;
            app.manage(shortcuts);
            if native {
                let plugin = tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(|app, _shortcut, event| {
                        if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            let _ = start_job(app.clone(), None, false, true);
                        }
                    })
                    .build();
                let runtime = app.state::<shortcut::Runtime>();
                let result = app
                    .handle()
                    .plugin(plugin)
                    .map_err(|_| shortcut::desktop_error("无法初始化原生快捷键。"))
                    .and_then(|_| {
                        runtime.native_capable.store(true, Ordering::Release);
                        shortcut::register(app.handle(), &config.shortcut)
                    });
                if let Err(error) = result {
                    *runtime.error.lock().map_err(|_| lock_error())? = Some(error.message.clone());
                    app.state::<AppState>()
                        .warnings
                        .lock()
                        .map_err(|_| lock_error())?
                        .push(error.message);
                }
            }
            if window::tray(app.handle()).is_err() {
                app.state::<AppState>()
                    .warnings
                    .lock()
                    .map_err(|_| lock_error())?
                    .push("系统托盘不可用；可通过再次启动应用重新打开窗口。".into());
            }
            if std::env::args().any(|arg| arg == "--translate-clipboard") {
                dispatch_clipboard(app.handle());
            } else {
                window::show(app.handle(), "main")?;
            }
            analysis::schedule(app.handle().clone());
            sync::schedule(app.handle().clone());
            Ok(())
        })
        .on_window_event(window::hide_on_close)
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            test_connection,
            get_job,
            translate_text,
            clear_translation_cache,
            translate_clipboard,
            copy_translation,
            show_settings,
            show_popup,
            quit_app,
            platform_info,
            get_shortcut_status,
            manage_shortcut,
            analysis::get_memory,
            analysis::get_analysis_status,
            analysis::set_term_feedback,
            analysis::delete_memory,
            analysis::select_profile,
            analysis::analyze_memory,
            sync::get_sync,
            sync::test_sync_connection,
            sync::connect_sync,
            sync::configure_sync,
            sync::sync_now,
            sync::sync_devices,
            sync::revoke_sync_device,
            sync::resolve_sync_conflict,
            sync::clear_sync_memory
        ])
        .run(tauri::generate_context!())
        .expect("AI Selection Assistant failed to start; check native Tauri prerequisites");
}

#[cfg(test)]
mod job_tests {
    use super::*;
    #[test]
    fn request_options_are_native_owned_and_do_not_change_with_settings() {
        let mut config = Settings::default();
        let options = RequestOptions::from(&config);
        config.target_language = "日本語".into();
        config.explain_terms = false;
        let job = Job {
            request_options: Some(options),
            ..Job::default()
        };
        let json = serde_json::to_value(&job).unwrap();
        assert_eq!(json["requestOptions"]["targetLanguage"], "简体中文");
        assert_eq!(json["requestOptions"]["explainTerms"], true);
        assert!(json.get("apiKey").is_none());
    }
}
