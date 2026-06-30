import { useState } from "react";
import type { ChatMessage } from "../store";

function tryFormatJson(value: string | undefined): string {
  if (!value) return "";
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}

interface ToolCallMessageProps {
  message: ChatMessage;
}

export default function ToolCallMessage({ message }: ToolCallMessageProps) {
  const [expanded, setExpanded] = useState(false);
  const isResult = message.role === "tool_result";
  const name = message.toolName || "tool";
  const id = message.toolId;

  return (
    <div
      className={`bg-bg-2 border border-border rounded-lg overflow-hidden my-2 max-w-150 w-full ${
        isResult
          ? "border-l-3 border-l-emerald-600"
          : "border-l-3 border-l-accent"
      }`}
    >
      <button
        type="button"
        className="w-full flex items-center gap-2 px-3 py-2 bg-transparent text-text-secondary text-xs font-mono text-left cursor-pointer transition-colors duration-150 hover:bg-accent/5"
        onClick={() => setExpanded((v) => !v)}
      >
        <span className="text-xs">{isResult ? "⚙️" : "🔧"}</span>
        <span className="font-semibold text-text-primary">{name}</span>
        {id && (
          <span className="text-text-muted text-xs ml-auto">{id}</span>
        )}
        <span className="text-text-muted text-xs">
          {expanded ? "▾" : "▸"}
        </span>
      </button>
      {expanded && (
        <div className="p-4 border-t border-border flex flex-col gap-4 bg-bg-1/40">
          {message.toolArguments && (
            <div className="flex flex-col gap-2">
              <div className="text-xs font-bold uppercase tracking-wider text-text-muted">
                Arguments
              </div>
              <pre className="bg-bg-1 border border-border rounded p-3 text-xs font-mono text-text-secondary overflow-x-auto whitespace-pre-wrap break-all">
                {tryFormatJson(message.toolArguments)}
              </pre>
            </div>
          )}
          {message.toolOutput && (
            <div className="flex flex-col gap-2">
              <div className="text-xs font-bold uppercase tracking-wider text-text-muted">
                Output
              </div>
              <pre className="bg-bg-1 border border-border rounded p-3 text-xs font-mono text-text-secondary overflow-x-auto whitespace-pre-wrap break-all">
                {tryFormatJson(message.toolOutput)}
              </pre>
            </div>
          )}
          {!message.toolArguments && !message.toolOutput && (
            <div className="text-xs text-text-muted italic">No details</div>
          )}
        </div>
      )}
    </div>
  );
}
