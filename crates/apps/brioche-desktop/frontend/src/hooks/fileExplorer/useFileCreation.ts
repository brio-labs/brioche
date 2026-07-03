import { useCallback, useState, type Dispatch, type SetStateAction } from "react";
import type { TreeEntry } from "./useFileTree";

interface UseFileCreationOptions {
	createNewFile: (path: string) => Promise<void>;
	createNewFolder: (path: string) => Promise<void>;
	expandedPaths: Set<string>;
	setExpandedPaths: Dispatch<SetStateAction<Set<string>>>;
	setChildrenMap: Dispatch<SetStateAction<Map<string, TreeEntry[]>>>;
	currentPath: string;
	workspaceRoot: string | undefined;
	readDirectory: (path: string) => Promise<TreeEntry[]>;
}

export function useFileCreation({
	createNewFile,
	createNewFolder,
	expandedPaths,
	setExpandedPaths,
	setChildrenMap,
	currentPath,
	workspaceRoot,
	readDirectory,
}: UseFileCreationOptions) {
	const [isCreating, setIsCreating] = useState(false);
	const [createType, setCreateType] = useState<"file" | "folder">("file");
	const [createParentPath, setCreateParentPath] = useState("");
	const [newName, setNewName] = useState("");

	const startCreation = useCallback(
		(type: "file" | "folder", targetPath: string | null, isDir: boolean) => {
			setIsCreating(true);
			setCreateType(type);
			setNewName("");
			if (targetPath === null) {
				setCreateParentPath(currentPath || workspaceRoot || "");
			} else if (isDir) {
				setCreateParentPath(targetPath);
			} else {
				const parent = targetPath.split("/").slice(0, -1).join("/") || "/";
				setCreateParentPath(parent);
			}
		},
		[currentPath, workspaceRoot],
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
			// Refresh the parent directory in the tree.
			if (expandedPaths.has(createParentPath)) {
				const entries = await readDirectory(createParentPath);
				setChildrenMap((prev) => new Map(prev).set(createParentPath, entries));
			} else {
				setExpandedPaths((prev) => new Set(prev).add(createParentPath));
				const entries = await readDirectory(createParentPath);
				setChildrenMap((prev) => new Map(prev).set(createParentPath, entries));
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
		expandedPaths,
		setChildrenMap,
		setExpandedPaths,
		readDirectory,
	]);

	return {
		isCreating,
		createType,
		createParentPath,
		newName,
		setNewName,
		startCreation,
		cancelCreation,
		handleCommitCreation,
	};
}
