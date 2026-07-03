import Tooltip from "../Tooltip";
import { SendIcon, PaperclipIcon, ImageIcon, ClearIcon } from "../Icons";

interface ChatInputProps {
  input: string;
  setInput: (value: string) => void;
  isLoading: boolean;
  handleSubmit: (e?: React.FormEvent) => Promise<void> | void;
  handleKeyDown: (e: React.KeyboardEvent) => void;
  handleAttach: () => Promise<void> | void;
  handleImage: () => Promise<void> | void;
  handleClearChat: () => void;
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
}: ChatInputProps) {
  return (
    <form
      className="input-bar flex gap-3 px-4 py-3 bg-bg-1/80 backdrop-blur-md border-t border-border shrink-0 relative"
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
            <ClearIcon className="w-4 h-4" />
          </button>
        </Tooltip>
        <Tooltip label="Attach file/folder">
          <button
            type="button"
            className="btn-icon w-8 h-8 text-fg-secondary hover:text-fg-primary"
            onClick={handleAttach}
            aria-label="Attach file/folder"
          >
            <PaperclipIcon className="w-4 h-4" />
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
        disabled={isLoading || !input.trim()}
        aria-label="Send message"
      >
        <SendIcon className="w-4 h-4" />
      </button>
    </form>
  );
}
