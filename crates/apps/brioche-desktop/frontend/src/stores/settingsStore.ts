import { create } from 'zustand';
import { getPathValue, setPathValue } from './settingsPath';
import { getSettings, setSettings, listSettingsSections } from '../ipc';
import type { Settings, SettingsSection } from '../ipc';

/// Returns the working directory from UI settings if present.
export function getWorkingDir(settings: Settings): string {
    const ui = settings.ui;
    if (ui && typeof ui === 'object' && 'working_dir' in ui) {
        const dir = (ui as Record<string, unknown>).working_dir;
        if (typeof dir === 'string') return dir;
    }
    return '';
}

interface SettingsStore {
    settings: Settings;
    sections: SettingsSection[];
    isLoading: boolean;
    hasLoaded: boolean;
    loadSettings: () => Promise<void>;
    saveSettings: (settings: Settings) => Promise<void>;
    updateSetting: (key: string, value: unknown) => void;
    getSetting: (key: string) => unknown;
    loadSections: () => Promise<void>;
}

export const useSettingsStore = create<SettingsStore>((set, get) => ({
    settings: {},
    sections: [],
    isLoading: false,
    hasLoaded: false,
    loadSettings: async () => {
        try {
            set({ isLoading: true });
            const settings = await getSettings();
            set({ settings, isLoading: false, hasLoaded: true });
        } catch (err) {
            console.error('Failed to load settings:', err);
            set({ isLoading: false });
        }
    },
    saveSettings: async (settings) => {
        try {
            set({ isLoading: true });
            await setSettings(settings);
            set({ settings, isLoading: false });
        } catch (err) {
            console.error('Failed to save settings:', err);
            set({ isLoading: false });
        }
    },
    updateSetting: (key, value) => {
        set((state) => ({ settings: setPathValue(state.settings, key, value) }));
    },
    getSetting: (key) => getPathValue(get().settings, key),
    loadSections: async () => {
        try {
            const sections = await listSettingsSections();
            set({ sections });
        } catch (err) {
            console.error('Failed to load settings sections:', err);
        }
    },
}));
