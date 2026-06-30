import { useCallback, useState, useEffect } from "react";
import { useFileStore } from "../stores/fileStore";
import { useSettingsStore, getWorkingDir } from "../stores/settingsStore";
import { readFile } from "../ipc";
import { open } from "@tauri-apps/plugin-dialog";
import {
	FolderIcon,
	FileIcon,
	ChevronUpIcon,
	RefreshIcon,
	TrashIcon,
	SaveIcon,
} from "./Icons";

/// Renders a file explorer with directory navigation, file preview, and context-menu creation.
///
/// Refs: I-Ui-FileExplorer
export default function FileExplorer() {
	const {
		currentPath,
		entries,
		isLoading,
		loadDirectory,
		navigateUp,
		navigateTo,
		createNewFile,
		createNewFolder,
		deleteExistingFile,
		writeExistingFile,
	} = useFileStore();
	const [preview, setPreview] = useState<{
		path: string;
		content: string;
	} | null>(null);
	const workspaceRoot = useSettingsStore((state) =>
		getWorkingDir(state.settings),
	);
	const [isCreating, setIsCreating] = useState(false);
	const [createType, setCreateType] = useState<"file" | "folder">("file");
	const [createParentPath, setCreateParentPath] = useState("");
	const [newName, setNewName] = useState("");

	const [contextMenu, setContextMenu] = useState<{
		x: number;
		y: number;
		targetPath: string | null;
		isDir: boolean;
	} | null>(null);

	const handleOpenFolder = useCallback(async () => {
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
			}
		} catch (err) {
			console.error("Failed to open directory picker:", err);
		}
	}, [loadDirectory]);

	const handleEntryClick = useCallback(
		(entry: { is_dir: boolean; path: string }) => {
			if (entry.is_dir) {
				navigateTo(entry.path);
			}
		},
		[navigateTo],
	);

	const handleEntryDoubleClick = useCallback(
		async (entry: { is_dir: boolean; path: string }) => {
			if (entry.is_dir) return;
			try {
				const content = await readFile(entry.path);
				setPreview({ path: entry.path, content });
			} catch (err) {
				console.error("Failed to read file:", err);
			}
		},
		[],
	);

	const handleDelete = useCallback(
		async (e: React.MouseEvent, path: string) => {
			e.stopPropagation();
			if (!confirm(`Delete ${path}?`)) return;
			try {
				await deleteExistingFile(path);
			} catch (err) {
				console.error("Failed to delete:", err);
			}
		},
		[deleteExistingFile],
	);

	const handleSavePreview = useCallback(async () => {
		if (!preview) return;
		try {
			await writeExistingFile(preview.path, preview.content);
			setPreview(null);
		} catch (err) {
			console.error("Failed to save file:", err);
		}
	}, [preview, writeExistingFile]);

	const handleContainerContextMenu = useCallback((e: React.MouseEvent) => {
		e.preventDefault();
		setContextMenu({
			x: e.clientX,
			y: e.clientY,
			targetPath: null,
			isDir: true,
		});
	}, []);

	const handleItemContextMenu = useCallback(
		(e: React.MouseEvent, entry: { path: string; is_dir: boolean }) => {
			e.preventDefault();
			e.stopPropagation();
			setContextMenu({
				x: e.clientX,
				y: e.clientY,
				targetPath: entry.path,
				isDir: entry.is_dir,
			});
		},
		[],
	);

	useEffect(() => {
		const handleOutsideClick = () => {
			setContextMenu(null);
		};
		window.addEventListener("click", handleOutsideClick);
		return () => window.removeEventListener("click", handleOutsideClick);
	}, []);

	const startCreation = useCallback(
		(type: "file" | "folder", targetPath: string | null, isDir: boolean) => {
			setIsCreating(true);
			setCreateType(type);
			setNewName("");

			if (targetPath === null) {
				setCreateParentPath(currentPath);
			} else if (isDir) {
				setCreateParentPath(targetPath);
			} else {
				const parent = targetPath.split("/").slice(0, -1).join("/") || "/";
				setCreateParentPath(parent);
			}
			setContextMenu(null);
		},
		[currentPath],
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
	]);

	const handleDeleteFromMenu = useCallback(
		async (path: string) => {
			setContextMenu(null);
			if (!confirm(`Delete ${path}?`)) return;
			try {
				await deleteExistingFile(path);
			} catch (err) {
				console.error("Failed to delete:", err);
			}
		},
		[deleteExistingFile],
	);

	return (
		<div className="relative flex h-full w-full flex-col overflow-hidden bg-transparent text-fg-primary">
			<div className="flex h-13 shrink-0 items-center justify-between border-b border-border bg-bg-base/30 px-5 py-4 backdrop-blur-sm">
				<h2 className="select-none text-xs font-bold uppercase tracking-widest text-fg-muted">
					Explorer
				</h2>
				<div className="flex items-center gap-2">
					<button
						type="button"
						className="btn-icon h-7 w-7"
						onClick={handleOpenFolder}
						title="Open Folder..."
					>
						<FolderIcon className="h-4 w-4" />
					</button>
					<button
						type="button"
						className="btn-icon h-7 w-7"
						onClick={() => loadDirectory(currentPath)}
						title="Refresh"
					>
						<RefreshIcon className="h-4 w-4" />
					</button>
				</div>
			</div>
			<div className="flex items-center gap-2 border-b border-border bg-bg-base/50 px-4 py-3">
				<button
					type="button"
					className="btn-icon h-6 w-6 disabled:cursor-not-allowed disabled:opacity-30"
					onClick={navigateUp}
					disabled={currentPath === "/" || currentPath === workspaceRoot}
					title="Parent directory"
				>
					<ChevronUpIcon className="h-3.5 w-3.5" />
				</button>
				<span
					className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs text-fg-muted"
					title={currentPath}
				>
					{currentPath || "No directory"}
				</span>
			</div>

			<div
				className="flex flex-1 flex-col overflow-y-auto py-2"
				onContextMenu={handleContainerContextMenu}
			>
				{isLoading && (
					<div className="py-4 text-center text-xs text-fg-muted">
						Loading...
					</div>
				)}

				{isCreating && createParentPath === currentPath && (
					<div className="flex items-center gap-2 bg-bg-elevated/50 px-3 py-2">
						{createType === "folder" ? (
							<FolderIcon className="h-3.5 w-3.5 text-accent-dim" />
						) : (
							<FileIcon className="h-3.5 w-3.5 text-fg-muted" />
						)}
						<input
							type="text"
							value={newName}
							onChange={(e) => setNewName(e.target.value)}
							placeholder={
								createType === "folder" ? "Folder Name" : "File Name"
							}
							autoFocus
							onBlur={handleCommitCreation}
							className="input-field flex-1 rounded-sm px-1.5 py-0.5 text-xs"
							onKeyDown={(e) => {
								if (e.key === "Enter") void handleCommitCreation();
								else if (e.key === "Escape") cancelCreation();
							}}
						/>
					</div>
				)}

				{entries.map((entry) => (
					<div key={entry.path}>
						<div
							className="group mx-2 flex cursor-pointer items-center gap-2.5 rounded-lg border border-transparent px-3 py-2 text-fg-secondary transition-all duration-200 hover:border-border-accent hover:bg-accent/5 hover:text-fg-primary"
							onClick={() => handleEntryClick(entry)}
							onDoubleClick={() => handleEntryDoubleClick(entry)}
							onContextMenu={(e) => handleItemContextMenu(e, entry)}
							title={entry.path}
						>
							{entry.is_dir ? (
								<FolderIcon className="h-3.5 w-3.5 shrink-0 text-accent-dim group-hover:text-accent" />
							) : (
								<FileIcon className="h-3.5 w-3.5 shrink-0 text-fg-muted group-hover:text-fg-secondary" />
							)}
							<span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs">
								{entry.name}
							</span>
							{!entry.is_dir && (
								<button
									type="button"
									className="btn-icon ml-2 h-6 w-6 border border-transparent text-fg-muted opacity-0 transition-opacity duration-200 hover:border-error-border hover:bg-error-bg hover:text-error-text group-hover:opacity-100"
									onClick={(e) => handleDelete(e, entry.path)}
									title="Delete"
								>
									<TrashIcon className="h-3.5 w-3.5" />
								</button>
							)}
						</div>
						{isCreating && createParentPath === entry.path && (
							<div className="flex items-center gap-2 bg-bg-elevated/50 py-2 pl-6 pr-3">
								{createType === "folder" ? (
									<FolderIcon className="h-3.5 w-3.5 text-accent-dim" />
								) : (
									<FileIcon className="h-3.5 w-3.5 text-fg-muted" />
								)}
								<input
									type="text"
									value={newName}
									onChange={(e) => setNewName(e.target.value)}
									placeholder={
										createType === "folder"
											? "Folder Name"
											: "File Name"
									}
									autoFocus
									onBlur={handleCommitCreation}
									className="input-field flex-1 rounded-sm px-1.5 py-0.5 text-xs"
									onKeyDown={(e) => {
										if (e.key === "Enter") void handleCommitCreation();
										else if (e.key === "Escape") cancelCreation();
									}}
								/>
							</div>
						)}
					</div>
				))}
				{entries.length === 0 && !isLoading && currentPath && (
					<div className="py-8 text-center text-xs text-fg-muted select-none">
						Empty
					</div>
				)}
				{!currentPath && !isLoading && (
					<div className="flex flex-1 flex-col items-center justify-center px-4 py-12 text-center text-xs text-fg-muted select-none">
						<span>No directory open</span>
						<button
							type="button"
							className="mt-3 w-full max-w-50 cursor-pointer rounded bg-accent py-2 px-3 text-sm font-medium text-white shadow-sm transition-colors hover:bg-accent-hover active:bg-accent-dim"
							onClick={handleOpenFolder}
						>
							Open Folder
						</button>
					</div>
				)}
			</div>

			{preview && (
				<div className="absolute bottom-0 left-0 right-0 z-10 flex h-[45%] flex-col border-t border-border bg-bg-surface">
					<div className="flex shrink-0 items-center justify-between border-b border-border bg-bg-surface/80 px-4 py-3">
						<span className="truncate font-mono text-xs text-fg-secondary">
							{preview.path}
						</span>
						<div className="flex gap-1">
							<button
								type="button"
								className="btn-icon h-7 w-7"
								onClick={handleSavePreview}
								title="Save"
							>
								<SaveIcon className="h-4 w-4" />
							</button>
							<button
								type="button"
								className="btn-icon h-7 w-7 text-sm font-semibold"
								onClick={() => setPreview(null)}
								title="Close"
							>
								×
							</button>
						</div>
					</div>
					<textarea
						value={preview.content}
						onChange={(e) =>
							setPreview({ ...preview, content: e.target.value })
						}
						spellCheck={false}
						className="textarea-field flex-1 resize-none rounded-none border-none p-4 leading-relaxed"
					/>
				</div>
			)}

			{contextMenu && (
				<div
					className="fixed z-9999 min-w-40 rounded-md border border-border bg-bg-surface py-1 shadow-lg backdrop-blur-sm"
					style={{
						left: contextMenu.x,
						top: contextMenu.y,
					}}
					onClick={(e) => e.stopPropagation()}
				>
					<div
						className="flex cursor-pointer items-center gap-2 px-4 py-2 text-sm text-fg-primary transition-colors duration-150 hover:bg-accent-dim hover:text-white"
						onClick={() =>
							startCreation("file", contextMenu.targetPath, contextMenu.isDir)
						}
					>
						New File
					</div>
					<div
						className="flex cursor-pointer items-center gap-2 px-4 py-2 text-sm text-fg-primary transition-colors duration-150 hover:bg-accent-dim hover:text-white"
						onClick={() =>
							startCreation("folder", contextMenu.targetPath, contextMenu.isDir)
						}
					>
						New Folder
					</div>
					{contextMenu.targetPath && (
						<div
							className="flex cursor-pointer items-center gap-2 px-4 py-2 text-sm text-error-text transition-colors duration-150 hover:bg-error-bg hover:text-error-text"
							onClick={() => handleDeleteFromMenu(contextMenu.targetPath!)}
						>
							Delete
						</div>
					)}
				</div>
			)}
		</div>
	);
}
