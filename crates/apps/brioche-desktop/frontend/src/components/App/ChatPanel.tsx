import type { ChatMessage } from "../../store";
import type { Attachment } from "../../hooks/app/useChatActions";
import MessageList from "./MessageList";
import ChatInput from "./ChatInput";

interface ChatPanelProps {
  messages: ChatMessage[];
  isLoading: boolean;
  input: string;
  setInput: (value: string) => void;
  handleSubmit: (e?: React.FormEvent) => Promise<void> | void;
  handleKeyDown: (e: React.KeyboardEvent) => void;
  handleAttach: () => Promise<void> | void;
  handleStop: () => void;
  messagesEndRef: React.RefObject<HTMLDivElement | null>;
  pendingAttachments: Attachment[];
  removeAttachment: (id: string) => void;
}

export default function ChatPanel({
  messages,
  isLoading,
  input,
  setInput,
  handleSubmit,
  handleKeyDown,
  handleAttach,
  handleStop,
  messagesEndRef,
  pendingAttachments,
  removeAttachment,
}: ChatPanelProps) {
  return (
    <>
      <MessageList
        messages={messages}
        isLoading={isLoading}
        messagesEndRef={messagesEndRef}
      />
      <ChatInput
        input={input}
        setInput={setInput}
        isLoading={isLoading}
        handleSubmit={handleSubmit}
        handleKeyDown={handleKeyDown}
        handleAttach={handleAttach}
        handleStop={handleStop}
        pendingAttachments={pendingAttachments}
        removeAttachment={removeAttachment}
      />
    </>
  );
}
