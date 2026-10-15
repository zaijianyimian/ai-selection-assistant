use crate::error::{AppError, AppResult};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

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
