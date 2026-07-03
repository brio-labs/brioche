import { useEffect } from "react";
import { useMemoryStore } from "../../stores/panelStores";
import PanelOverlay, { SearchBar, CategoryFilter } from "../PanelOverlay";
import { useMemoryPanel } from "../../hooks/memory";
import MemoryList from "./MemoryList";
import MemoryCreateForm from "./MemoryCreateForm";

/// Props for the memory management panel.
interface MemoryPanelProps {
	onClose: () => void;
}

const CATEGORIES = ["preference", "fact", "habit", "project", "other"];

const formatDate = (timestamp: number) => {
	return new Date(timestamp * 1000).toLocaleDateString();
};

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
		deleteExistingMemory,
		setSearchQuery,
		setSelectedCategory,
		setIsAdding,
	} = useMemoryStore();

	const {
		newKey,
		newValue,
		newCategory,
		setNewKey,
		setNewValue,
		setNewCategory,
		canSave,
		handleAdd,
		handleCancel,
	} = useMemoryPanel();

	useEffect(() => {
		loadMemories();
	}, [loadMemories]);

	const handleDelete = async (key: string) => {
		if (!isTauriAvailable) return;
		await deleteExistingMemory(key);
	};

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
				categories={CATEGORIES}
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

			<MemoryList
				memories={memories}
				formatDate={formatDate}
				onDelete={handleDelete}
			/>

			<div className="shrink-0 border-t border-border/30 pt-2">
				{isAdding ? (
					<MemoryCreateForm
						categories={CATEGORIES}
						newKey={newKey}
						newValue={newValue}
						newCategory={newCategory}
						canSave={canSave}
						setNewKey={setNewKey}
						setNewValue={setNewValue}
						setNewCategory={setNewCategory}
						handleAdd={handleAdd}
						handleCancel={handleCancel}
					/>
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
