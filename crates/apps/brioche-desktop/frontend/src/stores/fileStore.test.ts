import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { DirEntry } from '../ipc';

vi.mock('../ipc', () => ({
    readDirectory: vi.fn(),
    createFile: vi.fn(),
    deleteFile: vi.fn(),
    writeFile: vi.fn(),
    createDirectory: vi.fn(),
}));

import { useFileStore } from './fileStore';
import { useSettingsStore } from './settingsStore';
import { readDirectory, createFile, deleteFile, writeFile, createDirectory } from '../ipc';

const mockedReadDirectory = vi.mocked(readDirectory);
const mockedCreateFile = vi.mocked(createFile);
const mockedDeleteFile = vi.mocked(deleteFile);
const mockedWriteFile = vi.mocked(writeFile);
const mockedCreateDirectory = vi.mocked(createDirectory);

function resetFileStore() {
    useFileStore.setState({
        currentPath: '',
        entries: [],
        isLoading: false,
    });
}

function setWorkingDir(path: string) {
    useSettingsStore.setState({ settings: { ui: { working_dir: path } } });
}

describe('fileStore', () => {
    beforeEach(() => {
        vi.resetAllMocks();
        resetFileStore();
        useSettingsStore.setState({ settings: {} });
    });

    describe('default state', () => {
        it('starts with an empty path and no entries', () => {
            const state = useFileStore.getState();
            expect(state.currentPath).toBe('');
            expect(state.entries).toEqual([]);
            expect(state.isLoading).toBe(false);
        });
    });

    describe('loadDirectory', () => {
        it('is a no-op when given an empty path', async () => {
            await useFileStore.getState().loadDirectory('');

            expect(mockedReadDirectory).not.toHaveBeenCalled();
            expect(useFileStore.getState().currentPath).toBe('');
        });

        it('loads entries and preserves their order', async () => {
            const entries: DirEntry[] = [
                { name: 'b.txt', is_dir: false, path: '/work/b.txt' },
                { name: 'a', is_dir: true, path: '/work/a' },
            ];
            mockedReadDirectory.mockResolvedValue(entries);

            await useFileStore.getState().loadDirectory('/work');

            expect(mockedReadDirectory).toHaveBeenCalledWith('/work');
            const state = useFileStore.getState();
            expect(state.currentPath).toBe('/work');
            expect(state.entries).toEqual(entries);
            expect(state.isLoading).toBe(false);
        });

        it('clears loading state on error', async () => {
            mockedReadDirectory.mockRejectedValue(new Error('ipc failure'));

            await useFileStore.getState().loadDirectory('/work');

            expect(useFileStore.getState().isLoading).toBe(false);
        });
    });

    describe('navigateUp', () => {
        it('does nothing when there is no current path', async () => {
            await useFileStore.getState().navigateUp();

            expect(mockedReadDirectory).not.toHaveBeenCalled();
        });

        it('stops at the workspace root', async () => {
            setWorkingDir('/work');
            useFileStore.setState({ currentPath: '/work' });

            await useFileStore.getState().navigateUp();

            expect(mockedReadDirectory).not.toHaveBeenCalled();
        });

        it('loads the parent directory', async () => {
            setWorkingDir('/work');
            useFileStore.setState({ currentPath: '/work/subdir' });
            mockedReadDirectory.mockResolvedValue([]);

            await useFileStore.getState().navigateUp();

            expect(mockedReadDirectory).toHaveBeenCalledWith('/work');
            expect(useFileStore.getState().currentPath).toBe('/work');
        });

        it('falls back to root when parent is empty', async () => {
            useFileStore.setState({ currentPath: '/tmp' });
            mockedReadDirectory.mockResolvedValue([]);

            await useFileStore.getState().navigateUp();

            expect(mockedReadDirectory).toHaveBeenCalledWith('/');
        });
    });

    describe('navigateTo', () => {
        it('loads the target directory', async () => {
            mockedReadDirectory.mockResolvedValue([]);

            await useFileStore.getState().navigateTo('/another');

            expect(mockedReadDirectory).toHaveBeenCalledWith('/another');
            expect(useFileStore.getState().currentPath).toBe('/another');
        });
    });

    describe('createNewFile', () => {
        it('creates a file and refreshes the current directory', async () => {
            useFileStore.setState({ currentPath: '/work' });
            mockedCreateFile.mockResolvedValue(undefined);
            mockedReadDirectory.mockResolvedValue([]);

            await useFileStore.getState().createNewFile('/work/new.txt');

            expect(mockedCreateFile).toHaveBeenCalledWith('/work/new.txt');
            expect(mockedReadDirectory).toHaveBeenCalledWith('/work');
        });

        it('rethrows creation errors', async () => {
            mockedCreateFile.mockRejectedValue(new Error('denied'));

            await expect(useFileStore.getState().createNewFile('/work/new.txt')).rejects.toThrow('denied');
        });
    });

    describe('createNewFolder', () => {
        it('creates a folder and refreshes the current directory', async () => {
            useFileStore.setState({ currentPath: '/work' });
            mockedCreateDirectory.mockResolvedValue(undefined);
            mockedReadDirectory.mockResolvedValue([]);

            await useFileStore.getState().createNewFolder('/work/folder');

            expect(mockedCreateDirectory).toHaveBeenCalledWith('/work/folder');
            expect(mockedReadDirectory).toHaveBeenCalledWith('/work');
        });
    });

    describe('deleteExistingFile', () => {
        it('deletes a file and refreshes the current directory', async () => {
            useFileStore.setState({ currentPath: '/work' });
            mockedDeleteFile.mockResolvedValue(undefined);
            mockedReadDirectory.mockResolvedValue([]);

            await useFileStore.getState().deleteExistingFile('/work/old.txt');

            expect(mockedDeleteFile).toHaveBeenCalledWith('/work/old.txt');
            expect(mockedReadDirectory).toHaveBeenCalledWith('/work');
        });
    });

    describe('writeExistingFile', () => {
        it('writes content without refreshing the directory', async () => {
            mockedWriteFile.mockResolvedValue(undefined);

            await useFileStore.getState().writeExistingFile('/work/file.txt', 'hello');

            expect(mockedWriteFile).toHaveBeenCalledWith('/work/file.txt', 'hello');
            expect(mockedReadDirectory).not.toHaveBeenCalled();
        });

        it('rethrows write errors', async () => {
            mockedWriteFile.mockRejectedValue(new Error('denied'));

            await expect(useFileStore.getState().writeExistingFile('/work/file.txt', 'x')).rejects.toThrow('denied');
        });
    });
});
