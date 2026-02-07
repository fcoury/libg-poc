use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use nvim_rs::compat::tokio::Compat;
use nvim_rs::create::tokio as nvim_create;
use nvim_rs::{Handler, Neovim};
use rmpv::Value;
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::io::WriteHalf;
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

// -- Types --

type Writer = Compat<WriteHalf<UnixStream>>;

#[derive(Clone)]
struct NvimHandler {
    app_handle: tauri::AppHandle,
    terminal_id: String,
}

#[async_trait]
impl Handler for NvimHandler {
    type Writer = Writer;

    async fn handle_notify(
        &self,
        name: String,
        args: Vec<Value>,
        _neovim: Neovim<Self::Writer>,
    ) {
        if name != "libg_action" {
            return;
        }

        let payload = match args.into_iter().next() {
            Some(val) => val,
            None => return,
        };

        let action = match parse_nvim_action(payload) {
            Ok(a) => a,
            Err(e) => {
                log::error!("Failed to parse nvim action: {}", e);
                return;
            }
        };

        let event = NvimActionEvent {
            terminal_id: self.terminal_id.clone(),
            action,
        };

        let _ = self.app_handle.emit("nvim-action", &event);
    }
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

// -- Neovim action types (sent from Neovim → Tauri via rpcnotify) --

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", tag = "action")]
pub enum NvimAction {
    FixDiagnostic {
        file_path: String,
        cursor_line: i64,
        cursor_col: i64,
        diagnostic: ActionDiagnostic,
        context_lines: Vec<String>,
        context_start_line: i64,
    },
    Implement {
        file_path: String,
        file_type: String,
        cursor_line: i64,
        signature_lines: Vec<String>,
        context_lines: Vec<String>,
        context_start_line: i64,
    },
    Explain {
        file_path: String,
        file_type: String,
        cursor_line: i64,
        target_text: String,
        context_lines: Vec<String>,
        context_start_line: i64,
    },
    Ask {
        file_path: String,
        file_type: String,
        cursor_line: i64,
        prompt: String,
        selection: Option<String>,
        context_lines: Vec<String>,
        context_start_line: i64,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ActionDiagnostic {
    pub line: i64,
    pub col: i64,
    pub severity: i64,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NvimActionEvent {
    pub terminal_id: String,
    pub action: NvimAction,
}

// -- Helpers --

fn parse_nvim_action(value: Value) -> Result<NvimAction, String> {
    let json_value: serde_json::Value =
        rmpv::ext::from_value(value).map_err(|e| e.to_string())?;
    serde_json::from_value(json_value).map_err(|e| e.to_string())
}

fn build_lua_setup(channel_id: i64) -> String {
    format!(
        r#"
_G.libg = _G.libg or {{}}
_G.libg.channel = {channel_id}

-- Helper: get context lines around a 1-indexed line
local function get_context(radius)
    local bufnr = vim.api.nvim_get_current_buf()
    local cursor = vim.api.nvim_win_get_cursor(0)
    local line = cursor[1]
    local total = vim.api.nvim_buf_line_count(bufnr)
    local start = math.max(1, line - radius)
    local finish = math.min(total, line + radius)
    local lines = vim.api.nvim_buf_get_lines(bufnr, start - 1, finish, false)
    return lines, start
end

-- Helper: get visual selection text
local function get_visual_selection()
    local start_pos = vim.fn.getpos("'<")
    local end_pos = vim.fn.getpos("'>")
    local start_line = start_pos[2]
    local end_line = end_pos[2]
    local lines = vim.api.nvim_buf_get_lines(0, start_line - 1, end_line, false)
    if #lines == 0 then return "" end
    local start_col = start_pos[3]
    local end_col = end_pos[3]
    if #lines == 1 then
        lines[1] = string.sub(lines[1], start_col, end_col)
    else
        lines[1] = string.sub(lines[1], start_col)
        lines[#lines] = string.sub(lines[#lines], 1, end_col)
    end
    return table.concat(lines, "\n")
end

-- Action: fix diagnostic under cursor
function _G.libg.fix_diagnostic()
    local cursor = vim.api.nvim_win_get_cursor(0)
    local line = cursor[1]
    local col = cursor[2]
    local bufnr = vim.api.nvim_get_current_buf()
    local diags = vim.diagnostic.get(bufnr, {{ lnum = line - 1 }})
    if #diags == 0 then
        vim.notify("[libg] No diagnostic on current line", vim.log.levels.WARN)
        return
    end
    local d = diags[1]
    local context_lines, context_start = get_context(30)
    local file_path = vim.api.nvim_buf_get_name(bufnr)
    vim.rpcnotify({channel_id}, "libg_action", {{
        action = "fixDiagnostic",
        filePath = file_path,
        cursorLine = line,
        cursorCol = col,
        diagnostic = {{
            line = d.lnum,
            col = d.col,
            severity = d.severity,
            message = d.message,
            source = d.source or "",
        }},
        contextLines = context_lines,
        contextStartLine = context_start,
    }})
    vim.notify("[libg] Sent fix-diagnostic to agent", vim.log.levels.INFO)
end

-- Action: implement signature under cursor
function _G.libg.implement()
    local cursor = vim.api.nvim_win_get_cursor(0)
    local line = cursor[1]
    local bufnr = vim.api.nvim_get_current_buf()
    local file_path = vim.api.nvim_buf_get_name(bufnr)
    local file_type = vim.bo[bufnr].filetype
    -- Grab current line as the signature
    local sig_lines = vim.api.nvim_buf_get_lines(bufnr, line - 1, line, false)
    local context_lines, context_start = get_context(50)
    vim.rpcnotify({channel_id}, "libg_action", {{
        action = "implement",
        filePath = file_path,
        fileType = file_type,
        cursorLine = line,
        signatureLines = sig_lines,
        contextLines = context_lines,
        contextStartLine = context_start,
    }})
    vim.notify("[libg] Sent implement to agent", vim.log.levels.INFO)
end

-- Action: explain code (normal or visual)
function _G.libg.explain(use_selection)
    local cursor = vim.api.nvim_win_get_cursor(0)
    local line = cursor[1]
    local bufnr = vim.api.nvim_get_current_buf()
    local file_path = vim.api.nvim_buf_get_name(bufnr)
    local file_type = vim.bo[bufnr].filetype
    local target_text
    if use_selection then
        target_text = get_visual_selection()
    else
        target_text = vim.api.nvim_get_current_line()
    end
    local context_lines, context_start = get_context(30)
    vim.rpcnotify({channel_id}, "libg_action", {{
        action = "explain",
        filePath = file_path,
        fileType = file_type,
        cursorLine = line,
        targetText = target_text,
        contextLines = context_lines,
        contextStartLine = context_start,
    }})
    vim.notify("[libg] Sent explain to agent", vim.log.levels.INFO)
end

-- Action: ask with custom prompt (normal or visual)
function _G.libg.ask(use_selection)
    local prompt = vim.fn.input("libg ask: ")
    if prompt == "" then return end
    local cursor = vim.api.nvim_win_get_cursor(0)
    local line = cursor[1]
    local bufnr = vim.api.nvim_get_current_buf()
    local file_path = vim.api.nvim_buf_get_name(bufnr)
    local file_type = vim.bo[bufnr].filetype
    local selection = nil
    if use_selection then
        selection = get_visual_selection()
    end
    local context_lines, context_start = get_context(30)
    vim.rpcnotify({channel_id}, "libg_action", {{
        action = "ask",
        filePath = file_path,
        fileType = file_type,
        cursorLine = line,
        prompt = prompt,
        selection = selection,
        contextLines = context_lines,
        contextStartLine = context_start,
    }})
    vim.notify("[libg] Sent ask to agent", vim.log.levels.INFO)
end

-- Keybindings (<leader>m prefix = "model")
vim.keymap.set("n", "<leader>mf", function() _G.libg.fix_diagnostic() end, {{ desc = "[libg] Fix diagnostic" }})
vim.keymap.set("n", "<leader>mi", function() _G.libg.implement() end, {{ desc = "[libg] Implement" }})
vim.keymap.set("n", "<leader>me", function() _G.libg.explain(false) end, {{ desc = "[libg] Explain" }})
vim.keymap.set("v", "<leader>me", function() _G.libg.explain(true) end, {{ desc = "[libg] Explain selection" }})
vim.keymap.set("n", "<leader>ma", function() _G.libg.ask(false) end, {{ desc = "[libg] Ask" }})
vim.keymap.set("v", "<leader>ma", function() _G.libg.ask(true) end, {{ desc = "[libg] Ask with selection" }})

-- User commands
vim.api.nvim_create_user_command("LibgFixDiagnostic", function() _G.libg.fix_diagnostic() end, {{}})
vim.api.nvim_create_user_command("LibgImplement", function() _G.libg.implement() end, {{}})
vim.api.nvim_create_user_command("LibgExplain", function() _G.libg.explain(false) end, {{ range = true }})
vim.api.nvim_create_user_command("LibgAsk", function() _G.libg.ask(false) end, {{ range = true }})

vim.notify("[libg] Agent keybindings loaded", vim.log.levels.INFO)
"#,
        channel_id = channel_id,
    )
}

// -- Tauri IPC commands --

#[tauri::command]
pub async fn nvim_connect(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, Mutex<NvimBridgeState>>,
    terminal_id: String,
    socket_path: String,
) -> Result<(), String> {
    let handler = NvimHandler {
        app_handle,
        terminal_id: terminal_id.clone(),
    };

    let (nvim, io_handle) = nvim_create::new_path(&socket_path, handler)
        .await
        .map_err(|e| format!("Failed to connect to neovim at {}: {}", socket_path, e))?;

    // Get the channel ID so Lua can rpcnotify back to us
    let api_info = nvim
        .get_api_info()
        .await
        .map_err(|e| format!("Failed to get api info: {}", e))?;
    let channel_id = api_info
        .first()
        .and_then(|v| v.as_i64())
        .ok_or_else(|| "Failed to extract channel ID from api info".to_string())?;

    // Inject keybindings into neovim
    let lua_setup = build_lua_setup(channel_id);
    nvim.exec_lua(&lua_setup, vec![])
        .await
        .map_err(|e| format!("Failed to inject lua keybindings: {}", e))?;

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
