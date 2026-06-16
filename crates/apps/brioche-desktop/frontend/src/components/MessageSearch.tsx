import { useState, useCallback, useEffect, useRef } from 'react';
import { SearchIcon, XIcon } from './Icons';

interface MessageSearchProps {
    messages: Array<{
        id: string;
        role: string;
        content: string;
        timestamp: number;
    }>;
    onJumpTo: (messageId: string) => void;
    isOpen: boolean;
    onClose: () => void;
}

export default function MessageSearch({ messages, onJumpTo, isOpen, onClose }: MessageSearchProps) {
    const [query, setQuery] = useState('');
    const [results, setResults] = useState<Array<{ id: string; preview: string; role: string }>>([]);
    const [selectedIndex, setSelectedIndex] = useState(0);
    const inputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        if (isOpen) {
            setQuery('');
            setResults([]);
            setSelectedIndex(0);
            setTimeout(() => inputRef.current?.focus(), 50);
        }
    }, [isOpen]);

    useEffect(() => {
        if (!query.trim()) {
            setResults([]);
            return;
        }
        const q = query.toLowerCase();
        const found = messages
            .filter((m) => m.content.toLowerCase().includes(q))
            .map((m) => ({
                id: m.id,
                role: m.role,
                preview: getPreview(m.content, q),
            }));
        setResults(found);
        setSelectedIndex(0);
    }, [query, messages]);

    const handleKeyDown = useCallback(
        (e: React.KeyboardEvent) => {
            if (e.key === 'Escape') {
                onClose();
                return;
            }
            if (e.key === 'Enter' && results[selectedIndex]) {
                onJumpTo(results[selectedIndex].id);
                onClose();
                return;
            }
            if (e.key === 'ArrowDown') {
                e.preventDefault();
                setSelectedIndex((i) => (i + 1) % results.length);
                return;
            }
            if (e.key === 'ArrowUp') {
                e.preventDefault();
                setSelectedIndex((i) => (i - 1 + results.length) % results.length);
                return;
            }
        },
        [results, selectedIndex, onJumpTo, onClose],
    );

    if (!isOpen) return null;

    return (
        <div className="command-palette-overlay" onClick={onClose}>
            <div className="command-palette" onClick={(e) => e.stopPropagation()}>
                <div className="command-palette-input" style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                    <SearchIcon />
                    <input
                        ref={inputRef}
                        type="text"
                        value={query}
                        onChange={(e) => setQuery(e.target.value)}
                        onKeyDown={handleKeyDown}
                        placeholder="Search messages..."
                        style={{ flex: 1, background: 'transparent', border: 'none', color: 'var(--text-primary)', fontSize: 16, outline: 'none' }}
                    />
                    {query && (
                        <button
                            type="button"
                            className="icon-btn"
                            onClick={() => setQuery('')}
                            style={{ width: 24, height: 24 }}
                        >
                            <XIcon />
                        </button>
                    )}
                </div>
                <div className="command-palette-results">
                    {results.length === 0 && query.trim() && (
                        <div style={{ padding: 24, textAlign: 'center', color: 'var(--text-muted)' }}>
                            No messages found
                        </div>
                    )}
                    {results.map((result, idx) => (
                        <div
                            key={result.id}
                            className={`command-palette-item ${idx === selectedIndex ? 'selected' : ''}`}
                            onClick={() => {
                                onJumpTo(result.id);
                                onClose();
                            }}
                            onMouseEnter={() => setSelectedIndex(idx)}
                        >
                            <div className="command-palette-item-icon">
                                <span style={{ fontSize: 10, textTransform: 'uppercase', fontWeight: 700, color: 'var(--text-muted)' }}>
                                    {result.role}
                                </span>
                            </div>
                            <div className="command-palette-item-text" style={{ fontSize: 13, color: 'var(--text-secondary)' }}>
                                {result.preview}
                            </div>
                        </div>
                    ))}
                    {results.length > 0 && (
                        <div style={{ padding: '8px 12px', fontSize: 11, color: 'var(--text-muted)', borderTop: '1px solid var(--border)' }}>
                            {results.length} result{results.length !== 1 ? 's' : ''}
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}

function getPreview(content: string, query: string): string {
    const lowerContent = content.toLowerCase();
    const idx = lowerContent.indexOf(query.toLowerCase());
    if (idx === -1) return content.slice(0, 100);
    const start = Math.max(0, idx - 40);
    const end = Math.min(content.length, idx + query.length + 40);
    let preview = content.slice(start, end);
    if (start > 0) preview = '...' + preview;
    if (end < content.length) preview = preview + '...';
    return preview;
}
