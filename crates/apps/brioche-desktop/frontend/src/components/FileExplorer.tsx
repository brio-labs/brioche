import { useCallback } from 'react';
import { useFileStore } from '../stores/fileStore';
import { FolderIcon, FileIcon, ChevronUpIcon } from './Icons';

export default function FileExplorer() {
    const { currentPath, entries, isLoading, loadDirectory, navigateUp, navigateTo } = useFileStore();

    const handleEntryClick = useCallback(
        (entry: { is_dir: boolean; path: string }) => {
            if (entry.is_dir) {
                navigateTo(entry.path);
            }
        },
        [navigateTo]
    );

    return (
        <div className="sidebar sidebar-right file-sidebar">
            <div className="sidebar-header">
                <h2>Explorer</h2>
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
            <div className="file-list">
                {isLoading && <div className="loading">Loading...</div>}
                {entries.map((entry) => (
                    <div
                        key={entry.path}
                        className={`file-item ${entry.is_dir ? 'directory' : 'file'}`}
                        onClick={() => handleEntryClick(entry)}
                    >
                        {entry.is_dir ? <FolderIcon /> : <FileIcon />}
                        <span className="file-name">{entry.name}</span>
                    </div>
                ))}
                {entries.length === 0 && !isLoading && currentPath && (
                    <div className="empty-sidebar">Empty</div>
                )}
                {!currentPath && !isLoading && (
                    <div className="empty-sidebar">Set working directory in settings</div>
                )}
            </div>
        </div>
    );
}
