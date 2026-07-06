import { motion } from "framer-motion";
import {
  Brain,
  FileText,
  FileSpreadsheet,
  FileArchive,
  FileAudio,
  FileVideo,
  FileCode,
  File,
  Image as ImageIcon,
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

function getFileIcon(extension: string) {
  switch (extension) {
    case "pdf":
      return <FileText className="h-8 w-8 text-red-500 shrink-0" />;
    case "xlsx":
    case "xls":
    case "csv":
    case "ods":
      return <FileSpreadsheet className="h-8 w-8 text-emerald-500 shrink-0" />;
    case "zip":
    case "tar":
    case "gz":
    case "rar":
    case "7z":
      return <FileArchive className="h-8 w-8 text-amber-500 shrink-0" />;
    case "mp3":
    case "wav":
    case "ogg":
    case "aac":
      return <FileAudio className="h-8 w-8 text-sky-500 shrink-0" />;
    case "mp4":
    case "mkv":
    case "avi":
    case "mov":
      return <FileVideo className="h-8 w-8 text-indigo-500 shrink-0" />;
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
      return <FileCode className="h-8 w-8 text-violet-500 shrink-0" />;
    case "png":
    case "jpg":
    case "jpeg":
    case "gif":
    case "webp":
    case "svg":
      return <ImageIcon className="h-8 w-8 text-blue-500 shrink-0" />;
    default:
      return <File className="h-8 w-8 text-fg-muted shrink-0" />;
  }
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
      {messages.map((msg) => {
        // Check for attached local document reference
        const isAttachment = msg.role === "system" && msg.content.startsWith("Attached: ");
        // Check for attached image reference (markdown image)
        const imageMatch = msg.content.match(/^!\[(.*?)\]\((.*?)\)$/);

        return msg.role === "tool_request" || msg.role === "tool_result" ? (
          <motion.div
            id={`msg-${msg.id}`}
            key={msg.id}
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.2 }}
            className={`flex flex-col gap-2 relative max-w-[85%] ${
              msg.role === "tool_request" ? "self-end" : "self-start"
            }`}
          >
            <ToolCallMessage message={msg} />
          </motion.div>
        ) : (
          <motion.div
            id={`msg-${msg.id}`}
            key={msg.id}
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.2 }}
            className={`flex flex-col gap-2 relative max-w-[85%] ${
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
              {isAttachment ? (
                (() => {
                  const filePath = msg.content.substring("Attached: ".length).trim();
                  const fileName = filePath.split(/[/\\]/).pop() || "";
                  const extension = fileName.split(".").pop()?.toLowerCase() || "";
                  return (
                    <div className="flex items-center gap-3">
                      {getFileIcon(extension)}
                      <div className="flex flex-col min-w-0">
                        <span className="truncate text-xs font-semibold text-fg-primary" title={fileName}>
                          {fileName}
                        </span>
                        <span className="truncate text-[10px] text-fg-muted font-mono" title={filePath}>
                          {filePath}
                        </span>
                      </div>
                    </div>
                  );
                })()
              ) : imageMatch ? (
                (() => {
                  const imagePath = imageMatch[1];
                  const dataUrl = imageMatch[2];
                  const fileName = imagePath.split(/[/\\]/).pop() || "";
                  return (
                    <div className="flex flex-col gap-2">
                      <div className="flex items-center gap-3">
                        <div className="h-10 w-10 rounded-md border border-border overflow-hidden bg-bg-elevated shrink-0 flex items-center justify-center">
                          <img src={dataUrl} alt={fileName} className="h-full w-full object-cover" />
                        </div>
                        <div className="flex flex-col min-w-0">
                          <span className="truncate text-xs font-semibold text-fg-primary" title={fileName}>
                            {fileName}
                          </span>
                          <span className="truncate text-[10px] text-fg-muted font-mono" title={imagePath}>
                            {imagePath}
                          </span>
                        </div>
                      </div>
                      <div className="mt-1 rounded border border-border overflow-hidden bg-bg-elevated max-h-60 flex justify-center items-center">
                        <img src={dataUrl} alt={fileName} className="max-h-60 w-auto object-contain cursor-pointer hover:opacity-95" onClick={() => window.open(dataUrl)} />
                      </div>
                    </div>
                  );
                })()
              ) : (
                <MarkdownRenderer content={msg.content} />
              )}
            </div>
          </motion.div>
        );
      })}
      {isLoading && (
        <motion.div
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.2 }}
          className="flex flex-col gap-2 relative max-w-[85%] self-start"
        >
          <div className="flex items-center gap-2 mb-0.5 px-1">
            <span className="text-xs font-medium text-text-muted capitalize">
              assistant
            </span>
          </div>
          <div className="px-4 py-3 rounded-[8px] leading-relaxed text-sm wrap-break-word relative overflow-hidden bg-assistant-bg text-text-primary border border-border shadow-md">
            <div className="text-text-muted italic">Thinking...</div>
          </div>
        </motion.div>
      )}
      <div ref={messagesEndRef} />
    </div>
  );
}
