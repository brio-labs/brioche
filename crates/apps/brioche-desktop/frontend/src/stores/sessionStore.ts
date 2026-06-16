import { create } from 'zustand';
import { listSessions, switchSession, deleteSession, newSession } from '../ipc';

export interface Session {
    id: string;
    active: boolean;
}

interface SessionStore {
    sessions: Session[];
    currentSessionId: string | null;
    isLoading: boolean;
    loadSessions: () => Promise<void>;
    switchToSession: (id: string) => Promise<void>;
    deleteSession: (id: string) => Promise<void>;
    createSession: () => Promise<string | null>;
    setSessions: (sessions: Session[]) => void;
}

export const useSessionStore = create<SessionStore>((set) => ({
    sessions: [],
    currentSessionId: null,
    isLoading: false,
    loadSessions: async () => {
        try {
            const sessions = await listSessions();
            const current = sessions.find((s) => s.active);
            set({ sessions, currentSessionId: current?.id ?? null });
        } catch (err) {
            console.error('Failed to load sessions:', err);
        }
    },
    switchToSession: async (id) => {
        try {
            await switchSession(id);
            set((state) => ({
                sessions: state.sessions.map((s) => ({
                    ...s,
                    active: s.id === id,
                })),
                currentSessionId: id,
            }));
        } catch (err) {
            console.error('Failed to switch session:', err);
        }
    },
    deleteSession: async (id) => {
        try {
            await deleteSession(id);
            set((state) => ({
                sessions: state.sessions.filter((s) => s.id !== id),
            }));
        } catch (err) {
            console.error('Failed to delete session:', err);
        }
    },
    createSession: async () => {
        try {
            const id = await newSession();
            const sessions = await listSessions();
            const current = sessions.find((s) => s.active);
            set({ sessions, currentSessionId: current?.id ?? null });
            return id;
        } catch (err) {
            console.error('Failed to create session:', err);
            return null;
        }
    },
    setSessions: (sessions) => {
        const current = sessions.find((s) => s.active);
        set({ sessions, currentSessionId: current?.id ?? null });
    },
}));
