import { create } from "zustand";
import { listSessions, switchSession, deleteSession, newSession } from "../ipc";
import type { SessionSort } from "../ipc";

/// A Brioche session as displayed in the desktop UI.
///
/// Refs: I-Ui-Session
export interface Session {
	id: string;
	title?: string;
	active: boolean;
	created_at?: number;
	updated_at?: number;
	workspace?: string;
}

/// State and actions for the session list sidebar.
///
/// Refs: I-Ui-SessionStore
interface SessionStore {
	sessions: Session[];
	currentSessionId: string | null;
	sortMode: SessionSort;
	isLoading: boolean;
	loadSessions: () => Promise<void>;
	setSortMode: (sort: SessionSort) => void;
	switchToSession: (id: string) => Promise<void>;
	deleteSession: (id: string) => Promise<void>;
	createSession: () => Promise<string | null>;
	setSessions: (sessions: Session[]) => void;
}

/// Zustand store that owns the session list, sort mode, and active session id.
///
/// Refs: I-Ui-SessionStore
export const useSessionStore = create<SessionStore>((set, get) => ({
	sessions: [],
	currentSessionId: null,
	sortMode: "date",
	isLoading: false,

	loadSessions: async () => {
		try {
			const sessions = await listSessions(get().sortMode);
			// Keep draft if active
			const currentId = get().currentSessionId;
			if (currentId === "draft") {
				const hasDraft = sessions.some((s) => s.id === "draft");
				if (!hasDraft) {
					sessions.unshift({ id: "draft", title: "New Conversation", active: true });
				}
				set({ sessions, currentSessionId: "draft" });
				return;
			}
			const current = sessions.find((s) => s.active);
			set({ sessions, currentSessionId: current?.id ?? null });
		} catch (err: unknown) {
			console.error("Failed to load sessions:", err);
		}
	},

	setSortMode: (sort: SessionSort) => {
		set({ sortMode: sort });
		get().loadSessions();
	},

	switchToSession: async (id: string) => {
		try {
			if (id !== "draft") {
				await switchSession(id);
			}
			set((state) => ({
				sessions: state.sessions.map((s) => ({
					...s,
					active: s.id === id,
				})),
				currentSessionId: id,
			}));
		} catch (err: unknown) {
			console.error("Failed to switch session:", err);
		}
	},

	deleteSession: async (id: string) => {
		try {
			if (id !== "draft") {
				await deleteSession(id);
			}
			set((state) => ({
				sessions: state.sessions.filter((s) => s.id !== id),
			}));
		} catch (err: unknown) {
			console.error("Failed to delete session:", err);
		}
	},

	createSession: async () => {
		try {
			const currentSessions = get().sessions.map(s => ({ ...s, active: false })).filter(s => s.id !== "draft");
			currentSessions.unshift({
				id: "draft",
				title: "New Conversation",
				active: true,
			});
			set({ sessions: currentSessions, currentSessionId: "draft" });
			return "draft";
		} catch (err: unknown) {
			console.error("Failed to set draft session:", err);
			return null;
		}
	},

	setSessions: (sessions: Session[]) => {
		const current = sessions.find((s) => s.active);
		set({ sessions, currentSessionId: current?.id ?? null });
	},
}));
