import { create } from "zustand";
import type { MemoryEntry } from "../ipc";
import {
	listMemories,
	setMemory,
	deleteMemory,
	searchMemories,
	isTauri,
} from "../ipc";

/// State and actions for the memory panel.
///
/// Refs: I-Ui-MemoryStore
interface MemoryStore {
	memories: MemoryEntry[];
	searchQuery: string;
	selectedCategory: string;
	isAdding: boolean;
	isLoading: boolean;
	error: string | null;
	isTauriAvailable: boolean;
	loadMemories: () => Promise<void>;
	searchExistingMemories: () => Promise<void>;
	addNewMemory: (key: string, value: string, category: string) => Promise<boolean>;
	deleteExistingMemory: (key: string) => Promise<void>;
	setSearchQuery: (query: string) => void;
	setSelectedCategory: (category: string) => void;
	setIsAdding: (isAdding: boolean) => void;
	setError: (error: string | null) => void;
}

/// Zustand store for the memory list, category filter, and search state.
///
/// Refs: I-Ui-MemoryStore
export const useMemoryStore = create<MemoryStore>((set, get) => ({
	memories: [],
	searchQuery: "",
	selectedCategory: "all",
	isAdding: false,
	isLoading: false,
	error: null,
	isTauriAvailable: isTauri(),

	loadMemories: async () => {
		if (!get().isTauriAvailable) {
			set({ memories: [], error: null, isLoading: false });
			return;
		}
		set({ isLoading: true, error: null });
		try {
			const cat = get().selectedCategory;
			const data = cat === "all" ? await listMemories() : await listMemories(cat);
			set({ memories: data, isLoading: false });
		} catch (err: unknown) {
			console.error("Failed to load memories:", err);
			set({ error: String(err), isLoading: false });
		}
	},

	searchExistingMemories: async () => {
		const query = get().searchQuery.trim();
		if (!query) {
			await get().loadMemories();
			return;
		}
		if (!get().isTauriAvailable) {
			set({ error: "Memory search requires the Tauri desktop runtime." });
			return;
		}
		set({ isLoading: true, error: null });
		try {
			const results = await searchMemories(query);
			set({ memories: results, isLoading: false });
		} catch (err: unknown) {
			console.error("Failed to search memories:", err);
			set({ error: String(err), isLoading: false });
		}
	},

	addNewMemory: async (key, value, category) => {
		if (!get().isTauriAvailable) {
			set({ error: "Adding memories requires the Tauri desktop runtime." });
			return false;
		}
		set({ error: null });
		try {
			await setMemory(key, value, category);
			await get().loadMemories();
			return true;
		} catch (err: unknown) {
			console.error("Failed to add memory:", err);
			set({ error: String(err) });
			return false;
		}
	},

	deleteExistingMemory: async (key) => {
		if (!get().isTauriAvailable) {
			set({ error: "Deleting memories requires the Tauri desktop runtime." });
			return;
		}
		set({ error: null });
		try {
			await deleteMemory(key);
			await get().loadMemories();
		} catch (err: unknown) {
			console.error("Failed to delete memory:", err);
			set({ error: String(err) });
		}
	},

	setSearchQuery: (searchQuery) => set({ searchQuery }),
	setSelectedCategory: (category) => {
		set({ selectedCategory: category });
		void get().loadMemories();
	},
	setIsAdding: (isAdding) => set({ isAdding }),
	setError: (error) => set({ error }),
}));

