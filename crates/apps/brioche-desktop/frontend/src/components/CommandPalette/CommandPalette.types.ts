import type { ReactNode } from "react";

export interface Command {
	id: string;
	label: string;
	shortcut?: string;
	group: string;
	icon: ReactNode;
	action: () => void;
}

export interface CommandPaletteProps {
	isOpen: boolean;
	onClose: () => void;
	commands: Command[];
}
