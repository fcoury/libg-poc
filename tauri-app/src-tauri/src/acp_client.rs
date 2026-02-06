use std::path::PathBuf;

use agent_client_protocol as acp;
use acp::Agent as _;
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

// -- Serializable types for IPC --

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum AgentStatus {
    Stopped,
    Starting,
    Running,
    Error(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "camelCase")]
pub enum AcpEvent {
    ContentChunk(String),
    ThoughtChunk(String),
    ToolCallStarted {
        id: String,
        title: String,
        kind: String,
    },
    ToolCallUpdated {
        id: String,
        status: String,
    },
    Done {
        stop_reason: String,
    },
    Error(String),
}

// -- Channel-based communication with the !Send ACP connection --

enum AcpCommand {
    CreateSession {
        working_dir: PathBuf,
        reply: oneshot::Sender<Result<String, String>>,
    },
    Prompt {
        messages: Vec<String>,
        context: Option<String>,
        reply: oneshot::Sender<Result<String, String>>,
    },
    Shutdown,
}

struct AcpClientHandler {
    app_handle: tauri::AppHandle,
}

#[async_trait::async_trait(?Send)]
impl acp::Client for AcpClientHandler {
    async fn request_permission(
        &self,
        _args: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn session_notification(&self, args: acp::SessionNotification) -> acp::Result<()> {
        let event = match args.update {
            acp::SessionUpdate::AgentMessageChunk(chunk) => {
                if let acp::ContentBlock::Text(text) = chunk.content {
                    AcpEvent::ContentChunk(text.text)
                } else {
                    return Ok(());
                }
            }
            acp::SessionUpdate::AgentThoughtChunk(chunk) => {
                if let acp::ContentBlock::Text(text) = chunk.content {
                    AcpEvent::ThoughtChunk(text.text)
                } else {
                    return Ok(());
                }
            }
            acp::SessionUpdate::ToolCall(tool_call) => AcpEvent::ToolCallStarted {
                id: tool_call.tool_call_id.to_string(),
                title: tool_call.title,
                kind: format!("{:?}", tool_call.kind),
            },
            acp::SessionUpdate::ToolCallUpdate(update) => AcpEvent::ToolCallUpdated {
                id: update.tool_call_id.to_string(),
                status: "updated".to_string(),
            },
            _ => return Ok(()),
        };

        let _ = self.app_handle.emit("acp-event", &event);
        Ok(())
    }
}

/// Runs on a dedicated thread with a LocalSet. Owns the !Send ACP connection
/// and processes commands from the Send world via channels.
async fn acp_worker(
    app_handle: tauri::AppHandle,
    agent_path: String,
    mut cmd_rx: mpsc::Receiver<AcpCommand>,
    ready_tx: oneshot::Sender<Result<(), String>>,
) {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            // Spawn agent process
            let mut child = match tokio::process::Command::new(&agent_path)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()
            {
                Ok(child) => child,
                Err(e) => {
                    let _ = ready_tx
                        .send(Err(format!("Failed to spawn agent '{}': {}", agent_path, e)));
                    return;
                }
            };

            let agent_stdin = match child.stdin.take() {
                Some(stdin) => stdin.compat_write(),
                None => {
                    let _ = ready_tx.send(Err("Failed to take agent stdin".to_string()));
                    return;
                }
            };
            let agent_stdout = match child.stdout.take() {
                Some(stdout) => stdout.compat(),
                None => {
                    let _ = ready_tx.send(Err("Failed to take agent stdout".to_string()));
                    return;
                }
            };

            let handler = AcpClientHandler {
                app_handle: app_handle.clone(),
            };

            let (conn, io_future) = acp::ClientSideConnection::new(
                handler,
                agent_stdin,
                agent_stdout,
                |fut| {
                    tokio::task::spawn_local(fut);
                },
            );

            // Drive I/O in background
            tokio::task::spawn_local(io_future);

            // Initialize handshake
            let init_result = conn
                .initialize(
                    acp::InitializeRequest::new(acp::ProtocolVersion::V1).client_info(
                        acp::Implementation::new("libg", "0.1.0").title("libg Terminal IDE"),
                    ),
                )
                .await;

            match init_result {
                Ok(resp) => {
                    log::info!(
                        "ACP agent initialized: {:?}",
                        resp.agent_info.as_ref().map(|i| &i.name)
                    );
                    let _ = ready_tx.send(Ok(()));
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(format!("ACP initialize failed: {}", e)));
                    return;
                }
            }

            // Track session ID
            let mut current_session_id: Option<acp::SessionId> = None;

            // Process commands from the Send world
            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    AcpCommand::CreateSession { working_dir, reply } => {
                        let result = conn
                            .new_session(acp::NewSessionRequest::new(working_dir))
                            .await;
                        match result {
                            Ok(resp) => {
                                let sid = resp.session_id.to_string();
                                current_session_id = Some(resp.session_id);
                                let _ = reply.send(Ok(sid));
                            }
                            Err(e) => {
                                let _ =
                                    reply.send(Err(format!("Failed to create session: {}", e)));
                            }
                        }
                    }
                    AcpCommand::Prompt {
                        messages,
                        context,
                        reply,
                    } => {
                        let sid = match &current_session_id {
                            Some(sid) => sid.clone(),
                            None => {
                                let _ = reply.send(Err("No active session".to_string()));
                                continue;
                            }
                        };

                        let mut prompt_blocks: Vec<acp::ContentBlock> = Vec::new();
                        if let Some(ctx) = context {
                            prompt_blocks.push(ctx.into());
                        }
                        for msg in messages {
                            prompt_blocks.push(msg.into());
                        }

                        let result = conn.prompt(acp::PromptRequest::new(sid, prompt_blocks)).await;
                        match result {
                            Ok(resp) => {
                                let stop_reason = format!("{:?}", resp.stop_reason);
                                let _ = app_handle.emit(
                                    "acp-event",
                                    &AcpEvent::Done {
                                        stop_reason: stop_reason.clone(),
                                    },
                                );
                                let _ = reply.send(Ok(stop_reason));
                            }
                            Err(e) => {
                                let _ = reply.send(Err(format!("Prompt failed: {}", e)));
                            }
                        }
                    }
                    AcpCommand::Shutdown => {
                        break;
                    }
                }
            }

            // Clean up
            let _ = child.kill().await;
        })
        .await;
}

// -- Managed state --

pub struct AcpClientState {
    cmd_tx: Option<mpsc::Sender<AcpCommand>>,
    worker_handle: Option<std::thread::JoinHandle<()>>,
    status: AgentStatus,
}

impl AcpClientState {
    pub fn new() -> Self {
        Self {
            cmd_tx: None,
            worker_handle: None,
            status: AgentStatus::Stopped,
        }
    }
}

// -- Tauri IPC commands --

#[tauri::command]
pub async fn acp_start_agent(
    state: tauri::State<'_, Mutex<AcpClientState>>,
    app_handle: tauri::AppHandle,
    agent_path: String,
) -> Result<(), String> {
    let mut acp_state = state.lock().await;

    if acp_state.cmd_tx.is_some() {
        return Err("Agent already running. Stop it first.".to_string());
    }

    acp_state.status = AgentStatus::Starting;

    let (cmd_tx, cmd_rx) = mpsc::channel::<AcpCommand>(32);
    let (ready_tx, ready_rx) = oneshot::channel();

    let handle = app_handle.clone();
    let path = agent_path.clone();

    // Spawn a dedicated thread with its own tokio runtime + LocalSet
    let worker_handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create ACP worker runtime");

        rt.block_on(acp_worker(handle, path, cmd_rx, ready_tx));
    });

    // Wait for initialization to complete
    let init_result = ready_rx.await.map_err(|_| "Worker thread died".to_string())?;

    match init_result {
        Ok(()) => {
            acp_state.cmd_tx = Some(cmd_tx);
            acp_state.worker_handle = Some(worker_handle);
            acp_state.status = AgentStatus::Running;
            Ok(())
        }
        Err(e) => {
            acp_state.status = AgentStatus::Error(e.clone());
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn acp_stop_agent(
    state: tauri::State<'_, Mutex<AcpClientState>>,
) -> Result<(), String> {
    let mut acp_state = state.lock().await;

    if let Some(tx) = acp_state.cmd_tx.take() {
        let _ = tx.send(AcpCommand::Shutdown).await;
    }

    // The worker thread will exit after processing Shutdown
    if let Some(handle) = acp_state.worker_handle.take() {
        let _ = handle.join();
    }

    acp_state.status = AgentStatus::Stopped;
    Ok(())
}

#[tauri::command]
pub async fn acp_agent_status(
    state: tauri::State<'_, Mutex<AcpClientState>>,
) -> Result<AgentStatus, String> {
    let acp_state = state.lock().await;
    Ok(acp_state.status.clone())
}

#[tauri::command]
pub async fn acp_create_session(
    state: tauri::State<'_, Mutex<AcpClientState>>,
    working_dir: String,
) -> Result<String, String> {
    let acp_state = state.lock().await;

    let tx = acp_state
        .cmd_tx
        .as_ref()
        .ok_or("No agent running")?;

    let (reply_tx, reply_rx) = oneshot::channel();

    tx.send(AcpCommand::CreateSession {
        working_dir: PathBuf::from(&working_dir),
        reply: reply_tx,
    })
    .await
    .map_err(|_| "Agent worker died".to_string())?;

    reply_rx
        .await
        .map_err(|_| "Agent worker died".to_string())?
}

#[tauri::command]
pub async fn acp_send_prompt(
    state: tauri::State<'_, Mutex<AcpClientState>>,
    _app_handle: tauri::AppHandle,
    _session_id: String,
    messages: Vec<String>,
    context: Option<String>,
) -> Result<String, String> {
    let acp_state = state.lock().await;

    let tx = acp_state
        .cmd_tx
        .as_ref()
        .ok_or("No agent running")?;

    let (reply_tx, reply_rx) = oneshot::channel();

    tx.send(AcpCommand::Prompt {
        messages,
        context,
        reply: reply_tx,
    })
    .await
    .map_err(|_| "Agent worker died".to_string())?;

    reply_rx
        .await
        .map_err(|_| "Agent worker died".to_string())?
}
