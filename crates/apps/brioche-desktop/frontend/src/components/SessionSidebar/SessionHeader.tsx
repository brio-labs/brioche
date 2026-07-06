import { MessageSquare } from "lucide-react";
import { SectionHeader, SectionHeaderTitle } from "../ui";

export function SessionHeader() {
	return (
		<SectionHeader>
			<div className="flex items-center gap-2">
				<span className="flex h-4 w-4 shrink-0 items-center justify-center text-fg-muted">
					<MessageSquare className="h-full w-full" />
				</span>
				<SectionHeaderTitle>Sessions</SectionHeaderTitle>
			</div>
		</SectionHeader>
	);
}
