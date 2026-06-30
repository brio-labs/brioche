import { useCallback, useMemo } from "react";
import { useSessionStore } from "../stores/sessionStore";
import { PlusIcon, TrashIcon, MessageIcon } from "./Icons";
import { cn } from "./ui/lib";
import type { SessionSort } from "../ipc";

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

/// Extracts the last segment of a workspace path for display.
function workspaceName(workspace: string): string {
	if (!workspace) return "No workspace";
	const parts = workspace.split("/");
	return parts[parts.length - 1] || workspace;
}

/// Renders the session sidebar, listing sessions with grouping, sorting, and creation controls.
///
/// Refs: I-Ui-Sidebar
export default function SessionSidebar() {
	const {
		sessions,
		sortMode,
		setSortMode,
		switchToSession,
		deleteSession,
		createSession,
	} = useSessionStore();

	const handleNewSession = useCallback(async () => {
		await createSession();
	}, [createSession]);

	const groupedSessions = useMemo(() => {
		const sorted = [...sessions].sort((a, b) => {
			if (sortMode === "date") {
				return (b.created_at ?? 0) - (a.created_at ?? 0);
			}
			if (sortMode === "name") {
				return a.id.localeCompare(b.id);
			}
			return (a.workspace || "").localeCompare(b.workspace || "");
		});

		if (sortMode === "workspace") {
			const groups = new Map<string, typeof sorted>();
			for (const session of sorted) {
				const key = workspaceName(session.workspace || "");
				if (!groups.has(key)) {
					groups.set(key, []);
				}
				groups.get(key)!.push(session);
			}
			return groups;
		}

		const single = new Map<string, typeof sorted>();
		single.set(
			sortMode === "date" ? "Recent sessions" : "Sessions",
			sorted,
		);
		return single;
	}, [sessions, sortMode]);

	return (
		<div className="flex h-full w-full flex-col bg-transparent text-fg-primary">
			{/* Sidebar header */}
			<div className="flex h-13 shrink-0 items-center justify-between border-b border-border bg-bg-base/30 px-5 py-4 backdrop-blur-sm">
				<h2 className="select-none text-xs font-bold uppercase tracking-widest text-fg-muted">
					Sessions
				</h2>
				<button
					type="button"
					className="flex cursor-pointer items-center justify-center rounded-lg border border-border bg-bg-highlight/50 p-1.5 text-fg-secondary shadow-sm transition-all duration-200 hover:border-accent-dim/40 hover:bg-bg-highlight hover:text-fg-primary"
					onClick={handleNewSession}
					title="New session"
				>
					<span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
						<PlusIcon className="h-full w-full" />
					</span>
				</button>
			</div>

			{/* Sort controls */}
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

			{/* Session list */}
			<div className="flex flex-1 flex-col space-y-3 overflow-y-auto px-3 py-4">
				{sessions.length > 0 ? (
					Array.from(groupedSessions.entries()).map(([group, items]) => (
						<div key={group} className="space-y-1.5">
							{/* Group header */}
							<div className="mb-2 flex select-none items-center gap-2 px-4 text-xs font-bold uppercase tracking-widest text-fg-muted">
								<span>{group}</span>
								<div className="h-px flex-1 bg-border/30" />
							</div>

							{items.map((session) => (
								<div
									key={session.id}
									className={cn(
										"group relative flex cursor-pointer items-center justify-between rounded-lg border p-3 transition-all duration-200",
										session.active
											? "border-accent-dim/40 bg-bg-highlight/60 shadow-sm shadow-accent/5"
											: "border-transparent bg-transparent hover:border-border/60 hover:bg-bg-elevated/30",
									)}
									onClick={() => switchToSession(session.id)}
									title={session.workspace}
								>
									{/* Active indicator bar */}
									<div
										className={cn(
											"absolute left-0 top-1/2 w-0.75 -translate-y-1/2 rounded-r-full transition-all duration-200",
											session.active
												? "h-6 bg-accent shadow-[0_0_8px_var(--color-accent)]"
												: "h-0 bg-transparent",
										)}
									/>

									<div className="flex flex-1 min-w-0 items-start gap-2.5 pl-1">
										{/* Session icon */}
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
														{workspaceName(session.workspace)}
													</span>
												)}
											</div>
										</div>
									</div>

									{!session.active && (
										<button
											type="button"
											className="ml-2 cursor-pointer rounded-md border border-transparent p-1 text-fg-muted opacity-0 transition-all duration-200 hover:border-border hover:bg-bg-subtle hover:text-error-text group-hover:opacity-100"
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
							))}
						</div>
					))
				) : (
					<div className="flex flex-1 flex-col items-center justify-center px-5 py-12 text-center text-fg-muted select-none">
						<span className="text-xs">No sessions</span>
					</div>
				)}
			</div>
		</div>
	);
}
