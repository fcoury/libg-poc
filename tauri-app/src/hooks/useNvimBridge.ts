import { useState, useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  NvimContext,
  Diagnostic,
  BufferEdit,
  ConnectionStatus,
} from "../types/nvim";

export function useNvimBridge(terminalId: string | null) {
  const [status, setStatus] = useState<ConnectionStatus>("Disconnected");
  const [context, setContext] = useState<NvimContext | null>(null);
  const [diagnostics, setDiagnostics] = useState<Diagnostic[]>([]);
  const pollRef = useRef<number | null>(null);

  const connect = useCallback(
    async (socketPath: string) => {
      if (!terminalId) return;
      try {
        await invoke("nvim_connect", {
          terminalId,
          socketPath,
        });
        setStatus("Connected");
      } catch (e) {
        console.error("nvim_connect error:", e);
        setStatus("Error");
      }
    },
    [terminalId]
  );

  const disconnect = useCallback(async () => {
    if (!terminalId) return;
    try {
      await invoke("nvim_disconnect", { terminalId });
      setStatus("Disconnected");
      setContext(null);
      setDiagnostics([]);
    } catch (e) {
      console.error("nvim_disconnect error:", e);
    }
  }, [terminalId]);

  const refreshContext = useCallback(async () => {
    if (!terminalId || status !== "Connected") return;
    try {
      const ctx = await invoke<NvimContext>("nvim_get_context", {
        terminalId,
      });
      setContext(ctx);

      const diags = await invoke<Diagnostic[]>("nvim_get_diagnostics", {
        terminalId,
      });
      setDiagnostics(diags);
    } catch (e) {
      console.error("nvim context refresh error:", e);
    }
  }, [terminalId, status]);

  const applyEdit = useCallback(
    async (edit: BufferEdit) => {
      if (!terminalId) return;
      await invoke("nvim_apply_edit", { terminalId, edit });
    },
    [terminalId]
  );

  const applyEdits = useCallback(
    async (edits: BufferEdit[]) => {
      if (!terminalId) return;
      await invoke("nvim_apply_edits", { terminalId, edits });
    },
    [terminalId]
  );

  const execCommand = useCallback(
    async (command: string): Promise<string> => {
      if (!terminalId) return "";
      return invoke<string>("nvim_exec_command", { terminalId, command });
    },
    [terminalId]
  );

  // Poll context + diagnostics every 2s when connected
  useEffect(() => {
    if (status !== "Connected") {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
      return;
    }

    refreshContext();
    pollRef.current = window.setInterval(refreshContext, 2000);

    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [status, refreshContext]);

  return {
    status,
    context,
    diagnostics,
    connect,
    disconnect,
    refreshContext,
    applyEdit,
    applyEdits,
    execCommand,
  };
}
