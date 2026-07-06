import Tooltip from "../Tooltip";
import {
  Trash2,
  Paperclip,
  Image as ImageIcon,
  Send,
  FileText,
  FileSpreadsheet,
  FileArchive,
  FileAudio,
  FileVideo,
  FileCode,
  File,
  X,
} from "lucide-react";
import type { Attachment } from "../../hooks/app/useChatActions";

interface ChatInputProps {
  input: string;
  setInput: (value: string) => void;
  isLoading: boolean;
  handleSubmit: (e?: React.FormEvent) => Promise<void> | void;
  handleKeyDown: (e: React.KeyboardEvent) => void;
  handleAttach: () => Promise<void> | void;
  handleImage: () => Promise<void> | void;
  handleClearChat: () => void;
  pendingAttachments: Attachment[];
  removeAttachment: (id: string) => void;
}

function getFileIcon(extension: string) {
  switch (extension) {
    case "pdf":
      return <FileText className="h-4 w-4 text-red-500 shrink-0" />;
    case "xlsx":
    case "xls":
    case "csv":
    case "ods":
      return <FileSpreadsheet className="h-4 w-4 text-emerald-500 shrink-0" />;
    case "zip":
    case "tar":
    case "gz":
    case "rar":
    case "7z":
      return <FileArchive className="h-4 w-4 text-amber-500 shrink-0" />;
    case "mp3":
    case "wav":
    case "ogg":
    case "aac":
      return <FileAudio className="h-4 w-4 text-sky-500 shrink-0" />;
    case "mp4":
    case "mkv":
    case "avi":
    case "mov":
      return <FileVideo className="h-4 w-4 text-indigo-500 shrink-0" />;
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
      return <FileCode className="h-4 w-4 text-violet-500 shrink-0" />;
    default:
      return <File className="h-4 w-4 text-fg-muted shrink-0" />;
  }
}

export default function ChatInput({
  input,
  setInput,
  isLoading,
  handleSubmit,
  handleKeyDown,
  handleAttach,
  handleImage,
  handleClearChat,
  pendingAttachments,
  removeAttachment,
}: ChatInputProps) {
  return (
    <div className="flex flex-col bg-bg-surface border-t border-border shrink-0">
      {pendingAttachments.length > 0 && (
        <div className="flex flex-wrap gap-2 px-4 py-2 border-b border-border bg-bg-base/20 max-h-36 overflow-y-auto animate-fadeIn">
          {pendingAttachments.map((att) => (
            <div
              key={att.id}
              className="flex items-center gap-2 rounded bg-bg-elevated border border-border px-2 py-1 text-xs"
            >
              {att.type === "image" && att.dataUrl ? (
                <div className="h-6 w-6 rounded border border-border overflow-hidden bg-bg-surface shrink-0">
                  <img src={att.dataUrl} alt={att.name} className="h-full w-full object-cover" />
                </div>
              ) : (
                getFileIcon(att.name.split(".").pop()?.toLowerCase() || "")
              )}
              <span className="truncate max-w-40 font-mono text-[11px] text-fg-primary" title={att.path}>
                {att.name}
              </span>
              <button
                type="button"
                onClick={() => removeAttachment(att.id)}
                className="btn-icon h-4 w-4 rounded-full text-fg-muted hover:text-error-text"
                title="Remove attachment"
              >
                <X className="h-3 w-3" />
              </button>
            </div>
          ))}
        </div>
      )}

      <form
        className="input-bar flex gap-3 px-4 py-3 bg-transparent relative"
        onSubmit={handleSubmit}
      >
        <div className="flex items-center gap-2">
          <Tooltip label="Clear history">
            <button
              type="button"
              className="btn-icon w-8 h-8 text-fg-secondary hover:text-fg-primary"
              onClick={handleClearChat}
              aria-label="Clear history"
            >
              <Trash2 className="w-4 h-4" />
            </button>
          </Tooltip>
          <Tooltip label="Attach file/folder">
            <button
              type="button"
              className="btn-icon w-8 h-8 text-fg-secondary hover:text-fg-primary"
              onClick={handleAttach}
              aria-label="Attach file/folder"
            >
              <Paperclip className="w-4 h-4" />
            </button>
          </Tooltip>
          <Tooltip label="Send image">
            <button
              type="button"
              className="btn-icon w-8 h-8 text-fg-secondary hover:text-fg-primary"
              onClick={handleImage}
              aria-label="Send image"
            >
              <ImageIcon className="w-4 h-4" />
            </button>
          </Tooltip>
        </div>
        <textarea
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Type a message or /help..."
          disabled={isLoading}
          className="flex-1 bg-bg-2 border border-border text-text-primary px-4 py-3 rounded-md text-sm outline-none resize-none min-h-11 max-h-50 leading-relaxed transition-all duration-200 placeholder:text-text-dim disabled:opacity-50 disabled:cursor-not-allowed focus:border-accent-dim focus:bg-bg-3 focus:ring-2 focus:ring-accent-glow"
          rows={1}
        />
        <button
          type="submit"
          className="px-6 py-3 bg-accent text-white rounded-md cursor-pointer font-semibold text-sm tracking-wide transition-all duration-200 flex items-center justify-center relative overflow-hidden disabled:opacity-40 disabled:cursor-not-allowed disabled:bg-bg-5 hover:bg-accent-hover hover:shadow-lg hover:shadow-accent-glow/20 hover:-translate-y-0.5 active:translate-y-0"
          disabled={isLoading || (!input.trim() && pendingAttachments.length === 0)}
          aria-label="Send message"
        >
          <Send className="w-4 h-4" />
        </button>
      </form>
    </div>
  );
}
