import { create } from 'zustand';
import { getSettings, setSettings } from '../ipc';
import type { Settings } from '../ipc';

interface SettingsStore {
    settings: Settings;
    isLoading: boolean;
    hasLoaded: boolean;
    loadSettings: () => Promise<void>;
    saveSettings: (settings: Settings) => Promise<void>;
    updateSettings: (partial: Partial<Settings>) => void;
}

const defaultSettings: Settings = {
    api_key: '',
    model: 'gpt-4o-mini',
    base_url: 'https://api.openai.com/v1',
    working_dir: '',
    stream: true,
};

export const useSettingsStore = create<SettingsStore>((set, get) => ({
    settings: { ...defaultSettings },
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
    updateSettings: (partial) => {
        set((state) => ({
            settings: { ...state.settings, ...partial },
        }));
    },
}));
