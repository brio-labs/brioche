import { useCallback, useState } from 'react';
import { useFileStore } from '../stores/fileStore';
import { readFile, writeFile, deleteFile, createFile } from '../ipc';
import { FolderIcon, FileIcon, ChevronUpIcon, RefreshIcon, PlusIcon, TrashIcon, SaveIcon } from './Icons';

export default function FileExplorer() {
    const { currentPath, entries, isLoading, loadDirectory, navigateUp, navigateTo } = useFileStore();
    const [preview, setPreview] = useState<{ path: string; content: string } | null>(null);
    const [newName, setNewName] = useState('');
    const [showNewFile, setShowNewFile] = useState(false);

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
                await deleteFile(path);
                await loadDirectory(currentPath);
            } catch (err) {
                console.error('Failed to delete:', err);
            }
        },
        [currentPath, loadDirectory]
    );

    const handleCreateFile = useCallback(async () => {
        if (!newName.trim()) return;
        const path = `${currentPath.replace(/\/$/, '')}/${newName.trim()}`;
        try {
            await createFile(path);
            setNewName('');
            setShowNewFile(false);
            await loadDirectory(currentPath);
        } catch (err) {
            console.error('Failed to create file:', err);
        }
    }, [currentPath, loadDirectory, newName]);

    const handleSavePreview = useCallback(async () => {
        if (!preview) return;
        try {
            await writeFile(preview.path, preview.content);
            setPreview(null);
        } catch (err) {
            console.error('Failed to save file:', err);
        }
    }, [preview]);

    return (
        <div className="sidebar sidebar-right file-sidebar">
            <div className="sidebar-header">
                <h2>Explorer</h2>
                <div className="file-actions">
                    <button
                        type="button"
                        className="icon-btn"
                        onClick={() => setShowNewFile((v) => !v)}
                        title="New file"
                    >
                        <PlusIcon />
                    </button>
                    <button
                        type="button"
                        className="icon-btn"
                        onClick={() => loadDirectory(currentPath)}
                        title="Refresh"
                    >
                        <RefreshIcon />
                    </button>
                </div>
            </div>
            <div className="file-path-bar">
                <button
                    type="button"
                    className="path-up-btn"
                    onClick={navigateUp}
                    disabled={currentPath === '/'}
                    title="Parent directory"
                >
                    <ChevronUpIcon />
                </button>
                <span className="path-text" title={currentPath}>
                    {currentPath || 'No directory'}
                </span>
            </div>
            {showNewFile && (
                <div className="file-new-bar">
                    <input
                        type="text"
                        value={newName}
                        onChange={(e) => setNewName(e.target.value)}
                        placeholder="filename.ext"
                        onKeyDown={(e) => {
                            if (e.key === 'Enter') void handleCreateFile();
                        }}
                    />
                    <button type="button" className="icon-btn" onClick={handleCreateFile}>
                        <SaveIcon />
                    </button>
                </div>
            )}
            <div className="file-list">
                {isLoading && <div className="loading">Loading...</div>}
                {entries.map((entry) => (
                    <div
                        key={entry.path}
                        className={`file-item ${entry.is_dir ? 'directory' : 'file'}`}
                        onClick={() => handleEntryClick(entry)}
                        onDoubleClick={() => handleEntryDoubleClick(entry)}
                        title={entry.path}
                    >
                        {entry.is_dir ? <FolderIcon /> : <FileIcon />}
                        <span className="file-name">{entry.name}</span>
                        {!entry.is_dir && (
                            <button
                                type="button"
                                className="file-delete"
                                onClick={(e) => handleDelete(e, entry.path)}
                                title="Delete"
                            >
                                <TrashIcon />
                            </button>
                        )}
                    </div>
                ))}
                {entries.length === 0 && !isLoading && currentPath && (
                    <div className="empty-sidebar">Empty</div>
                )}
                {!currentPath && !isLoading && (
                    <div className="empty-sidebar">Set working directory in settings</div>
                )}
            </div>

            {preview && (
                <div className="file-preview">
                    <div className="file-preview-header">
                        <span className="file-preview-name">{preview.path}</span>
                        <div className="file-preview-actions">
                            <button type="button" className="icon-btn" onClick={handleSavePreview} title="Save">
                                <SaveIcon />
                            </button>
                            <button
                                type="button"
                                className="icon-btn"
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
                    />
                </div>
            )}
        </div>
    );
}
