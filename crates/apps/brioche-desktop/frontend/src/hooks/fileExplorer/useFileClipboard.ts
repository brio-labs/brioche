import { useCallback, useState, type Dispatch, type SetStateAction } from "react";
import type { TreeEntry } from "./useFileTree";

interface UseFileClipboardOptions {
	renamePath: (source: string, destination: string) => Promise<void>;
	copyPath: (source: string, destination: string) => Promise<void>;
	refreshDirectory: (path: string) => Promise<void>;
	expandedPaths: Set<string>;
	setExpandedPaths: Dispatch<SetStateAction<Set<string>>>;
	setChildrenMap: Dispatch<SetStateAction<Map<string, TreeEntry[]>>>;
}

export function useFileClipboard({
	renamePath,
	copyPath,
	refreshDirectory,
	expandedPaths,
	setExpandedPaths,
	setChildrenMap,
}: UseFileClipboardOptions) {
	const [clipboard, setClipboard] = useState<{
		path: string;
		operation: "copy" | "cut";
	} | null>(null);

	const handleCopy = useCallback((path: string) => {
		setClipboard({ path, operation: "copy" });
	}, []);

	const handleCut = useCallback((path: string) => {
		setClipboard({ path, operation: "cut" });
	}, []);

	const handlePaste = useCallback(
		async (targetPath: string, isDir: boolean) => {
			if (!clipboard) return;
			const targetDir = isDir
				? targetPath
				: targetPath.split("/").slice(0, -1).join("/") || "/";
			const baseName = clipboard.path.split("/").pop() || "";
			const destination = `${targetDir.replace(/\/$/, "")}/${baseName}`;
			try {
				if (clipboard.operation === "cut") {
					await renamePath(clipboard.path, destination);
				} else {
					await copyPath(clipboard.path, destination);
				}
				await refreshDirectory(targetDir);
				const sourceParent =
					clipboard.path.split("/").slice(0, -1).join("/") || "/";
				if (sourceParent !== targetDir) {
					await refreshDirectory(sourceParent);
				}
				if (expandedPaths.has(clipboard.path)) {
					if (clipboard.operation === "cut") {
						setExpandedPaths((prev) => {
							const next = new Set(prev);
							next.delete(clipboard.path);
							return next;
						});
						setChildrenMap((prev) => {
							const next = new Map(prev);
							next.delete(clipboard.path);
							return next;
						});
						setExpandedPaths((prev) => new Set(prev).add(destination));
					} else {
						await refreshDirectory(clipboard.path);
					}
				}
			} catch (err) {
				console.error("Failed to paste:", err);
			} finally {
				setClipboard(null);
			}
		},
		[
			clipboard,
			expandedPaths,
			renamePath,
			copyPath,
			refreshDirectory,
			setExpandedPaths,
			setChildrenMap,
		],
	);

	return { clipboard, handleCopy, handleCut, handlePaste };
}
