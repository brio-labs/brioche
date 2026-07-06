import Tooltip from "../Tooltip";
import {
  Paperclip,
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
  pendingAttachments,
  removeAttachment,
}: ChatInputProps) {
  return (
    <div className="flex flex-col bg-bg-surface border-t border-border shrink-0">
      {pendingAttachments.length > 0 && (
        <div className="flex flex-wrap gap-3 px-4 py-3 border-b border-border bg-bg-base/20 max-h-40 overflow-y-auto animate-fadeIn">
          {pendingAttachments.map((att) => (
            <div
              key={att.id}
              className="flex items-center gap-2.5 rounded-[4px] bg-bg-elevated border border-border p-2 pr-3 max-w-64 min-w-44 relative group shadow-sm transition-all duration-200 hover:border-border-hover"
            >
              {/* Square preview container */}
              <div className="h-10 w-10 rounded-[2px] bg-bg-highlight border border-border overflow-hidden shrink-0 flex items-center justify-center">
                {att.type === "image" && att.dataUrl ? (
                  <img src={att.dataUrl} alt={att.name} className="h-full w-full object-cover" />
                ) : (
                  (() => {
                    const ext = att.name.split(".").pop()?.toUpperCase() || "FILE";
                    return (
                      <div className="flex flex-col items-center justify-center h-full w-full p-1 bg-bg-surface/50">
                        {getFileIcon(ext.toLowerCase())}
                        <span className="text-[7.5px] font-bold text-accent mt-0.5 tracking-wider font-mono">{ext}</span>
                      </div>
                    );
                  })()
                )}
              </div>

              {/* Details */}
              <div className="flex flex-col min-w-0 flex-1">
                <span className="truncate text-xs font-semibold text-fg-primary leading-tight" title={att.path}>
                  {att.name}
                </span>
                <span className="text-[9px] text-fg-muted font-mono uppercase tracking-wider mt-0.5">
                  {att.type === "image" ? "Image" : att.name.split(".").pop() || "Document"}
                </span>
              </div>

              {/* Floating Close Button */}
              <button
                type="button"
                onClick={() => removeAttachment(att.id)}
                className="absolute -top-1.5 -right-1.5 h-5 w-5 rounded-full border border-border bg-bg-elevated text-fg-muted hover:text-error-text flex items-center justify-center shadow-md transition-colors cursor-pointer"
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
          <Tooltip label="Attach file or image">
            <button
              type="button"
              className="btn-icon w-8 h-8 text-fg-secondary hover:text-fg-primary"
              onClick={handleAttach}
              aria-label="Attach file or image"
            >
              <Paperclip className="w-4 h-4" />
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
