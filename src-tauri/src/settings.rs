use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Settings {
    pub base_url: String,
    pub model: String,
    pub temperature: f64,
    pub timeout_secs: u64,
    pub shortcut: String,
    pub explain_terms: bool,
    pub output_mode: String,
    pub capture_mode: String,
    pub theme: String,
    pub target_language: String,
    pub remember_terms: bool,
    pub personalize_explanations: bool,
    pub auto_analyze: bool,
    pub explanation_style: String,
    pub background_domain: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
            temperature: 0.2,
            timeout_secs: 45,
            shortcut: "Ctrl+Shift+Q".into(),
            explain_terms: true,
            output_mode: "auto".into(),
            capture_mode: "auto".into(),
            theme: "system".into(),
            target_language: "简体中文".into(),
            remember_terms: false,
            personalize_explanations: false,
            auto_analyze: false,
            explanation_style: "balanced".into(),
            background_domain: "general".into(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> AppResult<()> {
        crate::llm::normalize_endpoint(&self.base_url)?;
        if self.model.trim().is_empty() || self.model.len() > 200 {
            return Err(AppError::new(
                "settings",
                "请输入有效的 Model 名称（最多 200 字节）。",
            ));
        }
        if !self.temperature.is_finite() || !(0.0..=2.0).contains(&self.temperature) {
            return Err(AppError::new("settings", "Temperature 必须在 0–2 之间。"));
        }
        if !(5..=180).contains(&self.timeout_secs) {
            return Err(AppError::new("settings", "请求超时必须在 5–180 秒之间。"));
        }
        if !["auto", "json", "prompt"].contains(&self.output_mode.as_str())
            || !["auto", "clipboard"].contains(&self.capture_mode.as_str())
            || !["system", "light", "dark"].contains(&self.theme.as_str())
            || !["简体中文", "English", "日本語"].contains(&self.target_language.as_str())
            || !["balanced", "introductory", "concise", "practical"]
                .contains(&self.explanation_style.as_str())
            || !["general", "backend", "ai", "database"].contains(&self.background_domain.as_str())
            || (self.auto_analyze && !self.remember_terms)
        {
            return Err(AppError::new("settings", "设置选项无效，请重新选择。"));
        }
        crate::shortcut::parse(&self.shortcut)?;
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub settings: Settings,
    pub has_api_key: bool,
    pub warnings: Vec<String>,
}

pub fn load(path: &Path) -> AppResult<Settings> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let bytes = fs::read(path)
        .map_err(|_| AppError::new("settings_read", "无法读取配置文件，请检查文件权限。"))?;
    let settings: Settings = serde_json::from_slice(&bytes).map_err(|_| {
        AppError::new(
            "settings_read",
            "配置文件格式损坏，已使用默认设置；保存可修复。",
        )
    })?;
    settings.validate()?;
    Ok(settings)
}

pub fn persist(path: &Path, settings: &Settings) -> AppResult<()> {
    settings.validate()?;
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new("settings_write", "配置路径无效。"))?;
    fs::create_dir_all(parent)
        .map_err(|_| AppError::new("settings_write", "无法创建配置目录。"))?;
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|_| AppError::new("settings_write", "配置序列化失败。"))?;
    // rename replaces an existing destination on both supported OSes via std::fs.
    fs::write(&tmp, bytes)
        .and_then(|_| fs::rename(&tmp, path))
        .map_err(|_| AppError::new("settings_write", "保存配置失败，请检查目录写入权限。"))
}

fn entry() -> AppResult<keyring::Entry> {
    keyring::Entry::new("io.github.ai-selection-assistant", "provider-api-key")
        .map_err(|_| vault_error())
}
fn vault_error() -> AppError {
    AppError::new("keyring", "无法访问系统安全凭据存储。Windows 请检查凭据管理器；Ubuntu 请安装并解锁 GNOME Keyring / Secret Service。")
}
pub fn read_key() -> AppResult<Option<Zeroizing<String>>> {
    match entry()?.get_password() {
        Ok(key) => Ok(Some(Zeroizing::new(key))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(vault_error()),
    }
}
pub fn write_key(key: Option<&str>) -> AppResult<()> {
    let entry = entry()?;
    match key {
        Some(key) => entry.set_password(key).map_err(|_| vault_error()),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(vault_error()),
        },
    }
}
pub fn validate_key(key: &str) -> AppResult<()> {
    if key.len() > 8192 || key.chars().any(char::is_control) {
        return Err(AppError::new(
            "api_key",
            "API Key 格式无效，请检查换行或长度。",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_settings_do_not_opt_into_memory_or_analysis() {
        let config: Settings = serde_json::from_str(r#"{"model":"old-model"}"#).unwrap();
        assert!(!config.remember_terms && !config.auto_analyze && !config.personalize_explanations);
        let mut config = config;
        config.auto_analyze = true;
        assert!(config.validate().is_err());
    }
    #[test]
    fn settings_roundtrip_contains_no_secret() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = Settings::default();
        settings.model = "custom-model".into();
        persist(&path, &settings).unwrap();
        persist(&path, &settings).unwrap();
        assert_eq!(load(&path).unwrap(), settings);
        let json = fs::read_to_string(path).unwrap();
        assert!(!json.contains("apiKey"));
        assert!(!json.contains("api_key"));
    }
    #[test]
    fn reject_invalid_limits_and_injected_keys() {
        let mut settings = Settings::default();
        settings.timeout_secs = 0;
        assert!(settings.validate().is_err());
        settings.timeout_secs = 30;
        settings.temperature = f64::NAN;
        assert!(settings.validate().is_err());
        assert!(validate_key("test\r\nheader").is_err());
        assert!(serde_json::from_str::<Settings>(r#"{"apiKey":"secret"}"#).is_err());
    }
}
