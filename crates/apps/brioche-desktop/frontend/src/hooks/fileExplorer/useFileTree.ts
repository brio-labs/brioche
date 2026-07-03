import { useCallback, useEffect, useState } from "react";
import { readDirectory, isTauri } from "../../ipc";
import type { DirEntry } from "../../ipc";

export interface TreeEntry extends DirEntry {
	children?: TreeEntry[];
	isLoading?: boolean;
}

interface UseFileTreeOptions {
	workspaceRoot: string | undefined;
	loadDirectory: (path: string) => Promise<void>;
	setNotice: (notice: string | null) => void;
}

export function useFileTree({
	workspaceRoot,
	loadDirectory,
	setNotice,
}: UseFileTreeOptions) {
	const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
	const [childrenMap, setChildrenMap] = useState<Map<string, TreeEntry[]>>(
		new Map(),
	);
	const [loadingPaths, setLoadingPaths] = useState<Set<string>>(new Set());

	// Expand the workspace root whenever it changes and load its top-level entries.
	useEffect(() => {
		if (workspaceRoot) {
			void loadDirectory(workspaceRoot);
			setExpandedPaths(new Set([workspaceRoot]));
		}
	}, [workspaceRoot, loadDirectory]);

	const handleToggle = useCallback((path: string) => {
		setExpandedPaths((prev) => {
			const next = new Set(prev);
			if (next.has(path)) {
				next.delete(path);
			} else {
				next.add(path);
			}
			return next;
		});
	}, []);

	const handleLoadChildren = useCallback(
		async (path: string) => {
			if (!isTauri()) {
				setNotice("File system access requires the Tauri desktop app.");
				return;
			}
			if (childrenMap.has(path)) return;
			setLoadingPaths((prev) => new Set(prev).add(path));
			try {
				const entries = await readDirectory(path);
				setChildrenMap((prev) => new Map(prev).set(path, entries));
			} catch (err) {
				console.error("Failed to load directory children:", err);
			} finally {
				setLoadingPaths((prev) => {
					const next = new Set(prev);
					next.delete(path);
					return next;
				});
			}
		},
		[childrenMap, setNotice],
	);

	const refreshDirectory = useCallback(async (path: string) => {
		if (!path) return;
		try {
			const entries = await readDirectory(path);
			setChildrenMap((prev) => new Map(prev).set(path, entries));
		} catch (err) {
			console.error("Failed to refresh directory:", err);
		}
	}, []);

	return {
		expandedPaths,
		setExpandedPaths,
		childrenMap,
		setChildrenMap,
		loadingPaths,
		handleToggle,
		handleLoadChildren,
		refreshDirectory,
	};
}
