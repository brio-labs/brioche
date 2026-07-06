import { useState, useCallback, useEffect, useRef, useMemo } from "react";
import { ModalOverlay, ModalSearchHeader } from "../PanelOverlay";
import { cn } from "../ui/lib";
import type { Command, CommandPaletteProps } from "./CommandPalette.types";

export type { Command, CommandPaletteProps };

function CommandPaletteItem({
	cmd,
	isSelected,
	onMouseEnter,
	onAction,
}: {
	cmd: Command;
	isSelected: boolean;
	onMouseEnter: () => void;
	onAction: () => void;
}) {
	return (
		<div
			className={cn(
				"flex cursor-pointer items-center gap-3 rounded-sm px-4 py-3 text-sm text-fg-secondary transition-all hover:bg-bg-elevated hover:text-fg-primary",
				isSelected &&
					"border-l-2 border-accent bg-bg-highlight text-fg-primary",
			)}
			onClick={() => {
				cmd.action();
				onAction();
			}}
			onMouseEnter={onMouseEnter}
		>
			<div className="w-5 h-5 flex items-center justify-center shrink-0 text-fg-muted [&_svg]:w-3.5 [&_svg]:h-3.5">
				{cmd.icon}
			</div>
			<div className="flex-1">{cmd.label}</div>
			{cmd.shortcut && (
				<div className="rounded-sm bg-bg-highlight px-2 py-0.5 font-mono text-xs text-fg-muted">
					{cmd.shortcut}
				</div>
			)}
		</div>
	);
}

function CommandPaletteList({
	grouped,
	selectedIndex,
	setSelectedIndex,
	onAction,
}: {
	grouped: Record<string, Command[]>;
	selectedIndex: number;
	setSelectedIndex: (index: number) => void;
	onAction: () => void;
}) {
	let flatIndex = 0;
	const flatCount = Object.values(grouped).flat().length;

	return (
		<div className="flex-1 min-h-0 overflow-y-auto p-4">
			{Object.entries(grouped).map(([groupName, items]) => (
				<div key={groupName} className="mb-3">
					<div className="select-none px-3 py-2 text-xs font-medium text-fg-muted">
						{groupName}
					</div>
					{items.map((cmd) => {
						const idx = flatIndex++;
						return (
							<CommandPaletteItem
								key={cmd.id}
								cmd={cmd}
								isSelected={idx === selectedIndex}
								onMouseEnter={() => setSelectedIndex(idx)}
								onAction={onAction}
							/>
						);
					})}
				</div>
			))}
			{flatCount === 0 && (
				<div className="p-8 text-center text-sm text-fg-muted">
					No commands found
				</div>
			)}
		</div>
	);
}

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

	return (
		<ModalOverlay isOpen={isOpen} onClose={onClose}>
			<ModalSearchHeader
				query={query}
				onChange={setQuery}
				onKeyDown={handleKeyDown}
				inputRef={inputRef}
				placeholder="Type a command or search..."
			/>
			<CommandPaletteList
				grouped={grouped}
				selectedIndex={selectedIndex}
				setSelectedIndex={setSelectedIndex}
				onAction={onClose}
			/>
		</ModalOverlay>
	);
}
