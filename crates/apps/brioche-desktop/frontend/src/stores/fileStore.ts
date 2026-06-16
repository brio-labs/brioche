import { create } from 'zustand';
import { readDirectory } from '../ipc';
import type { DirEntry } from '../ipc';

interface FileStore {
    currentPath: string;
    entries: DirEntry[];
    isLoading: boolean;
    loadDirectory: (path: string) => Promise<void>;
    navigateUp: () => Promise<void>;
    navigateTo: (path: string) => Promise<void>;
}

export const useFileStore = create<FileStore>((set, get) => ({
    currentPath: '',
    entries: [],
    isLoading: false,
    loadDirectory: async (path) => {
        if (!path) return;
        try {
            set({ isLoading: true });
            const entries = await readDirectory(path);
            set({ currentPath: path, entries, isLoading: false });
        } catch (err) {
            console.error('Failed to load directory:', err);
            set({ isLoading: false });
        }
    },
    navigateUp: async () => {
        const { currentPath } = get();
        if (!currentPath) return;
        const parent = currentPath.split('/').slice(0, -1).join('/') || '/';
        await get().loadDirectory(parent);
    },
    navigateTo: async (path) => {
        await get().loadDirectory(path);
    },
}));
