import { Plus, MessageSquare } from "lucide-react";
import { SectionHeader, SectionHeaderTitle } from "../ui";

interface SessionHeaderProps {
	onNewSession: () => void;
}

export function SessionHeader({ onNewSession }: SessionHeaderProps) {
	return (
		<SectionHeader>
			<div className="flex items-center gap-2">
				<span className="flex h-4 w-4 shrink-0 items-center justify-center text-fg-muted">
					<MessageSquare className="h-full w-full" />
				</span>
				<SectionHeaderTitle>Sessions</SectionHeaderTitle>
			</div>
			<button
				type="button"
				className="flex cursor-pointer items-center justify-center rounded-md border border-border bg-bg-highlight p-1.5 text-fg-secondary shadow-sm transition-all duration-200 hover:border-accent-dim/40 hover:bg-bg-highlight hover:text-fg-primary"
				onClick={onNewSession}
				title="New session"
			>
				<span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
					<Plus className="h-full w-full" />
				</span>
			</button>
		</SectionHeader>
	);
}
