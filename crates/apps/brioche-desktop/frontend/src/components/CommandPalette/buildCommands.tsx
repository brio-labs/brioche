import {
	MessageIcon,
	SettingsIcon,
	TerminalIcon,
	PlusIcon,
	ExportIcon,
	XIcon,
} from "../Icons";
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
			icon: <PlusIcon />,
			action: handlers.newSession,
		},
		{
			id: "clear-chat",
			label: "Clear Chat",
			shortcut: "⌘K C",
			group: "Session",
			icon: <XIcon />,
			action: handlers.clearChat,
		},
		{
			id: "export-chat",
			label: "Export Chat",
			shortcut: "⌘E",
			group: "Session",
			icon: <ExportIcon />,
			action: handlers.exportChat,
		},
		{
			id: "toggle-sessions",
			label: "Toggle Sessions Panel",
			shortcut: "⌘B",
			group: "View",
			icon: <MessageIcon />,
			action: handlers.toggleSessions,
		},
		{
			id: "toggle-files",
			label: "Toggle Files Panel",
			shortcut: "⌘J",
			group: "View",
			icon: <TerminalIcon />,
			action: handlers.toggleFiles,
		},
		{
			id: "open-settings",
			label: "Settings",
			shortcut: "⌘,",
			group: "Settings",
			icon: <SettingsIcon />,
			action: handlers.openSettings,
		},
	];
}
