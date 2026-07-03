import { cn } from "../ui/lib";
import type { Command } from "./CommandPalette.types";

interface CommandPaletteItemProps {
	cmd: Command;
	isSelected: boolean;
	onMouseEnter: () => void;
	onAction: () => void;
}

export default function CommandPaletteItem({
	cmd,
	isSelected,
	onMouseEnter,
	onAction,
}: CommandPaletteItemProps) {
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
