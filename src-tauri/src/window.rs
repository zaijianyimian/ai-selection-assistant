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

pub fn wayland() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s == "wayland")
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Capture before showing any assistant window. The pointer approximates the selection endpoint.
pub fn selection_anchor(app: &tauri::AppHandle) -> Option<tauri::PhysicalPosition<f64>> {
    if wayland() {
        return None;
    }
    app.get_webview_window("translation")?
        .cursor_position()
        .ok()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Placement {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
fn place(
    anchor: (f64, f64),
    size: (f64, f64),
    area: (f64, f64, f64, f64),
    scale: f64,
) -> Option<Placement> {
    let (ax, ay) = anchor;
    let (left, top, aw, ah) = area;
    if ![ax, ay, size.0, size.1, left, top, aw, ah, scale]
        .iter()
        .all(|v| v.is_finite())
        || size.0 <= 0.0
        || size.1 <= 0.0
        || aw <= 0.0
        || ah <= 0.0
        || scale <= 0.0
    {
        return None;
    }
    let padding = (8.0 * scale).min(aw / 4.0).min(ah / 4.0);
    let width = size.0.min(aw - 2.0 * padding);
    let height = size.1.min(ah - 2.0 * padding);
    let mut x = ax + 12.0 * scale;
    let mut y = ay + 16.0 * scale;
    if x + width > left + aw - padding {
        x = ax - width - 12.0 * scale;
    }
    if y + height > top + ah - padding {
        y = ay - height - 16.0 * scale;
    }
    Some(Placement {
        x: x.clamp(left + padding, left + aw - padding - width),
        y: y.clamp(top + padding, top + ah - padding - height),
        width,
        height,
    })
}

/// Returns a non-fatal placement warning. Wayland keeps compositor-managed manual placement.
pub async fn show_translation(
    app: &tauri::AppHandle,
    anchor: Option<tauri::PhysicalPosition<f64>>,
) -> AppResult<Option<String>> {
    let window = app
        .get_webview_window("translation")
        .ok_or_else(|| AppError::new("window", "找不到翻译窗口，请重启应用。"))?;
    // GTK needs the hidden window mapped before the window manager accepts a move.
    // Source capture has already completed and the anchor is retained.
    show(app, "translation")?;
    let positioning = if wayland() {
        Ok(())
    } else {
        async {
            let anchor = anchor.ok_or(tauri::Error::FailedToReceiveMessage)?;
            let monitor = match window.monitor_from_point(anchor.x, anchor.y)? {
                Some(monitor) => monitor,
                None => window
                    .current_monitor()?
                    .ok_or(tauri::Error::FailedToReceiveMessage)?,
            };
            let area = monitor.work_area();
            let mut size = window.outer_size()?;
            // A previously hidden GTK window reports 0×0 until its first configure event.
            // Yield to the event loop instead of treating this transient state as a failure.
            for _ in 0..25 {
                if size.width > 0 && size.height > 0 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                size = window.outer_size()?;
            }
            let placement = place(
                (anchor.x, anchor.y),
                (size.width as f64, size.height as f64),
                (
                    area.position.x as f64,
                    area.position.y as f64,
                    area.size.width as f64,
                    area.size.height as f64,
                ),
                monitor.scale_factor(),
            )
            .ok_or(tauri::Error::FailedToReceiveMessage)?;
            if placement.width < size.width as f64 || placement.height < size.height as f64 {
                window.set_min_size(Some(tauri::PhysicalSize::new(
                    placement.width as u32,
                    placement.height as u32,
                )))?;
                window.set_size(tauri::PhysicalSize::new(
                    placement.width as u32,
                    placement.height as u32,
                ))?;
            }
            window.set_position(tauri::PhysicalPosition::new(
                placement.x.round() as i32,
                placement.y.round() as i32,
            ))
        }
        .await
    };
    Ok(positioning
        .err()
        .map(|_| "无法自动定位到划选附近，已沿用悬浮窗位置；译文仍可使用。".into()))
}

#[cfg(test)]
mod placement_tests {
    use super::*;
    #[test]
    fn placement_uses_scaled_physical_offsets_and_flips_at_edges() {
        assert_eq!(
            place(
                (100.0, 100.0),
                (100.0, 100.0),
                (0.0, 0.0, 1000.0, 800.0),
                1.0
            )
            .unwrap(),
            Placement {
                x: 112.0,
                y: 116.0,
                width: 100.0,
                height: 100.0
            }
        );
        let edge = place(
            (990.0, 790.0),
            (100.0, 100.0),
            (0.0, 0.0, 1000.0, 800.0),
            1.0,
        )
        .unwrap();
        assert_eq!((edge.x, edge.y), (878.0, 674.0));
        let scaled = place(
            (300.0, 300.0),
            (400.0, 500.0),
            (0.0, 0.0, 2000.0, 1600.0),
            2.0,
        )
        .unwrap();
        assert_eq!((scaled.x, scaled.y), (324.0, 332.0));
        let negative = place(
            (-10.0, 100.0),
            (400.0, 500.0),
            (-1920.0, 0.0, 1920.0, 1080.0),
            1.5,
        )
        .unwrap();
        assert!(negative.x >= -1908.0 && negative.x + negative.width <= -12.0);
    }
    #[test]
    fn placement_clamps_all_corners_and_shrinks_oversized_windows() {
        for anchor in [
            (0.0, 0.0),
            (500.0, 0.0),
            (0.0, 400.0),
            (500.0, 400.0),
            (-30.0, 600.0),
        ] {
            let p = place(anchor, (200.0, 180.0), (0.0, 0.0, 500.0, 400.0), 1.0).unwrap();
            assert!(p.x >= 8.0 && p.y >= 8.0 && p.x + p.width <= 492.0 && p.y + p.height <= 392.0);
        }
        let p = place(
            (150.0, 100.0),
            (2000.0, 2000.0),
            (0.0, 0.0, 300.0, 200.0),
            1.0,
        )
        .unwrap();
        assert_eq!(
            p,
            Placement {
                x: 8.0,
                y: 8.0,
                width: 284.0,
                height: 184.0
            }
        );
        assert!(place((f64::NAN, 0.0), (10.0, 10.0), (0.0, 0.0, 100.0, 100.0), 1.0).is_none());
        assert!(place((0.0, 0.0), (10.0, 10.0), (0.0, 0.0, 0.0, 100.0), 1.0).is_none());
    }
}
