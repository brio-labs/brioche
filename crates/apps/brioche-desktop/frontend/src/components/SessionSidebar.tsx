import { useCallback, useMemo } from 'react';
import { useSessionStore } from '../stores/sessionStore';
import { PlusIcon, TrashIcon } from './Icons';
import type { SessionSort } from '../ipc';

function formatDate(timestamp: number): string {
    if (!timestamp) return 'unknown';
    const date = new Date(timestamp * 1000);
    return date.toLocaleDateString(undefined, {
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
    });
}

function workspaceName(workspace: string): string {
    if (!workspace) return 'No workspace';
    const parts = workspace.split('/');
    return parts[parts.length - 1] || workspace;
}

export default function SessionSidebar() {
    const {
        sessions,
        sortMode,
        setSortMode,
        switchToSession,
        deleteSession,
        createSession,
    } = useSessionStore();

    const handleNewSession = useCallback(async () => {
        await createSession();
    }, [createSession]);

    const groupedSessions = useMemo(() => {
        const sorted = [...sessions].sort((a, b) => {
            if (sortMode === 'date') {
                return (b.created_at ?? 0) - (a.created_at ?? 0);
            }
            if (sortMode === 'name') {
                return a.id.localeCompare(b.id);
            }
            return (a.workspace || '').localeCompare(b.workspace || '');
        });

        if (sortMode === 'workspace') {
            const groups = new Map<string, typeof sorted>();
            for (const session of sorted) {
                const key = workspaceName(session.workspace || '');
                if (!groups.has(key)) {
                    groups.set(key, []);
                }
                groups.get(key)!.push(session);
            }
            return groups;
        }

        const single = new Map<string, typeof sorted>();
        single.set(
            sortMode === 'date' ? 'Recent sessions' : 'Sessions',
            sorted,
        );
        return single;
    }, [sessions, sortMode]);

    return (
        <div className="sidebar session-sidebar">
            <div className="sidebar-header">
                <h2>Sessions</h2>
                <button
                    type="button"
                    className="icon-btn"
                    onClick={handleNewSession}
                    title="New session"
                >
                    <PlusIcon />
                </button>
            </div>
            <div className="session-sort-bar">
                <label htmlFor="session-sort">Sort</label>
                <select
                    id="session-sort"
                    value={sortMode}
                    onChange={(e) => setSortMode(e.target.value as SessionSort)}
                >
                    <option value="date">Date</option>
                    <option value="workspace">Workspace</option>
                    <option value="name">Name</option>
                </select>
            </div>
            <div className="session-list">
                {Array.from(groupedSessions.entries()).map(([group, items]) => (
                    <div key={group} className="session-group">
                        <div className="session-group-title">{group}</div>
                        {items.map((session) => (
                            <div
                                key={session.id}
                                className={`session-item ${session.active ? 'active' : ''}`}
                                onClick={() => switchToSession(session.id)}
                                title={session.workspace}
                            >
                                <div className="session-indicator" />
                                <div className="session-info">
                                    <div className="session-name">{session.id}</div>
                                    <div className="session-meta">
                                        {formatDate(session.created_at ?? 0)}
                                        {session.workspace && (
                                            <span className="session-workspace">
                                                {' '}
                                                · {workspaceName(session.workspace)}
                                            </span>
                                        )}
                                    </div>
                                </div>
                                {!session.active && (
                                    <button
                                        type="button"
                                        className="session-delete"
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            deleteSession(session.id);
                                        }}
                                        title="Delete session"
                                    >
                                        <TrashIcon />
                                    </button>
                                )}
                            </div>
                        ))}
                    </div>
                ))}
                {sessions.length === 0 && (
                    <div className="empty-sidebar">No sessions</div>
                )}
            </div>
        </div>
    );
}
