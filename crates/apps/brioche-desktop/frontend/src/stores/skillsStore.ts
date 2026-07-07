import { create } from "zustand";
import type { Skill } from "../ipc";
import {
	listSkills,
	getSkillContent,
	setSkillEnabled,
	createSkill,
	deleteSkill,
	isTauri,
} from "../ipc";

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

