export type AcpEvent =
  | { type: "contentChunk"; data: string }
  | { type: "thoughtChunk"; data: string }
  | { type: "toolCallStarted"; data: { id: string; title: string; kind: string } }
  | { type: "toolCallUpdated"; data: { id: string; status: string } }
  | { type: "done"; data: { stopReason: string } }
  | { type: "error"; data: string };

export type AgentStatus = "Stopped" | "Starting" | "Running" | { Error: string };
