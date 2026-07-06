import { AnimatePresence } from "framer-motion";
import { Brain } from "lucide-react";
import { EmptyState } from "../ui";
import type { ChatMessage } from "../../store";
import { MessageListItem, ThinkingIndicator } from "./MessageListItem";

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
    <div className="flex-1 overflow-y-auto flex flex-col relative">
      <div className="flex flex-col gap-0 px-0 py-6 max-w-3xl mx-auto w-full">
        {messages.length === 0 && (
          <div className="flex-1 flex items-center justify-center py-20">
            <EmptyState
              icon={Brain}
              title="Brioche Desktop"
              description="Type a message or use /help for commands to begin."
            />
          </div>
        )}

        <AnimatePresence initial={false}>
          {messages.map((message) => (
            <MessageListItem key={message.id} message={message} />
          ))}
        </AnimatePresence>

        <AnimatePresence>{isLoading && <ThinkingIndicator />}</AnimatePresence>

        <div ref={messagesEndRef} className="h-4" />
      </div>
    </div>
  );
}
