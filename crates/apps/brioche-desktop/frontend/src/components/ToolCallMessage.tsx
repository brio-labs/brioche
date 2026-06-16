import { useState } from 'react';
import type { ChatMessage } from '../store';

function tryFormatJson(value: string | undefined): string {
    if (!value) return '';
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
    const isResult = message.role === 'tool_result';
    const name = message.toolName || 'tool';
    const id = message.toolId;

    return (
        <div className={`tool-call-card ${isResult ? 'result' : 'request'}`}>
            <button
                type="button"
                className="tool-call-header"
                onClick={() => setExpanded((v) => !v)}
            >
                <span className="tool-call-icon">{isResult ? '⚙️' : '🔧'}</span>
                <span className="tool-call-name">{name}</span>
                {id && <span className="tool-call-id">{id}</span>}
                <span className="tool-call-toggle">{expanded ? '▾' : '▸'}</span>
            </button>
            {expanded && (
                <div className="tool-call-body">
                    {message.toolArguments && (
                        <div className="tool-call-section">
                            <div className="tool-call-section-title">Arguments</div>
                            <pre>{tryFormatJson(message.toolArguments)}</pre>
                        </div>
                    )}
                    {message.toolOutput && (
                        <div className="tool-call-section">
                            <div className="tool-call-section-title">Output</div>
                            <pre>{tryFormatJson(message.toolOutput)}</pre>
                        </div>
                    )}
                    {!message.toolArguments && !message.toolOutput && (
                        <div className="tool-call-empty">No details</div>
                    )}
                </div>
            )}
        </div>
    );
}
