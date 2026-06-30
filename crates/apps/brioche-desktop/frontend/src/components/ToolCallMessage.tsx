import { useState } from "react";
import { cn } from "./ui/lib";
import type { ChatMessage } from "../store";

/**
 * Attempts to pretty-print a JSON string; returns the original text on failure.
 */
function tryFormatJson(value: string | undefined): string {
  if (!value) return "";
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}

/**
 * Props for {@link ToolCallMessage}.
 */
interface ToolCallMessageProps {
  message: ChatMessage;
}

/**
 * Renders a collapsible tool call or tool result message.
 *
 * Displays the tool name and ID, with expandable arguments and output.
 *
 * Refs: I-Ui-ChatToolRendering
 */
export default function ToolCallMessage({ message }: ToolCallMessageProps) {
  const [expanded, setExpanded] = useState(false);
  const isResult = message.role === "tool_result";
  const name = message.toolName || "tool";
  const id = message.toolId;

  return (
    <div
      className={cn(
        "overflow-hidden w-full max-w-150 my-2 rounded-lg border border-border border-l-3 bg-bg-elevated",
        isResult ? "border-l-success-border" : "border-l-accent"
      )}
    >
      <button
        type="button"
        className={cn(
          "flex w-full items-center gap-2 px-3 py-2 text-left font-mono text-xs text-fg-secondary",
          "transition-colors duration-150 hover:bg-accent/5 active:bg-accent/10",
          "focus-visible:bg-bg-highlight focus-visible:outline-none"
        )}
        onClick={() => setExpanded((v) => !v)}
      >
        <span className="text-xs">{isResult ? "⚙️" : "🔧"}</span>
        <span className="font-semibold text-fg-primary">{name}</span>
        {id && (
          <span className="ml-auto text-xs text-fg-muted">{id}</span>
        )}
        <span className="text-xs text-fg-muted">
          {expanded ? "▾" : "▸"}
        </span>
      </button>
      {expanded && (
        <div className="flex flex-col gap-4 p-4 border-t border-border bg-bg-surface/40">
          {message.toolArguments && (
            <div className="flex flex-col gap-2">
              <div className="text-xs font-bold uppercase tracking-wider text-fg-muted">
                Arguments
              </div>
              <pre className="overflow-x-auto whitespace-pre-wrap break-all rounded border border-border bg-bg-surface p-3 font-mono text-xs text-fg-secondary">
                {tryFormatJson(message.toolArguments)}
              </pre>
            </div>
          )}
          {message.toolOutput && (
            <div className="flex flex-col gap-2">
              <div className="text-xs font-bold uppercase tracking-wider text-fg-muted">
                Output
              </div>
              <pre className="overflow-x-auto whitespace-pre-wrap break-all rounded border border-border bg-bg-surface p-3 font-mono text-xs text-fg-secondary">
                {tryFormatJson(message.toolOutput)}
              </pre>
            </div>
          )}
          {!message.toolArguments && !message.toolOutput && (
            <div className="text-xs italic text-fg-muted">No details</div>
          )}
        </div>
      )}
    </div>
  );
}
