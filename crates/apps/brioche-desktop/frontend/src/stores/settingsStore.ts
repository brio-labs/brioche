import { create } from 'zustand';
import { getSettings, setSettings, listSettingsSections } from '../ipc';
import type { Settings, SettingsSection } from '../ipc';

/// Reads a dotted path from an arbitrary object.
///
/// Refs: I-Ui-SettingsPath
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

/// Sets a dotted path on a settings object without mutating the input.
///
/// Refs: I-Ui-SettingsPath
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
        current[part] = { ...((current[part] as Record<string, unknown>) || {}) };
        current = current[part] as Record<string, unknown>;
    }
    current[parts[parts.length - 1]] = value;
    return next;
}

/// Returns the working directory from UI settings if present.
///
/// Refs: I-Ui-WorkingDir
export function getWorkingDir(settings: Settings): string {
    const ui = settings.ui;
    if (ui && typeof ui === 'object' && 'working_dir' in ui) {
        const dir = (ui as Record<string, unknown>).working_dir;
        if (typeof dir === 'string') return dir;
    }
    return '';
}

/// State and actions for the settings editor.
///
/// Refs: I-Ui-SettingsStore
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

/// Zustand store that owns the settings object, sections list, and loading state.
///
/// Refs: I-Ui-SettingsStore
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
        } catch (err: unknown) {
            console.error('Failed to load settings:', err);
            set({ isLoading: false });
        }
    },

    saveSettings: async (settings: Settings) => {
        try {
            set({ isLoading: true });
            await setSettings(settings);
            set({ settings, isLoading: false });
        } catch (err: unknown) {
            console.error('Failed to save settings:', err);
            set({ isLoading: false });
        }
    },

    updateSetting: (key: string, value: unknown) => {
        set((state) => ({ settings: setValue(state.settings, key, value) }));
    },

    getSetting: (key: string) => getValue(get().settings, key),

    loadSections: async () => {
        try {
            const sections = await listSettingsSections();
            set({ sections });
        } catch (err: unknown) {
            console.error('Failed to load settings sections:', err);
        }
    },
}));
