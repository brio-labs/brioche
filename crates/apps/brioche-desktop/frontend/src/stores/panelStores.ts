import { create } from "zustand";
import type { Skill, MemoryEntry, ToolDescriptor } from "../ipc";
import {
	listSkills,
	getSkillContent,
	setSkillEnabled,
	createSkill,
	deleteSkill,
	listMemories,
	setMemory,
	deleteMemory,
	searchMemories,
	listTools,
	setToolEnabled,
	isTauri,
} from "../ipc";
import { useSettingsStore } from "./settingsStore";

const USER_TOOL_SOURCE = "user-json";

/// Returns true when the given tool was defined by the user.
///
/// Refs: I-Ui-UserTool
export function isUserTool(tool: ToolDescriptor): boolean {
	return tool.source === USER_TOOL_SOURCE;
}

// ============================================================================
// 1. Skills State Store
// ============================================================================

/// State and actions for the skills panel.
///
/// Refs: I-Ui-SkillsStore
interface SkillsStore {
	skills: Skill[];
	selectedSkill: Skill | null;
	skillContent: string;
	searchQuery: string;
	categoryFilter: string | null;
	isLoading: boolean;
	error: string | null;
	showCreate: boolean;
	isTauriAvailable: boolean;
	loadSkills: () => Promise<void>;
	selectSkill: (skill: Skill | null) => Promise<void>;
	toggleSkillEnabled: (skill: Skill) => Promise<void>;
	createNewSkill: (name: string, category: string, description: string, content: string) => Promise<boolean>;
	deleteExistingSkill: (skill: Skill) => Promise<boolean>;
	setSearchQuery: (query: string) => void;
	setCategoryFilter: (category: string | null) => void;
	setShowCreate: (show: boolean) => void;
	setError: (error: string | null) => void;
}

/// Zustand store for the skills list, selected skill content, and skill lifecycle.
///
/// Refs: I-Ui-SkillsStore
export const useSkillsStore = create<SkillsStore>((set, get) => ({
	skills: [],
	selectedSkill: null,
	skillContent: "",
	searchQuery: "",
	categoryFilter: null,
	isLoading: false,
	error: null,
	showCreate: false,
	isTauriAvailable: isTauri(),

	loadSkills: async () => {
		if (!get().isTauriAvailable) {
			set({ skills: [], error: null, isLoading: false });
			return;
		}
		set({ isLoading: true, error: null });
		try {
			const data = await listSkills();
			set({ skills: data, isLoading: false });
		} catch (err: unknown) {
			console.error("Failed to load skills:", err);
			set({ error: String(err), isLoading: false });
		}
	},

	selectSkill: async (skill) => {
		set({ selectedSkill: skill, error: null });
		if (!skill) {
			set({ skillContent: "" });
			return;
		}
		if (!get().isTauriAvailable) {
			set({ skillContent: "Skill preview requires the Tauri desktop runtime." });
			return;
		}
		try {
			const content = await getSkillContent(skill.name);
			set({ skillContent: content });
		} catch (err: unknown) {
			console.error("Failed to load skill content:", err);
			set({ skillContent: `Error loading skill: ${err}` });
		}
	},

	toggleSkillEnabled: async (skill) => {
		if (!get().isTauriAvailable) {
			set({ error: "Enabling/disabling skills requires the Tauri desktop runtime." });
			return;
		}
		set({ error: null });
		try {
			const newStatus = !skill.enabled;
			await setSkillEnabled(skill.name, newStatus);
			await get().loadSkills();
			const currentSelected = get().selectedSkill;
			if (currentSelected?.name === skill.name) {
				set({ selectedSkill: { ...currentSelected, enabled: newStatus } });
			}
		} catch (err: unknown) {
			console.error("Failed to toggle skill:", err);
			set({ error: String(err) });
		}
	},

	createNewSkill: async (name, category, description, content) => {
		if (!get().isTauriAvailable) {
			set({ error: "Creating skills requires the Tauri desktop runtime." });
			return false;
		}
		set({ error: null });
		try {
			await createSkill(name, category, description, content);
			await get().loadSkills();
			return true;
		} catch (err: unknown) {
			console.error("Failed to create skill:", err);
			set({ error: String(err) });
			return false;
		}
	},

	deleteExistingSkill: async (skill) => {
		if (!get().isTauriAvailable) {
			set({ error: "Deleting skills requires the Tauri desktop runtime." });
			return false;
		}
		set({ error: null });
		try {
			await deleteSkill(skill.name);
			if (get().selectedSkill?.name === skill.name) {
				set({ selectedSkill: null, skillContent: "" });
			}
			await get().loadSkills();
			return true;
		} catch (err: unknown) {
			console.error("Failed to delete skill:", err);
			set({ error: String(err) });
			return false;
		}
	},

	setSearchQuery: (searchQuery) => set({ searchQuery }),
	setCategoryFilter: (categoryFilter) => set({ categoryFilter }),
	setShowCreate: (showCreate) => set({ showCreate }),
	setError: (error) => set({ error }),
}));

// ============================================================================
// 2. Memory State Store
// ============================================================================

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

// ============================================================================
// 3. Tools State Store
// ============================================================================

/// State and actions for the tools panel.
///
/// Refs: I-Ui-ToolsStore
interface ToolsStore {
	tools: ToolDescriptor[];
	isLoading: boolean;
	error: string | null;
	isTauriAvailable: boolean;
	userToolsEnabled: boolean;
	loadTools: () => Promise<void>;
	toggleTool: (id: string, enabled: boolean) => Promise<void>;
}

/// Zustand store for the tool list and the user-defined tool security gate.
///
/// Refs: I-Ui-ToolsStore
export const useToolsStore = create<ToolsStore>((set, get) => {
	useSettingsStore.subscribe(() => {
		set({
			userToolsEnabled:
				useSettingsStore.getState().getSetting("tools.user_tools_enabled") === true,
		});
	});
	set({
		userToolsEnabled:
			useSettingsStore.getState().getSetting("tools.user_tools_enabled") === true,
	});

	return {
		tools: [],
		isLoading: false,
		error: null,
		isTauriAvailable: isTauri(),
		userToolsEnabled: false,

		loadTools: async () => {
			if (!get().isTauriAvailable) {
				set({ tools: [], error: null, isLoading: false });
				return;
			}
			set({ isLoading: true, error: null });
			try {
				const data = await listTools();
				set({ tools: data, isLoading: false });
			} catch (err: unknown) {
				console.error("Failed to load tools:", err);
				set({ error: String(err), isLoading: false });
			}
		},

		toggleTool: async (id, enabled) => {
			if (!get().isTauriAvailable) {
				set({ error: "Tool toggling requires the Tauri desktop runtime." });
				return;
			}
			set({ error: null });
			try {
				await setToolEnabled(id, enabled);
				await get().loadTools();
			} catch (err: unknown) {
				console.error("Failed to toggle tool:", err);
				set({ error: String(err) });
			}
		},
	};
});
