import type { NvimContext, Diagnostic, BufferEdit } from "./nvim";

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  timestamp: number;
  context?: NvimContext;
  diagnostics?: Diagnostic[];
  proposedEdits?: BufferEdit[];
  editStatus?: "pending" | "applied" | "rejected";
}

export interface AgentConfig {
  path: string;
  name?: string;
}
