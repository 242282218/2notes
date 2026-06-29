use tauri::{
    menu::{Menu, MenuEvent, MenuItem},
    tray::TrayIconBuilder,
    AppHandle,
};

use crate::{error::AppResult, system::windows};

pub fn create_tray(app: &AppHandle) -> AppResult<()> {
    let open_main = MenuItem::with_id(app, "open_main", "打开主窗口", true, None::<&str>)?;
    let quick_capture = MenuItem::with_id(app, "quick_capture", "快速记录", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_main, &quick_capture, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("2notes")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event: MenuEvent| match event.id().as_ref() {
            "open_main" => windows::show_main_window(app),
            "quick_capture" => {
                if let Err(err) = windows::show_quick_capture_window(app) {
                    log::error!("quick_capture_open_failed source={err}");
                }
            }
            "quit" => {
                if let Err(err) = windows::request_app_quit(app) {
                    log::error!("app_quit_request_failed source={err}");
                }
            }
            _ => {}
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;

    Ok(())
}
