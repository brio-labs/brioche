import { useState, useEffect } from "react";
import { useMemoryStore } from "../stores/panelStores";
import PanelOverlay, { SearchBar, CategoryFilter } from "./PanelOverlay";
import { ActionRow, FormFieldStack } from "./ui";

interface MemoryPanelProps {
	onClose: () => void;
}

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

	// Local temporary state for the "Add Memory" input form
	const [newKey, setNewKey] = useState("");
	const [newValue, setNewValue] = useState("");
	const [newCategory, setNewCategory] = useState("preference");

	useEffect(() => {
		loadMemories();
	}, [loadMemories]);

	const handleAdd = async () => {
		if (!newKey.trim() || !newValue.trim()) return;
		const success = await addNewMemory(newKey.trim(), newValue.trim(), newCategory);
		if (success) {
			setNewKey("");
			setNewValue("");
			setNewCategory("preference");
			setIsAdding(false);
		}
	};

	const handleDelete = async (key: string) => {
		if (!isTauriAvailable) return;
		await deleteExistingMemory(key);
	};

	const formatDate = (timestamp: number) => {
		return new Date(timestamp * 1000).toLocaleDateString();
	};

	return (
		<PanelOverlay title="Memory" onClose={onClose} panelClassName="bg-bg-1 border border-border rounded-lg w-[600px] max-w-[95vw] max-h-[85vh] flex flex-col overflow-hidden animate-slideUp shadow-2xl z-[1001]">
			<div className="flex flex-col h-full min-h-0 overflow-hidden p-5 gap-4">
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
					<div className="bg-error-bg text-[#e8a0a0] border border-error-border px-3.5 py-2.5 rounded-lg text-xs shrink-0">
						Memory panel preview mode: changes require the Tauri desktop app.
					</div>
				)}
				{error && <div className="bg-error-bg text-[#e8a0a0] border border-error-border px-3.5 py-2.5 rounded-lg text-xs shrink-0">{error}</div>}

				<div className="flex-1 overflow-y-auto min-h-0 flex flex-col gap-3 py-1">
					{memories.length === 0 ? (
						<div className="text-center text-text-muted py-12 text-sm select-none">No memories yet</div>
					) : (
						memories.map((memory) => (
							<div key={memory.key} className="p-3 bg-bg-2/30 border border-border rounded-lg flex flex-col gap-1.5 transition-all hover:border-border-hover">
								<div className="flex items-center justify-between gap-2">
									<span className="font-mono text-xs font-semibold text-text-primary">{memory.key}</span>
									<div className="flex items-center gap-2">
										<span className="px-1.5 py-0.5 rounded text-[9px] font-medium bg-accent/10 border border-accent/20 text-accent font-sans uppercase tracking-wider select-none">{memory.category}</span>
										<button
											className="p-1 text-text-muted hover:text-red-400 hover:bg-bg-3 rounded transition-colors cursor-pointer flex items-center justify-center shrink-0"
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
								<div className="text-sm text-text-secondary whitespace-pre-wrap leading-relaxed px-0.5">{memory.value}</div>
								<div className="text-[10px] text-text-dim px-0.5 select-none">
									Updated: {formatDate(memory.updated_at)} | Accessed:{" "}
									{memory.access_count} times
								</div>
							</div>
						))
					)}
				</div>

				<div className="shrink-0 pt-2 border-t border-border/30">
					{isAdding ? (
						<FormFieldStack>
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
							<ActionRow>
								<button onClick={handleAdd}>Save</button>
								<button onClick={() => setIsAdding(false)}>Cancel</button>
							</ActionRow>
						</FormFieldStack>
					) : (
						<button className="w-full py-2 bg-accent hover:bg-accent-hover text-white text-xs font-semibold rounded cursor-pointer transition-colors flex items-center justify-center gap-1 shadow-sm shadow-accent-glow/10" onClick={() => setIsAdding(true)}>
							+ Add Memory
						</button>
					)}
				</div>
			</div>
		</PanelOverlay>
	);
}
