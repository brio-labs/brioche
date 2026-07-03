import { cn } from "../ui/lib";
import { TrashIcon } from "../Icons";
import type { Session } from "../../stores/sessionStore";

/// Formats a Unix timestamp as a short relative string.
/// Examples: "now", "1m", "2h", "3d".
function formatTimeSince(timestamp: number): string {
	if (!timestamp) return "unknown";
	const seconds = Math.floor((Date.now() - timestamp * 1000) / 1000);
	if (seconds < 60) return "now";
	const minutes = Math.floor(seconds / 60);
	if (minutes < 60) return `${minutes}m`;
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return `${hours}h`;
	const days = Math.floor(hours / 24);
	return `${days}d`;
}

interface SessionItemProps {
	session: Session;
	switchToSession: (id: string) => Promise<void>;
	deleteSession: (id: string) => Promise<void>;
}

export function SessionItem({ session, switchToSession, deleteSession }: SessionItemProps) {
	const recency = session.updated_at ?? session.created_at ?? 0;

	return (
		<div
			className={cn(
				"group relative flex cursor-pointer items-center justify-between p-3 transition-all duration-200",
				session.active
					? "bg-bg-highlight/60"
					: "bg-transparent hover:bg-bg-elevated/30",
			)}
			onClick={() => switchToSession(session.id)}
			title={session.workspace}
		>
			<div
				className={cn(
					"absolute left-0 top-1/2 w-0.75 -translate-y-1/2 rounded-r-full transition-all duration-200",
					session.active
						? "h-6 bg-accent shadow-[0_0_8px_var(--color-accent)]"
						: "h-0 bg-transparent",
				)}
			/>

			<div className="flex flex-1 min-w-0 items-center gap-2 pl-1">
				<div
					className={cn(
						"truncate text-xs font-semibold transition-colors",
						session.active
							? "text-fg-primary"
							: "text-fg-secondary group-hover:text-fg-primary",
					)}
				>
					{session.id}
				</div>
			</div>

			<div className="flex shrink-0 items-center gap-2">
				<span className="font-mono text-[10px] leading-none text-fg-dim select-none">
					{formatTimeSince(recency)}
				</span>

				{!session.active && (
					<button
						type="button"
						className="cursor-pointer p-1 text-fg-muted opacity-0 transition-all duration-200 hover:bg-bg-subtle hover:text-error-text group-hover:opacity-100"
						onClick={(e) => {
							e.stopPropagation();
							deleteSession(session.id);
						}}
						title="Delete session"
					>
						<span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
							<TrashIcon className="h-full w-full" />
						</span>
					</button>
				)}
			</div>
		</div>
	);
}
