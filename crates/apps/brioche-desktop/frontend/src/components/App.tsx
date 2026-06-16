import { useEffect, useRef, useCallback, useState } from 'react';
import { useChatStore, MessageRole } from '../store';
import { useSessionStore } from '../stores/sessionStore';
import { useSettingsStore } from '../stores/settingsStore';
import { useFileStore } from '../stores/fileStore';
import {
    sendMessage,
    onChatMessage,
    onAppExit,
    onSessionChanged,
    onSessionsUpdated,
    getMessages,
} from '../ipc';
import SessionSidebar from './SessionSidebar';
import FileExplorer from './FileExplorer';
import SettingsPanel from './SettingsPanel';
import SkillsPanel from './SkillsPanel';
import MemoryPanel from './MemoryPanel';
import { MenuIcon, SettingsIcon, ClearIcon, SendIcon, BrainIcon, BookIcon } from './Icons';

export default function App() {
    const { messages, input, isLoading, addMessage, appendMessage, setInput, setLoading, clearMessages } =
        useChatStore();
    const { loadSessions, switchToSession, setSessions } = useSessionStore();
    const { loadSettings, settings } = useSettingsStore();
    const { loadDirectory } = useFileStore();
    const messagesEndRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef<HTMLInputElement>(null);
    const [showSettings, setShowSettings] = useState(false);
    const [showSkills, setShowSkills] = useState(false);
    const [showMemory, setShowMemory] = useState(false);
    const [leftSidebarOpen, setLeftSidebarOpen] = useState(true);
    const [rightSidebarOpen, setRightSidebarOpen] = useState(true);

    const scrollToBottom = useCallback(() => {
        messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }, []);

    useEffect(() => {
        scrollToBottom();
    }, [messages, scrollToBottom]);

    // Load sessions, settings, and working directory on mount
    useEffect(() => {
        loadSessions();
        loadSettings();
    }, [loadSessions, loadSettings]);

    // Load file explorer when working directory changes
    useEffect(() => {
        if (settings.working_dir) {
            loadDirectory(settings.working_dir);
        }
    }, [settings.working_dir, loadDirectory]);

    // Listen for session changes and refresh messages
    useEffect(() => {
        let unlistenChat: (() => void) | undefined;
        let unlistenExit: (() => void) | undefined;
        let unlistenSessionChanged: (() => void) | undefined;
        let unlistenSessionsUpdated: (() => void) | undefined;
        let cancelled = false;

        onChatMessage((msg) => {
            if (cancelled) return;
            const role = msg.role as MessageRole;
            if (role === 'assistant') {
                appendMessage(role, msg.content);
            } else {
                addMessage(role, msg.content);
            }
        }).then((fn) => {
            if (cancelled) {
                fn();
            } else {
                unlistenChat = fn;
            }
        });

        onAppExit(() => {
            if (!cancelled) window.close();
        }).then((fn) => {
            if (cancelled) {
                fn();
            } else {
                unlistenExit = fn;
            }
        });

        onSessionChanged(async (id) => {
            if (cancelled) return;
            clearMessages();
            loadSessions();
            try {
                const history = await getMessages();
                history.forEach((msg) => {
                    const role = msg.role as MessageRole;
                    if (role === 'assistant') {
                        appendMessage(role, msg.content);
                    } else {
                        addMessage(role, msg.content);
                    }
                });
            } catch (err) {
                console.error('Failed to load session messages:', err);
            }
        }).then((fn) => {
            if (cancelled) {
                fn();
            } else {
                unlistenSessionChanged = fn;
            }
        });

        onSessionsUpdated(() => {
            if (!cancelled) loadSessions();
        }).then((fn) => {
            if (cancelled) {
                fn();
            } else {
                unlistenSessionsUpdated = fn;
            }
        });

        return () => {
            cancelled = true;
            if (unlistenChat) unlistenChat();
            if (unlistenExit) unlistenExit();
            if (unlistenSessionChanged) unlistenSessionChanged();
            if (unlistenSessionsUpdated) unlistenSessionsUpdated();
        };
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    const handleSubmit = useCallback(
        async (e: React.FormEvent) => {
            e.preventDefault();
            const trimmed = input.trim();
            if (!trimmed || isLoading) return;

            setInput('');
            addMessage('user', trimmed);
            setLoading(true);

            try {
                await sendMessage(trimmed);
            } catch (err) {
                addMessage('error', String(err));
            } finally {
                setLoading(false);
            }
        },
        [input, isLoading, addMessage, setInput, setLoading],
    );

    const handleKeyDown = useCallback(
        (e: React.KeyboardEvent) => {
            if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                void handleSubmit(e);
            }
        },
        [handleSubmit],
    );

    return (
        <div className="app">
            <div className={`sidebar ${leftSidebarOpen ? '' : 'collapsed'}`}>
                <SessionSidebar />
            </div>

            <div className="main-panel">
                <header className="header">
                    <div className="header-left">
                        <button
                            type="button"
                            className="icon-btn"
                            onClick={() => setLeftSidebarOpen(!leftSidebarOpen)}
                            title="Toggle sessions"
                        >
                            <MenuIcon />
                        </button>
                        <span className="header-title">Brioche</span>
                    </div>
                    <div className="header-right">
                        <button
                            type="button"
                            className="header-btn"
                            onClick={() => setShowMemory(true)}
                            title="Memory"
                        >
                            <BrainIcon />
                            <span>Memory</span>
                        </button>
                        <button
                            type="button"
                            className="header-btn"
                            onClick={() => setShowSkills(true)}
                            title="Skills"
                        >
                            <BookIcon />
                            <span>Skills</span>
                        </button>
                        <button
                            type="button"
                            className="header-btn"
                            onClick={() => {
                                clearMessages();
                                void sendMessage('/clear');
                            }}
                            title="Clear history"
                        >
                            <ClearIcon />
                            <span>Clear</span>
                        </button>
                        <button
                            type="button"
                            className="header-btn"
                            onClick={() => setShowSettings(true)}
                            title="Settings"
                        >
                            <SettingsIcon />
                            <span>Settings</span>
                        </button>
                        <button
                            type="button"
                            className="icon-btn"
                            onClick={() => setRightSidebarOpen(!rightSidebarOpen)}
                            title="Toggle files"
                        >
                            <MenuIcon />
                        </button>
                    </div>
                </header>

                <div className="messages">
                    {messages.length === 0 && (
                        <div className="empty-state">
                            <div className="empty-state-title">Brioche Desktop</div>
                            <div className="empty-state-hint">Type a message or use /help for commands</div>
                        </div>
                    )}
                    {messages.map((msg) => (
                        <div key={msg.id} className={`message ${msg.role}`}>
                            <div className="message-header">
                                <span className="message-role">{msg.role}</span>
                            </div>
                            <div className="message-body">
                                <div className="message-content">{msg.content}</div>
                            </div>
                        </div>
                    ))}
                    {isLoading && (
                        <div className="message assistant">
                            <div className="message-header">
                                <span className="message-role">assistant</span>
                            </div>
                            <div className="message-body">
                                <div className="message-content loading">Thinking...</div>
                            </div>
                        </div>
                    )}
                    <div ref={messagesEndRef} />
                </div>

                <form className="input-bar" onSubmit={handleSubmit}>
                    <input
                        ref={inputRef}
                        type="text"
                        value={input}
                        onChange={(e) => setInput(e.target.value)}
                        onKeyDown={handleKeyDown}
                        placeholder="Type a message or /help..."
                        disabled={isLoading}
                        autoFocus
                    />
                    <button type="submit" disabled={isLoading || !input.trim()}>
                        <SendIcon />
                    </button>
                </form>
            </div>

            <div className={`sidebar sidebar-right ${rightSidebarOpen ? '' : 'collapsed'}`}>
                <FileExplorer />
            </div>

            {showSettings && <SettingsPanel onClose={() => setShowSettings(false)} />}
            {showSkills && <SkillsPanel onClose={() => setShowSkills(false)} />}
            {showMemory && <MemoryPanel onClose={() => setShowMemory(false)} />}
        </div>
    );
}
