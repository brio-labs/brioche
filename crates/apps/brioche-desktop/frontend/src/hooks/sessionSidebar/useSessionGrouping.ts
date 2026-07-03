import { useMemo } from "react";
import type { Session } from "../../stores/sessionStore";
import type { SessionSort } from "../../ipc";

/// Sort comparator for sessions within a project folder.
function sortSessions(sessions: Session[], sortMode: SessionSort): Session[] {
	return [...sessions].sort((a, b) => {
		if (sortMode === "name") {
			return a.id.localeCompare(b.id);
		}
		// Date recency (created_at or updated_at) is the default.
		const aTime = a.updated_at ?? a.created_at ?? 0;
		const bTime = b.updated_at ?? b.created_at ?? 0;
		return bTime - aTime;
	});
}

/// Returns sessions grouped by workspace (project folder) and sorted
/// according to the current sort mode within each folder.
///
/// Folders are ordered by their most recent session activity.
export function useSessionGrouping(sessions: Session[], sortMode: SessionSort) {
	return useMemo(() => {
		const groups = new Map<string, Session[]>();

		for (const session of sessions) {
			const workspace = session.workspace || "";
			const parts = workspace.split("/");
			const key = parts[parts.length - 1] || workspace || "No project";
			if (!groups.has(key)) {
				groups.set(key, []);
			}
			groups.get(key)!.push(session);
		}

		// Sort sessions inside each folder.
		for (const [key, items] of groups) {
			groups.set(key, sortSessions(items, sortMode));
		}

		// Sort folders by the most recent activity in each folder.
		const sortedEntries = Array.from(groups.entries()).sort((a, b) => {
			const aTime = Math.max(
				0,
				...a[1].map((s) => s.updated_at ?? s.created_at ?? 0),
			);
			const bTime = Math.max(
				0,
				...b[1].map((s) => s.updated_at ?? s.created_at ?? 0),
			);
			return bTime - aTime;
		});

		return new Map(sortedEntries);
	}, [sessions, sortMode]);
}
