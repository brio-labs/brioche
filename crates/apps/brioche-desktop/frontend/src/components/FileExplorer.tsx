import { useCallback, useState, useEffect } from "react";
import { useFileStore } from "../stores/fileStore";
import { useSettingsStore, getWorkingDir } from "../stores/settingsStore";
import { readFile } from "../ipc";
import { open } from "@tauri-apps/plugin-dialog";
import {
  FolderIcon,
  FileIcon,
  ChevronUpIcon,
  RefreshIcon,
  TrashIcon,
  SaveIcon,
} from "./Icons";

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
    writeExistingFile,
  } = useFileStore();
  const [preview, setPreview] = useState<{
    path: string;
    content: string;
  } | null>(null);
  const workspaceRoot = useSettingsStore((state) =>
    getWorkingDir(state.settings),
  );
  const [isCreating, setIsCreating] = useState(false);
  const [createType, setCreateType] = useState<"file" | "folder">("file");
  const [createParentPath, setCreateParentPath] = useState("");
  const [newName, setNewName] = useState("");

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
      if (selected && typeof selected === "string") {
        const store = useSettingsStore.getState();
        store.updateSetting("ui.working_dir", selected);
        await store.saveSettings(useSettingsStore.getState().settings);
        await loadDirectory(selected);
      }
    } catch (err) {
      console.error("Failed to open directory picker:", err);
    }
  }, [loadDirectory]);

  const handleEntryClick = useCallback(
    (entry: { is_dir: boolean; path: string }) => {
      if (entry.is_dir) {
        navigateTo(entry.path);
      }
    },
    [navigateTo],
  );

  const handleEntryDoubleClick = useCallback(
    async (entry: { is_dir: boolean; path: string }) => {
      if (entry.is_dir) return;
      try {
        const content = await readFile(entry.path);
        setPreview({ path: entry.path, content });
      } catch (err) {
        console.error("Failed to read file:", err);
      }
    },
    [],
  );

  const handleDelete = useCallback(
    async (e: React.MouseEvent, path: string) => {
      e.stopPropagation();
      if (!confirm(`Delete ${path}?`)) return;
      try {
        await deleteExistingFile(path);
      } catch (err) {
        console.error("Failed to delete:", err);
      }
    },
    [deleteExistingFile],
  );

  const handleSavePreview = useCallback(async () => {
    if (!preview) return;
    try {
      await writeExistingFile(preview.path, preview.content);
      setPreview(null);
    } catch (err) {
      console.error("Failed to save file:", err);
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

  const handleItemContextMenu = useCallback(
    (e: React.MouseEvent, entry: { path: string; is_dir: boolean }) => {
      e.preventDefault();
      e.stopPropagation();
      setContextMenu({
        x: e.clientX,
        y: e.clientY,
        targetPath: entry.path,
        isDir: entry.is_dir,
      });
    },
    [],
  );

  useEffect(() => {
    const handleOutsideClick = () => {
      setContextMenu(null);
    };
    window.addEventListener("click", handleOutsideClick);
    return () => window.removeEventListener("click", handleOutsideClick);
  }, []);

  const startCreation = useCallback(
    (type: "file" | "folder", targetPath: string | null, isDir: boolean) => {
      setIsCreating(true);
      setCreateType(type);
      setNewName("");

      if (targetPath === null) {
        setCreateParentPath(currentPath);
      } else if (isDir) {
        setCreateParentPath(targetPath);
      } else {
        const parent = targetPath.split("/").slice(0, -1).join("/") || "/";
        setCreateParentPath(parent);
      }
      setContextMenu(null);
    },
    [currentPath],
  );

  const cancelCreation = useCallback(() => {
    setIsCreating(false);
    setNewName("");
  }, []);

  const handleCommitCreation = useCallback(async () => {
    const trimmed = newName.trim();
    if (!trimmed) {
      cancelCreation();
      return;
    }
    const fullPath = `${createParentPath.replace(/\/$/, "")}/${trimmed}`;
    try {
      if (createType === "file") {
        await createNewFile(fullPath);
      } else {
        await createNewFolder(fullPath);
      }
    } catch (err) {
      console.error("Failed to create item:", err);
    } finally {
      setIsCreating(false);
      setNewName("");
    }
  }, [
    newName,
    createParentPath,
    createType,
    createNewFile,
    createNewFolder,
    cancelCreation,
  ]);

  const handleDeleteFromMenu = useCallback(
    async (path: string) => {
      setContextMenu(null);
      if (!confirm(`Delete ${path}?`)) return;
      try {
        await deleteExistingFile(path);
      } catch (err) {
        console.error("Failed to delete:", err);
      }
    },
    [deleteExistingFile],
  );

  return (
    <div className="flex flex-col h-full w-full bg-transparent text-fg-primary relative overflow-hidden">
      <div className="flex items-center justify-between px-5 py-4 border-b border-border h-13 shrink-0 bg-bg-base/30 backdrop-blur-sm">
        <h2 className="text-xs font-bold tracking-widest uppercase text-fg-muted select-none">
          Explorer
        </h2>
        <div className="flex items-center gap-2">
          <button
            type="button"
            className="btn-icon w-7 h-7"
            onClick={handleOpenFolder}
            title="Open Folder..."
          >
            <FolderIcon className="w-4 h-4" />
          </button>
          <button
            type="button"
            className="btn-icon w-7 h-7"
            onClick={() => loadDirectory(currentPath)}
            title="Refresh"
          >
            <RefreshIcon className="w-4 h-4" />
          </button>
        </div>
      </div>
      <div className="flex items-center gap-2 px-4 py-3 border-b border-border bg-bg-base/50">
        <button
          type="button"
          className="btn-icon w-6 h-6 disabled:opacity-30 disabled:cursor-not-allowed"
          onClick={navigateUp}
          disabled={currentPath === "/" || currentPath === workspaceRoot}
          title="Parent directory"
        >
          <ChevronUpIcon className="w-3.5 h-3.5" />
        </button>
        <span
          className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs text-fg-muted"
          title={currentPath}
        >
          {currentPath || "No directory"}
        </span>
      </div>

      <div
        className="flex-1 overflow-y-auto py-2 flex flex-col"
        onContextMenu={handleContainerContextMenu}
      >
        {isLoading && (
          <div className="text-center text-xs text-fg-muted py-4">
            Loading...
          </div>
        )}

        {isCreating && createParentPath === currentPath && (
          <div className="flex items-center gap-2 py-2 px-3 bg-bg-elevated/50">
            {createType === "folder" ? (
              <FolderIcon className="w-3.5 h-3.5 text-accent-dim" />
            ) : (
              <FileIcon className="w-3.5 h-3.5 text-fg-muted" />
            )}
            <input
              type="text"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder={
                createType === "folder" ? "Folder Name" : "File Name"
              }
              autoFocus
              onBlur={handleCommitCreation}
              className="input-field flex-1 py-0.5 px-1.5 text-xs rounded-sm"
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleCommitCreation();
                else if (e.key === "Escape") cancelCreation();
              }}
            />
          </div>
        )}

        {entries.map((entry) => (
          <div key={entry.path}>
            <div
              className="group flex items-center gap-2.5 px-3 py-2 mx-2 rounded-lg cursor-pointer transition-all duration-200 text-fg-secondary hover:text-fg-primary hover:bg-accent/5 border border-transparent hover:border-border-accent"
              onClick={() => handleEntryClick(entry)}
              onDoubleClick={() => handleEntryDoubleClick(entry)}
              onContextMenu={(e) => handleItemContextMenu(e, entry)}
              title={entry.path}
            >
              {entry.is_dir ? (
                <FolderIcon className="w-3.5 h-3.5 shrink-0 text-accent-dim group-hover:text-accent" />
              ) : (
                <FileIcon className="w-3.5 h-3.5 shrink-0 text-fg-muted group-hover:text-fg-secondary" />
              )}
              <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs">
                {entry.name}
              </span>
              {!entry.is_dir && (
                <button
                  type="button"
                  className="opacity-0 group-hover:opacity-100 btn-icon w-6 h-6 text-fg-muted hover:text-error-text hover:bg-error-bg border border-transparent hover:border-error-border transition-opacity duration-200 ml-2"
                  onClick={(e) => handleDelete(e, entry.path)}
                  title="Delete"
                >
                  <TrashIcon className="w-3.5 h-3.5" />
                </button>
              )}
            </div>
            {isCreating && createParentPath === entry.path && (
              <div className="flex items-center gap-2 py-2 px-3 pl-6 bg-bg-elevated/50">
                {createType === "folder" ? (
                  <FolderIcon className="w-3.5 h-3.5 text-accent-dim" />
                ) : (
                  <FileIcon className="w-3.5 h-3.5 text-fg-muted" />
                )}
                <input
                  type="text"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  placeholder={
                    createType === "folder" ? "Folder Name" : "File Name"
                  }
                  autoFocus
                  onBlur={handleCommitCreation}
                  className="input-field flex-1 py-0.5 px-1.5 text-xs rounded-sm"
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void handleCommitCreation();
                    else if (e.key === "Escape") cancelCreation();
                  }}
                />
              </div>
            )}
          </div>
        ))}
        {entries.length === 0 && !isLoading && currentPath && (
          <div className="text-center text-xs text-fg-muted py-8 select-none">
            Empty
          </div>
        )}
        {!currentPath && !isLoading && (
          <div className="flex-1 flex flex-col items-center justify-center text-center text-xs text-fg-muted px-4 py-12 select-none">
            <span>No directory open</span>
            <button
              type="button"
              className="mt-3 w-full max-w-50 py-2 px-3 text-sm bg-accent hover:bg-accent-hover active:bg-accent-dim text-white rounded font-medium cursor-pointer transition-colors shadow-sm"
              onClick={handleOpenFolder}
            >
              Open Folder
            </button>
          </div>
        )}
      </div>

      {preview && (
        <div className="absolute bottom-0 left-0 right-0 h-[45%] bg-bg-surface border-t border-border flex flex-col z-10">
          <div className="flex items-center justify-between px-4 py-3 border-b border-border bg-bg-surface/80 shrink-0">
            <span className="text-xs font-mono text-fg-secondary truncate">
              {preview.path}
            </span>
            <div className="flex gap-1">
              <button
                type="button"
                className="btn-icon w-7 h-7"
                onClick={handleSavePreview}
                title="Save"
              >
                <SaveIcon className="w-4 h-4" />
              </button>
              <button
                type="button"
                className="btn-icon w-7 h-7 text-sm font-semibold"
                onClick={() => setPreview(null)}
                title="Close"
              >
                ×
              </button>
            </div>
          </div>
          <textarea
            value={preview.content}
            onChange={(e) =>
              setPreview({ ...preview, content: e.target.value })
            }
            spellCheck={false}
            className="textarea-field flex-1 resize-none border-none rounded-none leading-relaxed p-4"
          />
        </div>
      )}

      {contextMenu && (
        <div
          className="fixed bg-bg-surface border border-border shadow-lg rounded-md z-9999 py-1 min-w-40 backdrop-blur-sm"
          style={{
            left: contextMenu.x,
            top: contextMenu.y,
          }}
          onClick={(e) => e.stopPropagation()}
        >
          <div
            className="px-4 py-2 text-sm text-fg-primary cursor-pointer flex items-center gap-2 transition-colors duration-150 hover:bg-accent-dim hover:text-white"
            onClick={() =>
              startCreation("file", contextMenu.targetPath, contextMenu.isDir)
            }
          >
            New File
          </div>
          <div
            className="px-4 py-2 text-sm text-fg-primary cursor-pointer flex items-center gap-2 transition-colors duration-150 hover:bg-accent-dim hover:text-white"
            onClick={() =>
              startCreation("folder", contextMenu.targetPath, contextMenu.isDir)
            }
          >
            New Folder
          </div>
          {contextMenu.targetPath && (
            <div
              className="px-4 py-2 text-sm cursor-pointer flex items-center gap-2 transition-colors duration-150 text-error-text hover:bg-error-bg hover:text-error-text"
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
