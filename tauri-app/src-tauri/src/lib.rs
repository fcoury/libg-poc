mod ghostty_embed;

use ghostty_embed::{with_manager, GhosttyOptions, GhosttyRect};

#[tauri::command]
fn ghostty_create(
    window: tauri::Window,
    id: String,
    rect: GhosttyRect,
    options: Option<GhosttyOptions>,
) -> Result<(), String> {
    let options = options.unwrap_or_default();
    let (tx, rx) = std::sync::mpsc::channel();
    let window_clone = window.clone();

    window
        .run_on_main_thread(move || {
            let res = with_manager(|manager| manager.create(&window_clone, id, rect, options));
            let _ = tx.send(res);
        })
        .map_err(|e| e.to_string())?;

    rx.recv().unwrap_or_else(|_| Err("ghostty_create failed".to_string()))
}

#[tauri::command]
fn ghostty_update_rect(
    window: tauri::Window,
    id: String,
    rect: GhosttyRect,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let window_clone = window.clone();

    window
        .run_on_main_thread(move || {
            let res = with_manager(|manager| manager.update_rect(&window_clone, &id, rect));
            let _ = tx.send(res);
        })
        .map_err(|e| e.to_string())?;

    rx.recv().unwrap_or_else(|_| Err("ghostty_update_rect failed".to_string()))
}

#[tauri::command]
fn ghostty_destroy(
    window: tauri::Window,
    id: String,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();

    window
        .run_on_main_thread(move || {
            let res = with_manager(|manager| manager.destroy(&id));
            let _ = tx.send(res);
        })
        .map_err(|e| e.to_string())?;

    rx.recv().unwrap_or_else(|_| Err("ghostty_destroy failed".to_string()))
}

#[tauri::command]
fn ghostty_set_visible(
    window: tauri::Window,
    id: String,
    visible: bool,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();

    window
        .run_on_main_thread(move || {
            let res = with_manager(|manager| manager.set_visible(&id, visible));
            let _ = tx.send(res);
        })
        .map_err(|e| e.to_string())?;

    rx.recv().unwrap_or_else(|_| Err("ghostty_set_visible failed".to_string()))
}

#[tauri::command]
fn ghostty_focus(
    window: tauri::Window,
    id: String,
    focused: bool,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();

    window
        .run_on_main_thread(move || {
            let res = with_manager(|manager| manager.focus(&id, focused));
            let _ = tx.send(res);
        })
        .map_err(|e| e.to_string())?;

    rx.recv().unwrap_or_else(|_| Err("ghostty_focus failed".to_string()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            ghostty_create,
            ghostty_update_rect,
            ghostty_destroy,
            ghostty_set_visible,
            ghostty_focus
        ]);

    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(tauri_plugin_mcp_bridge::init());
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
