import { X } from "lucide-react";
import type { MemoryEntry } from "../../ipc";

interface MemoryListItemProps {
	memory: MemoryEntry;
	formatDate: (timestamp: number) => string;
	onDelete: (key: string) => void;
}

/// Renders a single memory entry card with a category badge and delete button.
///
/// Refs: I-Ui-MemoryPanel
export default function MemoryListItem({
	memory,
	formatDate,
	onDelete,
}: MemoryListItemProps) {
	return (
		<div className="flex flex-col gap-2 rounded-none border border-border bg-bg-elevated p-3">
			<div className="flex items-center justify-between gap-2">
				<span className="font-mono text-xs font-semibold text-fg-primary">
					{memory.key}
				</span>
				<div className="flex items-center gap-2">
					<span className="rounded-sm border border-border bg-bg-surface px-2 py-0.5 font-sans text-xs font-medium uppercase tracking-wider text-fg-secondary select-none">
						{memory.category}
					</span>
					<button
						type="button"
						className="flex shrink-0 cursor-pointer items-center justify-center rounded-md p-1 text-fg-muted transition-colors hover:bg-bg-highlight hover:text-error-text"
						onClick={() => onDelete(memory.key)}
						aria-label={`Delete memory ${memory.key}`}
					>
						<X className="h-3.5 w-3.5" />
					</button>
				</div>
			</div>
			<div className="px-0.5 text-sm leading-relaxed whitespace-pre-wrap text-fg-secondary">
				{memory.value}
			</div>
			<div className="px-0.5 text-xs text-fg-dim select-none">
				Updated: {formatDate(memory.updated_at)} | Accessed:{" "}
				{memory.access_count} times
			</div>
		</div>
	);
}
