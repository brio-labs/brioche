import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { Settings, SettingsSection } from '../ipc';

vi.mock('../ipc', () => ({
    getSettings: vi.fn(),
    setSettings: vi.fn(),
    listSettingsSections: vi.fn(),
}));

import { useSettingsStore, getWorkingDir } from './settingsStore';
import { FALLBACK_SECTIONS } from './settingsSections';
import { getSettings, setSettings, listSettingsSections } from '../ipc';

const mockedGetSettings = vi.mocked(getSettings);
const mockedSetSettings = vi.mocked(setSettings);
const mockedListSettingsSections = vi.mocked(listSettingsSections);

function resetStore() {
    useSettingsStore.setState({
        settings: {},
        sections: [],
        isLoading: false,
        hasLoaded: false,
    });
}

describe('settingsStore', () => {
    beforeEach(() => {
        vi.resetAllMocks();
        resetStore();
    });

    describe('default state', () => {
        it('has an empty settings object', () => {
            expect(useSettingsStore.getState().settings).toEqual({});
        });

        it('has no loaded sections', () => {
            expect(useSettingsStore.getState().sections).toEqual([]);
        });

        it('is not loading and has not loaded', () => {
            const state = useSettingsStore.getState();
            expect(state.isLoading).toBe(false);
            expect(state.hasLoaded).toBe(false);
        });
    });

    describe('getWorkingDir', () => {
        it('returns the working directory from ui settings', () => {
            const settings: Settings = { ui: { working_dir: '/home/brioche' } };
            expect(getWorkingDir(settings)).toBe('/home/brioche');
        });

        it('returns an empty string when ui settings are missing', () => {
            expect(getWorkingDir({})).toBe('');
        });

        it('returns an empty string when working_dir is not a string', () => {
            const settings: Settings = { ui: { working_dir: 123 } };
            expect(getWorkingDir(settings)).toBe('');
        });
    });

    describe('updateSetting', () => {
        it('updates a nested setting value', () => {
            useSettingsStore.getState().updateSetting('chat.model', 'custom-model');
            expect(useSettingsStore.getState().settings).toEqual({
                chat: { model: 'custom-model' },
            });
        });

        it('preserves unrelated settings', () => {
            useSettingsStore.setState({ settings: { ui: { working_dir: '/tmp' } } });
            useSettingsStore.getState().updateSetting('chat.model', 'custom-model');
            expect(useSettingsStore.getState().settings).toEqual({
                ui: { working_dir: '/tmp' },
                chat: { model: 'custom-model' },
            });
        });

        it('ignores keys without a module prefix', () => {
            useSettingsStore.getState().updateSetting('standalone', 'value');
            expect(useSettingsStore.getState().settings).toEqual({});
        });
    });

    describe('getSetting', () => {
        it('returns a value at a nested path', () => {
            useSettingsStore.setState({
                settings: { context: { enabled: true, trigger_percentage: 75 } },
            });
            expect(useSettingsStore.getState().getSetting('context.enabled')).toBe(true);
            expect(useSettingsStore.getState().getSetting('context.trigger_percentage')).toBe(75);
        });

        it('returns undefined for missing paths', () => {
            expect(useSettingsStore.getState().getSetting('missing.key')).toBeUndefined();
        });
    });

    describe('loadSettings', () => {
        it('loads settings from IPC and marks hasLoaded', async () => {
            const loaded: Settings = { ui: { working_dir: '/project' } };
            mockedGetSettings.mockResolvedValue(loaded);

            await useSettingsStore.getState().loadSettings();

            expect(mockedGetSettings).toHaveBeenCalledTimes(1);
            const state = useSettingsStore.getState();
            expect(state.settings).toEqual(loaded);
            expect(state.isLoading).toBe(false);
            expect(state.hasLoaded).toBe(true);
        });

        it('clears loading state on error without marking loaded', async () => {
            mockedGetSettings.mockRejectedValue(new Error('ipc failure'));

            await useSettingsStore.getState().loadSettings();

            const state = useSettingsStore.getState();
            expect(state.isLoading).toBe(false);
            expect(state.hasLoaded).toBe(false);
        });
    });

    describe('saveSettings', () => {
        it('sends settings to IPC and updates local state', async () => {
            mockedSetSettings.mockResolvedValue(undefined);
            const next: Settings = { chat: { model: 'saved' } };

            await useSettingsStore.getState().saveSettings(next);

            expect(mockedSetSettings).toHaveBeenCalledWith(next);
            const state = useSettingsStore.getState();
            expect(state.settings).toEqual(next);
            expect(state.isLoading).toBe(false);
        });

        it('clears loading state on error', async () => {
            mockedSetSettings.mockRejectedValue(new Error('ipc failure'));

            await useSettingsStore.getState().saveSettings({});

            expect(useSettingsStore.getState().isLoading).toBe(false);
        });
    });

    describe('loadSections', () => {
        it('loads sections from IPC', async () => {
            const sections: SettingsSection[] = FALLBACK_SECTIONS.slice(0, 1);
            mockedListSettingsSections.mockResolvedValue(sections);

            await useSettingsStore.getState().loadSections();

            expect(mockedListSettingsSections).toHaveBeenCalledTimes(1);
            expect(useSettingsStore.getState().sections).toEqual(sections);
        });

        it('leaves sections empty when IPC fails', async () => {
            mockedListSettingsSections.mockRejectedValue(new Error('ipc failure'));

            await useSettingsStore.getState().loadSections();

            expect(useSettingsStore.getState().sections).toEqual([]);
        });
    });
});
