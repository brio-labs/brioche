import { useCallback, useEffect, useState } from "react";
import { useFileStore } from "../../stores/fileStore";
import { useSettingsStore, getWorkingDir } from "../../stores/settingsStore";
import { isTauri, readDirectory } from "../../ipc";
import { open } from "@tauri-apps/plugin-dialog";
import { useFileTree } from "./useFileTree";
import { useFileClipboard } from "./useFileClipboard";
import { useFileRename } from "./useFileRename";
import { useFilePreview } from "./useFilePreview";
import { useFileCreation } from "./useFileCreation";

export function useFileExplorer() {
	const {
		currentPath,
		entries,
		isLoading: storeLoading,
		loadDirectory,
		createNewFile,
		createNewFolder,
		deleteExistingFile,
		writeExistingFile,
		renamePath,
		copyPath,
	} = useFileStore();
	const workspaceRoot = useSettingsStore((state) =>
		getWorkingDir(state.settings),
	);
	const [notice, setNotice] = useState<string | null>(null);

	// Clear transient notices after a short delay.
	useEffect(() => {
		if (!notice) return;
		const timeout = window.setTimeout(() => setNotice(null), 3000);
		return () => window.clearTimeout(timeout);
	}, [notice]);

	const {
		expandedPaths,
		setExpandedPaths,
		childrenMap,
		setChildrenMap,
		loadingPaths,
		handleToggle,
		handleLoadChildren,
		refreshDirectory,
	} = useFileTree({ workspaceRoot, loadDirectory, setNotice });

	const { clipboard, handleCopy, handleCut, handlePaste } = useFileClipboard({
		renamePath,
		copyPath,
		refreshDirectory,
		expandedPaths,
		setExpandedPaths,
		setChildrenMap,
	});

	const {
		renamingPath,
		renameValue,
		handleStartRename,
		handleRenameValueChange,
		handleCommitRename,
		handleCancelRename,
	} = useFileRename({
		renamePath,
		refreshDirectory,
		expandedPaths,
	});

	const { preview, setPreview, handlePreview, handleSavePreview } = useFilePreview(
		{ writeExistingFile },
	);

	const {
		isCreating,
		createType,
		createParentPath,
		newName,
		setNewName,
		startCreation,
		cancelCreation,
		handleCommitCreation,
	} = useFileCreation({
		createNewFile,
		createNewFolder,
		expandedPaths,
		setExpandedPaths,
		setChildrenMap,
		currentPath,
		workspaceRoot,
		readDirectory,
	});

	const handleOpenFolder = useCallback(async () => {
		if (!isTauri()) {
			setNotice("Folder picker requires the Tauri desktop app.");
			return;
		}
		try {
			const selected = await open({
				multiple: false,
				directory: true,
			});
			if (selected && typeof selected === "string") {
				const store = useSettingsStore.getState();
				store.updateSetting("ui.working_dir", selected);
				await store.saveSettings(useSettingsStore.getState().settings);
				await loadDirectory(selected);
				setExpandedPaths(new Set([selected]));
				setChildrenMap(new Map());
			}
		} catch (err) {
			console.error("Failed to open directory picker:", err);
		}
	}, [loadDirectory, setExpandedPaths, setChildrenMap]);

	const handleRefresh = useCallback(() => {
		// Reload all currently expanded paths from root outward.
		const paths = Array.from(expandedPaths).sort((a, b) => a.length - b.length);
		setChildrenMap(new Map());
		void (async () => {
			for (const path of paths) {
				try {
					const entries = await readDirectory(path);
					setChildrenMap((prev) => new Map(prev).set(path, entries));
				} catch (err) {
					console.error("Failed to refresh directory:", err);
				}
			}
		})();
		if (currentPath) {
			void loadDirectory(currentPath);
		}
	}, [expandedPaths, currentPath, loadDirectory, setChildrenMap]);

	const handleDelete = useCallback(
		async (path: string) => {
			if (!confirm(`Delete ${path}?`)) return;
			try {
				await deleteExistingFile(path);
				// Refresh parent directories that are expanded.
				const parent = path.split("/").slice(0, -1).join("/") || "/";
				if (expandedPaths.has(parent)) {
					const entries = await readDirectory(parent);
					setChildrenMap((prev) => new Map(prev).set(parent, entries));
				}
			} catch (err) {
				console.error("Failed to delete:", err);
			}
		},
		[deleteExistingFile, expandedPaths, setChildrenMap],
	);

	const creatingFor = isCreating ? createParentPath : null;

	return {
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
		isCreating,
		createType,
		createParentPath,
		newName,
		creatingFor,
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
	};
}
