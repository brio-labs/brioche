import { cn } from "../ui/lib";
import { MessageIcon, TrashIcon } from "../Icons";
import type { Session } from "../../stores/sessionStore";

/// Formats a Unix timestamp as a localized, human-readable date string.
function formatDate(timestamp: number): string {
	if (!timestamp) return "unknown";
	const date = new Date(timestamp * 1000);
	return date.toLocaleDateString(undefined, {
		month: "short",
		day: "numeric",
		hour: "2-digit",
		minute: "2-digit",
	});
}

interface SessionItemProps {
	session: Session;
	switchToSession: (id: string) => Promise<void>;
	deleteSession: (id: string) => Promise<void>;
}

export function SessionItem({ session, switchToSession, deleteSession }: SessionItemProps) {
	const workspace = session.workspace || "";
	const workspaceDisplay = workspace.split("/").pop() || workspace;

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

			<div className="flex flex-1 min-w-0 items-start gap-2.5 pl-1">
				<div
					className={cn(
						"mt-0.5 h-3.5 w-3.5 shrink-0 transition-colors",
						session.active
							? "text-accent"
							: "text-fg-muted group-hover:text-fg-secondary",
					)}
				>
					<MessageIcon className="h-full w-full" />
				</div>

				<div className="flex flex-1 min-w-0 flex-col">
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

					<div className="mt-0.5 flex flex-col items-start gap-1">
						<span className="font-mono text-xs leading-none text-fg-muted">
							{formatDate(session.created_at ?? 0)}
						</span>

						{session.workspace && (
							<span className="inline-flex max-w-full items-center truncate rounded border border-border bg-bg-subtle px-1.5 py-0.5 font-mono text-xs font-medium leading-none text-fg-tertiary select-none">
								{workspaceDisplay}
							</span>
						)}
					</div>
				</div>
			</div>

			{!session.active && (
				<button
					type="button"
					className="ml-2 cursor-pointer p-1 text-fg-muted opacity-0 transition-all duration-200 hover:bg-bg-subtle hover:text-error-text group-hover:opacity-100"
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
	);
}
