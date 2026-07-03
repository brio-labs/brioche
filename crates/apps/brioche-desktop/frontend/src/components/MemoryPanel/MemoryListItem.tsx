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
		<div className="surface-card flex flex-col gap-1.5">
			<div className="flex items-center justify-between gap-2">
				<span className="font-mono text-xs font-semibold text-fg-primary">
					{memory.key}
				</span>
				<div className="flex items-center gap-2">
					<span className="rounded border border-accent/20 bg-accent/10 px-1.5 py-0.5 font-sans text-xs font-medium uppercase tracking-wider text-accent select-none">
						{memory.category}
					</span>
					<button
						type="button"
						className="flex shrink-0 cursor-pointer items-center justify-center rounded p-1.5 text-fg-muted transition-colors hover:bg-bg-highlight hover:text-error-text"
						onClick={() => onDelete(memory.key)}
						aria-label={`Delete memory ${memory.key}`}
					>
						<svg
							width="12"
							height="12"
							viewBox="0 0 12 12"
							fill="currentColor"
						>
							<path
								d="M3 3l6 6M3 9l6-6"
								stroke="currentColor"
								strokeWidth="1.5"
							/>
						</svg>
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
