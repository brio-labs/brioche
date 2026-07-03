import { SessionItem } from "./SessionItem";
import type { Session } from "../../stores/sessionStore";

interface SessionGroupProps {
	title: string;
	sessions: Session[];
	switchToSession: (id: string) => Promise<void>;
	deleteSession: (id: string) => Promise<void>;
}

export function SessionGroup({ title, sessions, switchToSession, deleteSession }: SessionGroupProps) {
	return (
		<div className="space-y-0.5">
			<div className="mb-2 flex select-none items-center gap-2 px-4 text-xs font-bold uppercase tracking-widest text-fg-muted">
				<span>{title}</span>
				<div className="h-px flex-1 bg-border/30" />
			</div>

			{sessions.map((session) => (
				<SessionItem
					key={session.id}
					session={session}
					switchToSession={switchToSession}
					deleteSession={deleteSession}
				/>
			))}
		</div>
	);
}
