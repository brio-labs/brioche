import type { Command } from "./CommandPalette.types";
import CommandPaletteItem from "./CommandPaletteItem";

interface CommandPaletteListProps {
	grouped: Record<string, Command[]>;
	selectedIndex: number;
	setSelectedIndex: (index: number) => void;
	onAction: () => void;
}

export default function CommandPaletteList({
	grouped,
	selectedIndex,
	setSelectedIndex,
	onAction,
}: CommandPaletteListProps) {
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
