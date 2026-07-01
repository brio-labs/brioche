// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import FileExplorer from "./FileExplorer";
import { useFileStore } from "../stores/fileStore";
import { useSettingsStore } from "../stores/settingsStore";

vi.mock("../ipc", () => ({
	readDirectory: vi.fn(),
	readFile: vi.fn(),
	createFile: vi.fn(),
	deleteFile: vi.fn(),
	writeFile: vi.fn(),
	createDirectory: vi.fn(),
	getMessages: vi.fn(),
	getSettings: vi.fn(),
	setSettings: vi.fn(),
	listSettingsSections: vi.fn(),
	getSessions: vi.fn(),
	createSession: vi.fn(),
	getWorkingDir: vi.fn((settings: { ui?: { working_dir?: string } }) => settings.ui?.working_dir),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
	open: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock("./Tooltip", () => ({
	default: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));

function resetStores() {
	useFileStore.setState({
		currentPath: "",
		entries: [],
		isLoading: false,
	});
	useSettingsStore.setState({
		settings: {},
		sections: [],
		isLoading: false,
		loaded: false,
	});
}

describe("FileExplorer", () => {
	beforeEach(() => {
		resetStores();
		vi.clearAllMocks();
	});

	it("renders empty state when no workspace is set", () => {
		render(<FileExplorer />);
		expect(screen.getByText("No directory open")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Open Folder" })).toBeInTheDocument();
	});
});
