import { useCallback, useState, useEffect } from 'react';
import { useFileStore } from '../stores/fileStore';
import { useSettingsStore } from '../stores/settingsStore';
import { readFile } from '../ipc';
import { open } from '@tauri-apps/plugin-dialog';
import { FolderIcon, FileIcon, ChevronUpIcon, RefreshIcon, TrashIcon, SaveIcon } from './Icons';

export default function FileExplorer() {
    const { 
        currentPath, 
        entries, 
        isLoading, 
        loadDirectory, 
        navigateUp, 
        navigateTo,
        createNewFile,
        createNewFolder,
        deleteExistingFile,
        writeExistingFile
    } = useFileStore();
    const [preview, setPreview] = useState<{ path: string; content: string } | null>(null);
    const workspaceRoot = useSettingsStore((state) => (state.settings.ui as any)?.working_dir || '');

    // Inline Creation State
    const [isCreating, setIsCreating] = useState(false);
    const [createType, setCreateType] = useState<'file' | 'folder'>('file');
    const [createParentPath, setCreateParentPath] = useState('');
    const [newName, setNewName] = useState('');

    // Context Menu State
    const [contextMenu, setContextMenu] = useState<{
        x: number;
        y: number;
        targetPath: string | null;
        isDir: boolean;
    } | null>(null);

    const handleOpenFolder = useCallback(async () => {
        try {
            const selected = await open({
                multiple: false,
                directory: true,
            });
            if (selected && typeof selected === 'string') {
                const store = useSettingsStore.getState();
                store.updateSetting('ui.working_dir', selected);
                await store.saveSettings(useSettingsStore.getState().settings);
                await loadDirectory(selected);
            }
        } catch (err) {
            console.error('Failed to open directory picker:', err);
        }
    }, [loadDirectory]);

    const handleEntryClick = useCallback(
        (entry: { is_dir: boolean; path: string }) => {
            if (entry.is_dir) {
                navigateTo(entry.path);
            }
        },
        [navigateTo]
    );

    const handleEntryDoubleClick = useCallback(
        async (entry: { is_dir: boolean; path: string }) => {
            if (entry.is_dir) return;
            try {
                const content = await readFile(entry.path);
                setPreview({ path: entry.path, content });
            } catch (err) {
                console.error('Failed to read file:', err);
            }
        },
        []
    );

    const handleDelete = useCallback(
        async (e: React.MouseEvent, path: string) => {
            e.stopPropagation();
            if (!confirm(`Delete ${path}?`)) return;
            try {
                await deleteExistingFile(path);
            } catch (err) {
                console.error('Failed to delete:', err);
            }
        },
        [deleteExistingFile]
    );

    const handleSavePreview = useCallback(async () => {
        if (!preview) return;
        try {
            await writeExistingFile(preview.path, preview.content);
            setPreview(null);
        } catch (err) {
            console.error('Failed to save file:', err);
        }
    }, [preview, writeExistingFile]);

    // Context Menu Handlers
    const handleContainerContextMenu = useCallback((e: React.MouseEvent) => {
        e.preventDefault();
        setContextMenu({
            x: e.clientX,
            y: e.clientY,
            targetPath: null,
            isDir: true,
        });
    }, []);

    const handleItemContextMenu = useCallback((e: React.MouseEvent, entry: { path: string; is_dir: boolean }) => {
        e.preventDefault();
        e.stopPropagation();
        setContextMenu({
            x: e.clientX,
            y: e.clientY,
            targetPath: entry.path,
            isDir: entry.is_dir,
        });
    }, []);

    useEffect(() => {
        const handleOutsideClick = () => {
            setContextMenu(null);
        };
        window.addEventListener('click', handleOutsideClick);
        return () => window.removeEventListener('click', handleOutsideClick);
    }, []);

    const startCreation = useCallback((type: 'file' | 'folder', targetPath: string | null, isDir: boolean) => {
        setIsCreating(true);
        setCreateType(type);
        setNewName('');
        
        if (targetPath === null) {
            setCreateParentPath(currentPath);
        } else if (isDir) {
            setCreateParentPath(targetPath);
        } else {
            const parent = targetPath.split('/').slice(0, -1).join('/') || '/';
            setCreateParentPath(parent);
        }
        setContextMenu(null);
    }, [currentPath]);

    const cancelCreation = useCallback(() => {
        setIsCreating(false);
        setNewName('');
    }, []);

    const handleCommitCreation = useCallback(async () => {
        const trimmed = newName.trim();
        if (!trimmed) {
            cancelCreation();
            return;
        }
        const fullPath = `${createParentPath.replace(/\/$/, '')}/${trimmed}`;
        try {
            if (createType === 'file') {
                await createNewFile(fullPath);
            } else {
                await createNewFolder(fullPath);
            }
        } catch (err) {
            console.error('Failed to create item:', err);
        } finally {
            setIsCreating(false);
            setNewName('');
        }
    }, [newName, createParentPath, createType, createNewFile, createNewFolder, cancelCreation]);

    const handleDeleteFromMenu = useCallback(async (path: string) => {
        setContextMenu(null);
        if (!confirm(`Delete ${path}?`)) return;
        try {
            await deleteExistingFile(path);
        } catch (err) {
            console.error('Failed to delete:', err);
        }
    }, [deleteExistingFile]);

    return (
        <div className="flex flex-col h-full w-full bg-transparent text-text-primary relative overflow-hidden">
            <div className="flex items-center justify-between px-4 py-3 border-b border-border h-[52px] shrink-0 bg-bg-0/30 backdrop-blur-sm">
                <h2 className="text-[11px] font-bold tracking-[0.14em] uppercase text-text-muted select-none">Explorer</h2>
                <div className="flex items-center gap-2">
                    <button
                        type="button"
                        className="w-7 h-7 flex items-center justify-center rounded bg-transparent text-text-muted hover:text-text-secondary hover:bg-bg-3 active:bg-bg-4 transition-all duration-200 cursor-pointer"
                        onClick={handleOpenFolder}
                        title="Open Folder..."
                    >
                        <FolderIcon className="w-4 h-4" />
                    </button>
                    <button
                        type="button"
                        className="w-7 h-7 flex items-center justify-center rounded bg-transparent text-text-muted hover:text-text-secondary hover:bg-bg-3 active:bg-bg-4 transition-all duration-200 cursor-pointer"
                        onClick={() => loadDirectory(currentPath)}
                        title="Refresh"
                    >
                        <RefreshIcon className="w-4 h-4" />
                    </button>
                </div>
            </div>
            <div className="flex items-center gap-2 px-3 py-2 border-b border-border bg-bg-0/50">
                <button
                    type="button"
                    className="p-1 hover:bg-bg-3 rounded-md text-text-muted hover:text-text-secondary disabled:opacity-30 disabled:cursor-not-allowed transition-all duration-200 cursor-pointer"
                    onClick={navigateUp}
                    disabled={currentPath === '/' || currentPath === workspaceRoot}
                    title="Parent directory"
                >
                    <ChevronUpIcon className="w-3.5 h-3.5" />
                </button>
                <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[11px] text-text-muted" title={currentPath}>
                    {currentPath || 'No directory'}
                </span>
            </div>

            <div className="flex-1 overflow-y-auto py-2" onContextMenu={handleContainerContextMenu}>
                {isLoading && <div className="text-center text-xs text-text-muted py-4">Loading...</div>}
                
                {isCreating && createParentPath === currentPath && (
                    <div className="flex items-center gap-[var(--space-2)] py-[var(--space-2)] px-[var(--space-3)] bg-white/5">
                        {createType === 'folder' ? <FolderIcon className="w-3.5 h-3.5 text-accent-dim" /> : <FileIcon className="w-3.5 h-3.5 text-text-muted" />}
                        <input
                            type="text"
                            value={newName}
                            onChange={(e) => setNewName(e.target.value)}
                            placeholder={createType === 'folder' ? 'Folder Name' : 'File Name'}
                            autoFocus
                            onBlur={handleCommitCreation}
                            className="flex-1 bg-[var(--bg-2)] border border-[var(--border)] text-[var(--text-primary)] py-0.5 px-1.5 rounded-[var(--radius-sm)] text-[12px] font-[var(--font-mono)] outline-none focus:border-[var(--accent-dim)]"
                            onKeyDown={(e) => {
                                if (e.key === 'Enter') void handleCommitCreation();
                                else if (e.key === 'Escape') cancelCreation();
                            }}
                        />
                    </div>
                )}

                {entries.map((entry) => (
                    <div key={entry.path}>
                        <div
                            className="group flex items-center gap-2.5 px-3 py-2 mx-2 rounded-lg cursor-pointer transition-all duration-200 text-text-secondary hover:text-text-primary hover:bg-accent/5 border border-transparent hover:border-accent/10"
                            onClick={() => handleEntryClick(entry)}
                            onDoubleClick={() => handleEntryDoubleClick(entry)}
                            onContextMenu={(e) => handleItemContextMenu(e, entry)}
                            title={entry.path}
                        >
                            {entry.is_dir ? (
                                <FolderIcon className="w-3.5 h-3.5 shrink-0 text-accent-dim group-hover:text-accent" />
                            ) : (
                                <FileIcon className="w-3.5 h-3.5 shrink-0 text-text-muted group-hover:text-text-secondary" />
                            )}
                            <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[11px]">{entry.name}</span>
                            {!entry.is_dir && (
                                <button
                                    type="button"
                                    className="opacity-0 group-hover:opacity-100 p-1 hover:bg-bg-4 border border-transparent hover:border-border rounded-md text-text-muted hover:text-red-400 transition-all duration-200 cursor-pointer ml-2"
                                    onClick={(e) => handleDelete(e, entry.path)}
                                    title="Delete"
                                >
                                    <TrashIcon className="w-3.5 h-3.5" />
                                </button>
                            )}
                        </div>
                        {isCreating && createParentPath === entry.path && (
                            <div className="flex items-center gap-[var(--space-2)] py-[var(--space-2)] px-[var(--space-3)] bg-white/5 pl-6">
                                {createType === 'folder' ? <FolderIcon className="w-3.5 h-3.5 text-accent-dim" /> : <FileIcon className="w-3.5 h-3.5 text-text-muted" />}
                                <input
                                    type="text"
                                    value={newName}
                                    onChange={(e) => setNewName(e.target.value)}
                                    placeholder={createType === 'folder' ? 'Folder Name' : 'File Name'}
                                    autoFocus
                                    onBlur={handleCommitCreation}
                                    className="flex-1 bg-[var(--bg-2)] border border-[var(--border)] text-[var(--text-primary)] py-0.5 px-1.5 rounded-[var(--radius-sm)] text-[12px] font-[var(--font-mono)] outline-none focus:border-[var(--accent-dim)]"
                                    onKeyDown={(e) => {
                                        if (e.key === 'Enter') void handleCommitCreation();
                                        else if (e.key === 'Escape') cancelCreation();
                                    }}
                                />
                            </div>
                        )}
                    </div>
                ))}
                {entries.length === 0 && !isLoading && currentPath && (
                    <div className="text-center text-xs text-text-muted py-8 select-none">Empty</div>
                )}
                {!currentPath && !isLoading && (
                    <div className="text-center text-xs text-text-muted py-8 flex flex-col gap-2 px-4 select-none">
                        <span>No directory open</span>
                        <button 
                            type="button" 
                            className="w-full py-2 px-3 text-[13px] bg-accent hover:bg-accent-hover text-white rounded font-medium cursor-pointer transition-colors shadow-sm mt-2" 
                            onClick={handleOpenFolder}
                        >
                            Open Folder
                        </button>
                    </div>
                )}
            </div>

            {preview && (
                <div className="absolute bottom-0 left-0 right-0 h-[45%] bg-bg-1 border-t border-border flex flex-col z-10">
                    <div className="flex items-center justify-between px-3 py-2 border-b border-border bg-bg-1/80 shrink-0">
                        <span className="text-[11px] font-mono text-text-secondary truncate">{preview.path}</span>
                        <div className="flex gap-1">
                            <button 
                                type="button" 
                                className="w-7 h-7 flex items-center justify-center rounded bg-transparent text-text-muted hover:text-text-secondary hover:bg-bg-3 active:bg-bg-4 transition-all duration-200 cursor-pointer"
                                onClick={handleSavePreview} 
                                title="Save"
                            >
                                <SaveIcon className="w-4 h-4" />
                            </button>
                            <button
                                type="button"
                                className="w-7 h-7 flex items-center justify-center rounded bg-transparent text-text-muted hover:text-text-secondary hover:bg-bg-3 active:bg-bg-4 transition-all duration-200 cursor-pointer text-sm font-semibold"
                                onClick={() => setPreview(null)}
                                title="Close"
                            >
                                ×
                            </button>
                        </div>
                    </div>
                    <textarea
                        value={preview.content}
                        onChange={(e) => setPreview({ ...preview, content: e.target.value })}
                        spellCheck={false}
                        className="flex-1 bg-bg-0 text-text-primary p-3 text-xs font-mono resize-none outline-none leading-relaxed border-none"
                    />
                </div>
            )}

            {contextMenu && (
                <div 
                    className="fixed bg-[var(--bg-1)] border border-[var(--border)] shadow-[0_4px_16px_rgba(0,0,0,0.5)] rounded-[var(--radius-md)] z-[9999] py-1 min-w-[150px] backdrop-blur-sm" 
                    style={{ 
                        left: contextMenu.x, 
                        top: contextMenu.y 
                    }}
                    onClick={(e) => e.stopPropagation()}
                >
                    <div 
                        className="px-4 py-2 text-[13px] text-[var(--text-primary)] cursor-pointer flex items-center gap-[var(--space-2)] transition-colors duration-150 hover:bg-[var(--accent-dim)] hover:text-white" 
                        onClick={() => startCreation('file', contextMenu.targetPath, contextMenu.isDir)}
                    >
                        New File
                    </div>
                    <div 
                        className="px-4 py-2 text-[13px] text-[var(--text-primary)] cursor-pointer flex items-center gap-[var(--space-2)] transition-colors duration-150 hover:bg-[var(--accent-dim)] hover:text-white" 
                        onClick={() => startCreation('folder', contextMenu.targetPath, contextMenu.isDir)}
                    >
                        New Folder
                    </div>
                    {contextMenu.targetPath && (
                        <div 
                            className="px-4 py-2 text-[13px] cursor-pointer flex items-center gap-[var(--space-2)] transition-colors duration-150 text-[#ff5555] hover:bg-[var(--error-bg)] hover:text-[#ff8888]" 
                            onClick={() => handleDeleteFromMenu(contextMenu.targetPath!)}
                        >
                            Delete
                        </div>
                    )}
                </div>
            )}
        </div>
    );
}
