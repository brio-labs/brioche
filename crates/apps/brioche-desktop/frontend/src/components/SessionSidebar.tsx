import { useCallback } from 'react';
import { useSessionStore } from '../stores/sessionStore';
import { PlusIcon, TrashIcon } from './Icons';

export default function SessionSidebar() {
    const { sessions, currentSessionId, switchToSession, deleteSession, createSession } =
        useSessionStore();

    const handleNewSession = useCallback(async () => {
        await createSession();
    }, [createSession]);

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
            <div className="session-list">
                {sessions.map((session) => (
                    <div
                        key={session.id}
                        className={`session-item ${session.active ? 'active' : ''}`}
                        onClick={() => switchToSession(session.id)}
                    >
                        <div className="session-indicator" />
                        <div className="session-name">{session.id}</div>
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
                {sessions.length === 0 && (
                    <div className="empty-sidebar">No sessions</div>
                )}
            </div>
        </div>
    );
}
