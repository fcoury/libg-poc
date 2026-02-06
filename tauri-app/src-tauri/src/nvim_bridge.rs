use std::collections::HashMap;
use std::sync::Arc;

use nvim_rs::compat::tokio::Compat;
use nvim_rs::create::tokio as nvim_create;
use nvim_rs::{Handler, Neovim};
use rmpv::Value;
use serde::{Deserialize, Serialize};
use tokio::io::WriteHalf;
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

// -- Types --

type Writer = Compat<WriteHalf<UnixStream>>;

#[derive(Clone)]
struct NvimHandler;

impl Handler for NvimHandler {
    type Writer = Writer;
}

struct NvimConnection {
    nvim: Neovim<Writer>,
    _io_handle: JoinHandle<Result<(), Box<nvim_rs::error::LoopError>>>,
    socket_path: String,
}

pub struct NvimBridgeState {
    connections: HashMap<String, Arc<Mutex<NvimConnection>>>,
}

impl NvimBridgeState {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }
}

// -- Serializable types for IPC --

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CursorPosition {
    pub line: i64,
    pub col: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NvimContext {
    pub cursor: CursorPosition,
    pub file_path: String,
    pub file_type: String,
    pub buffer_id: i64,
    pub line_count: i64,
    pub modified: bool,
    pub visible_lines: Vec<String>,
    pub visible_range: (i64, i64),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub line: i64,
    pub col: i64,
    pub severity: i64,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BufferContent {
    pub file_path: String,
    pub lines: Vec<String>,
    pub line_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BufferEdit {
    pub start_line: i64,
    pub end_line: i64,
    pub new_lines: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionStatus {
    Connected { socket_path: String },
    Disconnected,
    Error(String),
}

// -- Tauri IPC commands --

#[tauri::command]
pub async fn nvim_connect(
    state: tauri::State<'_, Mutex<NvimBridgeState>>,
    terminal_id: String,
    socket_path: String,
) -> Result<(), String> {
    let (nvim, io_handle) = nvim_create::new_path(&socket_path, NvimHandler)
        .await
        .map_err(|e| format!("Failed to connect to neovim at {}: {}", socket_path, e))?;

    let conn = NvimConnection {
        nvim,
        _io_handle: io_handle,
        socket_path: socket_path.clone(),
    };

    let mut bridge = state.lock().await;
    bridge
        .connections
        .insert(terminal_id, Arc::new(Mutex::new(conn)));
    Ok(())
}

#[tauri::command]
pub async fn nvim_disconnect(
    state: tauri::State<'_, Mutex<NvimBridgeState>>,
    terminal_id: String,
) -> Result<(), String> {
    let mut bridge = state.lock().await;
    bridge.connections.remove(&terminal_id);
    Ok(())
}

#[tauri::command]
pub async fn nvim_connection_status(
    state: tauri::State<'_, Mutex<NvimBridgeState>>,
    terminal_id: String,
) -> Result<ConnectionStatus, String> {
    let bridge = state.lock().await;
    match bridge.connections.get(&terminal_id) {
        Some(conn) => {
            let conn = conn.lock().await;
            Ok(ConnectionStatus::Connected {
                socket_path: conn.socket_path.clone(),
            })
        }
        None => Ok(ConnectionStatus::Disconnected),
    }
}

#[tauri::command]
pub async fn nvim_get_context(
    state: tauri::State<'_, Mutex<NvimBridgeState>>,
    terminal_id: String,
) -> Result<NvimContext, String> {
    let bridge = state.lock().await;
    let conn = bridge
        .connections
        .get(&terminal_id)
        .ok_or_else(|| format!("No neovim connection for terminal: {}", terminal_id))?
        .clone();
    drop(bridge);

    let conn = conn.lock().await;
    let nvim = &conn.nvim;

    let win = nvim.get_current_win().await.map_err(|e| e.to_string())?;
    let buf = nvim.get_current_buf().await.map_err(|e| e.to_string())?;

    let (cursor_line, cursor_col) = win.get_cursor().await.map_err(|e| e.to_string())?;
    let file_path = buf.get_name().await.map_err(|e| e.to_string())?;
    let line_count = buf.line_count().await.map_err(|e| e.to_string())?;

    let file_type = nvim
        .exec_lua(
            "return vim.bo[vim.api.nvim_get_current_buf()].filetype",
            vec![],
        )
        .await
        .map_err(|e| e.to_string())?;
    let file_type = match file_type {
        Value::String(s) => s.into_str().unwrap_or_default(),
        _ => String::new(),
    };

    let modified = nvim
        .exec_lua(
            "return vim.bo[vim.api.nvim_get_current_buf()].modified",
            vec![],
        )
        .await
        .map_err(|e| e.to_string())?;
    let modified = matches!(modified, Value::Boolean(true));

    let buffer_id = buf.get_number().await.map_err(|e| e.to_string())?;

    // Get visible lines: cursor_line +/- 50
    let start = (cursor_line - 50).max(1) - 1; // 0-indexed for get_lines
    let end = (cursor_line + 50).min(line_count);
    let visible_lines = buf
        .get_lines(start, end, false)
        .await
        .map_err(|e| e.to_string())?;

    Ok(NvimContext {
        cursor: CursorPosition {
            line: cursor_line,
            col: cursor_col,
        },
        file_path,
        file_type,
        buffer_id,
        line_count,
        modified,
        visible_lines,
        visible_range: (start + 1, end), // 1-indexed for display
    })
}

#[tauri::command]
pub async fn nvim_get_diagnostics(
    state: tauri::State<'_, Mutex<NvimBridgeState>>,
    terminal_id: String,
) -> Result<Vec<Diagnostic>, String> {
    let bridge = state.lock().await;
    let conn = bridge
        .connections
        .get(&terminal_id)
        .ok_or_else(|| format!("No neovim connection for terminal: {}", terminal_id))?
        .clone();
    drop(bridge);

    let conn = conn.lock().await;
    let nvim = &conn.nvim;

    let result = nvim
        .exec_lua(
            r#"
            local bufnr = vim.api.nvim_get_current_buf()
            local diagnostics = vim.diagnostic.get(bufnr)
            local result = {}
            for _, d in ipairs(diagnostics) do
                table.insert(result, {
                    lnum = d.lnum,
                    col = d.col,
                    severity = d.severity,
                    message = d.message,
                    source = d.source or "",
                })
            end
            return vim.json.encode(result)
            "#,
            vec![],
        )
        .await
        .map_err(|e| e.to_string())?;

    let json_str = match result {
        Value::String(s) => s.as_str().unwrap_or("[]").to_string(),
        _ => "[]".to_string(),
    };

    let raw: Vec<serde_json::Value> =
        serde_json::from_str(&json_str).map_err(|e| e.to_string())?;

    let diagnostics = raw
        .into_iter()
        .map(|d| Diagnostic {
            line: d["lnum"].as_i64().unwrap_or(0),
            col: d["col"].as_i64().unwrap_or(0),
            severity: d["severity"].as_i64().unwrap_or(0),
            message: d["message"].as_str().unwrap_or("").to_string(),
            source: d["source"].as_str().unwrap_or("").to_string(),
        })
        .collect();

    Ok(diagnostics)
}

#[tauri::command]
pub async fn nvim_get_buffer_content(
    state: tauri::State<'_, Mutex<NvimBridgeState>>,
    terminal_id: String,
) -> Result<BufferContent, String> {
    let bridge = state.lock().await;
    let conn = bridge
        .connections
        .get(&terminal_id)
        .ok_or_else(|| format!("No neovim connection for terminal: {}", terminal_id))?
        .clone();
    drop(bridge);

    let conn = conn.lock().await;
    let nvim = &conn.nvim;

    let buf = nvim.get_current_buf().await.map_err(|e| e.to_string())?;
    let file_path = buf.get_name().await.map_err(|e| e.to_string())?;
    let line_count = buf.line_count().await.map_err(|e| e.to_string())?;
    let lines = buf
        .get_lines(0, line_count, false)
        .await
        .map_err(|e| e.to_string())?;

    Ok(BufferContent {
        file_path,
        lines,
        line_count,
    })
}

#[tauri::command]
pub async fn nvim_apply_edit(
    state: tauri::State<'_, Mutex<NvimBridgeState>>,
    terminal_id: String,
    edit: BufferEdit,
) -> Result<(), String> {
    let bridge = state.lock().await;
    let conn = bridge
        .connections
        .get(&terminal_id)
        .ok_or_else(|| format!("No neovim connection for terminal: {}", terminal_id))?
        .clone();
    drop(bridge);

    let conn = conn.lock().await;
    let nvim = &conn.nvim;

    let buf = nvim.get_current_buf().await.map_err(|e| e.to_string())?;
    buf.set_lines(edit.start_line, edit.end_line, false, edit.new_lines)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn nvim_apply_edits(
    state: tauri::State<'_, Mutex<NvimBridgeState>>,
    terminal_id: String,
    edits: Vec<BufferEdit>,
) -> Result<(), String> {
    let bridge = state.lock().await;
    let conn = bridge
        .connections
        .get(&terminal_id)
        .ok_or_else(|| format!("No neovim connection for terminal: {}", terminal_id))?
        .clone();
    drop(bridge);

    let conn = conn.lock().await;
    let nvim = &conn.nvim;

    let buf = nvim.get_current_buf().await.map_err(|e| e.to_string())?;

    // Apply edits in reverse order to preserve line numbers
    let mut sorted_edits = edits;
    sorted_edits.sort_by(|a, b| b.start_line.cmp(&a.start_line));

    for edit in sorted_edits {
        buf.set_lines(edit.start_line, edit.end_line, false, edit.new_lines)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn nvim_exec_command(
    state: tauri::State<'_, Mutex<NvimBridgeState>>,
    terminal_id: String,
    command: String,
) -> Result<String, String> {
    let bridge = state.lock().await;
    let conn = bridge
        .connections
        .get(&terminal_id)
        .ok_or_else(|| format!("No neovim connection for terminal: {}", terminal_id))?
        .clone();
    drop(bridge);

    let conn = conn.lock().await;
    let nvim = &conn.nvim;

    let output = nvim
        .command_output(&command)
        .await
        .map_err(|e| e.to_string())?;

    Ok(output)
}
