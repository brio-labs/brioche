import { motion, AnimatePresence } from "framer-motion";
import {
  Brain,
  Sparkles,
  FileText,
  FileSpreadsheet,
  FileArchive,
  FileAudio,
  FileVideo,
  FileCode,
  File,
} from "lucide-react";
import { EmptyState } from "../ui";
import MarkdownRenderer from "../MarkdownRenderer";
import type { ChatMessage } from "../../store";
import ToolCallMessage from "../ToolCallMessage";

interface MessageListProps {
  messages: ChatMessage[];
  isLoading: boolean;
  messagesEndRef: React.RefObject<HTMLDivElement | null>;
}

/* ── File icon helper (same as ChatInput) ───────────────────────────── */
function getFileIcon(ext: string) {
  switch (ext) {
    case "pdf":
      return <FileText className="h-7 w-7 text-red-400 shrink-0" />;
    case "xlsx":
    case "xls":
    case "csv":
    case "ods":
      return <FileSpreadsheet className="h-7 w-7 text-emerald-400 shrink-0" />;
    case "zip":
    case "tar":
    case "gz":
    case "rar":
    case "7z":
      return <FileArchive className="h-7 w-7 text-amber-400 shrink-0" />;
    case "mp3":
    case "wav":
    case "ogg":
    case "aac":
      return <FileAudio className="h-7 w-7 text-sky-400 shrink-0" />;
    case "mp4":
    case "mkv":
    case "avi":
    case "mov":
      return <FileVideo className="h-7 w-7 text-indigo-400 shrink-0" />;
    case "js":
    case "ts":
    case "tsx":
    case "jsx":
    case "html":
    case "css":
    case "py":
    case "rs":
    case "go":
    case "json":
      return <FileCode className="h-7 w-7 text-violet-400 shrink-0" />;
    default:
      return <File className="h-7 w-7 text-fg-muted shrink-0" />;
  }
}

/* ── Attachment card shown inside a sent user message ───────────────── */
function AttachmentCard({ content }: { content: string }) {
  const filePath = content.substring("Attached: ".length).trim();
  const fileName = filePath.split(/[/\\]/).pop() || "";
  const ext = fileName.split(".").pop()?.toLowerCase() || "";
  return (
    <div className="flex items-center gap-3 rounded-md border border-white/10 bg-white/5 px-3 py-2.5 mt-1 max-w-xs">
      {getFileIcon(ext)}
      <div className="flex flex-col min-w-0">
        <span
          className="truncate text-[12px] font-semibold text-white/90"
          title={fileName}
        >
          {fileName}
        </span>
        <span
          className="truncate text-[10px] text-white/40 font-mono"
          title={filePath}
        >
          {filePath}
        </span>
      </div>
    </div>
  );
}

/* ── Image preview shown inside a sent user message ─────────────────── */
function ImageAttachment({ content }: { content: string }) {
  const match = content.match(/^!\[(.*?)\]\((.*?)\)$/);
  if (!match) return null;
  const [, imagePath, dataUrl] = match;
  const fileName = imagePath.split(/[/\\]/).pop() || "";
  return (
    <div
      className="mt-1.5 rounded-md overflow-hidden border border-white/10 w-full max-w-55 cursor-pointer"
      onClick={() => window.open(dataUrl)}
    >
      <img
        src={dataUrl}
        alt={fileName}
        className="w-full max-h-40 object-cover hover:opacity-90 transition-opacity"
      />
      <div className="px-2 py-1 bg-black/20 text-[10px] text-white/50 font-mono truncate">
        {fileName}
      </div>
    </div>
  );
}

/* ── Typing / thinking dots ─────────────────────────────────────────── */
function ThinkingDots() {
  return (
    <span className="inline-flex items-center gap-1 h-4">
      {[0, 1, 2].map((i) => (
        <motion.span
          key={i}
          className="block h-1.5 w-1.5 rounded-full bg-fg-muted"
          animate={{ opacity: [0.3, 1, 0.3] }}
          transition={{ duration: 1.2, repeat: Infinity, delay: i * 0.2 }}
        />
      ))}
    </span>
  );
}

/* ── Main component ─────────────────────────────────────────────────── */
export default function MessageList({
  messages,
  isLoading,
  messagesEndRef,
}: MessageListProps) {
  return (
    <div className="flex-1 overflow-y-auto flex flex-col relative">
      {/* Scrollable content area */}
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
          {messages.map((msg) => {
            /* ── Tool call / result ─────────────────────── */
            if (
              msg.role === "tool_request" ||
              msg.role === "tool_result" ||
              msg.role === "tool_argument" ||
              msg.role === "tool_done"
            ) {
              return (
                <motion.div
                  id={`msg-${msg.id}`}
                  key={msg.id}
                  initial={{ opacity: 0, y: 8 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.18 }}
                  className="px-6 py-1"
                >
                  <ToolCallMessage message={msg} />
                </motion.div>
              );
            }

            /* ── System / error notices ─────────────────── */
            if (msg.role === "system" || msg.role === "error") {
              const isAttachment =
                msg.role === "system" && msg.content.startsWith("Attached: ");
              const isError = msg.role === "error";
              return (
                <motion.div
                  id={`msg-${msg.id}`}
                  key={msg.id}
                  initial={{ opacity: 0, scale: 0.97 }}
                  animate={{ opacity: 1, scale: 1 }}
                  transition={{ duration: 0.15 }}
                  className="flex justify-center px-6 py-2"
                >
                  <div
                    className={`
                    inline-flex items-center gap-2 rounded-sm px-4 py-1.5 text-xs font-mono
                    ${
                      isError
                        ? "bg-error-bg border border-error-border text-error-text"
                        : "bg-bg-elevated border border-border text-fg-muted"
                    }
                  `}
                  >
                    {isAttachment ? (
                      <>
                        📎{" "}
                        {msg.content
                          .substring("Attached: ".length)
                          .trim()
                          .split(/[/\\]/)
                          .pop()}
                      </>
                    ) : (
                      msg.content
                    )}
                  </div>
                </motion.div>
              );
            }

            /* ── User message ─────────────────────────────
               Right-aligned, solid accent-tinted bubble     */
            if (msg.role === "user") {
              const isAttachment = msg.content.startsWith("Attached: ");
              const imageMatch = msg.content.match(/^!\[(.*?)\]\((.*?)\)$/);

              // Split on embedded attachments appended during submit
              const parts = msg.content.split(/\n\n(?=Attached: |!\[)/);
              const textPart = parts[0].trim();
              const attachParts = parts.slice(1);

              return (
                <motion.div
                  id={`msg-${msg.id}`}
                  key={msg.id}
                  initial={{ opacity: 0, y: 10, x: 20 }}
                  animate={{ opacity: 1, y: 0, x: 0 }}
                  transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
                  className="flex justify-start px-6 py-1.5 group"
                >
                  <div className="flex flex-col gap-1 max-w-[75%] min-w-0">
                    {/* Bubble */}
                    <div
                      className="
                      bg-brio-bg text-brio-foam px-2 rounded-sm
                      text-sm leading-relaxed
                      wrap-break-word overflow-hidden w-full
                    "
                    >
                      {isAttachment || imageMatch ? null : (
                        <MarkdownRenderer content={textPart || msg.content} />
                      )}
                      {/* Inline attachments */}
                      {attachParts.map((part, i) => {
                        if (part.startsWith("Attached: "))
                          return <AttachmentCard key={i} content={part} />;
                        if (part.match(/^!\[/))
                          return <ImageAttachment key={i} content={part} />;
                        return null;
                      })}
                      {/* Standalone attachment/image message */}
                      {isAttachment && <AttachmentCard content={msg.content} />}
                      {imageMatch && <ImageAttachment content={msg.content} />}
                    </div>
                  </div>
                </motion.div>
              );
            }

            /* ── Assistant message ────────────────────────
               Left-aligned, no background, with avatar      */
            return (
              <motion.div
                id={`msg-${msg.id}`}
                key={msg.id}
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
                className="flex items-start gap-3 px-6 py-1.5 group"
              >
                {/* Avatar */}
                <div className="mt-1 h-7 w-7 shrink-0 rounded-sm bg-bg-elevated border border-border flex items-center justify-center text-accent">
                  <Sparkles className="h-3.5 w-3.5" />
                </div>

                {/* Content — no bubble, just clean prose */}
                <div className="flex-1 min-w-0 text-[13.5px] leading-relaxed text-fg-primary pt-0.5">
                  <MarkdownRenderer content={msg.content} />
                </div>
              </motion.div>
            );
          })}
        </AnimatePresence>

        {/* Thinking indicator */}
        <AnimatePresence>
          {isLoading && (
            <motion.div
              key="thinking"
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -4 }}
              transition={{ duration: 0.18 }}
              className="flex items-start gap-3 px-6 py-1.5"
            >
              <div className="mt-1 h-7 w-7 shrink-0 rounded-sm bg-bg-elevated border border-border flex items-center justify-center text-accent">
                <Sparkles className="h-3.5 w-3.5" />
              </div>
              <div className="flex-1 pt-2">
                <ThinkingDots />
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        <div ref={messagesEndRef} className="h-4" />
      </div>
    </div>
  );
}
