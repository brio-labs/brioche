import { create } from 'zustand';
import { readDirectory, createFile, deleteFile, writeFile, createDirectory } from '../ipc';
import type { DirEntry } from '../ipc';
import { useSettingsStore, getWorkingDir } from './settingsStore';

/// State and actions for the file explorer panel.
///
/// Refs: I-Ui-FileStore
interface FileStore {
    currentPath: string;
    entries: DirEntry[];
    isLoading: boolean;
    loadDirectory: (path: string) => Promise<void>;
    navigateUp: () => Promise<void>;
    navigateTo: (path: string) => Promise<void>;
    createNewFile: (path: string) => Promise<void>;
    createNewFolder: (path: string) => Promise<void>;
    deleteExistingFile: (path: string) => Promise<void>;
    writeExistingFile: (path: string, content: string) => Promise<void>;
}

/// Zustand store that owns the current directory path and its entries.
///
/// Refs: I-Ui-FileStore
export const useFileStore = create<FileStore>((set, get) => ({
    currentPath: '',
    entries: [],
    isLoading: false,

    loadDirectory: async (path: string) => {
        if (!path) return;
        try {
            set({ isLoading: true });
            const entries = await readDirectory(path);
            set({ currentPath: path, entries, isLoading: false });
        } catch (err: unknown) {
            console.error('Failed to load directory:', err);
            set({ isLoading: false });
        }
    },

    navigateUp: async () => {
        const { currentPath } = get();
        if (!currentPath) return;
        const workspaceRoot = getWorkingDir(useSettingsStore.getState().settings);
        if (currentPath === workspaceRoot) return;
        const parent = currentPath.split('/').slice(0, -1).join('/') || '/';
        await get().loadDirectory(parent);
    },

    navigateTo: async (path: string) => {
        await get().loadDirectory(path);
    },

    createNewFile: async (path: string) => {
        try {
            await createFile(path);
            await get().loadDirectory(get().currentPath);
        } catch (err: unknown) {
            console.error('Failed to create file:', err);
            throw err;
        }
    },

    createNewFolder: async (path: string) => {
        try {
            await createDirectory(path);
            await get().loadDirectory(get().currentPath);
        } catch (err: unknown) {
            console.error('Failed to create folder:', err);
            throw err;
        }
    },

    deleteExistingFile: async (path: string) => {
        try {
            await deleteFile(path);
            await get().loadDirectory(get().currentPath);
        } catch (err: unknown) {
            console.error('Failed to delete file:', err);
            throw err;
        }
    },

    writeExistingFile: async (path: string, content: string) => {
        try {
            await writeFile(path, content);
        } catch (err: unknown) {
            console.error('Failed to write file:', err);
            throw err;
        }
    },
}));
