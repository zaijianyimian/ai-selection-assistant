mod analysis;
mod error;
mod llm;
mod memory;
mod selection;
mod settings;
mod shortcut;
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

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub revision: u64,
    pub status: String,
    pub original: String,
    pub source: String,
    pub warning: Option<String>,
    pub result: Option<translation::Translation>,
    pub error: Option<AppError>,
}

pub struct AppState {
    settings: tokio::sync::Mutex<Settings>,
    path: PathBuf,
    warnings: Mutex<Vec<String>>,
    job: Mutex<Job>,
    busy: AtomicBool,
    shortcut_available: AtomicBool,
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
    if shortcut_changed && !state.shortcut_available.load(Ordering::Acquire) {
        return Err(AppError::new(
            "shortcut_unavailable",
            "当前桌面无法使用全局快捷键；请保留原快捷键设置并使用「翻译剪贴板」。",
        ));
    }
    let key_changed = replacement.is_some() || remove_api_key;
    // Do this before any credential/shortcut mutations, so failure leaves those untouched.
    if config.remember_terms != current.remember_terms
        || config.auto_analyze != current.auto_analyze
    {
        app.state::<analysis::Runtime>()
            .generation
            .fetch_add(1, Ordering::AcqRel);
        if let Some(db) = &app.state::<analysis::Runtime>().db {
            db.invalidate().await?;
        }
    }
    let previous_key = if key_changed {
        vault_key().await?
    } else {
        None
    };
    // Reserve the replacement first: if it conflicts, the existing shortcut stays registered.
    if shortcut_changed {
        shortcut::register(&app, &config.shortcut)?;
    }
    if key_changed {
        let key = replacement.as_ref().map(|k| k.to_string());
        let result = tauri::async_runtime::spawn_blocking(move || {
            let key = key.map(Zeroizing::new);
            settings::write_key(key.as_ref().map(|k| k.as_str()))
        })
        .await
        .map_err(|_| lock_error())?;
        if let Err(error) = result {
            if shortcut_changed {
                let _ = shortcut::unregister(&app, &config.shortcut);
            }
            return Err(error);
        }
    }
    if let Err(error) = settings::persist(&state.path, &config) {
        let rollback_error = if key_changed {
            tauri::async_runtime::spawn_blocking(move || {
                settings::write_key(previous_key.as_ref().map(|k| k.as_str()))
            })
            .await
            .map_err(|_| lock_error())?
        } else {
            Ok(())
        };
        if shortcut_changed {
            let _ = shortcut::unregister(&app, &config.shortcut);
        }
        if rollback_error.is_err() {
            return Err(AppError::new(
                "settings_rollback",
                "配置保存失败，且系统凭据回滚失败。请重新保存 API Key 后再使用。",
            ));
        }
        return Err(error);
    }
    let old_shortcut = current.shortcut.clone();
    *current = config;
    if shortcut_changed {
        if let Err(error) = shortcut::unregister(&app, &old_shortcut) {
            state
                .warnings
                .lock()
                .map_err(|_| lock_error())?
                .push(error.message);
        } else {
            state
                .warnings
                .lock()
                .map_err(|_| lock_error())?
                .retain(|s| !s.contains("快捷键"));
        }
    }
    drop(current);
    let _ = app.emit("settings-changed", ());
    analysis::changed(&app);
    get_settings(state).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionTest {
    message: String,
    elapsed_ms: u128,
    compatibility_fallback: bool,
}

#[tauri::command]
async fn test_connection(config: Settings, api_key: Option<String>) -> AppResult<ConnectionTest> {
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
    if let Some(text) = &input {
        translation::validate_input(text)?;
    }
    let state = app.state::<AppState>();
    let memory_generation = app
        .state::<analysis::Runtime>()
        .generation
        .load(Ordering::Acquire);
    state
        .busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| AppError::new("busy", "正在处理上一段文字，请稍候。"))?;
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
        let outcome = run_job(&app, input, clipboard_only, popup, memory_generation).await;
        match outcome {
            Ok(result) => update_job(&app, |job| {
                job.status = "success".into();
                job.result = Some(result);
            }),
            Err(error) => {
                update_job(&app, |job| {
                    job.status = "error".into();
                    job.error = Some(error);
                });
                if popup {
                    let _ = window::show(&app, "translation");
                }
            }
        }
    });
    Ok(())
}

async fn run_job(
    app: &tauri::AppHandle,
    input: Option<String>,
    clipboard_only: bool,
    popup: bool,
    memory_generation: u64,
) -> AppResult<translation::Translation> {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().await;
    let config = settings.clone();
    let db = app.state::<analysis::Runtime>().db.clone();
    let epoch = if config.remember_terms {
        match &db {
            Some(db) => db.epoch().await.ok(),
            None => None,
        }
    } else {
        None
    };
    drop(settings);
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
        window::show(app, "translation")?;
    }
    let key = vault_key().await?.unwrap_or_default();
    let context = if config.personalize_explanations {
        match &db {
            Some(db) => db.context(selection.text.clone()).await.unwrap_or_default(),
            None => String::new(),
        }
    } else {
        String::new()
    };
    let mut result = llm::translate_with_context(&config, &key, &selection.text, &context).await?;
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
            Err(e) => Some(e.message),
        };
        if let Some(warning) = warning {
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
    Ok(result)
}

#[tauri::command]
fn translate_text(app: tauri::AppHandle, text: String) -> AppResult<()> {
    start_job(app, Some(text), false, false)
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
            "Wayland · 请先 Ctrl+C 复制后翻译".into()
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
                let _ = start_job(app.clone(), None, true, true);
            } else { let _ = window::show(app, "main"); }
        }))
        .setup(|app| {
            let path = app.path().app_config_dir()?.join("settings.json");
            let mut warnings = vec![];
            let config = settings::load(&path).unwrap_or_else(|error| { warnings.push(error.message); Settings::default() });
            let db=memory::Memory::open(app.path().app_data_dir()?.join("memory.sqlite3")).map_err(|e|warnings.push(e.message)).ok();
            app.manage(analysis::Runtime::new(db));
            app.manage(AppState {
                settings: tokio::sync::Mutex::new(config.clone()), path,
                warnings: Mutex::new(warnings), job: Mutex::new(Job { status: "idle".into(), ..Job::default() }),
                busy: AtomicBool::new(false), shortcut_available: AtomicBool::new(false),
            });
            let plugin = tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let _ = start_job(app.clone(), None, false, true);
                    }
                }).build();
            let state = app.state::<AppState>();
            if app.handle().plugin(plugin).is_ok() {
                state.shortcut_available.store(true, Ordering::Release);
                if let Err(error) = shortcut::register(app.handle(), &config.shortcut) {
                    state.warnings.lock().map_err(|_| lock_error())?.push(error.message);
                }
            } else {
                state.warnings.lock().map_err(|_| lock_error())?.push("当前桌面无法启用全局快捷键，请使用「翻译剪贴板」。Wayland 可配置系统自定义快捷键执行应用 --translate-clipboard。".into());
            }
            if window::tray(app.handle()).is_err() {
                state.warnings.lock().map_err(|_| lock_error())?.push("系统托盘不可用；可通过再次启动应用重新打开窗口。".into());
            }
            if std::env::args().any(|arg| arg == "--translate-clipboard") {
                let _ = start_job(app.handle().clone(), None, true, true);
            }
            analysis::schedule(app.handle().clone());
            Ok(())
        })
        .on_window_event(window::hide_on_close)
        .invoke_handler(tauri::generate_handler![get_settings, save_settings, test_connection, get_job, translate_text, translate_clipboard, copy_translation, show_settings, show_popup, quit_app, platform_info, analysis::get_memory,analysis::get_analysis_status,analysis::set_term_feedback,analysis::delete_memory,analysis::select_profile,analysis::analyze_memory])
        .run(tauri::generate_context!())
        .expect("AI Selection Assistant failed to start; check native Tauri prerequisites");
}
