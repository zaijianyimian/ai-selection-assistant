use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[cfg(target_os = "linux")]
pub mod gnome;
pub const COMMAND: &str = "/usr/bin/ai-selection-assistant --translate-clipboard";

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    Native,
    Gnome,
    Manual,
}

/// Session routing never treats an XWayland DISPLAY as native X11 support.
pub fn select_backend(windows: bool, wayland: bool, desktop: &str, prerequisites: bool) -> Backend {
    if windows || !wayland {
        Backend::Native
    } else if desktop.split(':').any(|s| s.eq_ignore_ascii_case("gnome")) && prerequisites {
        Backend::Gnome
    } else {
        Backend::Manual
    }
}
pub struct Runtime {
    pub backend: Backend,
    pub native_capable: AtomicBool,
    pub error: Mutex<Option<String>>,
}
impl Runtime {
    pub fn detect() -> Self {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s == "wayland");
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let mut prerequisites = false;
        let mut error = None;
        #[cfg(target_os = "linux")]
        if wayland && desktop.split(':').any(|s| s.eq_ignore_ascii_case("gnome")) {
            let installed = std::env::current_exe()
                .ok()
                .zip(fs::canonicalize("/usr/bin/ai-selection-assistant").ok())
                .is_some_and(|(a, b)| a == b);
            if installed && std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some() {
                match gnome::Desktop(&gnome::System).available() {
                    Ok(()) => prerequisites = true,
                    Err(e) => error = Some(e.message),
                }
            } else {
                error=Some("自动管理需要已安装的 DEB 和当前用户 GNOME 会话；开发构建请手动配置系统快捷键。".into());
            }
        }
        Self {
            backend: select_backend(cfg!(windows), wayland, &desktop, prerequisites),
            native_capable: AtomicBool::new(false),
            error: Mutex::new(error),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Metadata {
    #[serde(default)]
    version: u8,
    #[serde(default)]
    enabled: bool,
}
impl Default for Metadata {
    fn default() -> Self {
        Self {
            version: 1,
            enabled: false,
        }
    }
}
fn metadata(path: &Path) -> PathBuf {
    path.with_file_name("desktop-shortcut.json")
}
pub fn recovery_path(path: &Path) -> PathBuf {
    path.with_file_name("desktop-shortcut.recovery")
}
fn load_metadata(path: &Path) -> AppResult<Metadata> {
    match fs::read(metadata(path)) {
        Ok(bytes) => {
            let m: Metadata = serde_json::from_slice(&bytes)
                .map_err(|_| desktop_error("快捷键本机状态损坏，请检查配置目录。"))?;
            if m.version != 1 {
                return Err(desktop_error("不支持的快捷键本机状态版本。"));
            }
            Ok(m)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Metadata::default()),
        Err(_) => Err(desktop_error("无法读取快捷键本机状态。")),
    }
}
pub fn desktop_error(message: &str) -> AppError {
    AppError::new("desktop_shortcut", message)
}
pub fn atomic_json(path: &Path, value: &impl Serialize) -> AppResult<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| desktop_error("配置目录无效。"))?;
    fs::create_dir_all(parent).map_err(|_| desktop_error("无法创建快捷键配置目录。"))?;
    let tmp = parent.join(format!(".shortcut-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&serde_json::to_vec(value)?)?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok::<_, Box<dyn std::error::Error>>(())
    })();
    let _ = fs::remove_file(tmp);
    result.map_err(|_| desktop_error("保存快捷键状态失败，请检查目录权限和磁盘空间。"))
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub backend: Backend,
    pub capable: bool,
    pub enabled: bool,
    pub desired: String,
    pub observed: Option<String>,
    pub status: String,
    pub message: String,
    pub command: &'static str,
}
pub fn inspect(app: &tauri::AppHandle, path: &Path, desired: &str) -> Status {
    let runtime = app.state::<Runtime>();
    let mut status = Status {
        backend: runtime.backend,
        capable: false,
        enabled: false,
        desired: desired.into(),
        observed: None,
        status: "unavailable".into(),
        message: "请在系统设置中添加自定义快捷键；先复制文字，再执行下方命令。".into(),
        command: COMMAND,
    };
    if crate::settings_commit::marker(path).exists() || recovery_path(path).exists() {
        status.status = "recovery-required".into();
        status.message = "快捷键或设置恢复尚未完成；请恢复后再翻译。".into();
        return status;
    }
    match runtime.backend {
        Backend::Native => {
            status.capable = runtime.native_capable.load(Ordering::Acquire);
            if status.capable
                && parse(desired).is_ok_and(|s| app.global_shortcut().is_registered(s))
            {
                status.enabled = true;
                status.observed = Some(desired.into());
                status.status = "ready".into();
                status.message = "原生快捷键已注册。".into();
            } else {
                status.message = "原生快捷键注册失败；请更换组合键或使用翻译剪贴板。".into();
            }
        }
        #[cfg(target_os = "linux")]
        Backend::Gnome => {
            status.capable = true;
            let result = load_metadata(path)
                .and_then(|m| gnome::Desktop(&gnome::System).snapshot().map(|s| (m, s)));
            match result {
                Ok((m, s)) => {
                    status.enabled = m.enabled;
                    if s.listed && s.command == COMMAND && !s.binding.is_empty() {
                        status.observed = Some(s.binding.clone());
                        status.status = if gnome::accelerator(desired)
                            .is_ok_and(|b| gnome::equivalent(&b, &s.binding))
                        {
                            "configured"
                        } else {
                            "external-change"
                        }
                        .into();
                        status.message="系统已配置；请先 Ctrl+C，再按快捷键验证触发。若无响应，请改键或检查系统快捷键。".into();
                    } else if !s.command.is_empty() && s.command != COMMAND {
                        status.status = "external-change".into();
                        status.message =
                            "专属配置路径已被其他命令使用，请在系统设置中手动处理。".into();
                    } else {
                        status.status = if m.enabled {
                            "external-change"
                        } else {
                            "not-configured"
                        }
                        .into();
                        status.message =
                            "尚未配置或已在系统中删除；启用/重新应用前不会自动创建。".into();
                    }
                }
                Err(e) => {
                    status.capable = false;
                    status.message = e.message;
                }
            }
        }
        _ => {}
    }
    if !status.capable {
        if let Ok(error) = runtime.error.lock() {
            if let Some(error) = error.as_ref() {
                status.message = error.clone();
            }
        }
    }
    status
}

// Some(token) delegates the entire change to the desktop backend. None keeps
// the native reserve-new/retire-old protocol used by existing services/tests.
pub fn prepare_change(
    app: &tauri::AppHandle,
    path: &Path,
    previous: &str,
    next: &str,
) -> AppResult<Option<serde_json::Value>> {
    match app.state::<Runtime>().backend {
        Backend::Native => Ok(None),
        #[cfg(target_os = "linux")]
        Backend::Gnome if load_metadata(path)?.enabled => {
            let desktop = gnome::Desktop(&gnome::System);
            let current = desktop.snapshot()?;
            if !current.listed
                || current.command != COMMAND
                || !gnome::equivalent(&current.binding, &gnome::accelerator(previous)?)
            {
                return Err(desktop_error(
                    "系统绑定已被外部修改或删除，请先刷新并显式重新应用，再保存新的快捷键。",
                ));
            }
            Ok(Some(
                serde_json::to_value(desktop.prepare(Some(next))?)
                    .map_err(|_| desktop_error("无法准备快捷键更新。"))?,
            ))
        }
        _ => Ok(Some(serde_json::Value::Null)),
    }
}
pub fn apply_change(value: &serde_json::Value, rollback: bool) -> AppResult<()> {
    if value.is_null() {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        let change: gnome::Change = serde_json::from_value(value.clone())
            .map_err(|_| desktop_error("快捷键恢复信息无效。"))?;
        let desktop = gnome::Desktop(&gnome::System);
        return if rollback {
            desktop.rollback(&change)
        } else {
            desktop.apply(&change)
        };
    }
    #[cfg(not(target_os = "linux"))]
    Err(desktop_error("当前系统无法恢复 GNOME 快捷键。"))
}

#[cfg(target_os = "linux")]
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u8,
    before: Metadata,
    after: Metadata,
    change: gnome::Change,
}

pub fn manage(path: &Path, desired: &str, action: &str) -> AppResult<()> {
    #[cfg(target_os = "linux")]
    {
        manage_with(path, desired, action, &gnome::System)
    }
    #[cfg(not(target_os = "linux"))]
    Err(desktop_error("当前系统不支持 GNOME 快捷键管理。"))
}
#[cfg(target_os = "linux")]
fn manage_with(
    path: &Path,
    desired: &str,
    action: &str,
    runner: &dyn gnome::Runner,
) -> AppResult<()> {
    let recovery = recovery_path(path);
    let desktop = gnome::Desktop(runner);
    if action == "recover" {
        let journal: Journal = serde_json::from_slice(&fs::read(&recovery).map_err(|_| {
            desktop_error("没有可恢复的桌面操作；若为设置恢复，请重新输入或删除密钥后保存设置。")
        })?)
        .map_err(|_| desktop_error("快捷键恢复记录损坏。"))?;
        if journal.version != 1 {
            return Err(desktop_error("不支持的快捷键恢复版本。"));
        }
        desktop.rollback(&journal.change)?;
        atomic_json(&metadata(path), &journal.before)?;
        fs::remove_file(recovery).map_err(|_| desktop_error("无法清除快捷键恢复记录。"))?;
        return Ok(());
    }
    if recovery.exists() || crate::settings_commit::marker(path).exists() {
        return Err(desktop_error("请先恢复未完成的快捷键或设置操作。"));
    }
    let enabled = match action {
        "enable" | "reapply" => true,
        "disable" => false,
        _ => return Err(desktop_error("无效的快捷键操作。")),
    };
    let journal = Journal {
        version: 1,
        before: load_metadata(path)?,
        after: Metadata {
            version: 1,
            enabled,
        },
        change: desktop.prepare(enabled.then_some(desired))?,
    };
    atomic_json(&recovery, &journal)?;
    let result = desktop
        .apply(&journal.change)
        .and_then(|_| atomic_json(&metadata(path), &journal.after));
    if let Err(error) = result {
        if desktop
            .rollback(&journal.change)
            .and_then(|_| atomic_json(&metadata(path), &journal.before))
            .and_then(|_| {
                fs::remove_file(&recovery).map_err(|_| desktop_error("无法清除恢复记录。"))
            })
            .is_err()
        {
            return Err(desktop_error(
                "快捷键补偿未完成，模型请求已暂停；请点击恢复后重试。",
            ));
        }
        return Err(error);
    }
    fs::remove_file(recovery)
        .map_err(|_| desktop_error("快捷键已写入，但恢复记录无法清除，请点击恢复后重试。"))?;
    Ok(())
}

pub fn parse(value: &str) -> AppResult<Shortcut> {
    let shortcut: Shortcut = value.parse().map_err(|_| {
        AppError::new(
            "shortcut",
            "快捷键格式无效，例如 Ctrl+Shift+Q、Alt+Shift+T。",
        )
    })?;
    let value = value.to_ascii_lowercase();
    if value.len() > 80
        || ![
            "ctrl+",
            "control+",
            "alt+",
            "super+",
            "commandorcontrol+",
            "cmdorctrl+",
        ]
        .iter()
        .any(|m| value.contains(m))
    {
        return Err(AppError::new(
            "shortcut",
            "全局快捷键至少需要 Ctrl、Alt 或 Super 修饰键。",
        ));
    }
    Ok(shortcut)
}

pub fn register(app: &tauri::AppHandle, value: &str) -> AppResult<()> {
    if !app
        .state::<Runtime>()
        .native_capable
        .load(Ordering::Acquire)
    {
        return Err(desktop_error(
            "当前会话无法注册原生快捷键，请使用系统快捷键或翻译剪贴板。",
        ));
    }
    app.global_shortcut().register(parse(value)?).map_err(|_| AppError::new("shortcut_conflict", "无法注册全局快捷键，可能已被其他应用占用，或桌面环境不支持。请更换快捷键；Wayland 可使用应用内「翻译剪贴板」。"))
}

pub fn unregister(app: &tauri::AppHandle, value: &str) -> AppResult<()> {
    app.global_shortcut()
        .unregister(parse(value)?)
        .map_err(|_| AppError::new("shortcut", "无法注销旧快捷键，请重启应用后重试。"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_plain_keys_and_accepts_custom_chords() {
        assert!(parse("Q").is_err());
        assert!(parse("Shift+Q").is_err());
        assert!(parse("Ctrl+Shift+Q").is_ok());
        assert!(parse("Alt+Shift+T").is_ok());
        assert!(parse("Ctrl+Nonsense").is_err());
    }
}
