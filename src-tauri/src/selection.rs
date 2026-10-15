use crate::{
    error::{AppError, AppResult},
    translation::validate_input,
};
use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub text: String,
    pub source: String,
    pub warning: Option<String>,
}

pub async fn capture(clipboard_only: bool) -> AppResult<Selection> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || windows::capture(clipboard_only))
            .await
            .map_err(|_| AppError::new("selection", "选区读取任务失败，请使用手动输入。"))?
    }
    #[cfg(target_os = "linux")]
    {
        linux::capture(clipboard_only).await
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = clipboard_only;
        Err(AppError::new(
            "platform",
            "当前 MVP 仅支持 Windows 和 Ubuntu Linux。",
        ))
    }
}

pub async fn copy_text(text: String) -> AppResult<()> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s == "wayland")
    {
        use std::{process::Stdio, time::Duration};
        use tokio::io::AsyncWriteExt;
        let operation = async {
            let mut child = tokio::process::Command::new("wl-copy")
                .args(["--type", "text/plain;charset=utf-8"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .map_err(|_| clipboard_error())?;
            let mut stdin = child.stdin.take().ok_or_else(clipboard_error)?;
            stdin
                .write_all(text.as_bytes())
                .await
                .map_err(|_| clipboard_error())?;
            drop(stdin);
            if !child.wait().await.map_err(|_| clipboard_error())?.success() {
                return Err(clipboard_error());
            }
            Ok(())
        };
        return tokio::time::timeout(Duration::from_secs(3), operation)
            .await
            .map_err(|_| {
                AppError::new("clipboard_timeout", "复制超时，请检查 Wayland 剪贴板权限。")
            })?;
    }
    tauri::async_runtime::spawn_blocking(move || copy_native_text(text))
        .await
        .map_err(|_| clipboard_error())?
}

fn copy_native_text(text: String) -> AppResult<()> {
    use std::sync::{Mutex, OnceLock};
    // On X11 the clipboard owner must stay alive after set_text returns.
    static CLIPBOARD: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();
    let mut guard = CLIPBOARD
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| AppError::new("clipboard", "剪贴板状态暂时不可用。"))?;
    if guard.is_none() {
        *guard = Some(arboard::Clipboard::new().map_err(|_| clipboard_error())?);
    }
    guard
        .as_mut()
        .ok_or_else(clipboard_error)?
        .set_text(text)
        .map_err(|_| clipboard_error())
}

fn clipboard_error() -> AppError {
    AppError::new(
        "clipboard",
        "无法访问剪贴板。请检查桌面会话与权限；Wayland 请安装 wl-clipboard，并使用系统复制操作。",
    )
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{
        mem::size_of,
        thread::sleep,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        System::DataExchange::GetClipboardSequenceNumber,
        UI::{
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
                KEYEVENTF_KEYUP, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
            },
            WindowsAndMessaging::GetForegroundWindow,
        },
    };

    pub fn capture(clipboard_only: bool) -> AppResult<Selection> {
        let mut clipboard = arboard::Clipboard::new().map_err(|_| clipboard_error())?;
        if clipboard_only {
            return Ok(Selection {
                text: validate_input(&clipboard.get_text().map_err(|_| clipboard_error())?)?,
                source: "clipboard".into(),
                warning: None,
            });
        }
        let foreground = unsafe { GetForegroundWindow() };
        let deadline = Instant::now() + Duration::from_millis(1500);
        while [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN]
            .iter()
            .any(|key| unsafe { GetAsyncKeyState(*key as i32) } < 0)
        {
            if Instant::now() > deadline {
                return Err(AppError::new(
                    "shortcut_held",
                    "请松开快捷键后再试；未执行模拟复制。",
                ));
            }
            sleep(Duration::from_millis(15));
        }
        if unsafe { GetForegroundWindow() } != foreground {
            return Err(AppError::new(
                "focus_changed",
                "选区所在窗口已变化，请重新选择文字并触发快捷键。",
            ));
        }
        let (before_sequence, previous) = {
            let _lock =
                clipboard_win::Clipboard::new_attempts(10).map_err(|_| clipboard_error())?;
            let sequence = unsafe { GetClipboardSequenceNumber() };
            let mut bytes = Vec::new();
            let previous = if clipboard_win::raw::is_format_avail(
                clipboard_win::formats::CF_UNICODETEXT,
            ) {
                clipboard_win::raw::get_string(&mut bytes).map_err(|_| clipboard_error())?;
                Some(String::from_utf8(bytes).map_err(|_| clipboard_error())?)
            } else {
                if clipboard_win::raw::count_formats().unwrap_or(1) > 0 {
                    return Err(AppError::new("clipboard_preservation", "剪贴板包含无法备份的非文本内容。请先手动复制选区，再用「翻译剪贴板」，或在设置中启用仅剪贴板模式。"));
                }
                None
            };
            (sequence, previous)
        };
        // Do not destroy an image/file-only clipboard that cannot be backed up as text.
        if unsafe { GetClipboardSequenceNumber() } != before_sequence {
            return Err(AppError::new(
                "clipboard_changed",
                "剪贴板正在被其他应用修改，请稍后重试。",
            ));
        }
        let key = |vk, flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = [
            key(VK_CONTROL, 0),
            key(0x43, 0),
            key(0x43, KEYEVENTF_KEYUP),
            key(VK_CONTROL, KEYEVENTF_KEYUP),
        ];
        if unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                size_of::<INPUT>() as i32,
            )
        } != inputs.len() as u32
        {
            // Ensure a partial SendInput never leaves a synthetic modifier held down.
            let release = [key(0x43, KEYEVENTF_KEYUP), key(VK_CONTROL, KEYEVENTF_KEYUP)];
            unsafe {
                SendInput(
                    release.len() as u32,
                    release.as_ptr(),
                    size_of::<INPUT>() as i32,
                );
            }
            return Err(AppError::new(
                "selection_permission",
                "系统拒绝模拟复制。目标应用可能以管理员身份运行；请手动复制后翻译。",
            ));
        }
        let deadline = Instant::now() + Duration::from_millis(1000);
        while unsafe { GetClipboardSequenceNumber() } == before_sequence {
            if Instant::now() >= deadline {
                return Err(AppError::new("empty_selection", "未检测到选中文字，或目标应用阻止复制。请重新选择，或手动 Ctrl+C 后使用「翻译剪贴板」。"));
            }
            sleep(Duration::from_millis(20));
        }
        let copied_sequence = unsafe { GetClipboardSequenceNumber() };
        let mut selected = clipboard.get_text();
        // Some applications update the sequence before finishing delayed rendering.
        for _ in 0..5 {
            if selected.is_ok() || unsafe { GetClipboardSequenceNumber() } != copied_sequence {
                break;
            }
            sleep(Duration::from_millis(20));
            selected = clipboard.get_text();
        }
        let mut warning = None;
        if let Ok(_lock) = clipboard_win::Clipboard::new_attempts(10) {
            // Check and restore while holding the OS clipboard lock, avoiding a TOCTOU overwrite.
            if unsafe { GetClipboardSequenceNumber() } == copied_sequence {
                let restored = match previous {
                    Some(text) => clipboard_win::raw::set_string(&text).is_ok(),
                    None => clipboard_win::raw::empty().is_ok(),
                };
                if !restored {
                    warning = Some("已读取选区，但原剪贴板文本恢复失败。".into());
                }
            } else {
                warning = Some("检测到其他应用修改剪贴板，已跳过恢复以保留最新内容。".into());
            }
        } else {
            warning = Some("已读取选区，但剪贴板被其他应用占用，未恢复原内容。".into());
        }
        Ok(Selection {
            text: validate_input(&selected.map_err(|_| clipboard_error())?)?,
            source: "windows-selection".into(),
            warning,
        })
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::{process::Stdio, time::Duration};
    use tokio::io::AsyncReadExt;

    pub async fn capture(clipboard_only: bool) -> AppResult<Selection> {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s == "wayland");
        if wayland {
            let operation = async {
                let mut child = tokio::process::Command::new("wl-paste")
                    .args(["--no-newline", "--type", "text"])
                    .stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true)
                    .spawn().map_err(|_| AppError::new("wayland_clipboard", "Wayland 需要 wl-clipboard。请安装：sudo apt install wl-clipboard，然后 Ctrl+C 复制文字再翻译。"))?;
                let stdout = child.stdout.take().ok_or_else(clipboard_error)?;
                let mut bytes = Vec::new();
                stdout
                    .take(128_001)
                    .read_to_end(&mut bytes)
                    .await
                    .map_err(|_| clipboard_error())?;
                if bytes.len() > 128_000 {
                    return Err(AppError::new(
                        "input_too_long",
                        "剪贴板内容过大，请缩小选区。",
                    ));
                }
                if !child.wait().await.map_err(|_| clipboard_error())?.success() {
                    return Err(AppError::new(
                        "empty_selection",
                        "未读到 Wayland 剪贴板文字。请先 Ctrl+C，再点击「翻译剪贴板」。",
                    ));
                }
                let text = String::from_utf8(bytes).map_err(|_| clipboard_error())?;
                Ok(Selection {
                    text: validate_input(&text)?,
                    source: "wayland-clipboard".into(),
                    warning: if clipboard_only {
                        None
                    } else {
                        Some("Wayland 使用已复制的剪贴板文字；请确认已先按 Ctrl+C。".into())
                    },
                })
            };
            return tokio::time::timeout(Duration::from_secs(3), operation)
                .await
                .map_err(|_| {
                    AppError::new("clipboard_timeout", "剪贴板读取超时，请重新复制后重试。")
                })?;
        }
        let task = tauri::async_runtime::spawn_blocking(move || {
            use arboard::{GetExtLinux, LinuxClipboardKind};
            let mut clipboard = arboard::Clipboard::new().map_err(|_| clipboard_error())?;
            if !clipboard_only {
                if let Ok(text) = clipboard
                    .get()
                    .clipboard(LinuxClipboardKind::Primary)
                    .text()
                {
                    if !text.trim().is_empty() {
                        return Ok(Selection {
                            text: validate_input(&text)?,
                            source: "x11-primary".into(),
                            warning: None,
                        });
                    }
                }
            }
            let text = clipboard.get_text().map_err(|_| clipboard_error())?;
            Ok(Selection {
                text: validate_input(&text)?,
                source: "clipboard".into(),
                warning: if clipboard_only {
                    None
                } else {
                    Some("PRIMARY 选区为空，已使用剪贴板。请确认内容是刚刚复制的文字。".into())
                },
            })
        });
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .map_err(|_| {
                AppError::new(
                    "clipboard_timeout",
                    "X11 剪贴板读取超时，请重新复制后重试。",
                )
            })?
            .map_err(|_| clipboard_error())?
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        #[ignore = "Requires an isolated Xvfb display; see docs/verification.md"]
        async fn isolated_x11_primary_clipboard_and_copy_roundtrip() {
            assert_eq!(
                std::env::var("AI_SELECTION_TEST_X11").as_deref(),
                Ok("1"),
                "Run only under an isolated Xvfb display, never against the user's desktop clipboard"
            );
            assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
            use arboard::{LinuxClipboardKind, SetExtLinux};
            let mut owner = arboard::Clipboard::new().unwrap();
            owner
                .set()
                .clipboard(LinuxClipboardKind::Primary)
                .text("Selected JVM text")
                .unwrap();
            owner.set_text("Explicit clipboard text").unwrap();

            let primary = capture(false).await.unwrap();
            assert_eq!(primary.text, "Selected JVM text");
            assert_eq!(primary.source, "x11-primary");
            assert!(primary.warning.is_none());

            let clipboard = capture(true).await.unwrap();
            assert_eq!(clipboard.text, "Explicit clipboard text");
            assert_eq!(clipboard.source, "clipboard");

            super::super::copy_text("复制译文测试".into())
                .await
                .unwrap();
            assert_eq!(owner.get_text().unwrap(), "复制译文测试");
        }
    }
}
