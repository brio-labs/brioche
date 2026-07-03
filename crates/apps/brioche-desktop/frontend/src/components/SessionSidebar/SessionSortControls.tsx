import type { SessionSort } from "../../ipc";

interface SessionSortControlsProps {
	sortMode: SessionSort;
	setSortMode: (sort: SessionSort) => void;
}

export function SessionSortControls({ sortMode, setSortMode }: SessionSortControlsProps) {
	return (
		<div className="flex select-none items-center gap-3 border-b border-border bg-bg-surface px-4 py-3">
			<label
				htmlFor="session-sort"
				className="shrink-0 select-none text-xs font-medium text-fg-muted"
			>
				Sort By
			</label>
			<div className="relative flex-1">
				<select
					id="session-sort"
					value={sortMode}
					onChange={(e) => setSortMode(e.target.value as SessionSort)}
					className="w-full cursor-pointer appearance-none rounded-md border border-border bg-bg-elevated px-2 py-1 text-xs font-medium text-fg-secondary shadow-sm outline-none transition-all duration-200 hover:bg-bg-highlight hover:text-fg-primary focus:border-accent-dim/60 focus:ring-1 focus:ring-accent-dim/30"
				>
					<option value="date" className="bg-bg-surface text-fg-primary">
						Date
					</option>
					<option value="name" className="bg-bg-surface text-fg-primary">
						Name
					</option>
				</select>
			</div>
		</div>
	);
}
