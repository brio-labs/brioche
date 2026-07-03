import { Brain } from "lucide-react";
import { EmptyState } from "../ui";
import type { ChatMessage } from "../../store";
import ToolCallMessage from "../ToolCallMessage";

interface MessageListProps {
  messages: ChatMessage[];
  isLoading: boolean;
  messagesEndRef: React.RefObject<HTMLDivElement | null>;
}

export default function MessageList({
  messages,
  isLoading,
  messagesEndRef,
}: MessageListProps) {
  return (
    <div className="flex-1 overflow-y-auto px-6 py-4 flex flex-col gap-4 relative">
      {messages.length === 0 && (
        <EmptyState
          icon={Brain}
          title="Brioche Desktop"
          description="Type a message or use /help for commands to begin."
        />
      )}
      {messages.map((msg) =>
        msg.role === "tool_request" || msg.role === "tool_result" ? (
          <div
            id={`msg-${msg.id}`}
            key={msg.id}
            className={`flex flex-col gap-2 relative animate-fadeIn max-w-[85%] ${
              msg.role === "tool_request" ? "self-end" : "self-start"
            }`}
          >
            <ToolCallMessage message={msg} />
          </div>
        ) : (
          <div
            id={`msg-${msg.id}`}
            key={msg.id}
            className={`flex flex-col gap-2 relative animate-fadeIn max-w-[85%] ${
              msg.role === "user"
                ? "self-end"
                : msg.role === "assistant"
                  ? "self-start max-w-[90%]"
                  : "self-center max-w-150 w-full"
            }`}
          >
            <div className="flex items-center gap-2 mb-0.5 px-1">
              <span className="text-xs font-medium text-text-muted capitalize">
                {msg.role}
              </span>
            </div>
            <div
              className={`px-4 py-3 rounded-[8px] leading-relaxed text-sm wrap-break-word relative overflow-hidden ${
                msg.role === "user"
                  ? "bg-user-bg text-text-primary border border-accent/15 shadow-md"
                  : msg.role === "assistant"
                    ? "bg-assistant-bg text-text-primary border border-border shadow-md"
                    : msg.role === "system"
                      ? "bg-system-bg text-text-secondary border border-border rounded-[8px] text-xs font-mono"
                      : "bg-error-bg text-error-text border border-error-border rounded-[8px] text-sm"
              }`}
            >
              <div className="message-content">{msg.content}</div>
            </div>
          </div>
        ),
      )}
      {isLoading && (
        <div className="flex flex-col gap-2 relative animate-fadeIn max-w-[85%] self-start">
          <div className="flex items-center gap-2 mb-0.5 px-1">
            <span className="text-xs font-medium text-text-muted capitalize">
              assistant
            </span>
          </div>
          <div className="px-4 py-3 rounded-[8px] leading-relaxed text-sm wrap-break-word relative overflow-hidden bg-assistant-bg text-text-primary border border-border shadow-md">
            <div className="text-text-muted italic">Thinking...</div>
          </div>
        </div>
      )}
      <div ref={messagesEndRef} />
    </div>
  );
}
