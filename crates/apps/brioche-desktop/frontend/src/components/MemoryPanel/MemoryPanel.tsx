import { useEffect } from "react";
import { Brain, Plus, X } from "lucide-react";
import { useMemoryStore } from "../../stores/memoryStore";
import type { MemoryEntry } from "../../ipc";
import PanelOverlay, { SearchBar, CategoryFilter } from "../PanelOverlay";
import { useMemoryPanel } from "../../hooks/memory";
import { Button, EmptyState, Input, Textarea } from "../ui";

interface MemoryPanelProps {
	onClose: () => void;
}

const CATEGORIES = ["preference", "fact", "habit", "project", "other"];


function MemoryListItem({
	memory,
	onDelete,
}: {
	memory: MemoryEntry;
	onDelete: (key: string) => void;
}) {
	return (
		<div className="flex flex-col gap-2 rounded-none border border-border bg-bg-elevated p-3">
			<div className="flex items-center justify-between gap-2">
				<span className="font-mono text-xs font-semibold text-fg-primary">
					{memory.key}
				</span>
				<div className="flex items-center gap-2">
					<span className="rounded-sm border border-border bg-bg-surface px-2 py-0.5 font-sans text-xs font-medium uppercase tracking-wider text-fg-secondary select-none">
						{memory.category}
					</span>
					<button
						type="button"
						className="flex shrink-0 cursor-pointer items-center justify-center rounded-md p-1 text-fg-muted transition-colors hover:bg-bg-highlight hover:text-error-text"
						onClick={() => onDelete(memory.key)}
						aria-label={`Delete memory ${memory.key}`}
					>
						<X className="h-3.5 w-3.5" />
					</button>
				</div>
			</div>
			<div className="px-0.5 text-sm leading-relaxed whitespace-pre-wrap text-fg-secondary">
				{memory.value}
			</div>
			<div className="px-0.5 text-xs text-fg-dim select-none">
				Updated: {new Date(memory.updated_at * 1000).toLocaleDateString()} | Accessed:{" "}
				{memory.access_count} times
			</div>
		</div>
	);
}

function MemoryList({
	memories,
	onDelete,
}: {
	memories: MemoryEntry[];
	onDelete: (key: string) => void;
}) {
	if (memories.length === 0) {
		return (
			<EmptyState
				icon={Brain}
				title="No memories yet"
				description="Add facts or preferences below to persist context for the model."
			/>
		);
	}

	return (
		<div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto py-1">
			{memories.map((memory) => (
				<MemoryListItem
					key={memory.key}
					memory={memory}
					onDelete={onDelete}
				/>
			))}
		</div>
	);
}

function MemoryCreateForm({
	categories,
	newKey,
	newValue,
	newCategory,
	canSave,
	setNewKey,
	setNewValue,
	setNewCategory,
	handleAdd,
	handleCancel,
}: {
	categories: string[];
	newKey: string;
	newValue: string;
	newCategory: string;
	canSave: boolean;
	setNewKey: (value: string) => void;
	setNewValue: (value: string) => void;
	setNewCategory: (value: string) => void;
	handleAdd: () => void;
	handleCancel: () => void;
}) {
	return (
		<div className="flex flex-col gap-2 rounded-none border border-border bg-bg-elevated p-4">
			<Input
				type="text"
				placeholder="Key (e.g., user_name)"
				value={newKey}
				onChange={(e) => setNewKey(e.target.value)}
				className="rounded-md"
			/>
			<Textarea
				placeholder="Value"
				value={newValue}
				onChange={(e) => setNewValue(e.target.value)}
				rows={3}
				className="rounded-md"
			/>
			<select
				value={newCategory}
				onChange={(e) => setNewCategory(e.target.value)}
				className="w-full cursor-pointer appearance-none rounded-md border border-border bg-bg-elevated px-3 py-2 text-xs text-fg-primary outline-none focus:border-accent-dim/60"
			>
				{categories.map((cat) => (
					<option key={cat} value={cat}>
						{cat.charAt(0).toUpperCase() + cat.slice(1)}
					</option>
				))}
			</select>
			<div className="flex justify-end gap-2 mt-1">
				<Button type="button" onClick={handleAdd} disabled={!canSave}>
					Save
				</Button>
				<Button type="button" variant="secondary" onClick={handleCancel}>
					Cancel
				</Button>
			</div>
		</div>
	);
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
				onDelete={handleDelete}
			/>

			<div className="shrink-0 border-t border-border pt-2">
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
						className="flex w-full cursor-pointer items-center justify-center gap-2 rounded-md border border-fg-primary/18 bg-fg-primary/14 py-2 text-xs font-semibold text-fg-primary shadow-sm transition-colors hover:bg-fg-primary/22"
						onClick={() => setIsAdding(true)}
					>
						<Plus className="h-4 w-4" />
						Add Memory
					</button>
				)}
			</div>
		</PanelOverlay>
	);
}
