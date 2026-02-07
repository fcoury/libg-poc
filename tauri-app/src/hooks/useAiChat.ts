import { useState, useCallback, useRef, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useNvimBridge } from "./useNvimBridge";
import { useAcpAgent } from "./useAcpAgent";
import { useLocalStorage } from "./useLocalStorage";
import type { ChatMessage } from "../types/ai-chat";
import type { AcpEvent } from "../types/acp";
import type { NvimAction, NvimActionEvent } from "../types/nvim";

function nextMessageId(): string {
  return `msg-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

const MAX_PERSISTED_MESSAGES = 200;

export function useAiChat(terminalId: string | null) {
  const nvim = useNvimBridge(terminalId);
  const acp = useAcpAgent();
  const [messages, setMessages] = useLocalStorage<ChatMessage[]>('libg:chatMessages', []);
  const [isStreaming, setIsStreaming] = useState(false);
  const [autoApply, setAutoApply] = useLocalStorage<boolean>('libg:autoApply', false);

  // Trim messages on mount to prevent unbounded growth
  useEffect(() => {
    setMessages((prev) => {
      if (prev.length > MAX_PERSISTED_MESSAGES) {
        return prev.slice(-MAX_PERSISTED_MESSAGES);
      }
      return prev;
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const currentAssistantIdRef = useRef<string | null>(null);
  const autoApplyRef = useRef(autoApply);
  const actionTriggeredRef = useRef(false);

  // Keep ref in sync with state for use in event callbacks
  useEffect(() => {
    autoApplyRef.current = autoApply;
  }, [autoApply]);

  // Wire up ACP streaming events to chat messages
  useEffect(() => {
    acp.onEvent((event: AcpEvent) => {
      switch (event.type) {
        case "contentChunk": {
          const assistantId = currentAssistantIdRef.current;
          if (!assistantId) return;
          setMessages((prev) =>
            prev.map((m) =>
              m.id === assistantId
                ? { ...m, content: m.content + event.data }
                : m
            )
          );
          break;
        }
        case "done": {
          setIsStreaming(false);
          const wasActionTriggered = actionTriggeredRef.current;
          actionTriggeredRef.current = false;
          currentAssistantIdRef.current = null;

          // Auto-apply edits if enabled and this was a nvim-triggered action
          if (wasActionTriggered && autoApplyRef.current) {
            setMessages((prev) => {
              const lastAssistant = [...prev]
                .reverse()
                .find((m) => m.role === "assistant" && m.proposedEdits);
              if (lastAssistant?.proposedEdits && lastAssistant.editStatus !== "applied") {
                nvim.applyEdits(lastAssistant.proposedEdits).then(() => {
                  setMessages((curr) =>
                    curr.map((m) =>
                      m.id === lastAssistant.id
                        ? { ...m, editStatus: "applied" }
                        : m
                    )
                  );
                });
              }
              return prev;
            });
          }
          break;
        }
        case "error": {
          setIsStreaming(false);
          actionTriggeredRef.current = false;
          const assistantId = currentAssistantIdRef.current;
          if (assistantId) {
            setMessages((prev) =>
              prev.map((m) =>
                m.id === assistantId
                  ? { ...m, content: m.content + `\n\n**Error:** ${event.data}` }
                  : m
              )
            );
          }
          currentAssistantIdRef.current = null;
          break;
        }
      }
    });
  }, [acp, nvim]);

  const sendMessage = useCallback(
    async (content: string) => {
      if (!content.trim() || isStreaming) return;

      // Add user message
      const userMsg: ChatMessage = {
        id: nextMessageId(),
        role: "user",
        content,
        timestamp: Date.now(),
        context: nvim.context ?? undefined,
        diagnostics:
          nvim.diagnostics.length > 0 ? nvim.diagnostics : undefined,
      };
      setMessages((prev) => [...prev, userMsg]);

      // Create assistant placeholder
      const assistantId = nextMessageId();
      const assistantMsg: ChatMessage = {
        id: assistantId,
        role: "assistant",
        content: "",
        timestamp: Date.now(),
      };
      currentAssistantIdRef.current = assistantId;
      setMessages((prev) => [...prev, assistantMsg]);
      setIsStreaming(true);

      // Build context string from nvim state
      let contextStr: string | undefined;
      if (nvim.context) {
        const ctx = nvim.context;
        const parts = [
          `File: ${ctx.filePath} (${ctx.fileType})`,
          `Cursor: line ${ctx.cursor.line}, col ${ctx.cursor.col}`,
          `Buffer lines ${ctx.visibleRange[0]}-${ctx.visibleRange[1]}:`,
          "```",
          ...ctx.visibleLines,
          "```",
        ];
        if (nvim.diagnostics.length > 0) {
          parts.push(
            "\nDiagnostics:",
            ...nvim.diagnostics.map(
              (d) =>
                `  Line ${d.line + 1}: [${severityLabel(d.severity)}] ${d.message}${d.source ? ` (${d.source})` : ""}`
            )
          );
        }
        contextStr = parts.join("\n");
      }

      try {
        await acp.sendPrompt([content], contextStr);
      } catch (e) {
        setIsStreaming(false);
        setMessages((prev) =>
          prev.map((m) =>
            m.id === assistantId
              ? { ...m, content: `**Error:** ${String(e)}` }
              : m
          )
        );
        currentAssistantIdRef.current = null;
      }
    },
    [isStreaming, nvim.context, nvim.diagnostics, acp]
  );

  // Listen for nvim-action events from Neovim keybindings
  useEffect(() => {
    if (!terminalId) return;

    const unlisten = listen<NvimActionEvent>("nvim-action", (event) => {
      const { terminalId: eventTerminalId, action } = event.payload;
      if (eventTerminalId !== terminalId) return;
      if (isStreaming) return;

      actionTriggeredRef.current = true;
      const prompt = buildActionPrompt(action);
      sendMessage(prompt);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [terminalId, isStreaming, sendMessage]);

  const applyProposedEdits = useCallback(
    async (messageId: string) => {
      const msg = messages.find((m) => m.id === messageId);
      if (!msg?.proposedEdits) return;

      try {
        await nvim.applyEdits(msg.proposedEdits);
        setMessages((prev) =>
          prev.map((m) =>
            m.id === messageId ? { ...m, editStatus: "applied" } : m
          )
        );
      } catch (e) {
        console.error("Failed to apply edits:", e);
      }
    },
    [messages, nvim]
  );

  const rejectProposedEdits = useCallback(
    (messageId: string) => {
      setMessages((prev) =>
        prev.map((m) =>
          m.id === messageId ? { ...m, editStatus: "rejected" } : m
        )
      );
    },
    []
  );

  const clearMessages = useCallback(() => {
    setMessages([]);
  }, []);

  return {
    messages,
    isStreaming,
    autoApply,
    setAutoApply,
    sendMessage,
    applyProposedEdits,
    rejectProposedEdits,
    clearMessages,
    nvim,
    acp,
  };
}

function severityLabel(severity: number): string {
  switch (severity) {
    case 1:
      return "ERROR";
    case 2:
      return "WARN";
    case 3:
      return "INFO";
    case 4:
      return "HINT";
    default:
      return "UNKNOWN";
  }
}

function buildActionPrompt(action: NvimAction): string {
  const numberLines = (lines: string[], startLine: number) =>
    lines.map((l, i) => `${startLine + i}: ${l}`).join("\n");

  switch (action.action) {
    case "fixDiagnostic": {
      const d = action.diagnostic;
      const sev = severityLabel(d.severity);
      return [
        `Fix the following diagnostic in ${action.filePath} at line ${action.cursorLine}:`,
        `[${sev}] ${d.message}${d.source ? ` (${d.source})` : ""}`,
        "",
        "Context:",
        "```",
        numberLines(action.contextLines, action.contextStartLine),
        "```",
      ].join("\n");
    }
    case "implement": {
      return [
        `Implement the following in ${action.filePath} (${action.fileType}):`,
        "```",
        action.signatureLines.join("\n"),
        "```",
        "",
        "Surrounding context:",
        "```",
        numberLines(action.contextLines, action.contextStartLine),
        "```",
      ].join("\n");
    }
    case "explain": {
      return [
        `Explain the following code from ${action.filePath} (${action.fileType}):`,
        "```",
        action.targetText,
        "```",
        "",
        "Context:",
        "```",
        numberLines(action.contextLines, action.contextStartLine),
        "```",
      ].join("\n");
    }
    case "ask": {
      const parts = [action.prompt];
      if (action.selection) {
        parts.push("", "Selected code:", "```", action.selection, "```");
      }
      parts.push(
        "",
        "Context:",
        "```",
        numberLines(action.contextLines, action.contextStartLine),
        "```"
      );
      return parts.join("\n");
    }
  }
}
