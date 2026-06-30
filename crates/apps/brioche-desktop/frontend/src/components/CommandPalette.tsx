import { useState, useCallback, useEffect, useRef, useMemo } from "react";
import {
	MessageIcon,
	SettingsIcon,
	TerminalIcon,
	PlusIcon,
	ExportIcon,
	XIcon,
} from "./Icons";
import { ModalOverlay, ModalSearchHeader } from "./PanelOverlay";
import { cn } from "./ui/lib";

/**
 * A single command that can be triggered from the command palette.
 *
 * Refs: I-Ui-OverlayCohesion
 */
interface Command {
	id: string;
	label: string;
	shortcut?: string;
	group: string;
	icon: React.ReactNode;
	action: () => void;
}

/**
 * Props for the command palette overlay.
 *
 * Refs: I-Ui-OverlayCohesion
 */
interface CommandPaletteProps {
	isOpen: boolean;
	onClose: () => void;
	commands: Command[];
}

/**
 * Renders a searchable command palette overlay for quick navigation.
 *
 * Refs: I-Ui-OverlayCohesion
 */
export default function CommandPalette({
	isOpen,
	onClose,
	commands,
}: CommandPaletteProps) {
	const [query, setQuery] = useState("");
	const [selectedIndex, setSelectedIndex] = useState(0);
	const inputRef = useRef<HTMLInputElement>(null);

	const filtered = useMemo(() => {
		if (!query.trim()) return commands;
		const q = query.toLowerCase();
		return commands.filter(
			(c) =>
				c.label.toLowerCase().includes(q) ||
				c.group.toLowerCase().includes(q) ||
				(c.shortcut && c.shortcut.toLowerCase().includes(q)),
		);
	}, [query, commands]);

	const grouped = useMemo(() => {
		const groups: Record<string, Command[]> = {};
		filtered.forEach((cmd) => {
			if (!groups[cmd.group]) groups[cmd.group] = [];
			groups[cmd.group].push(cmd);
		});
		return groups;
	}, [filtered]);

	useEffect(() => {
		if (isOpen) {
			setQuery("");
			setSelectedIndex(0);
			setTimeout(() => inputRef.current?.focus(), 50);
		}
	}, [isOpen]);

	useEffect(() => {
		setSelectedIndex(0);
	}, [query]);

	const handleKeyDown = useCallback(
		(e: React.KeyboardEvent) => {
			if (e.key === "Escape") {
				onClose();
				return;
			}
			if (e.key === "Enter") {
				const flat = Object.values(grouped).flat();
				if (flat[selectedIndex]) {
					flat[selectedIndex].action();
					onClose();
				}
				return;
			}
			if (e.key === "ArrowDown") {
				e.preventDefault();
				const total = Object.values(grouped).flat().length;
				setSelectedIndex((i) => (i + 1) % total);
				return;
			}
			if (e.key === "ArrowUp") {
				e.preventDefault();
				const total = Object.values(grouped).flat().length;
				setSelectedIndex((i) => (i - 1 + total) % total);
				return;
			}
		},
		[grouped, selectedIndex, onClose],
	);

	let flatIndex = 0;

	return (
		<ModalOverlay isOpen={isOpen} onClose={onClose}>
			<ModalSearchHeader
				query={query}
				onChange={setQuery}
				onKeyDown={handleKeyDown}
				inputRef={inputRef}
				placeholder="Type a command or search..."
			/>
			<div className="flex-1 min-h-0 overflow-y-auto p-4">
				{Object.entries(grouped).map(([groupName, items]) => (
					<div key={groupName} className="mb-3">
						<div className="select-none px-3 py-2 text-xs font-bold uppercase tracking-wider text-fg-muted">
							{groupName}
						</div>
						{items.map((cmd) => {
							const isSelected = flatIndex === selectedIndex;
							const idx = flatIndex++;
							return (
								<div
									key={cmd.id}
									className={cn(
										"flex cursor-pointer items-center gap-3 rounded-sm px-4 py-3 text-sm text-fg-secondary transition-all hover:bg-accent/10 hover:text-fg-primary",
										isSelected &&
											"border-l-2 border-accent bg-accent/10 text-fg-primary",
									)}
									onClick={() => {
										cmd.action();
										onClose();
									}}
									onMouseEnter={() => setSelectedIndex(idx)}
								>
									<div className="w-5 h-5 flex items-center justify-center shrink-0 text-fg-muted [&_svg]:w-3.5 [&_svg]:h-3.5">
										{cmd.icon}
									</div>
									<div className="flex-1">{cmd.label}</div>
									{cmd.shortcut && (
										<div className="rounded bg-bg-highlight px-1.5 py-0.5 font-mono text-xs text-fg-muted">
											{cmd.shortcut}
										</div>
									)}
								</div>
							);
						})}
					</div>
				))}
				{filtered.length === 0 && (
					<div className="p-8 text-center text-sm text-fg-muted">
						No commands found
					</div>
				)}
			</div>
		</ModalOverlay>
	);
}

/**
 * Builds the default command list from high-level application handlers.
 *
 * Refs: I-Ui-OverlayCohesion
 */
export function buildCommands(handlers: {
	newSession: () => void;
	clearChat: () => void;
	openSettings: () => void;
	toggleSessions: () => void;
	toggleFiles: () => void;
	exportChat: () => void;
}): Command[] {
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
