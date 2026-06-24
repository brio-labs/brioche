import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { ToolDescriptor } from '../ipc';

vi.mock('../ipc', () => ({
    isTauri: vi.fn(() => true),
    listSkills: vi.fn(),
    getSkillContent: vi.fn(),
    setSkillEnabled: vi.fn(),
    createSkill: vi.fn(),
    deleteSkill: vi.fn(),
    listMemories: vi.fn(),
    setMemory: vi.fn(),
    deleteMemory: vi.fn(),
    searchMemories: vi.fn(),
    listTools: vi.fn(),
    setToolEnabled: vi.fn(),
}));

import { useToolsStore } from './panelStores';
import { listTools, setToolEnabled } from '../ipc';

const mockedListTools = vi.mocked(listTools);
const mockedSetToolEnabled = vi.mocked(setToolEnabled);

function resetToolsStore() {
    useToolsStore.setState({
        tools: [],
        isLoading: false,
        error: null,
    });
}

describe('useToolsStore', () => {
    beforeEach(() => {
        vi.resetAllMocks();
        resetToolsStore();
    });

    describe('default state', () => {
        it('starts with an empty tool list', () => {
            expect(useToolsStore.getState().tools).toEqual([]);
        });

        it('is not loading and has no error', () => {
            const state = useToolsStore.getState();
            expect(state.isLoading).toBe(false);
            expect(state.error).toBeNull();
        });

        it('reports Tauri as available', () => {
            expect(useToolsStore.getState().isTauriAvailable).toBe(true);
        });
    });

    describe('loadTools', () => {
        it('loads the tool list from IPC', async () => {
            const tools: ToolDescriptor[] = [
                { id: 'tool-1', name: 'Read', enabled: true },
                { id: 'tool-2', name: 'Write', enabled: false },
            ];
            mockedListTools.mockResolvedValue(tools);

            await useToolsStore.getState().loadTools();

            expect(mockedListTools).toHaveBeenCalledTimes(1);
            const state = useToolsStore.getState();
            expect(state.tools).toEqual(tools);
            expect(state.isLoading).toBe(false);
            expect(state.error).toBeNull();
        });

        it('records an error when IPC fails', async () => {
            mockedListTools.mockRejectedValue(new Error('ipc failure'));

            await useToolsStore.getState().loadTools();

            const state = useToolsStore.getState();
            expect(state.isLoading).toBe(false);
            expect(state.error).toBe('Error: ipc failure');
        });
    });

    describe('toggleTool', () => {
        it('enables a tool and refreshes the list', async () => {
            mockedSetToolEnabled.mockResolvedValue(undefined);
            const refreshed: ToolDescriptor[] = [{ id: 'tool-1', name: 'Read', enabled: true }];
            mockedListTools.mockResolvedValue(refreshed);

            await useToolsStore.getState().toggleTool('tool-1', true);

            expect(mockedSetToolEnabled).toHaveBeenCalledWith('tool-1', true);
            expect(mockedListTools).toHaveBeenCalledTimes(1);
            expect(useToolsStore.getState().tools).toEqual(refreshed);
            expect(useToolsStore.getState().error).toBeNull();
        });

        it('disables a tool and refreshes the list', async () => {
            mockedSetToolEnabled.mockResolvedValue(undefined);
            mockedListTools.mockResolvedValue([]);

            await useToolsStore.getState().toggleTool('tool-1', false);

            expect(mockedSetToolEnabled).toHaveBeenCalledWith('tool-1', false);
            expect(useToolsStore.getState().tools).toEqual([]);
        });

        it('records an error when IPC fails', async () => {
            mockedSetToolEnabled.mockRejectedValue(new Error('ipc failure'));

            await useToolsStore.getState().toggleTool('tool-1', true);

            expect(useToolsStore.getState().error).toBe('Error: ipc failure');
        });
    });
});
