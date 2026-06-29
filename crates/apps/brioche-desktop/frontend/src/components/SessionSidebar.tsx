import { useCallback, useMemo } from "react";
import { useSessionStore } from "../stores/sessionStore";
import { PlusIcon, TrashIcon, MessageIcon } from "./Icons";
import type { SessionSort } from "../ipc";

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

function workspaceName(workspace: string): string {
	if (!workspace) return "No workspace";
	const parts = workspace.split("/");
	return parts[parts.length - 1] || workspace;
}

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
		single.set(sortMode === "date" ? "Recent sessions" : "Sessions", sorted);
		return single;
	}, [sessions, sortMode]);

	return (
		<div className="flex flex-col h-full w-full bg-transparent text-text-primary">
			{/* Sleek Sidebar Header */}
			<div className="flex items-center justify-between px-4 py-3 border-b border-border h-[52px] shrink-0 bg-bg-0/30 backdrop-blur-sm">
				<h2 className="text-[11px] font-bold tracking-[0.14em] uppercase text-text-muted select-none">Sessions</h2>
				<button
					type="button"
					className="p-1.5 bg-bg-3/50 hover:bg-bg-3 border border-border hover:border-accent-dim/40 rounded-lg text-text-secondary hover:text-text-primary transition-all duration-200 cursor-pointer shadow-sm flex items-center justify-center"
					onClick={handleNewSession}
					title="New session"
				>
					<span className="w-3.5 h-3.5 flex items-center justify-center shrink-0">
						<PlusIcon className="w-full h-full" />
					</span>
				</button>
			</div>

			{/* Elegant Sort Section */}
			<div className="flex items-center gap-[var(--space-3)] px-[var(--space-4)] py-[var(--space-3)] border-b border-border bg-bg-0/50 backdrop-blur-sm select-none">
				<label htmlFor="session-sort" className="text-[9px] font-bold uppercase tracking-[0.1em] text-text-muted select-none shrink-0">Sort By</label>
				<div className="relative flex-1">
					<select
						id="session-sort"
						value={sortMode}
						onChange={(e) => setSortMode(e.target.value as SessionSort)}
						className="w-full bg-bg-2/40 hover:bg-bg-2/80 border border-border/80 text-text-secondary hover:text-text-primary px-2.5 py-1 text-[11px] font-medium outline-none cursor-pointer appearance-none pr-7 transition-all duration-200 focus:border-accent-dim/60 focus:ring-1 focus:ring-accent-dim/30 shadow-sm rounded-md"
					>
						<option value="date" className="bg-bg-1 text-text-primary">Date Created</option>
						<option value="workspace" className="bg-bg-1 text-text-primary">Workspace</option>
						<option value="name" className="bg-bg-1 text-text-primary">Session Name</option>
					</select>
				</div>
			</div>

			{/* Session Cards list */}
			<div className="flex-1 overflow-y-auto py-3 space-y-3 flex flex-col">
				{sessions.length > 0 ? (
					Array.from(groupedSessions.entries()).map(([group, items]) => (
						<div key={group} className="space-y-1.5">
							{/* Header with horizontal separator line */}
							<div className="px-4 text-[10px] font-bold uppercase tracking-[0.12em] text-text-muted select-none mb-1 flex items-center gap-2">
								<span>{group}</span>
								<div className="flex-1 h-[1px] bg-border/30"></div>
							</div>
							
							{items.map((session) => (
								<div
									key={session.id}
									className={`group relative flex items-center justify-between p-[var(--space-3)] mx-[var(--space-2)] rounded-lg cursor-pointer transition-all duration-200 border ${
										session.active 
											? "bg-bg-3/60 border-accent-dim/40 shadow-sm shadow-accent/5" 
											: "bg-transparent border-transparent hover:bg-bg-2/30 hover:border-border/60"
									}`}
									onClick={() => switchToSession(session.id)}
									title={session.workspace}
								>
									{/* Indicator bar left */}
									<div className={`absolute left-0 top-1/2 -translate-y-1/2 w-[3px] rounded-r-full transition-all duration-200 ${
										session.active ? "h-6 bg-accent shadow-[0_0_8px_var(--color-accent)]" : "h-0 bg-transparent"
									}`} />
									
									<div className="flex-1 flex items-start gap-2.5 min-w-0 pl-1">
										{/* Icon representation */}
										<div className={`mt-0.5 w-3.5 h-3.5 shrink-0 transition-colors ${
											session.active ? "text-accent" : "text-text-muted group-hover:text-text-secondary"
										}`}>
											<MessageIcon className="w-full h-full" />
										</div>

										<div className="flex-1 min-w-0">
											<div className={`text-xs font-semibold truncate transition-colors ${
												session.active ? "text-text-primary" : "text-text-secondary group-hover:text-text-primary"
											}`}>
												{session.id}
											</div>
											
											<div className="flex flex-col items-start gap-1 mt-0.5">
												<span className="text-[9px] text-text-muted font-mono leading-none">
													{formatDate(session.created_at ?? 0)}
												</span>
												
												{session.workspace && (
													<span className="inline-flex items-center px-1.5 py-0.5 rounded text-[9px] font-medium bg-bg-4 border border-border text-text-tertiary max-w-full truncate font-mono select-none leading-none">
														{workspaceName(session.workspace)}
													</span>
												)}
											</div>
										</div>
									</div>

									{!session.active && (
										<button
											type="button"
											className="opacity-0 group-hover:opacity-100 p-1 hover:bg-bg-4 border border-transparent hover:border-border rounded-md text-text-muted hover:text-red-400 transition-all duration-200 cursor-pointer ml-2"
											onClick={(e) => {
												e.stopPropagation();
												deleteSession(session.id);
											}}
											title="Delete session"
										>
											<span className="w-3.5 h-3.5 flex items-center justify-center shrink-0">
												<TrashIcon className="w-full h-full" />
											</span>
										</button>
									)}
								</div>
							))}
						</div>
					))
				) : (
					<div className="flex-1 flex flex-col items-center justify-center text-center text-text-muted px-[var(--space-4)] select-none">
						<span className="text-xs">No sessions</span>
					</div>
				)}
			</div>
		</div>
	);
}
