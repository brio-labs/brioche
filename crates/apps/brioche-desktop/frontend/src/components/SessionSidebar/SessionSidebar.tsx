import { useCallback } from "react";
import { useSessionStore } from "../../stores/sessionStore";
import { useSessionGrouping } from "../../hooks/sessionSidebar/useSessionGrouping";
import { SessionHeader } from "./SessionHeader";
import { SessionSortControls } from "./SessionSortControls";
import { SessionGroup } from "./SessionGroup";

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

	const groupedSessions = useSessionGrouping(sessions, sortMode);

	return (
		<div className="flex h-full w-full flex-col bg-transparent text-fg-primary">
			<SessionHeader onNewSession={handleNewSession} />
			<SessionSortControls sortMode={sortMode} setSortMode={setSortMode} />
			<div className="flex flex-1 flex-col space-y-3 overflow-y-auto py-4">
				{sessions.length > 0 ? (
					Array.from(groupedSessions.entries()).map(([group, items]) => (
						<SessionGroup
							key={group}
							title={group}
							sessions={items}
							switchToSession={switchToSession}
							deleteSession={deleteSession}
						/>
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
