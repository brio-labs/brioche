import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { SessionInfo, SessionSort } from '../ipc';

vi.mock('../ipc', () => ({
    listSessions: vi.fn(),
    switchSession: vi.fn(),
    deleteSession: vi.fn(),
    newSession: vi.fn(),
}));

import { useSessionStore } from './sessionStore';
import { listSessions, switchSession, deleteSession, newSession } from '../ipc';

const mockedListSessions = vi.mocked(listSessions);
const mockedSwitchSession = vi.mocked(switchSession);
const mockedDeleteSession = vi.mocked(deleteSession);
const mockedNewSession = vi.mocked(newSession);

function resetStore() {
    useSessionStore.setState({
        sessions: [],
        currentSessionId: null,
        sortMode: 'date',
        isLoading: false,
    });
}

async function flushMicrotasks() {
    for (let i = 0; i < 5; i++) {
        await Promise.resolve();
    }
}

describe('sessionStore', () => {
    beforeEach(() => {
        vi.resetAllMocks();
        resetStore();
    });

    describe('default state', () => {
        it('starts with an empty session list', () => {
            expect(useSessionStore.getState().sessions).toEqual([]);
        });

        it('has no current session selected', () => {
            expect(useSessionStore.getState().currentSessionId).toBeNull();
        });

        it('defaults to date sort', () => {
            expect(useSessionStore.getState().sortMode).toBe('date');
        });
    });

    describe('setSessions', () => {
        it('updates sessions and derives the active session id', () => {
            useSessionStore.getState().setSessions([
                { id: 'a', active: false },
                { id: 'b', active: true },
            ]);

            const state = useSessionStore.getState();
            expect(state.sessions).toHaveLength(2);
            expect(state.currentSessionId).toBe('b');
        });

        it('falls back to null when no session is active', () => {
            useSessionStore.getState().setSessions([
                { id: 'a', active: false },
                { id: 'b', active: false },
            ]);

            expect(useSessionStore.getState().currentSessionId).toBeNull();
        });
    });

    describe('loadSessions', () => {
        it('fetches sessions using the current sort mode', async () => {
            const sessions: SessionInfo[] = [
                { id: 's1', active: false },
                { id: 's2', active: true },
            ];
            mockedListSessions.mockResolvedValue(sessions);

            await useSessionStore.getState().loadSessions();

            expect(mockedListSessions).toHaveBeenCalledWith('date');
            const state = useSessionStore.getState();
            expect(state.sessions).toEqual(sessions);
            expect(state.currentSessionId).toBe('s2');
        });

        it('keeps existing state when IPC fails', async () => {
            useSessionStore.setState({ sessions: [{ id: 'old', active: true }], currentSessionId: 'old' });
            mockedListSessions.mockRejectedValue(new Error('ipc failure'));

            await useSessionStore.getState().loadSessions();

            const state = useSessionStore.getState();
            expect(state.sessions).toEqual([{ id: 'old', active: true }]);
            expect(state.currentSessionId).toBe('old');
        });
    });

    describe('setSortMode', () => {
        it('updates the sort mode and reloads sessions', async () => {
            const sessions: SessionInfo[] = [{ id: 'x', active: true }];
            mockedListSessions.mockResolvedValue(sessions);

            useSessionStore.getState().setSortMode('name');
            await flushMicrotasks();

            const state = useSessionStore.getState();
            expect(state.sortMode).toBe('name');
            expect(mockedListSessions).toHaveBeenCalledWith('name');
            expect(state.sessions).toEqual(sessions);
            expect(state.currentSessionId).toBe('x');
        });
    });

    describe('switchToSession', () => {
        it('calls IPC and updates active state locally', async () => {
            mockedSwitchSession.mockResolvedValue(undefined);
            useSessionStore.setState({
                sessions: [
                    { id: 's1', active: true },
                    { id: 's2', active: false },
                ],
                currentSessionId: 's1',
            });

            await useSessionStore.getState().switchToSession('s2');

            expect(mockedSwitchSession).toHaveBeenCalledWith('s2');
            const state = useSessionStore.getState();
            expect(state.sessions.find((s) => s.id === 's1')?.active).toBe(false);
            expect(state.sessions.find((s) => s.id === 's2')?.active).toBe(true);
            expect(state.currentSessionId).toBe('s2');
        });

        it('does not change state when IPC fails', async () => {
            mockedSwitchSession.mockRejectedValue(new Error('ipc failure'));
            useSessionStore.setState({
                sessions: [{ id: 's1', active: true }],
                currentSessionId: 's1',
            });

            await useSessionStore.getState().switchToSession('s2');

            expect(useSessionStore.getState().currentSessionId).toBe('s1');
        });
    });

    describe('deleteSession', () => {
        it('removes the session after IPC succeeds', async () => {
            mockedDeleteSession.mockResolvedValue(undefined);
            useSessionStore.setState({
                sessions: [{ id: 's1', active: true }, { id: 's2', active: false }],
                currentSessionId: 's1',
            });

            await useSessionStore.getState().deleteSession('s1');

            expect(mockedDeleteSession).toHaveBeenCalledWith('s1');
            const state = useSessionStore.getState();
            expect(state.sessions).toEqual([{ id: 's2', active: false }]);
        });
    });

    describe('createSession', () => {
        it('creates a session, reloads the list, and returns the id', async () => {
            const sessions: SessionInfo[] = [
                { id: 'new', active: true },
                { id: 'old', active: false },
            ];
            mockedNewSession.mockResolvedValue('new');
            mockedListSessions.mockResolvedValue(sessions);

            const id = await useSessionStore.getState().createSession();

            expect(id).toBe('new');
            expect(mockedNewSession).toHaveBeenCalledTimes(1);
            expect(mockedListSessions).toHaveBeenCalledWith('date');
            const state = useSessionStore.getState();
            expect(state.sessions).toEqual(sessions);
            expect(state.currentSessionId).toBe('new');
        });

        it('returns null when IPC fails', async () => {
            mockedNewSession.mockRejectedValue(new Error('ipc failure'));

            const id = await useSessionStore.getState().createSession();

            expect(id).toBeNull();
        });
    });
});
