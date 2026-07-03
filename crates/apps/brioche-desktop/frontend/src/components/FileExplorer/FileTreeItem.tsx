import { useCallback } from "react";
import { FolderIcon, FileIcon } from "../Icons";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
} from "../ui/context-menu";
import { cn } from "../ui/lib";
import FileTreeItemMenu from "./FileTreeItemMenu";
import type { TreeEntry } from "../../hooks/fileExplorer";

interface FileTreeItemProps {
  entry: TreeEntry;
  depth: number;
  workspaceRoot: string;
  expandedPaths: Set<string>;
  childrenMap: Map<string, TreeEntry[]>;
  clipboard: { path: string; operation: "copy" | "cut" } | null;
  renamingPath: string | null;
  renameValue: string;
  onToggle: (path: string) => void;
  onLoadChildren: (path: string) => void;
  onPreview: (path: string) => void;
  onDelete: (path: string) => void;
  onNewFile: (targetPath: string, isDir: boolean) => void;
  onNewFolder: (targetPath: string, isDir: boolean) => void;
  onRename: (path: string, name: string) => void;
  onCopy: (path: string) => void;
  onCut: (path: string) => void;
  onPaste: (targetPath: string, isDir: boolean) => void;
  onRenameValueChange: (value: string) => void;
  onCommitRename: () => void;
  onCancelRename: () => void;
  creatingFor: string | null;
  createType: "file" | "folder";
  newName: string;
  onNewNameChange: (value: string) => void;
  onCommitCreation: () => void;
  onCancelCreation: () => void;
}

export default function FileTreeItem({
  entry,
  depth,
  workspaceRoot,
  expandedPaths,
  childrenMap,
  clipboard,
  renamingPath,
  renameValue,
  onToggle,
  onLoadChildren,
  onPreview,
  onDelete,
  onNewFile,
  onNewFolder,
  onRename,
  onCopy,
  onCut,
  onPaste,
  onRenameValueChange,
  onCommitRename,
  onCancelRename,
  creatingFor,
  createType,
  newName,
  onNewNameChange,
  onCommitCreation,
  onCancelCreation,
}: FileTreeItemProps) {
  const isExpanded = expandedPaths.has(entry.path);
  const children = childrenMap.get(entry.path);
  const isCreatingHere = creatingFor === entry.path;
  const isRenamingHere = renamingPath === entry.path;
  const indent = depth * 0.75;

  const handleClick = useCallback(() => {
    if (entry.is_dir) {
      if (!isExpanded && !childrenMap.has(entry.path)) {
        onLoadChildren(entry.path);
      }
      onToggle(entry.path);
    }
  }, [entry, isExpanded, childrenMap, onLoadChildren, onToggle]);

  const handleDoubleClick = useCallback(() => {
    if (!entry.is_dir) {
      onPreview(entry.path);
    }
  }, [entry, onPreview]);

  return (
    <div>
      <ContextMenu>
        <ContextMenuTrigger
          asChild
          onContextMenu={(e) => e.stopPropagation()}
        >
          <div
            className={cn(
              "group flex w-full cursor-pointer items-center gap-2 border border-transparent px-3 py-2 text-fg-secondary transition-all duration-200 hover:border-border-accent hover:bg-accent/5 hover:text-fg-primary",
            )}
            style={{ paddingLeft: `${0.75 + indent}rem` }}
            onClick={handleClick}
            onDoubleClick={handleDoubleClick}
            title={entry.path}
          >
            {entry.is_dir ? (
              <FolderIcon className="h-3.5 w-3.5 shrink-0 text-accent-dim group-hover:text-accent" />
            ) : (
              <FileIcon className="h-3.5 w-3.5 shrink-0 text-fg-muted group-hover:text-fg-secondary" />
            )}
            {isRenamingHere ? (
              <input
                type="text"
                value={renameValue}
                onChange={(e) => onRenameValueChange(e.target.value)}
                autoFocus
                onBlur={onCommitRename}
                onClick={(e) => e.stopPropagation()}
                className="input-field flex-1 rounded-sm px-1.5 py-0.5 text-xs"
                onKeyDown={(e) => {
                  if (e.key === "Enter") onCommitRename();
                  else if (e.key === "Escape") onCancelRename();
                }}
              />
            ) : (
              <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs">
                {entry.name}
              </span>
            )}
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent>
          <FileTreeItemMenu
            entry={entry}
            clipboard={clipboard}
            onNewFile={onNewFile}
            onNewFolder={onNewFolder}
            onRename={onRename}
            onCopy={onCopy}
            onCut={onCut}
            onPaste={onPaste}
            onDelete={onDelete}
          />
        </ContextMenuContent>
      </ContextMenu>

      {isCreatingHere && (
        <div
          className="flex items-center gap-2 bg-bg-elevated/50 py-2 pr-3"
          style={{ paddingLeft: `${1.5 + indent}rem` }}
        >
          {createType === "folder" ? (
            <FolderIcon className="h-3.5 w-3.5 text-accent-dim" />
          ) : (
            <FileIcon className="h-3.5 w-3.5 text-fg-muted" />
          )}
          <input
            type="text"
            value={newName}
            onChange={(e) => onNewNameChange(e.target.value)}
            placeholder={createType === "folder" ? "Folder Name" : "File Name"}
            autoFocus
            onBlur={onCommitCreation}
            className="input-field flex-1 rounded-sm px-1.5 py-0.5 text-xs"
            onKeyDown={(e) => {
              if (e.key === "Enter") onCommitCreation();
              else if (e.key === "Escape") onCancelCreation();
            }}
          />
        </div>
      )}

      {isExpanded && entry.is_dir && (
        <div>
          {entry.isLoading ? (
            <div className="py-2 text-xs text-fg-muted">Loading...</div>
          ) : children && children.length > 0 ? (
            children.map((child) => (
              <FileTreeItem
                key={child.path}
                entry={child}
                depth={depth + 1}
                workspaceRoot={workspaceRoot}
                expandedPaths={expandedPaths}
                childrenMap={childrenMap}
                clipboard={clipboard}
                renamingPath={renamingPath}
                renameValue={renameValue}
                onToggle={onToggle}
                onLoadChildren={onLoadChildren}
                onPreview={onPreview}
                onDelete={onDelete}
                onNewFile={onNewFile}
                onNewFolder={onNewFolder}
                onRename={onRename}
                onCopy={onCopy}
                onCut={onCut}
                onPaste={onPaste}
                onRenameValueChange={onRenameValueChange}
                onCommitRename={onCommitRename}
                onCancelRename={onCancelRename}
                creatingFor={creatingFor}
                createType={createType}
                newName={newName}
                onNewNameChange={onNewNameChange}
                onCommitCreation={onCommitCreation}
                onCancelCreation={onCancelCreation}
              />
            ))
          ) : (
            <div
              className="py-2 text-xs text-fg-muted"
              style={{ paddingLeft: `${1.5 + indent}rem` }}
            >
              Empty
            </div>
          )}
        </div>
      )}
    </div>
  );
}
