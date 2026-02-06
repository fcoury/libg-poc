import type { NvimContext, Diagnostic, ConnectionStatus } from "../../types/nvim";
import type { AgentStatus } from "../../types/acp";

type Props = {
  nvimStatus: ConnectionStatus;
  agentStatus: AgentStatus;
  context: NvimContext | null;
  diagnostics: Diagnostic[];
};

function statusDot(status: ConnectionStatus): { color: string; label: string } {
  if (status === "Connected") return { color: "#22c55e", label: "nvim" };
  if (status === "Disconnected") return { color: "#6b7280", label: "nvim" };
  return { color: "#ef4444", label: "nvim" };
}

function agentDot(status: AgentStatus): { color: string; label: string } {
  if (status === "Running") return { color: "#22c55e", label: "agent" };
  if (status === "Starting") return { color: "#eab308", label: "agent" };
  if (status === "Stopped") return { color: "#6b7280", label: "agent" };
  return { color: "#ef4444", label: "agent" };
}

function diagnosticCounts(diagnostics: Diagnostic[]) {
  let errors = 0;
  let warnings = 0;
  for (const d of diagnostics) {
    if (d.severity === 1) errors++;
    else if (d.severity === 2) warnings++;
  }
  return { errors, warnings };
}

export function ContextBadge({ nvimStatus, agentStatus, context, diagnostics }: Props) {
  const nvim = statusDot(nvimStatus);
  const agent = agentDot(agentStatus);
  const { errors, warnings } = diagnosticCounts(diagnostics);

  return (
    <div className="context-badge">
      <div className="context-badge__status">
        <span className="context-badge__dot" style={{ background: nvim.color }} />
        <span className="context-badge__label">{nvim.label}</span>
        <span className="context-badge__dot" style={{ background: agent.color }} />
        <span className="context-badge__label">{agent.label}</span>
      </div>

      {context && (
        <div className="context-badge__file">
          <span className="context-badge__filename">
            {context.filePath.split("/").pop() || context.filePath}
          </span>
          <span className="context-badge__position">
            :{context.cursor.line}:{context.cursor.col}
          </span>
          {context.modified && (
            <span className="context-badge__modified" title="Modified">
              M
            </span>
          )}
        </div>
      )}

      {(errors > 0 || warnings > 0) && (
        <div className="context-badge__diagnostics">
          {errors > 0 && (
            <span className="context-badge__diag context-badge__diag--error">
              {errors}E
            </span>
          )}
          {warnings > 0 && (
            <span className="context-badge__diag context-badge__diag--warn">
              {warnings}W
            </span>
          )}
        </div>
      )}
    </div>
  );
}
