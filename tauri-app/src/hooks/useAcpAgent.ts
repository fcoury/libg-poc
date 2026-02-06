import { useState, useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AcpEvent, AgentStatus } from "../types/acp";

const DEFAULT_AGENT_PATH = "codex-acp";

export function useAcpAgent() {
  const [status, setStatus] = useState<AgentStatus>("Stopped");
  const [sessionId, setSessionId] = useState<string | null>(null);
  const listenersRef = useRef<UnlistenFn[]>([]);
  const eventCallbackRef = useRef<((event: AcpEvent) => void) | null>(null);

  // Set up event listener for streaming ACP events
  useEffect(() => {
    let cancelled = false;

    const setup = async () => {
      const unlisten = await listen<AcpEvent>("acp-event", (event) => {
        if (!cancelled && eventCallbackRef.current) {
          eventCallbackRef.current(event.payload);
        }
      });
      if (!cancelled) {
        listenersRef.current.push(unlisten);
      } else {
        unlisten();
      }
    };

    setup();

    return () => {
      cancelled = true;
      for (const unlisten of listenersRef.current) {
        unlisten();
      }
      listenersRef.current = [];
    };
  }, []);

  const onEvent = useCallback((callback: (event: AcpEvent) => void) => {
    eventCallbackRef.current = callback;
  }, []);

  const startAgent = useCallback(
    async (agentPath: string = DEFAULT_AGENT_PATH) => {
      try {
        setStatus("Starting");
        await invoke("acp_start_agent", { agentPath });
        setStatus("Running");
      } catch (e) {
        setStatus({ Error: String(e) });
        throw e;
      }
    },
    []
  );

  const stopAgent = useCallback(async () => {
    try {
      await invoke("acp_stop_agent");
      setStatus("Stopped");
      setSessionId(null);
    } catch (e) {
      console.error("acp_stop_agent error:", e);
    }
  }, []);

  const createSession = useCallback(async (workingDir: string) => {
    const sid = await invoke<string>("acp_create_session", {
      workingDir,
    });
    setSessionId(sid);
    return sid;
  }, []);

  const sendPrompt = useCallback(
    async (messages: string[], context?: string) => {
      if (!sessionId) throw new Error("No active session");
      return invoke<string>("acp_send_prompt", {
        sessionId,
        messages,
        context: context ?? null,
      });
    },
    [sessionId]
  );

  const refreshStatus = useCallback(async () => {
    const s = await invoke<AgentStatus>("acp_agent_status");
    setStatus(s);
    return s;
  }, []);

  return {
    status,
    sessionId,
    startAgent,
    stopAgent,
    createSession,
    sendPrompt,
    onEvent,
    refreshStatus,
  };
}
