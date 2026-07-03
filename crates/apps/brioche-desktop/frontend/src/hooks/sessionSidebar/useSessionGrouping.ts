import { useMemo } from "react";
import type { Session } from "../../stores/sessionStore";
import type { SessionSort } from "../../ipc";

/// Returns sessions grouped and sorted according to the current sort mode.
export function useSessionGrouping(sessions: Session[], sortMode: SessionSort) {
	return useMemo(() => {
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
				const workspace = session.workspace || "";
				const parts = workspace.split("/");
				const key = parts[parts.length - 1] || workspace || "No workspace";
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
}
