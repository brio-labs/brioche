import { useCallback } from "react";
import { Folder, FolderOpen, File } from "lucide-react";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuSeparator,
} from "../ui/ContextMenu";
import { cn } from "../ui/lib";
import { useFileExplorerContext } from "./FileExplorerContext";
import type { TreeEntry } from "../../hooks/fileExplorer";

interface FileTreeItemProps {
  entry: TreeEntry;
  depth: number;
}

export default function FileTreeItem({ entry, depth }: FileTreeItemProps) {
  const {
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
    onNewFile,
    onNewFolder,
    onRename,
    onCopy,
    onCut,
    onPaste,
    onRenameValueChange,
    onCommitRename,
    onCancelRename,
    onNewNameChange,
    onCommitCreation,
    onCancelCreation,
  } = useFileExplorerContext();

  const isExpanded = expandedPaths.has(entry.path);
  const children = childrenMap.get(entry.path);
  const isCreatingHere = creatingFor === entry.path;
  const isRenamingHere = renamingPath === entry.path;
  const indent = depth * 0.75;

  const handleClick = useCallback(() => {
    if (entry.is_dir) {
      if (!isExpanded && !childrenMap.has(entry.path)) {
        handleLoadChildren(entry.path);
      }
      handleToggle(entry.path);
    }
  }, [entry, isExpanded, childrenMap, handleLoadChildren, handleToggle]);

  const handleDoubleClick = useCallback(() => {
    if (!entry.is_dir) {
      handlePreview(entry.path);
    }
  }, [entry, handlePreview]);

  return (
    <div>
      <ContextMenu>
        <ContextMenuTrigger
          asChild
          onContextMenu={(e) => e.stopPropagation()}
        >
          <div
            className={cn(
              "group flex w-full cursor-pointer items-center gap-2 border border-transparent px-3 py-2 text-fg-secondary transition-all duration-200 hover:bg-bg-elevated hover:text-fg-primary",
            )}
            style={{ paddingLeft: `${0.75 + indent}rem` }}
            onClick={handleClick}
            onDoubleClick={handleDoubleClick}
            title={entry.path}
          >
            {entry.is_dir ? (
              isExpanded ? (
                <FolderOpen className="h-3.5 w-3.5 shrink-0 text-accent-dim group-hover:text-accent" />
              ) : (
                <Folder className="h-3.5 w-3.5 shrink-0 fill-current text-accent-dim group-hover:text-accent" />
              )
            ) : (
              <File className="h-3.5 w-3.5 shrink-0 text-fg-muted group-hover:text-fg-secondary" />
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
          {entry.is_dir && (
            <>
              <ContextMenuGroup>
                <ContextMenuItem onClick={() => onNewFile(entry.path, entry.is_dir)}>
                  New File
                </ContextMenuItem>
                <ContextMenuItem
                  onClick={() => onNewFolder(entry.path, entry.is_dir)}
                >
                  New Folder
                </ContextMenuItem>
              </ContextMenuGroup>
              <ContextMenuSeparator />
            </>
          )}
          <ContextMenuGroup>
            <ContextMenuItem onClick={() => onRename(entry.path, entry.name)}>
              Rename
            </ContextMenuItem>
            <ContextMenuItem onClick={() => onCopy(entry.path)}>
              Copy
            </ContextMenuItem>
            <ContextMenuItem onClick={() => onCut(entry.path)}>
              Cut
            </ContextMenuItem>
            <ContextMenuItem
              onClick={() => onPaste(entry.path, entry.is_dir)}
              disabled={!clipboard}
            >
              Paste
            </ContextMenuItem>
          </ContextMenuGroup>
          <ContextMenuSeparator />
          <ContextMenuGroup>
            <ContextMenuItem onClick={() => handleDelete(entry.path)} destructive>
              Delete
            </ContextMenuItem>
          </ContextMenuGroup>
        </ContextMenuContent>
      </ContextMenu>

      {isCreatingHere && (
        <div
          className="flex items-center gap-2 bg-bg-elevated py-2 pr-3"
          style={{ paddingLeft: `${1.5 + indent}rem` }}
        >
          {createType === "folder" ? (
            <Folder className="h-3.5 w-3.5 fill-current text-accent-dim" />
          ) : (
            <File className="h-3.5 w-3.5 text-fg-muted" />
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
