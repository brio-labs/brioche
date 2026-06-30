import { useState, useEffect } from "react";
import { useMemoryStore } from "../stores/panelStores";
import PanelOverlay, { SearchBar, CategoryFilter } from "./PanelOverlay";

/// Props for the memory management panel.
interface MemoryPanelProps {
	onClose: () => void;
}

/// Renders the memory management panel with search, category filtering, and creation.
///
/// Refs: I-Ui-MemoryPanel
export default function MemoryPanel({ onClose }: MemoryPanelProps) {
	const {
		memories,
		searchQuery,
		selectedCategory,
		isAdding,
		error,
		isTauriAvailable,
		loadMemories,
		searchExistingMemories,
		addNewMemory,
		deleteExistingMemory,
		setSearchQuery,
		setSelectedCategory,
		setIsAdding,
	} = useMemoryStore();

	const categories = ["preference", "fact", "habit", "project", "other"];

	const [newKey, setNewKey] = useState("");
	const [newValue, setNewValue] = useState("");
	const [newCategory, setNewCategory] = useState("preference");

	useEffect(() => {
		loadMemories();
	}, [loadMemories]);

	const handleAdd = async () => {
		if (!newKey.trim() || !newValue.trim()) return;
		const success = await addNewMemory(
			newKey.trim(),
			newValue.trim(),
			newCategory,
		);
		if (success) {
			setNewKey("");
			setNewValue("");
			setNewCategory("preference");
			setIsAdding(false);
		}
	};

	const handleCancel = () => {
		setNewKey("");
		setNewValue("");
		setNewCategory("preference");
		setIsAdding(false);
	};

	const handleDelete = async (key: string) => {
		if (!isTauriAvailable) return;
		await deleteExistingMemory(key);
	};

	const formatDate = (timestamp: number) => {
		return new Date(timestamp * 1000).toLocaleDateString();
	};

	const canSave = Boolean(newKey.trim() && newValue.trim());

	return (
		<PanelOverlay title="Memory" onClose={onClose} size="sm">
			<SearchBar
				placeholder="Search memories..."
				value={searchQuery}
				onChange={setSearchQuery}
				onSearch={searchExistingMemories}
				containerClassName="shrink-0"
			/>

			<CategoryFilter
				categories={categories}
				activeCategory={selectedCategory === "all" ? null : selectedCategory}
				onSelect={(cat) => setSelectedCategory(cat || "all")}
				containerClassName="shrink-0"
			/>

			{!isTauriAvailable && (
				<div className="notice-error shrink-0">
					Memory panel preview mode: changes require the Tauri desktop app.
				</div>
			)}
			{error && <div className="notice-error shrink-0">{error}</div>}

			<div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto py-1">
				{memories.length === 0 ? (
					<div className="empty-state">No memories yet</div>
				) : (
					memories.map((memory) => (
						<div key={memory.key} className="surface-card flex flex-col gap-1.5">
							<div className="flex items-center justify-between gap-2">
								<span className="font-mono text-xs font-semibold text-fg-primary">
									{memory.key}
								</span>
								<div className="flex items-center gap-2">
									<span className="rounded border border-accent/20 bg-accent/10 px-1.5 py-0.5 font-sans text-xs font-medium uppercase tracking-wider text-accent select-none">
										{memory.category}
									</span>
									<button
										type="button"
										className="flex shrink-0 cursor-pointer items-center justify-center rounded p-1.5 text-fg-muted transition-colors hover:bg-bg-highlight hover:text-error-text"
										onClick={() => handleDelete(memory.key)}
										aria-label={`Delete memory ${memory.key}`}
									>
										<svg
											width="12"
											height="12"
											viewBox="0 0 12 12"
											fill="currentColor"
										>
											<path
												d="M3 3l6 6M3 9l6-6"
												stroke="currentColor"
												strokeWidth="1.5"
											/>
										</svg>
									</button>
								</div>
							</div>
							<div className="px-0.5 text-sm leading-relaxed whitespace-pre-wrap text-fg-secondary">
								{memory.value}
							</div>
							<div className="px-0.5 text-xs text-fg-dim select-none">
								Updated: {formatDate(memory.updated_at)} | Accessed:{" "}
								{memory.access_count} times
							</div>
						</div>
					))
				)}
			</div>

			<div className="shrink-0 border-t border-border/30 pt-2">
				{isAdding ? (
					<div className="flex flex-col gap-2.5 rounded-lg border border-border bg-bg-elevated/30 p-3.5 [&_input]:rounded [&_input]:border [&_input]:border-border [&_input]:bg-bg-elevated [&_input]:px-2.5 [&_input]:py-1.5 [&_input]:text-xs [&_input]:text-fg-primary [&_input]:outline-none [&_input]:focus:border-accent-dim/60 [&_textarea]:resize-none [&_textarea]:rounded [&_textarea]:border [&_textarea]:border-border [&_textarea]:bg-bg-elevated [&_textarea]:px-2.5 [&_textarea]:py-1.5 [&_textarea]:text-xs [&_textarea]:text-fg-primary [&_textarea]:outline-none [&_textarea]:focus:border-accent-dim/60 [&_select]:cursor-pointer [&_select]:appearance-none [&_select]:rounded [&_select]:border [&_select]:border-border [&_select]:bg-bg-elevated [&_select]:px-2.5 [&_select]:py-1.5 [&_select]:text-xs [&_select]:text-fg-primary [&_select]:outline-none [&_select]:focus:border-accent-dim/60">
						<input
							type="text"
							placeholder="Key (e.g., user_name)"
							value={newKey}
							onChange={(e) => setNewKey(e.target.value)}
						/>
						<textarea
							placeholder="Value"
							value={newValue}
							onChange={(e) => setNewValue(e.target.value)}
							rows={3}
						/>
						<select
							value={newCategory}
							onChange={(e) => setNewCategory(e.target.value)}
						>
							{categories.map((cat) => (
								<option key={cat} value={cat}>
									{cat.charAt(0).toUpperCase() + cat.slice(1)}
								</option>
							))}
						</select>
						<div className="flex justify-end gap-2 [&_button]:cursor-pointer [&_button]:rounded [&_button]:px-3 [&_button]:py-1.5 [&_button]:text-xs [&_button]:font-medium [&_button:first-child]:bg-accent [&_button:first-child]:text-white [&_button:first-child]:hover:bg-accent-hover [&_button:first-child]:disabled:cursor-not-allowed [&_button:first-child]:disabled:opacity-50 [&_button:last-child]:border [&_button:last-child]:border-border [&_button:last-child]:bg-transparent [&_button:last-child]:text-fg-secondary [&_button:last-child]:hover:bg-bg-elevated">
							<button
								type="button"
								onClick={handleAdd}
								disabled={!canSave}
							>
								Save
							</button>
							<button type="button" onClick={handleCancel}>
								Cancel
							</button>
						</div>
					</div>
				) : (
					<button
						type="button"
						className="flex w-full cursor-pointer items-center justify-center gap-1 rounded bg-accent py-2.5 text-xs font-semibold text-white shadow-sm shadow-accent-glow/10 transition-colors hover:bg-accent-hover"
						onClick={() => setIsAdding(true)}
					>
						+ Add Memory
					</button>
				)}
			</div>
		</PanelOverlay>
	);
}
