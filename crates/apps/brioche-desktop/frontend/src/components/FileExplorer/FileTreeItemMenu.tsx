import {
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuSeparator,
} from "../ui/context-menu";
import { useFileExplorerContext } from "./FileExplorerContext";
import type { TreeEntry } from "../../hooks/fileExplorer";

interface FileTreeItemMenuProps {
  entry: TreeEntry;
}

export default function FileTreeItemMenu({ entry }: FileTreeItemMenuProps) {
  const {
    clipboard,
    onNewFile,
    onNewFolder,
    onRename,
    onCopy,
    onCut,
    onPaste,
    onDelete,
  } = useFileExplorerContext();

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
