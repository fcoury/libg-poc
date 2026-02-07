import { useRef, useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAiChat } from "../../hooks/useAiChat";
import { ContextBadge } from "./ContextBadge";
import { ChatMessage } from "./ChatMessage";
import { ChatInput } from "./ChatInput";
import "./AiChat.css";

type Props = {
  terminalId: string | null;
};

export function AiChat({ terminalId }: Props) {
  const {
    messages,
    isStreaming,
    autoApply,
    setAutoApply,
    sendMessage,
    applyProposedEdits,
    rejectProposedEdits,
    nvim,
    acp,
  } = useAiChat(terminalId);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const [connectInput, setConnectInput] = useState("");
  const [showManualConnect, setShowManualConnect] = useState(false);
  const [isStartingNvim, setIsStartingNvim] = useState(false);

  // Auto-scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const handleConnect = useCallback(async () => {
    if (!connectInput.trim()) return;
    await nvim.connect(connectInput.trim());
  }, [connectInput, nvim]);

  const handleStartNvim = useCallback(async () => {
    if (!terminalId) return;
    setIsStartingNvim(true);
    try {
      const socketPath = `/tmp/libg-nvim-${terminalId}.sock`;
      await invoke("ghostty_write_text", {
        id: terminalId,
        text: `nvim --listen ${socketPath} .\n`,
      });
      // Wait for nvim to boot before connecting
      await new Promise((resolve) => setTimeout(resolve, 1500));
      await nvim.connect(socketPath);
    } catch (e) {
      console.error("Failed to start neovim:", e);
    } finally {
      setIsStartingNvim(false);
    }
  }, [terminalId, nvim]);

  const handleStartAgent = useCallback(async () => {
    try {
      await acp.startAgent();
      // Also create a session with the terminal's working directory
      if (nvim.context?.filePath) {
        const dir = nvim.context.filePath.replace(/\/[^/]+$/, "") || "/";
        await acp.createSession(dir);
      } else {
        await acp.createSession("/");
      }
    } catch (e) {
      console.error("Failed to start agent:", e);
    }
  }, [acp, nvim.context]);

  const isConnected = nvim.status === "Connected";
  const isAgentRunning = acp.status === "Running";

  return (
    <div className="ai-chat">
      <div className="ai-chat__header">
        <div className="ai-chat__title-row">
          <div className="ai-chat__title">AI Chat</div>
          {isConnected && isAgentRunning && (
            <label className="ai-chat__auto-apply">
              <input
                type="checkbox"
                checked={autoApply}
                onChange={(e) => setAutoApply(e.target.checked)}
              />
              <span>Auto-apply</span>
            </label>
          )}
        </div>
        <ContextBadge
          nvimStatus={nvim.status}
          agentStatus={acp.status}
          context={nvim.context}
          diagnostics={nvim.diagnostics}
        />
      </div>

      <div className="ai-chat__messages">
        {!isConnected && (
          <div className="ai-chat__connect-prompt">
            <p>Connect to neovim to enable AI assistance.</p>
            <button
              type="button"
              className="ai-chat__connect-btn ai-chat__connect-btn--primary"
              onClick={handleStartNvim}
              disabled={!terminalId || isStartingNvim}
            >
              {isStartingNvim ? "Starting Neovim..." : "Start Neovim"}
            </button>
            {!showManualConnect ? (
              <button
                type="button"
                className="ai-chat__connect-link"
                onClick={() => setShowManualConnect(true)}
              >
                Connect to existing...
              </button>
            ) : (
              <div className="ai-chat__connect-row">
                <input
                  className="ai-chat__connect-input"
                  type="text"
                  value={connectInput}
                  onChange={(e) => setConnectInput(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && handleConnect()}
                  placeholder="/tmp/libg-nvim.sock"
                />
                <button
                  type="button"
                  className="ai-chat__connect-btn"
                  onClick={handleConnect}
                >
                  Connect
                </button>
              </div>
            )}
          </div>
        )}

        {isConnected && !isAgentRunning && (
          <div className="ai-chat__connect-prompt">
            <p>Start an AI agent to begin chatting.</p>
            <button
              type="button"
              className="ai-chat__connect-btn"
              onClick={handleStartAgent}
            >
              Start Agent (codex-acp)
            </button>
          </div>
        )}

        {messages.map((msg) => (
          <ChatMessage
            key={msg.id}
            message={msg}
            onApplyEdits={applyProposedEdits}
            onRejectEdits={rejectProposedEdits}
          />
        ))}

        {isStreaming && (
          <div className="ai-chat__streaming-indicator">
            <span className="ai-chat__streaming-dot" />
            Generating...
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      <div className="ai-chat__footer">
        <ChatInput
          onSend={sendMessage}
          disabled={!isConnected || !isAgentRunning || isStreaming}
          placeholder={
            !isConnected
              ? "Connect to neovim first..."
              : !isAgentRunning
                ? "Start an agent first..."
                : isStreaming
                  ? "Waiting for response..."
                  : "Ask about your code... (Cmd+Enter to send)"
          }
        />
      </div>
    </div>
  );
}
