import { createContext, useContext, type ReactNode } from "react";
import type { TreeEntry } from "../../hooks/fileExplorer";

export interface FileExplorerContextValue {
	expandedPaths: Set<string>;
	childrenMap: Map<string, TreeEntry[]>;
	clipboard: { path: string; operation: "copy" | "cut" } | null;
	renamingPath: string | null;
	renameValue: string;
	creatingFor: string | null;
	createType: "file" | "folder";
	newName: string;
	handleToggle: (path: string) => void;
	handleLoadChildren: (path: string) => void;
	handlePreview: (path: string) => void;
	handleDelete: (path: string) => void;
	onNewFile: (targetPath: string, isDir: boolean) => void;
	onNewFolder: (targetPath: string, isDir: boolean) => void;
	onRename: (path: string, name: string) => void;
	onCopy: (path: string) => void;
	onCut: (path: string) => void;
	onPaste: (targetPath: string, isDir: boolean) => void;
	onRenameValueChange: (value: string) => void;
	onCommitRename: () => void;
	onCancelRename: () => void;
	onNewNameChange: (value: string) => void;
	onCommitCreation: () => void;
	onCancelCreation: () => void;
}

const FileExplorerContext = createContext<FileExplorerContextValue | null>(
	null,
);

export interface FileExplorerProviderProps {
	value: FileExplorerContextValue;
	children: ReactNode;
}

export function FileExplorerProvider({
	value,
	children,
}: FileExplorerProviderProps) {
	return (
		<FileExplorerContext.Provider value={value}>
			{children}
		</FileExplorerContext.Provider>
	);
}

export function useFileExplorerContext(): FileExplorerContextValue {
	const context = useContext(FileExplorerContext);
	if (context === null) {
		throw new Error(
			"useFileExplorerContext must be used within a FileExplorerProvider",
		);
	}
	return context;
}
