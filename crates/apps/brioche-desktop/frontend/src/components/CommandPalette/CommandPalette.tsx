import { useState, useCallback, useEffect, useRef, useMemo } from "react";
import { ModalOverlay } from "../PanelOverlay";
import CommandPaletteInput from "./CommandPaletteInput";
import CommandPaletteList from "./CommandPaletteList";
import type { Command, CommandPaletteProps } from "./CommandPalette.types";

export type { Command, CommandPaletteProps };

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
			<CommandPaletteInput
				query={query}
				onChange={setQuery}
				onKeyDown={handleKeyDown}
				inputRef={inputRef}
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
