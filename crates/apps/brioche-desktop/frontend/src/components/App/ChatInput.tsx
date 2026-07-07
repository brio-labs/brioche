import { useRef, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import Tooltip from "../Tooltip";
import { Paperclip, Send, Square, X } from "lucide-react";
import FileTypeIcon from "./FileTypeIcon";
import type { Attachment } from "../../hooks/app/useChatActions";

interface ChatInputProps {
  input: string;
  setInput: (value: string) => void;
  isLoading: boolean;
  handleSubmit: (e?: React.FormEvent) => Promise<void> | void;
  handleKeyDown: (e: React.KeyboardEvent) => void;
  handleAttach: () => Promise<void> | void;
  handleStop: () => void;
  pendingAttachments: Attachment[];
  removeAttachment: (id: string) => void;
}


export default function ChatInput({
  input,
  setInput,
  isLoading,
  handleSubmit,
  handleKeyDown,
  handleAttach,
  handleStop,
  pendingAttachments,
  removeAttachment,
}: ChatInputProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Auto-resize textarea
  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
  }, [input]);

  const canSend =
    (input.trim().length > 0 || pendingAttachments.length > 0) && !isLoading;

  return (
    <div className="shrink-0 px-6 py-4 bg-transparent">
      <div className="max-w-3xl mx-auto w-full flex flex-col gap-3">
        {/* ── Pending attachment chips ── */}
        <AnimatePresence>
          {pendingAttachments.length > 0 && (
            <motion.div
              key="attachments"
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              exit={{ opacity: 0, height: 0 }}
              transition={{ duration: 0.18 }}
              className="flex flex-wrap gap-2 overflow-hidden"
            >
              {pendingAttachments.map((att) => (
                <div
                  key={att.id}
                  className="flex items-center gap-2 rounded-sm border border-border/60 bg-bg-elevated/35 backdrop-blur-md px-2 py-1.5 pr-1.5 max-w-56 relative group shadow-sm hover:border-border-hover transition-colors"
                >
                  {/* Square preview */}
                  <div className="h-9 w-9 rounded-xs bg-bg-highlight/35 border border-border/60 overflow-hidden shrink-0 flex items-center justify-center">
                    {att.type === "image" && att.dataUrl ? (
                      <img
                        src={att.dataUrl}
                        alt={att.name}
                        className="h-full w-full object-cover"
                      />
                    ) : (
                      <div className="flex flex-col items-center justify-center h-full w-full p-1">
                        <FileTypeIcon
                          extension={att.name.split(".").pop()?.toLowerCase() || ""}
                          className="h-4 w-4 shrink-0"
                        />
                        <span className="text-[7px] font-bold text-accent mt-0.5 tracking-wider font-mono uppercase">
                          {att.name.split(".").pop() || "file"}
                        </span>
                      </div>
                    )}
                  </div>

                  {/* Name + type */}
                  <div className="flex flex-col min-w-0 flex-1">
                    <span
                      className="truncate text-[11px] font-medium text-fg-primary leading-tight"
                      title={att.path}
                    >
                      {att.name}
                    </span>
                    <span className="text-[9px] text-fg-muted font-mono uppercase tracking-wide mt-0.5">
                      {att.type === "image"
                        ? "Image"
                        : att.name.split(".").pop() || "Doc"}
                    </span>
                  </div>

                  {/* Remove */}
                  <button
                    type="button"
                    onClick={() => removeAttachment(att.id)}
                    className="h-5 w-5 rounded-sm flex items-center justify-center text-fg-muted hover:text-error-text hover:bg-error-bg transition-colors cursor-pointer shrink-0"
                    title="Remove"
                  >
                    <X className="h-3 w-3" />
                  </button>
                </div>
              ))}
            </motion.div>
          )}
        </AnimatePresence>

        {/* ── Unified input pill ── */}
        <form onSubmit={handleSubmit} className="relative">
          <div
            className="
            flex items-end gap-0
            border border-border/60 bg-bg-elevated/28 backdrop-blur-xl
            focus-within:border-accent/40 focus-within:ring-2 focus-within:ring-accent/10
            transition-all duration-200 shadow-sm overflow-hidden
          "
          >
            {/* Attach button — left inside the pill */}
            <Tooltip label="Attach file or image">
              <button
                type="button"
                onClick={handleAttach}
                disabled={isLoading}
                className="shrink-0 flex items-center justify-center h-10 w-10 text-fg-muted hover:text-fg-primary hover:bg-bg-highlight disabled:opacity-40 transition-colors cursor-pointer"
                aria-label="Attach file or image"
              >
                <Paperclip className="h-4 w-4" />
              </button>
            </Tooltip>

            {/* Divider */}
            <div className="w-px bg-border self-stretch my-2" />

            {/* Textarea — grows with content */}
            <textarea
              ref={textareaRef}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Type a message… (Shift+Enter for new line)"
              disabled={isLoading}
              rows={1}
              className="
                flex-1 bg-transparent text-fg-primary text-[13.5px] leading-relaxed
                px-3 py-2.5 outline-none resize-none
                placeholder:text-fg-muted/50
                disabled:opacity-50 disabled:cursor-not-allowed
                min-h-10 max-h-50
                scrollbar-thin scrollbar-thumb-border
              "
            />

            {/* Divider */}
            <div className="w-px bg-border self-stretch my-2" />

            {/* Send / Stop button — right inside the pill */}
            <AnimatePresence mode="wait" initial={false}>
              {isLoading ? (
                <motion.button
                  key="stop"
                  type="button"
                  onClick={handleStop}
                  initial={{ opacity: 0, scale: 0.8 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.8 }}
                  transition={{ duration: 0.12 }}
                  className="shrink-0 flex items-center justify-center h-10 w-10 text-error-text hover:bg-error-bg transition-colors cursor-pointer"
                  aria-label="Stop generation"
                  title="Stop"
                >
                  <Square className="h-4 w-4 fill-current" />
                </motion.button>
              ) : (
                <motion.button
                  key="send"
                  type="submit"
                  disabled={!canSend}
                  initial={{ opacity: 0, scale: 0.8 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.8 }}
                  transition={{ duration: 0.12 }}
                  className="shrink-0 flex items-center justify-center h-10 w-10 text-fg-primary hover:bg-fg-primary/10 disabled:opacity-30 disabled:cursor-not-allowed transition-colors cursor-pointer"
                  aria-label="Send message"
                  title="Send"
                >
                  <Send className="h-4 w-4" />
                </motion.button>
              )}
            </AnimatePresence>
          </div>
        </form>
      </div>
    </div>
  );
}
