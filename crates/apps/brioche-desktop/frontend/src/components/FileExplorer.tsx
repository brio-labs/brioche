import { useCallback, useEffect, useMemo, useState } from "react";
import { useFileStore } from "../stores/fileStore";
import { useSettingsStore, getWorkingDir } from "../stores/settingsStore";
import { readDirectory, readFile, isTauri } from "../ipc";
import { open } from "@tauri-apps/plugin-dialog";
import type { DirEntry } from "../ipc";
import {
	FolderIcon,
	FileIcon,
	ChevronRightIcon,
	RefreshIcon,
	TrashIcon,
	SaveIcon,
} from "./Icons";
import { cn } from "./ui/lib";

interface TreeEntry extends DirEntry {
	children?: TreeEntry[];
	isLoading?: boolean;
}

interface FileTreeItemProps {
	entry: TreeEntry;
	depth: number;
	workspaceRoot: string;
	expandedPaths: Set<string>;
	childrenMap: Map<string, TreeEntry[]>;
	onToggle: (path: string) => void;
	onLoadChildren: (path: string) => void;
	onPreview: (path: string) => void;
	onDelete: (path: string) => void;
	onContextMenu: (
		e: React.MouseEvent,
		entry: { path: string; is_dir: boolean },
	) => void;
	creatingFor: string | null;
	createType: "file" | "folder";
	newName: string;
	onNewNameChange: (value: string) => void;
	onCommitCreation: () => void;
	onCancelCreation: () => void;
}

/**
 * Renders a single node in the file explorer tree, including recursive children.
 *
 * Refs: I-Ui-FileExplorer
 */
function FileTreeItem({
	entry,
	depth,
	expandedPaths,
	childrenMap,
	onToggle,
	onLoadChildren,
	onPreview,
	onDelete,
	onContextMenu,
	creatingFor,
	createType,
	newName,
	onNewNameChange,
	onCommitCreation,
	onCancelCreation,
}: FileTreeItemProps) {
	const isExpanded = expandedPaths.has(entry.path);
	const children = childrenMap.get(entry.path);
	const isCreatingHere = creatingFor === entry.path;
	const indent = depth * 0.75;

	const handleClick = useCallback(() => {
		if (entry.is_dir) {
			if (!isExpanded && !childrenMap.has(entry.path)) {
				onLoadChildren(entry.path);
			}
			onToggle(entry.path);
		}
	}, [entry, isExpanded, childrenMap, onLoadChildren, onToggle]);

	const handleDoubleClick = useCallback(() => {
		if (!entry.is_dir) {
			onPreview(entry.path);
		}
	}, [entry, onPreview]);

	return (
		<div>
			<div
				className={cn(
					"group mx-2 flex cursor-pointer items-center gap-2 rounded-lg border border-transparent px-3 py-2 text-fg-secondary transition-all duration-200 hover:border-border-accent hover:bg-accent/5 hover:text-fg-primary",
				)}
				style={{ paddingLeft: `${0.75 + indent}rem` }}
				onClick={handleClick}
				onDoubleClick={handleDoubleClick}
				onContextMenu={(e) =>
					onContextMenu(e, { path: entry.path, is_dir: entry.is_dir })
				}
				title={entry.path}
			>
				{entry.is_dir ? (
					<ChevronRightIcon
						className={cn(
							"h-3.5 w-3.5 shrink-0 text-fg-muted transition-transform duration-150",
							isExpanded && "rotate-90",
						)}
					/>
				) : (
					<span className="h-3.5 w-3.5 shrink-0" />
				)}
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
						onClick={(e) => {
							e.stopPropagation();
							onDelete(entry.path);
						}}
						title="Delete"
						aria-label={`Delete ${entry.name}`}
					>
						<TrashIcon className="h-3.5 w-3.5" />
					</button>
				)}
			</div>

			{isCreatingHere && (
				<div
					className="flex items-center gap-2 bg-bg-elevated/50 py-2 pr-3"
					style={{ paddingLeft: `${1.5 + indent}rem` }}
				>
					{createType === "folder" ? (
						<FolderIcon className="h-3.5 w-3.5 text-accent-dim" />
					) : (
						<FileIcon className="h-3.5 w-3.5 text-fg-muted" />
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
								workspaceRoot={workspaceRoot}
								expandedPaths={expandedPaths}
								childrenMap={childrenMap}
								onToggle={onToggle}
								onLoadChildren={onLoadChildren}
								onPreview={onPreview}
								onDelete={onDelete}
								onContextMenu={onContextMenu}
								creatingFor={creatingFor}
								createType={createType}
								newName={newName}
								onNewNameChange={onNewNameChange}
								onCommitCreation={onCommitCreation}
								onCancelCreation={onCancelCreation}
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

/// Renders a file explorer with an IDE-style collapsible tree.
///
/// Refs: I-Ui-FileExplorer
export default function FileExplorer() {
	const {
		currentPath,
		entries,
		isLoading: storeLoading,
		loadDirectory,
		createNewFile,
		createNewFolder,
		deleteExistingFile,
		writeExistingFile,
	} = useFileStore();
	const workspaceRoot = useSettingsStore((state) =>
		getWorkingDir(state.settings),
	);
	const [preview, setPreview] = useState<{ path: string; content: string } | null>(null);
	const [isCreating, setIsCreating] = useState(false);
	const [createType, setCreateType] = useState<"file" | "folder">("file");
	const [createParentPath, setCreateParentPath] = useState("");
	const [newName, setNewName] = useState("");
	const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
	const [childrenMap, setChildrenMap] = useState<Map<string, TreeEntry[]>>(new Map());
	const [loadingPaths, setLoadingPaths] = useState<Set<string>>(new Set());
	const [notice, setNotice] = useState<string | null>(null);
	const [contextMenu, setContextMenu] = useState<{
		x: number;
		y: number;
		targetPath: string | null;
		isDir: boolean;
	} | null>(null);

	// Clear transient notices after a short delay.
	useEffect(() => {
		if (!notice) return;
		const timeout = window.setTimeout(() => setNotice(null), 3000);
		return () => window.clearTimeout(timeout);
	}, [notice]);

	// Load the workspace root when it changes.
	useEffect(() => {
		if (workspaceRoot) {
			void loadDirectory(workspaceRoot);
			setExpandedPaths(new Set([workspaceRoot]));
		}
	}, [workspaceRoot, loadDirectory]);

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
	}, [loadDirectory]);

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
	}, [expandedPaths, currentPath, loadDirectory]);

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

	const handleLoadChildren = useCallback(async (path: string) => {
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
	}, [childrenMap]);

	const handlePreview = useCallback(async (path: string) => {
		try {
			const content = await readFile(path);
			setPreview({ path, content });
		} catch (err) {
			console.error("Failed to read file:", err);
		}
	}, []);

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
		[deleteExistingFile, expandedPaths],
	);

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
		const handleOutsideClick = () => setContextMenu(null);
		window.addEventListener("click", handleOutsideClick);
		return () => window.removeEventListener("click", handleOutsideClick);
	}, []);

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
			setContextMenu(null);
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
	]);

	const handleDeleteFromMenu = useCallback(
		async (path: string) => {
			setContextMenu(null);
			await handleDelete(path);
		},
		[handleDelete],
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

	const rootEntries = useMemo(() => {
		if (!workspaceRoot) return [];
		return entries.map((entry) => ({
			...entry,
			isLoading: loadingPaths.has(entry.path),
			children: childrenMap.get(entry.path),
		}));
	}, [entries, childrenMap, loadingPaths, workspaceRoot]);

	const creatingFor = isCreating ? createParentPath : null;

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
						onClick={handleRefresh}
						title="Refresh"
					>
						<RefreshIcon className="h-4 w-4" />
					</button>
				</div>
			</div>
			{!isTauri() && (
				<div className="notice-error shrink-0 px-5 py-2 text-xs">
					Explorer preview mode: folder operations require the Tauri desktop
					app.
				</div>
			)}
			{notice && (
				<div className="notice-error shrink-0 px-5 py-2 text-xs">{notice}</div>
			)}
			<div className="flex items-center gap-2 border-b border-border bg-bg-base/50 px-4 py-3">
				<span
					className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs text-fg-muted"
					title={workspaceRoot || currentPath}
				>
					{workspaceRoot || currentPath || "No directory"}
				</span>
			</div>

			<div
				className="flex flex-1 flex-col overflow-y-auto py-2"
				onContextMenu={handleContainerContextMenu}
			>
				{storeLoading && (
					<div className="py-4 text-center text-xs text-fg-muted">Loading...</div>
				)}

				{rootEntries.map((entry) => (
					<FileTreeItem
						key={entry.path}
						entry={entry}
						depth={0}
						workspaceRoot={workspaceRoot || ""}
						expandedPaths={expandedPaths}
						childrenMap={childrenMap}
						onToggle={handleToggle}
						onLoadChildren={handleLoadChildren}
						onPreview={handlePreview}
						onDelete={handleDelete}
						onContextMenu={handleItemContextMenu}
						creatingFor={creatingFor}
						createType={createType}
						newName={newName}
						onNewNameChange={setNewName}
						onCommitCreation={handleCommitCreation}
						onCancelCreation={cancelCreation}
					/>
				))}
				{rootEntries.length === 0 && !storeLoading && workspaceRoot && (
					<div className="py-8 text-center text-xs text-fg-muted select-none">Empty</div>
				)}
				{!workspaceRoot && !storeLoading && (
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
