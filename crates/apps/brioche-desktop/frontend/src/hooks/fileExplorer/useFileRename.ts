import { useCallback, useState } from "react";

interface UseFileRenameOptions {
	renamePath: (source: string, destination: string) => Promise<void>;
	refreshDirectory: (path: string) => Promise<void>;
	expandedPaths: Set<string>;
}

export function useFileRename({
	renamePath,
	refreshDirectory,
	expandedPaths,
}: UseFileRenameOptions) {
	const [renamingPath, setRenamingPath] = useState<string | null>(null);
	const [renameValue, setRenameValue] = useState("");

	const handleStartRename = useCallback((path: string, name: string) => {
		setRenamingPath(path);
		setRenameValue(name);
	}, []);

	const handleRenameValueChange = useCallback((value: string) => {
		setRenameValue(value);
	}, []);

	const handleCommitRename = useCallback(async () => {
		if (!renamingPath) return;
		const trimmed = renameValue.trim();
		if (!trimmed) {
			setRenamingPath(null);
			setRenameValue("");
			return;
		}
		const parent = renamingPath.split("/").slice(0, -1).join("/") || "/";
		const newPath = `${parent.replace(/\/$/, "")}/${trimmed}`;
		try {
			await renamePath(renamingPath, newPath);
			if (expandedPaths.has(parent)) {
				await refreshDirectory(parent);
			}
		} catch (err) {
			console.error("Failed to rename:", err);
		} finally {
			setRenamingPath(null);
			setRenameValue("");
		}
	}, [renamingPath, renameValue, renamePath, expandedPaths, refreshDirectory]);

	const handleCancelRename = useCallback(() => {
		setRenamingPath(null);
		setRenameValue("");
	}, []);

	return {
		renamingPath,
		renameValue,
		handleStartRename,
		handleRenameValueChange,
		handleCommitRename,
		handleCancelRename,
	};
}
