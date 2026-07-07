import { create } from "zustand";
import type { ToolDescriptor } from "../ipc";
import { listTools, setToolEnabled, isTauri } from "../ipc";
import { useSettingsStore } from "./settingsStore";

const USER_TOOL_SOURCE = "user-json";

/// Returns true when the given tool was defined by the user.
///
/// Refs: I-Ui-UserTool
export function isUserTool(tool: ToolDescriptor): boolean {
	return tool.source === USER_TOOL_SOURCE;
}

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
