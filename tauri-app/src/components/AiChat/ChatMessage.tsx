import type { ChatMessage as ChatMessageType } from "../../types/ai-chat";
import { DiffPreview } from "./DiffPreview";

type Props = {
  message: ChatMessageType;
  onApplyEdits?: (messageId: string) => void;
  onRejectEdits?: (messageId: string) => void;
};

export function ChatMessage({ message, onApplyEdits, onRejectEdits }: Props) {
  const isUser = message.role === "user";

  return (
    <div className={`chat-message chat-message--${message.role}`}>
      <div className="chat-message__header">
        <span className="chat-message__role">
          {isUser ? "You" : "Assistant"}
        </span>
        <span className="chat-message__time">
          {new Date(message.timestamp).toLocaleTimeString()}
        </span>
      </div>

      <div className="chat-message__content">
        {renderContent(message.content)}
      </div>

      {message.proposedEdits && message.proposedEdits.length > 0 && (
        <div className="chat-message__edits">
          {message.proposedEdits.map((edit, i) => (
            <DiffPreview key={i} edit={edit} />
          ))}
          {message.editStatus === "pending" && (
            <div className="chat-message__edit-actions">
              <button
                type="button"
                className="chat-message__btn chat-message__btn--apply"
                onClick={() => onApplyEdits?.(message.id)}
              >
                Apply
              </button>
              <button
                type="button"
                className="chat-message__btn chat-message__btn--reject"
                onClick={() => onRejectEdits?.(message.id)}
              >
                Reject
              </button>
            </div>
          )}
          {message.editStatus === "applied" && (
            <span className="chat-message__edit-status chat-message__edit-status--applied">
              Applied
            </span>
          )}
          {message.editStatus === "rejected" && (
            <span className="chat-message__edit-status chat-message__edit-status--rejected">
              Rejected
            </span>
          )}
        </div>
      )}
    </div>
  );
}

function renderContent(content: string) {
  // Simple markdown-like rendering for code blocks
  const parts = content.split(/(```[\s\S]*?```)/g);
  return parts.map((part, i) => {
    if (part.startsWith("```") && part.endsWith("```")) {
      const inner = part.slice(3, -3);
      const newlineIdx = inner.indexOf("\n");
      const code = newlineIdx >= 0 ? inner.slice(newlineIdx + 1) : inner;
      return (
        <pre key={i} className="chat-message__code-block">
          <code>{code}</code>
        </pre>
      );
    }
    return (
      <span key={i} className="chat-message__text">
        {part}
      </span>
    );
  });
}
