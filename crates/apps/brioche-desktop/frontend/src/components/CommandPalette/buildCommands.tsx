import {
	Plus,
	X,
	Download,
	MessageSquare,
	Terminal,
	Settings,
} from "lucide-react";
import type { Command } from "./CommandPalette.types";

export interface BuildCommandsHandlers {
	newSession: () => void;
	clearChat: () => void;
	openSettings: () => void;
	toggleSessions: () => void;
	toggleFiles: () => void;
	exportChat: () => void;
}

export function buildCommands(handlers: BuildCommandsHandlers): Command[] {
	return [
		{
			id: "new-session",
			label: "New Session",
			shortcut: "⌘N",
			group: "Session",
			icon: <Plus />,
			action: handlers.newSession,
		},
		{
			id: "clear-chat",
			label: "Clear Chat",
			shortcut: "⌘K C",
			group: "Session",
			icon: <X />,
			action: handlers.clearChat,
		},
		{
			id: "export-chat",
			label: "Export Chat",
			shortcut: "⌘E",
			group: "Session",
			icon: <Download />,
			action: handlers.exportChat,
		},
		{
			id: "toggle-sessions",
			label: "Toggle Sessions Panel",
			shortcut: "⌘B",
			group: "View",
			icon: <MessageSquare />,
			action: handlers.toggleSessions,
		},
		{
			id: "toggle-files",
			label: "Toggle Files Panel",
			shortcut: "⌘J",
			group: "View",
			icon: <Terminal />,
			action: handlers.toggleFiles,
		},
		{
			id: "open-settings",
			label: "Settings",
			shortcut: "⌘,",
			group: "Settings",
			icon: <Settings />,
			action: handlers.openSettings,
		},
	];
}
