import { useState } from "react";
import { ChevronRight } from "lucide-react";
import { cn } from "../ui/lib";
import { SessionItem } from "./SessionItem";
import type { Session } from "../../stores/sessionStore";

interface SessionGroupProps {
	title: string;
	sessions: Session[];
	switchToSession: (id: string) => Promise<void>;
	deleteSession: (id: string) => Promise<void>;
}

export function SessionGroup({
	title,
	sessions,
	switchToSession,
	deleteSession,
}: SessionGroupProps) {
	const [isOpen, setIsOpen] = useState(true);

	return (
		<div className="flex flex-col">
			<button
				type="button"
				onClick={() => setIsOpen((prev) => !prev)}
				className="group flex w-full select-none items-center gap-2 px-4 py-2 text-xs font-medium text-fg-muted transition-colors hover:text-fg-secondary"
			>
				<span
					className={cn(
						"flex h-3.5 w-3.5 shrink-0 items-center justify-center transition-transform duration-200",
						isOpen && "rotate-90",
					)}
				>
					<ChevronRight className="h-full w-full" />
				</span>
				<span className="truncate">{title}</span>
				<span className="ml-auto text-[10px] font-medium text-fg-dim opacity-0 transition-opacity group-hover:opacity-100">
					{sessions.length}
				</span>
			</button>

			{isOpen && (
				<div className="flex flex-col">
					{sessions.map((session) => (
						<SessionItem
							key={session.id}
							session={session}
							switchToSession={switchToSession}
							deleteSession={deleteSession}
						/>
					))}
				</div>
			)}
		</div>
	);
}
