import { useMemo } from "react";
import { FolderOpen, Save } from "lucide-react";
import { useFileExplorer } from "../../hooks/fileExplorer";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuGroup,
} from "../ui/ContextMenu";
import { SectionHeader, SectionHeaderTitle, EmptyState } from "../ui";
import { isTauri } from "../../ipc";
import FileTreeItem from "./FileTreeItem";
import { FileExplorerProvider, type FileExplorerContextValue } from "./FileExplorerContext";

export default function FileExplorer() {
  const {
    workspaceRoot,
    currentPath,
    entries,
    storeLoading,
    notice,
    expandedPaths,
    childrenMap,
    loadingPaths,
    clipboard,
    renamingPath,
    renameValue,
    preview,
    creatingFor,
    createType,
    newName,
    handleOpenFolder,
    handleRefresh,
    handleToggle,
    handleLoadChildren,
    handlePreview,
    handleDelete,
    startCreation,
    handleCopy,
    handleCut,
    handlePaste,
    handleStartRename,
    handleRenameValueChange,
    handleCommitRename,
    handleCancelRename,
    setPreview,
    setNewName,
    handleCommitCreation,
    cancelCreation,
    handleSavePreview,
  } = useFileExplorer();

  const rootEntries = useMemo(() => {
    if (!workspaceRoot) return [];
    return entries.map((entry) => ({
      ...entry,
      isLoading: loadingPaths.has(entry.path),
      children: childrenMap.get(entry.path),
    }));
  }, [entries, childrenMap, loadingPaths, workspaceRoot]);

  const folderName = useMemo(() => {
    if (!workspaceRoot) return "No folder open";
    return workspaceRoot.split(/[/\\]/).pop() || "";
  }, [workspaceRoot]);

  const contextValue = useMemo<FileExplorerContextValue>(
    () => ({
      expandedPaths,
      childrenMap,
      clipboard,
      renamingPath,
      renameValue,
      creatingFor,
      createType,
      newName,
      handleToggle,
      handleLoadChildren,
      handlePreview,
      handleDelete,
      onNewFile: (targetPath, isDir) => startCreation("file", targetPath, isDir),
      onNewFolder: (targetPath, isDir) =>
        startCreation("folder", targetPath, isDir),
      onRename: handleStartRename,
      onCopy: handleCopy,
      onCut: handleCut,
      onPaste: handlePaste,
      onRenameValueChange: handleRenameValueChange,
      onCommitRename: handleCommitRename,
      onCancelRename: handleCancelRename,
      onNewNameChange: setNewName,
      onCommitCreation: handleCommitCreation,
      onCancelCreation: cancelCreation,
    }),
    [
      expandedPaths,
      childrenMap,
      clipboard,
      renamingPath,
      renameValue,
      creatingFor,
      createType,
      newName,
      handleToggle,
      handleLoadChildren,
      handlePreview,
      handleDelete,
      startCreation,
      handleStartRename,
      handleCopy,
      handleCut,
      handlePaste,
      handleRenameValueChange,
      handleCommitRename,
      handleCancelRename,
      setNewName,
      handleCommitCreation,
      cancelCreation,
    ],
  );

  return (
    <div className="relative flex h-full w-full flex-col overflow-hidden bg-transparent text-fg-primary">
      <SectionHeader className="bg-bg-surface/10 backdrop-blur-md">
        <SectionHeaderTitle>{folderName}</SectionHeaderTitle>
        <div className="flex items-center gap-2">
          <button
            type="button"
            className="btn-icon h-7 w-7"
            onClick={handleOpenFolder}
            title="Open Folder..."
          >
            <FolderOpen className="h-4 w-4" />
          </button>
        </div>
      </SectionHeader>
      {!isTauri() && (
        <div className="notice-error shrink-0 px-5 py-2 text-xs">
          Explorer preview mode: folder operations require the Tauri desktop
          app.
        </div>
      )}
      {notice && (
        <div className="notice-error shrink-0 px-5 py-2 text-xs">{notice}</div>
      )}

      <FileExplorerProvider value={contextValue}>
        <ContextMenu>
          <ContextMenuTrigger asChild>
            <div className="flex flex-1 flex-col overflow-y-auto py-2">
              {storeLoading && (
                <div className="py-4 text-center text-xs text-fg-muted">
                  Loading...
                </div>
              )}

              {rootEntries.map((entry) => (
                <FileTreeItem
                  key={entry.path}
                  entry={entry}
                  depth={0}
                />
              ))}
              {rootEntries.length === 0 && !storeLoading && workspaceRoot && (
                <EmptyState
                  title="Folder is empty"
                  description="Create a file to start coding."
                />
              )}
              {!workspaceRoot && !storeLoading && (
                <EmptyState
                  icon={FolderOpen}
                  title="No directory open"
                  description="Open a workspace folder to start viewing and editing files."
                  action={
                    <button
                      type="button"
                      className="mt-3 w-full max-w-50 cursor-pointer rounded-md border border-fg-primary/18 bg-fg-primary/14 py-2 px-3 text-sm font-medium text-fg-primary shadow-sm transition-colors hover:bg-fg-primary/22 active:bg-fg-primary/28"
                      onClick={handleOpenFolder}
                    >
                      Open Folder
                    </button>
                  }
                />
              )}
            </div>
          </ContextMenuTrigger>
          <ContextMenuContent>
            <ContextMenuGroup>
              <ContextMenuItem
                onClick={() => startCreation("file", null, true)}
              >
                New File
              </ContextMenuItem>
              <ContextMenuItem
                onClick={() => startCreation("folder", null, true)}
              >
                New Folder
              </ContextMenuItem>
            </ContextMenuGroup>
          </ContextMenuContent>
        </ContextMenu>
      </FileExplorerProvider>
 
      {preview && (
        <div className="absolute bottom-0 left-0 right-0 z-10 flex h-[45%] flex-col border-t border-border/60 bg-bg-surface/25 backdrop-blur-lg">
          <div className="flex shrink-0 items-center justify-between border-b border-border/60 bg-bg-surface/15 px-4 py-3">
            <span className="truncate font-mono text-xs text-fg-secondary">
              {preview.path}
            </span>
            <div className="flex gap-1">
              <button
                type="button"
                className="btn-icon h-7 w-7"
                onClick={handleSavePreview}
                title="Save"
              >
                <Save className="h-4 w-4" />
              </button>
              <button
                type="button"
                className="btn-icon h-7 w-7 text-sm font-semibold"
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
            className="textarea-field flex-1 resize-none rounded-none border-none p-4 leading-relaxed"
          />
        </div>
      )}
    </div>
  );
}
