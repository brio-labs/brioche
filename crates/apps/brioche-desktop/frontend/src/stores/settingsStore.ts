import { create } from 'zustand';
import { getSettings, setSettings } from '../ipc';
import type { Settings } from '../ipc';

function getValue(obj: unknown, path: string): unknown {
    const parts = path.split('.');
    let current: unknown = obj;
    for (const part of parts) {
        if (current && typeof current === 'object' && !Array.isArray(current)) {
            current = (current as Record<string, unknown>)[part];
        } else {
            return undefined;
        }
    }
    return current;
}

function setValue(obj: Settings, path: string, value: unknown): Settings {
    const parts = path.split('.');
    if (parts.length < 2) return obj;
    const next = { ...obj };
    const moduleName = parts[0];
    const moduleObj = { ...((next[moduleName] as Record<string, unknown>) || {}) };
    next[moduleName] = moduleObj;
    let current: Record<string, unknown> = moduleObj;
    for (let i = 1; i < parts.length - 1; i++) {
        const part = parts[i];
        current[part] = { ...(current[part] as Record<string, unknown> || {}) };
        current = current[part] as Record<string, unknown>;
    }
    current[parts[parts.length - 1]] = value;
    return next;
}

interface SettingsStore {
    settings: Settings;
    isLoading: boolean;
    hasLoaded: boolean;
    loadSettings: () => Promise<void>;
    saveSettings: (settings: Settings) => Promise<void>;
    updateSetting: (key: string, value: unknown) => void;
    getSetting: (key: string) => unknown;
}

export const useSettingsStore = create<SettingsStore>((set, get) => ({
    settings: {},
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
        set((state) => ({ settings: setValue(state.settings, key, value) }));
    },
    getSetting: (key) => getValue(get().settings, key),
}));
