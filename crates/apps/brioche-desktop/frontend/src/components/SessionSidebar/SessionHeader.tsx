import { PlusIcon } from "../Icons";

interface SessionHeaderProps {
	onNewSession: () => void;
}

export function SessionHeader({ onNewSession }: SessionHeaderProps) {
	return (
		<div className="flex h-13 shrink-0 items-center justify-between border-b border-border bg-bg-base/30 px-5 py-4 backdrop-blur-sm">
			<h2 className="select-none text-xs font-bold uppercase tracking-widest text-fg-muted">
				Sessions
			</h2>
			<button
				type="button"
				className="flex cursor-pointer items-center justify-center rounded-md border border-border bg-bg-highlight/50 p-1.5 text-fg-secondary shadow-sm transition-all duration-200 hover:border-accent-dim/40 hover:bg-bg-highlight hover:text-fg-primary"
				onClick={onNewSession}
				title="New session"
			>
				<span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
					<PlusIcon className="h-full w-full" />
				</span>
			</button>
		</div>
	);
}
