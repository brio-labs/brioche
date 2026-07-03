import {
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuSeparator,
} from "../ui/context-menu";
import type { TreeEntry } from "../../hooks/fileExplorer";

interface FileTreeItemMenuProps {
  entry: TreeEntry;
  clipboard: { path: string; operation: "copy" | "cut" } | null;
  onNewFile: (targetPath: string, isDir: boolean) => void;
  onNewFolder: (targetPath: string, isDir: boolean) => void;
  onRename: (path: string, name: string) => void;
  onCopy: (path: string) => void;
  onCut: (path: string) => void;
  onPaste: (targetPath: string, isDir: boolean) => void;
  onDelete: (path: string) => void;
}

export default function FileTreeItemMenu({
  entry,
  clipboard,
  onNewFile,
  onNewFolder,
  onRename,
  onCopy,
  onCut,
  onPaste,
  onDelete,
}: FileTreeItemMenuProps) {
  if (entry.is_dir) {
    return (
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
        <ContextMenuGroup>
          <ContextMenuItem onClick={() => onRename(entry.path, entry.name)}>
            Rename
          </ContextMenuItem>
          <ContextMenuItem onClick={() => onCopy(entry.path)}>Copy</ContextMenuItem>
          <ContextMenuItem onClick={() => onCut(entry.path)}>Cut</ContextMenuItem>
          <ContextMenuItem
            onClick={() => onPaste(entry.path, entry.is_dir)}
            disabled={!clipboard}
          >
            Paste
          </ContextMenuItem>
        </ContextMenuGroup>
        <ContextMenuSeparator />
        <ContextMenuGroup>
          <ContextMenuItem onClick={() => onDelete(entry.path)} destructive>
            Delete
          </ContextMenuItem>
        </ContextMenuGroup>
      </>
    );
  }

  return (
    <>
      <ContextMenuGroup>
        <ContextMenuItem onClick={() => onRename(entry.path, entry.name)}>
          Rename
        </ContextMenuItem>
        <ContextMenuItem onClick={() => onCopy(entry.path)}>Copy</ContextMenuItem>
        <ContextMenuItem onClick={() => onCut(entry.path)}>Cut</ContextMenuItem>
        <ContextMenuItem
          onClick={() => onPaste(entry.path, entry.is_dir)}
          disabled={!clipboard}
        >
          Paste
        </ContextMenuItem>
      </ContextMenuGroup>
      <ContextMenuSeparator />
      <ContextMenuGroup>
        <ContextMenuItem onClick={() => onDelete(entry.path)} destructive>
          Delete
        </ContextMenuItem>
      </ContextMenuGroup>
    </>
  );
}
