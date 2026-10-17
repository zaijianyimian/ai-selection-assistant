//! GNOME configuration adapter. Construction and inspection are read-only.
use super::*;
use glib::{variant::ToVariant, Variant, VariantTy};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

pub const SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
pub const PATH: &str =
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/ai-selection-assistant/";
const ENTRY: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
const NAME: &str = "AI Selection Assistant · 翻译剪贴板";

pub trait Runner: Send + Sync {
    fn run(&self, args: &[&str]) -> AppResult<String>;
}
pub struct System;
impl Runner for System {
    fn run(&self, args: &[&str]) -> AppResult<String> {
        run_program("gsettings", args, Duration::from_secs(3))
    }
}
// Drain a bounded pipe concurrently so a noisy child cannot block wait or grow memory.
pub(super) fn run_program(program: &str, args: &[&str], limit: Duration) -> AppResult<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| {
            failure("无法运行 gsettings；请安装 libglib2.0-bin，或使用系统手动快捷键。")
        })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| failure("无法读取系统快捷键配置。"))?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.take(131_073).read_to_end(&mut bytes);
        let _ = tx.send((result, bytes));
    });
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                let (result, bytes) = rx
                    .recv_timeout(remaining)
                    .map_err(|_| failure("读取系统快捷键超时。"))?;
                if !status.success() || result.is_err() || bytes.len() > 131_072 {
                    return Err(failure(
                        "系统快捷键操作失败或输出异常，请检查 GNOME 会话与设置权限。",
                    ));
                }
                return String::from_utf8(bytes)
                    .map(|s| s.trim().to_owned())
                    .map_err(|_| failure("系统快捷键返回了无效文本。"));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(failure("无法等待系统快捷键操作。"));
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(failure("系统快捷键操作超时，请刷新状态后重试。"));
            }
            _ => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}
pub fn failure(message: &str) -> AppError {
    AppError::new("desktop_shortcut", message)
}
fn conflict() -> AppError {
    AppError::new(
        "shortcut_conflict",
        "快捷键已被可见的系统绑定占用，请更换组合键。",
    )
}
fn ownership() -> AppError {
    failure("此快捷键配置已被外部修改，请先在系统设置中处理；应用不会覆盖其他命令。")
}
fn string(text: &str) -> AppResult<String> {
    Variant::parse(Some(VariantTy::STRING), text)
        .ok()
        .and_then(|v| v.str().map(str::to_owned))
        .ok_or_else(|| failure("系统快捷键字符串格式无效。"))
}
fn array(text: &str) -> AppResult<Vec<String>> {
    Variant::parse(Some(VariantTy::STRING_ARRAY), text)
        .ok()
        .and_then(|v| v.get::<Vec<String>>())
        .ok_or_else(|| failure("系统快捷键列表格式无效。"))
}
fn encode(value: &str) -> String {
    value.to_variant().print(true).to_string()
}

pub fn accelerator(value: &str) -> AppResult<String> {
    use tauri_plugin_global_shortcut::Modifiers;
    let key = parse(value)?;
    let code = key.key.to_string();
    let symbol = if let Some(c) = code.strip_prefix("Key").filter(|c| c.len() == 1) {
        c.to_ascii_lowercase()
    } else if let Some(c) = code.strip_prefix("Digit").filter(|c| c.len() == 1) {
        c.into()
    } else if code.starts_with('F') && code[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n))
    {
        code
    } else {
        match code.as_str() {
            "Space" => "space",
            "Enter" => "Return",
            "Escape" => "Escape",
            "Tab" => "Tab",
            "Backspace" => "BackSpace",
            "Delete" => "Delete",
            "Insert" => "Insert",
            "Home" => "Home",
            "End" => "End",
            "PageUp" => "Page_Up",
            "PageDown" => "Page_Down",
            "ArrowUp" => "Up",
            "ArrowDown" => "Down",
            "ArrowLeft" => "Left",
            "ArrowRight" => "Right",
            _ => {
                return Err(failure(
                    "此按键暂不支持 GNOME 映射，请使用字母、数字、F1–F24 或常见导航键。",
                ))
            }
        }
        .into()
    };
    let mut result = String::new();
    for (flag, name) in [
        (Modifiers::CONTROL, "<Control>"),
        (Modifiers::ALT, "<Alt>"),
        (Modifiers::SHIFT, "<Shift>"),
        (Modifiers::SUPER, "<Super>"),
    ] {
        if key.mods.contains(flag) {
            result.push_str(name);
        }
    }
    result.push_str(&symbol);
    Ok(result)
}
fn normalized(value: &str) -> Option<String> {
    let text = value.trim().to_ascii_lowercase();
    let mut rest = text.as_str();
    let mut mask = 0u8;
    while let Some(tail) = rest.strip_prefix('<') {
        let (modifier, tail) = tail.split_once('>')?;
        mask |= match modifier {
            "control" | "ctrl" | "primary" => 1,
            "alt" | "mod1" => 2,
            "shift" => 4,
            "super" | "mod4" => 8,
            _ => return None,
        };
        rest = tail;
    }
    if rest.is_empty() || rest.contains(['<', '>', ' ']) {
        return None;
    }
    Some(format!("{mask}:{rest}"))
}
pub fn equivalent(a: &str, b: &str) -> bool {
    normalized(a).is_some() && normalized(a) == normalized(b)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub listed: bool,
    pub name: String,
    pub command: String,
    pub binding: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub version: u8,
    pub before: Snapshot,
    pub after: Snapshot,
}
pub struct Desktop<'a>(pub &'a dyn Runner);
impl Desktop<'_> {
    fn entry(path: &str) -> String {
        format!("{ENTRY}:{path}")
    }
    fn get(&self, schema: &str, key: &str) -> AppResult<String> {
        self.0.run(&["get", schema, key])
    }
    fn set(&self, schema: &str, key: &str, value: &str) -> AppResult<()> {
        self.0.run(&["set", schema, key, value]).map(|_| ())
    }
    pub fn available(&self) -> AppResult<()> {
        let schemas = self.0.run(&["list-schemas"])?;
        let relocatable = self.0.run(&["list-relocatable-schemas"])?;
        if !schemas.lines().any(|s| s == SCHEMA) || !relocatable.lines().any(|s| s == ENTRY) {
            return Err(failure(
                "当前桌面缺少 GNOME 快捷键 schema，请手动配置系统快捷键。",
            ));
        }
        self.paths()?;
        Ok(())
    }
    fn paths(&self) -> AppResult<Vec<String>> {
        array(&self.get(SCHEMA, "custom-keybindings")?)
    }
    pub fn snapshot(&self) -> AppResult<Snapshot> {
        let schema = Self::entry(PATH);
        Ok(Snapshot {
            listed: self.paths()?.iter().any(|p| p == PATH),
            name: string(&self.get(&schema, "name")?)?,
            command: string(&self.get(&schema, "command")?)?,
            binding: string(&self.get(&schema, "binding")?)?,
        })
    }
    pub fn prepare(&self, value: Option<&str>) -> AppResult<Change> {
        let before = self.snapshot()?;
        if !before.command.is_empty() && before.command != COMMAND {
            return Err(ownership());
        }
        let after = if let Some(value) = value {
            let binding = accelerator(value)?;
            self.check_conflict(&binding)?;
            Snapshot {
                listed: true,
                name: NAME.into(),
                command: COMMAND.into(),
                binding,
            }
        } else {
            Snapshot::default()
        };
        Ok(Change {
            version: 1,
            before,
            after,
        })
    }
    fn check_conflict(&self, binding: &str) -> AppResult<()> {
        let desired = normalized(binding).ok_or_else(|| failure("快捷键映射无效。"))?;
        for path in self.paths()? {
            if path == PATH {
                continue;
            }
            if !path.starts_with('/') || !path.ends_with('/') || path.len() > 1024 {
                return Err(failure("系统自定义快捷键路径无效。"));
            }
            let other = string(&self.get(&Self::entry(&path), "binding")?)?;
            if normalized(&other).as_ref() == Some(&desired) {
                return Err(conflict());
            }
        }
        let schemas = self.0.run(&["list-schemas"])?;
        for schema in [
            SCHEMA,
            "org.gnome.desktop.wm.keybindings",
            "org.gnome.mutter.keybindings",
            "org.gnome.shell.keybindings",
        ] {
            if !schemas.lines().any(|s| s == schema) {
                continue;
            }
            let keys = self.0.run(&["list-keys", schema])?;
            for key in keys.lines().filter(|key| *key != "custom-keybindings") {
                let text = self.get(schema, key)?;
                // These schemas also contain booleans and other settings: inspect only strings/as.
                let values = array(&text).or_else(|_| string(&text).map(|s| vec![s]));
                if values.is_ok_and(|values| {
                    values
                        .iter()
                        .any(|v| normalized(v).as_ref() == Some(&desired))
                }) {
                    return Err(conflict());
                }
            }
        }
        Ok(())
    }
    fn membership(&self, listed: bool) -> AppResult<()> {
        for _ in 0..3 {
            let before = self.paths()?;
            let mut after = before
                .iter()
                .filter(|p| p.as_str() != PATH)
                .cloned()
                .collect::<Vec<_>>();
            if listed {
                after.push(PATH.into());
            }
            if before == after {
                return Ok(());
            }
            if self.paths()? != before {
                continue;
            }
            self.set(
                SCHEMA,
                "custom-keybindings",
                &after.to_variant().print(true),
            )?;
            let observed = self.paths()?;
            if observed == after {
                return Ok(());
            }
            // Preserve additions observed since our last write on the next attempt.
        }
        Err(failure("快捷键列表正在被其他程序修改，请刷新后重试。"))
    }
    fn write(&self, from: &Snapshot, to: &Snapshot) -> AppResult<()> {
        let current = self.snapshot()?;
        if &current != from {
            return Err(ownership());
        }
        let schema = Self::entry(PATH);
        let mut expected = from.clone();
        for (key, value) in [
            ("name", &to.name),
            ("command", &to.command),
            ("binding", &to.binding),
        ] {
            if self.snapshot()? != expected {
                return Err(ownership());
            }
            self.set(&schema, key, &encode(value))?;
            match key {
                "name" => expected.name = value.clone(),
                "command" => expected.command = value.clone(),
                _ => expected.binding = value.clone(),
            }
        }
        if self.snapshot()? != expected {
            return Err(ownership());
        }
        self.membership(to.listed)?;
        if self.snapshot()? != *to {
            return Err(failure("快捷键配置读回不一致，请恢复后重试。"));
        }
        Ok(())
    }
    pub fn apply(&self, change: &Change) -> AppResult<()> {
        self.validate(change)?;
        self.write(&change.before, &change.after)
    }
    pub fn rollback(&self, change: &Change) -> AppResult<()> {
        self.validate(change)?;
        let current = self.snapshot()?;
        if current == change.before {
            return Ok(());
        }
        // Partial writes are ours only if every field is still either the old or requested value.
        for (actual, before, after) in [
            (&current.name, &change.before.name, &change.after.name),
            (
                &current.command,
                &change.before.command,
                &change.after.command,
            ),
            (
                &current.binding,
                &change.before.binding,
                &change.after.binding,
            ),
        ] {
            if actual != before && actual != after {
                return Err(ownership());
            }
        }
        self.write(&current, &change.before)
    }
    fn validate(&self, change: &Change) -> AppResult<()> {
        if change.version != 1
            || [&change.before, &change.after]
                .iter()
                .any(|s| !s.command.is_empty() && s.command != COMMAND)
        {
            return Err(failure("快捷键恢复记录无效，未修改系统设置。"));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests;
