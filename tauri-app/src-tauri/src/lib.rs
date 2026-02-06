mod acp_client;
mod ghostty_embed;
mod nvim_bridge;

use ghostty_embed::{with_manager, GhosttyOptions, GhosttyRect};
use tokio::sync::Mutex;

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

#[tauri::command]
fn ghostty_write_text(
    window: tauri::Window,
    id: String,
    text: String,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();

    window
        .run_on_main_thread(move || {
            let res = with_manager(|manager| manager.write_text(&id, &text));
            let _ = tx.send(res);
        })
        .map_err(|e| e.to_string())?;

    rx.recv().unwrap_or_else(|_| Err("ghostty_write_text failed".to_string()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(nvim_bridge::NvimBridgeState::new()))
        .manage(Mutex::new(acp_client::AcpClientState::new()))
        .invoke_handler(tauri::generate_handler![
            // Ghostty
            ghostty_create,
            ghostty_update_rect,
            ghostty_destroy,
            ghostty_set_visible,
            ghostty_focus,
            ghostty_write_text,
            // Neovim bridge
            nvim_bridge::nvim_connect,
            nvim_bridge::nvim_disconnect,
            nvim_bridge::nvim_connection_status,
            nvim_bridge::nvim_get_context,
            nvim_bridge::nvim_get_diagnostics,
            nvim_bridge::nvim_get_buffer_content,
            nvim_bridge::nvim_apply_edit,
            nvim_bridge::nvim_apply_edits,
            nvim_bridge::nvim_exec_command,
            // ACP agent
            acp_client::acp_start_agent,
            acp_client::acp_stop_agent,
            acp_client::acp_agent_status,
            acp_client::acp_create_session,
            acp_client::acp_send_prompt,
        ]);

    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(tauri_plugin_mcp_bridge::init());
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
