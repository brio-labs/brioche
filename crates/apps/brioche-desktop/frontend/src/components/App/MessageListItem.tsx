import { motion } from "framer-motion";
import {
  Sparkles,
  FileText,
  FileSpreadsheet,
  FileArchive,
  FileAudio,
  FileVideo,
  FileCode,
  File,
} from "lucide-react";
import MarkdownRenderer from "../MarkdownRenderer";
import ToolCallMessage from "../ToolCallMessage";
import type { ChatMessage } from "../../store";

function getFileIcon(ext: string) {
  switch (ext) {
    case "pdf":
      return <FileText className="h-7 w-7 text-red-400 shrink-0" />;
    case "xlsx":
    case "xls":
    case "csv":
      return <FileSpreadsheet className="h-7 w-7 text-emerald-400 shrink-0" />;
    case "zip":
    case "tar":
    case "gz":
    case "rar":
      return <FileArchive className="h-7 w-7 text-amber-400 shrink-0" />;
    case "mp3":
    case "wav":
    case "ogg":
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
    case "json":
      return <FileCode className="h-7 w-7 text-violet-400 shrink-0" />;
    default:
      return <File className="h-7 w-7 text-fg-muted shrink-0" />;
  }
}

function AttachmentCard({ content }: { content: string }) {
  const filePath = content.substring("Attached: ".length).trim();
  const fileName = filePath.split(/[/\\]/).pop() || "";
  const ext = fileName.split(".").pop()?.toLowerCase() || "";
  return (
    <div className="flex items-center gap-3 rounded-md border border-border bg-fg-primary/5 px-3 py-2.5 mt-1 max-w-xs">
      {getFileIcon(ext)}
      <div className="flex flex-col min-w-0">
        <span
          className="truncate text-[12px] font-semibold text-fg-primary"
          title={fileName}
        >
          {fileName}
        </span>
        <span
          className="truncate text-[10px] text-fg-muted font-mono"
          title={filePath}
        >
          {filePath}
        </span>
      </div>
    </div>
  );
}

function ImageAttachment({ content }: { content: string }) {
  const match = content.match(/^!\[(.*?)\]\((.*?)\)$/);
  if (!match) return null;
  const [, imagePath, dataUrl] = match;
  const fileName = imagePath.split(/[/\\]/).pop() || "";
  return (
    <div
      className="mt-1.5 rounded-md overflow-hidden border border-border w-full max-w-55 cursor-pointer"
      onClick={() => window.open(dataUrl)}
    >
      <img
        src={dataUrl}
        alt={fileName}
        className="w-full max-h-40 object-cover hover:opacity-90 transition-opacity"
      />
      <div className="px-2 py-1 bg-bg-base/35 text-[10px] text-fg-muted font-mono truncate">
        {fileName}
      </div>
    </div>
  );
}

function AssistantAvatar() {
  return (
    <div className="mt-1 h-7 w-7 shrink-0 rounded-sm bg-bg-elevated border border-border flex items-center justify-center text-accent">
      <Sparkles className="h-3.5 w-3.5" />
    </div>
  );
}

function ToolMessage({ message }: { message: ChatMessage }) {
  return (
    <motion.div
      id={`msg-${message.id}`}
      key={message.id}
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.18 }}
      className="px-6 py-1"
    >
      <ToolCallMessage message={message} />
    </motion.div>
  );
}

function SystemMessage({ message }: { message: ChatMessage }) {
  const isAttachment =
    message.role === "system" && message.content.startsWith("Attached: ");
  const isError = message.role === "error";
  return (
    <motion.div
      id={`msg-${message.id}`}
      key={message.id}
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
            📎 {message.content.substring("Attached: ".length).trim().split(/[/\\]/).pop()}
          </>
        ) : (
          message.content
        )}
      </div>
    </motion.div>
  );
}

function UserMessage({ message }: { message: ChatMessage }) {
  const isAttachment = message.content.startsWith("Attached: ");
  const imageMatch = message.content.match(/^!\[(.*?)\]\((.*?)\)$/);
  const parts = message.content.split(/\n\n(?=Attached: |!\[)/);
  const textPart = parts[0].trim();
  const attachParts = parts.slice(1);

  return (
    <motion.div
      id={`msg-${message.id}`}
      key={message.id}
      initial={{ opacity: 0, y: 10, x: 20 }}
      animate={{ opacity: 1, y: 0, x: 0 }}
      transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
      className="flex justify-start px-6 py-1.5 group"
    >
      <div className="flex flex-col gap-1 max-w-[75%] min-w-0">
        <div className="bg-bg-surface/70 text-fg-primary px-2 rounded-sm text-sm leading-relaxed wrap-break-word overflow-hidden w-full">
          {isAttachment || imageMatch ? null : (
            <MarkdownRenderer content={textPart || message.content} />
          )}
          {attachParts.map((part, index) => {
            if (part.startsWith("Attached: ")) {
              return <AttachmentCard key={index} content={part} />;
            }
            if (part.match(/^!\[/)) {
              return <ImageAttachment key={index} content={part} />;
            }
            return null;
          })}
          {isAttachment && <AttachmentCard content={message.content} />}
          {imageMatch && <ImageAttachment content={message.content} />}
        </div>
      </div>
    </motion.div>
  );
}

function AgentAnswer({ message }: { message: ChatMessage }) {
  return (
    <motion.div
      id={`msg-${message.id}`}
      key={message.id}
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
      className="flex items-start gap-3 px-6 py-1.5 group"
    >
      <AssistantAvatar />
      <div className="flex-1 min-w-0 text-[13.5px] leading-relaxed text-fg-primary pt-0.5">
        <MarkdownRenderer content={message.content} />
      </div>
    </motion.div>
  );
}

export function ThinkingIndicator() {
  return (
    <motion.div
      key="thinking"
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -4 }}
      transition={{ duration: 0.18 }}
      className="flex items-start gap-3 px-6 py-1.5"
    >
      <AssistantAvatar />
      <div className="flex-1 pt-2">
        <span className="inline-flex items-center gap-1 px-1">
          <motion.span
            animate={{ opacity: [0.3, 1, 0.3] }}
            transition={{ duration: 1.2, repeat: Infinity, delay: 0 }}
            className="h-1.5 w-1.5 rounded-full bg-fg-muted"
          />
          <motion.span
            animate={{ opacity: [0.3, 1, 0.3] }}
            transition={{ duration: 1.2, repeat: Infinity, delay: 0.2 }}
            className="h-1.5 w-1.5 rounded-full bg-fg-muted"
          />
          <motion.span
            animate={{ opacity: [0.3, 1, 0.3] }}
            transition={{ duration: 1.2, repeat: Infinity, delay: 0.4 }}
            className="h-1.5 w-1.5 rounded-full bg-fg-muted"
          />
        </span>
      </div>
    </motion.div>
  );
}

export function MessageListItem({ message }: { message: ChatMessage }) {
  if (
    message.role === "tool_request" ||
    message.role === "tool_result" ||
    message.role === "tool_argument" ||
    message.role === "tool_done"
  ) {
    return <ToolMessage message={message} />;
  }

  if (message.role === "system" || message.role === "error") {
    return <SystemMessage message={message} />;
  }

  if (message.role === "user") {
    return <UserMessage message={message} />;
  }

  return <AgentAnswer message={message} />;
}
