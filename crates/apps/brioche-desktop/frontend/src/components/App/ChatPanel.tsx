import type { ChatMessage } from "../../store";
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
  handleImage: () => Promise<void> | void;
  handleClearChat: () => void;
  messagesEndRef: React.RefObject<HTMLDivElement | null>;
}

export default function ChatPanel({
  messages,
  isLoading,
  input,
  setInput,
  handleSubmit,
  handleKeyDown,
  handleAttach,
  handleImage,
  handleClearChat,
  messagesEndRef,
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
        handleImage={handleImage}
        handleClearChat={handleClearChat}
      />
    </>
  );
}
