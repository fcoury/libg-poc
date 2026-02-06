import { useState, useCallback, useRef, useEffect } from "react";
import { useNvimBridge } from "./useNvimBridge";
import { useAcpAgent } from "./useAcpAgent";
import type { ChatMessage } from "../types/ai-chat";
import type { AcpEvent } from "../types/acp";

let messageIdCounter = 0;
function nextMessageId(): string {
  return `msg-${++messageIdCounter}-${Date.now()}`;
}

export function useAiChat(terminalId: string | null) {
  const nvim = useNvimBridge(terminalId);
  const acp = useAcpAgent();
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const currentAssistantIdRef = useRef<string | null>(null);

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
          currentAssistantIdRef.current = null;
          break;
        }
        case "error": {
          setIsStreaming(false);
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
  }, [acp]);

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
