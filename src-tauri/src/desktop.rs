//! Native window lifecycle. Hiding keeps the webview and background tasks alive.
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

const WINDOW_FLAGS: StateFlags = StateFlags::SIZE
    .union(StateFlags::POSITION)
    .union(StateFlags::MAXIMIZED);

pub fn window_state_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_window_state::Builder::default()
        .with_filename(
            crate::path_utils::app_data_dir()
                .join("window-state.json")
                .to_string_lossy(),
        )
        .with_state_flags(WINDOW_FLAGS)
        .with_filter(|label| label == "main")
        .build()
}

pub fn save_window_state(app: &tauri::AppHandle) {
    // A failed preferences write must never destroy or prevent showing a window.
    let _ = app.save_window_state(WINDOW_FLAGS);
}

fn labels(language: &str) -> (String, String) {
    let text = if language == "en" {
        include_str!("../../src/i18n/locales/en.json")
    } else {
        include_str!("../../src/i18n/locales/zh.json")
    };
    let data: serde_json::Value =
        serde_json::from_str(text).expect("Bundled translations are valid");
    (
        data["desktop"]["tray"]["show"]
            .as_str()
            .expect("Show translation")
            .into(),
        data["desktop"]["tray"]["quit"]
            .as_str()
            .expect("Quit translation")
            .into(),
    )
}

struct TrayMenu {
    show: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

pub fn update_language(app: &tauri::AppHandle, language: &str) {
    if let Some(menu) = app.try_state::<TrayMenu>() {
        let (show, quit) = labels(language);
        let _ = menu.show.set_text(show);
        let _ = menu.quit.set_text(quit);
    }
}

pub fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let config =
        crate::config_store::ConfigStore::new(&crate::path_utils::app_data_dir()).load()?;
    let language = config
        .settings
        .get("language")
        .map(String::as_str)
        .unwrap_or("zh");
    let (show_label, quit_label) = labels(language);
    let show = MenuItem::with_id(app, "show-main", show_label, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("Missing application icon")?;
    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("Arsenal")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show-main" => show_main(app),
            "quit" => {
                // The frontend retains the existing confirmation for active tasks.
                show_main(app);
                let _ = app.emit("arsenal-exit-requested", ());
            }
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
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    app.manage(TrayMenu { show, quit });
    if let Some(window) = app.get_webview_window("main") {
        let handle = app.handle().clone();
        let main = window.clone();
        window.on_window_event(move |event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                save_window_state(&handle);
                let _ = main.hide();
            }
            WindowEvent::Focused(false) => save_window_state(&handle),
            _ => {}
        });
    }
    Ok(())
}

#[tauri::command]
pub fn exit_app(app: tauri::AppHandle) {
    save_window_state(&app);
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restores_geometry_but_never_a_hidden_or_minimized_startup() {
        assert!(
            WINDOW_FLAGS.contains(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)
        );
        assert!(!WINDOW_FLAGS
            .intersects(StateFlags::VISIBLE | StateFlags::FULLSCREEN | StateFlags::DECORATIONS));
    }
    #[test]
    fn tray_menu_uses_shared_translations() {
        assert_eq!(labels("zh"), ("显示主窗口".into(), "退出".into()));
        assert_eq!(labels("en"), ("Show main window".into(), "Quit".into()));
    }
}
