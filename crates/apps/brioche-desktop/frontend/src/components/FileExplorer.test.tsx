// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import "@testing-library/jest-dom/vitest";
import FileExplorer from "./FileExplorer";
import { useFileStore } from "../stores/fileStore";
import { useSettingsStore } from "../stores/settingsStore";
import { readDirectory } from "../ipc";

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
	isTauri: vi.fn(() => false),
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
	cleanup();
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

	it("shows a notice when Open Folder is clicked without Tauri", async () => {
		render(<FileExplorer />);
		const user = userEvent.setup();
		const button = screen.getByRole("button", { name: "Open Folder" });
		await user.click(button);
		expect(
			screen.getByText("Folder picker requires the Tauri desktop app."),
		).toBeInTheDocument();
	});

	it("shows the file context menu on right-click", async () => {
		vi.mocked(readDirectory).mockResolvedValue([
			{ name: "file.txt", path: "/workspace/file.txt", is_dir: false },
		]);
		useSettingsStore.setState({
			settings: { ui: { working_dir: "/workspace" } },
		});
		useFileStore.setState({
			currentPath: "/workspace",
			entries: [
				{ name: "file.txt", path: "/workspace/file.txt", is_dir: false },
			],
		});

		render(<FileExplorer />);

		const row = await screen.findByText("file.txt");
		fireEvent.contextMenu(row);

		await waitFor(() => {
			expect(screen.getByRole("menu")).toBeInTheDocument();
		});
		expect(screen.getByText("Rename")).toBeInTheDocument();
		expect(screen.getByText("Copy")).toBeInTheDocument();
		expect(screen.getByText("Cut")).toBeInTheDocument();
		expect(screen.getByText("Delete")).toBeInTheDocument();
		const paste = screen.getByText("Paste");
		expect(paste).toBeInTheDocument();
		expect(paste).toHaveAttribute("aria-disabled", "true");
	});

	it("shows the folder context menu on right-click", async () => {
		vi.mocked(readDirectory).mockResolvedValue([
			{ name: "folder", path: "/workspace/folder", is_dir: true },
		]);
		useSettingsStore.setState({
			settings: { ui: { working_dir: "/workspace" } },
		});
		useFileStore.setState({
			currentPath: "/workspace",
			entries: [
				{ name: "folder", path: "/workspace/folder", is_dir: true },
			],
		});

		render(<FileExplorer />);

		const row = await screen.findByText("folder");
		fireEvent.contextMenu(row);

		await waitFor(() => {
			expect(screen.getByRole("menu")).toBeInTheDocument();
		});
		expect(screen.getByText("New File")).toBeInTheDocument();
		expect(screen.getByText("New Folder")).toBeInTheDocument();
		expect(screen.getByText("Rename")).toBeInTheDocument();
		expect(screen.getByText("Copy")).toBeInTheDocument();
		expect(screen.getByText("Cut")).toBeInTheDocument();
		expect(screen.getByText("Delete")).toBeInTheDocument();
		const paste = screen.getByText("Paste");
		expect(paste).toBeInTheDocument();
		expect(paste).toHaveAttribute("aria-disabled", "true");
	});

	it("shows the empty-area context menu on right-click", async () => {
		vi.mocked(readDirectory).mockResolvedValue([]);
		useSettingsStore.setState({
			settings: { ui: { working_dir: "/workspace" } },
		});
		useFileStore.setState({
			currentPath: "/workspace",
			entries: [],
		});

		render(<FileExplorer />);

		const empty = await screen.findByText("Folder is empty");
		fireEvent.contextMenu(empty);

		await waitFor(() => {
			expect(screen.getByRole("menu")).toBeInTheDocument();
		});
		expect(screen.getByText("New File")).toBeInTheDocument();
		expect(screen.getByText("New Folder")).toBeInTheDocument();
	});
});
