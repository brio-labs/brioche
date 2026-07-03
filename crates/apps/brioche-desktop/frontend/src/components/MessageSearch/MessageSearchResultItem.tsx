import { cn } from "../ui/lib";

export interface MessageSearchResultItemProps {
	result: { id: string; role: string; preview: string };
	isSelected: boolean;
	onSelect: () => void;
	onHover: () => void;
}

/**
 * A single message search result row.
 *
 * Refs: I-Ui-OverlayCohesion
 */
export function MessageSearchResultItem({
	result,
	isSelected,
	onSelect,
	onHover,
}: MessageSearchResultItemProps) {
	return (
		<div
			className={cn(
				"flex cursor-pointer items-start gap-3 rounded-sm px-4 py-4 text-sm text-fg-secondary transition-all hover:bg-accent/10 hover:text-fg-primary",
				isSelected &&
					"border-l-2 border-accent bg-accent/10 text-fg-primary",
			)}
			onClick={onSelect}
			onMouseEnter={onHover}
		>
			<div className="w-12 shrink-0 pt-0.5">
				<span className="text-xs font-bold uppercase tracking-wider text-fg-muted">
					{result.role}
				</span>
			</div>
			<div className="flex-1 leading-snug">{result.preview}</div>
		</div>
	);
}
