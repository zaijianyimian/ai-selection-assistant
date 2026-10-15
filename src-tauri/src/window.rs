use crate::error::{AppError, AppResult};
use tauri::{Manager, Window};

pub fn show(app: &tauri::AppHandle, label: &str) -> AppResult<()> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| AppError::new("window", "找不到应用窗口，请重启应用。"))?;
    window
        .show()
        .and_then(|_| window.unminimize())
        .and_then(|_| window.set_focus())
        .map_err(|_| AppError::new("window", "无法显示窗口，请从系统托盘重试。"))
}

pub fn hide_on_close(window: &Window, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
    }
}

pub fn tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::{
        menu::{Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };
    let open = MenuItem::with_id(app, "open", "打开翻译 / 设置", true, None::<&str>)?;
    let clipboard = MenuItem::with_id(app, "clipboard", "翻译剪贴板", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出应用", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &clipboard, &quit])?;
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))?;
    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("AI Selection Assistant · 划词翻译")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                let _ = show(app, "main");
            }
            "clipboard" => {
                let _ = crate::start_job(app.clone(), None, true, true);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                let _ = show(tray.app_handle(), "main");
            }
        })
        .build(app)?;
    Ok(())
}
