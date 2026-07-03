import type { SessionSort } from "../../ipc";

interface SessionSortControlsProps {
	sortMode: SessionSort;
	setSortMode: (sort: SessionSort) => void;
}

export function SessionSortControls({ sortMode, setSortMode }: SessionSortControlsProps) {
	return (
		<div className="flex select-none items-center gap-3 border-b border-border bg-bg-base/50 px-5 py-4 backdrop-blur-sm">
			<label
				htmlFor="session-sort"
				className="shrink-0 select-none text-xs font-bold uppercase tracking-widest text-fg-muted"
			>
				Sort By
			</label>
			<div className="relative flex-1">
				<select
					id="session-sort"
					value={sortMode}
					onChange={(e) => setSortMode(e.target.value as SessionSort)}
					className="w-full cursor-pointer appearance-none rounded-md border border-border/80 bg-bg-elevated/40 px-2.5 py-1 text-xs font-medium text-fg-secondary shadow-sm outline-none transition-all duration-200 hover:bg-bg-elevated/80 hover:text-fg-primary focus:border-accent-dim/60 focus:ring-1 focus:ring-accent-dim/30"
				>
					<option value="date" className="bg-bg-surface text-fg-primary">
						Date Created
					</option>
					<option value="workspace" className="bg-bg-surface text-fg-primary">
						Workspace
					</option>
					<option value="name" className="bg-bg-surface text-fg-primary">
						Session Name
					</option>
				</select>
			</div>
		</div>
	);
}
