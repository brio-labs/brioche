import { useState, useCallback, useEffect, useRef } from 'react';
import {
    SearchIcon,
    CommandIcon,
    MessageIcon,
    SettingsIcon,
    TerminalIcon,
    PlusIcon,
    ExportIcon,
    XIcon,
} from './Icons';

interface Command {
    id: string;
    label: string;
    shortcut?: string;
    group: string;
    icon: React.ReactNode;
    action: () => void;
}

interface CommandPaletteProps {
    isOpen: boolean;
    onClose: () => void;
    commands: Command[];
}

export default function CommandPalette({ isOpen, onClose, commands }: CommandPaletteProps) {
    const [query, setQuery] = useState('');
    const [selectedIndex, setSelectedIndex] = useState(0);
    const inputRef = useRef<HTMLInputElement>(null);

    const filtered = useMemo(() => {
        if (!query.trim()) return commands;
        const q = query.toLowerCase();
        return commands.filter(
            (c) =>
                c.label.toLowerCase().includes(q) ||
                c.group.toLowerCase().includes(q) ||
                (c.shortcut && c.shortcut.toLowerCase().includes(q)),
        );
    }, [query, commands]);

    const grouped = useMemo(() => {
        const groups: Record<string, Command[]> = {};
        filtered.forEach((cmd) => {
            if (!groups[cmd.group]) groups[cmd.group] = [];
            groups[cmd.group].push(cmd);
        });
        return groups;
    }, [filtered]);

    useEffect(() => {
        if (isOpen) {
            setQuery('');
            setSelectedIndex(0);
            setTimeout(() => inputRef.current?.focus(), 50);
        }
    }, [isOpen]);

    useEffect(() => {
        setSelectedIndex(0);
    }, [query]);

    const handleKeyDown = useCallback(
        (e: React.KeyboardEvent) => {
            if (e.key === 'Escape') {
                onClose();
                return;
            }
            if (e.key === 'Enter') {
                const flat = Object.values(grouped).flat();
                if (flat[selectedIndex]) {
                    flat[selectedIndex].action();
                    onClose();
                }
                return;
            }
            if (e.key === 'ArrowDown') {
                e.preventDefault();
                const total = Object.values(grouped).flat().length;
                setSelectedIndex((i) => (i + 1) % total);
                return;
            }
            if (e.key === 'ArrowUp') {
                e.preventDefault();
                const total = Object.values(grouped).flat().length;
                setSelectedIndex((i) => (i - 1 + total) % total);
                return;
            }
        },
        [grouped, selectedIndex, onClose],
    );

    if (!isOpen) return null;

    let flatIndex = 0;

    return (
        <div className="command-palette-overlay" onClick={onClose}>
            <div className="command-palette" onClick={(e) => e.stopPropagation()}>
                <div className="command-palette-input">
                    <SearchIcon />
                    <input
                        ref={inputRef}
                        type="text"
                        value={query}
                        onChange={(e) => setQuery(e.target.value)}
                        onKeyDown={handleKeyDown}
                        placeholder="Type a command or search..."
                    />
                </div>
                <div className="command-palette-results">
                    {Object.entries(grouped).map(([groupName, items]) => (
                        <div key={groupName} className="command-palette-group">
                            <div className="command-palette-group-title">{groupName}</div>
                            {items.map((cmd) => {
                                const isSelected = flatIndex === selectedIndex;
                                const idx = flatIndex++;
                                return (
                                    <div
                                        key={cmd.id}
                                        className={`command-palette-item ${isSelected ? 'selected' : ''}`}
                                        onClick={() => {
                                            cmd.action();
                                            onClose();
                                        }}
                                        onMouseEnter={() => setSelectedIndex(idx)}
                                    >
                                        <div className="command-palette-item-icon">{cmd.icon}</div>
                                        <div className="command-palette-item-text">{cmd.label}</div>
                                        {cmd.shortcut && (
                                            <div className="command-palette-item-shortcut">{cmd.shortcut}</div>
                                        )}
                                    </div>
                                );
                            })}
                        </div>
                    ))}
                    {filtered.length === 0 && (
                        <div style={{ padding: 24, textAlign: 'center', color: 'var(--text-muted)' }}>
                            No commands found
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}

// Helper to build commands
export function buildCommands(
    handlers: {
        newSession: () => void;
        clearChat: () => void;
        openSettings: () => void;
        toggleSessions: () => void;
        toggleFiles: () => void;
        exportChat: () => void;
    },
): Command[] {
    return [
        {
            id: 'new-session',
            label: 'New Session',
            shortcut: '⌘N',
            group: 'Session',
            icon: <PlusIcon />,
            action: handlers.newSession,
        },
        {
            id: 'clear-chat',
            label: 'Clear Chat',
            shortcut: '⌘K C',
            group: 'Session',
            icon: <XIcon />,
            action: handlers.clearChat,
        },
        {
            id: 'export-chat',
            label: 'Export Chat',
            shortcut: '⌘E',
            group: 'Session',
            icon: <ExportIcon />,
            action: handlers.exportChat,
        },
        {
            id: 'toggle-sessions',
            label: 'Toggle Sessions Panel',
            shortcut: '⌘B',
            group: 'View',
            icon: <MessageIcon />,
            action: handlers.toggleSessions,
        },
        {
            id: 'toggle-files',
            label: 'Toggle Files Panel',
            shortcut: '⌘J',
            group: 'View',
            icon: <TerminalIcon />,
            action: handlers.toggleFiles,
        },
        {
            id: 'open-settings',
            label: 'Settings',
            shortcut: '⌘,',
            group: 'Settings',
            icon: <SettingsIcon />,
            action: handlers.openSettings,
        },
    ];
}
