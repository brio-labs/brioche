import { Brain } from "lucide-react";
import { EmptyState } from "../ui";
import type { MemoryEntry } from "../../ipc";
import MemoryListItem from "./MemoryListItem";

interface MemoryListProps {
	memories: MemoryEntry[];
	formatDate: (timestamp: number) => string;
	onDelete: (key: string) => void;
}

/// Renders a scrollable list of memory entries.
///
/// Refs: I-Ui-MemoryPanel
export default function MemoryList({
	memories,
	formatDate,
	onDelete,
}: MemoryListProps) {
	if (memories.length === 0) {
		return (
			<EmptyState
				icon={Brain}
				title="No memories yet"
				description="Add facts or preferences below to persist context for the model."
			/>
		);
	}

	return (
		<div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto py-1">
			{memories.map((memory) => (
				<MemoryListItem
					key={memory.key}
					memory={memory}
					formatDate={formatDate}
					onDelete={onDelete}
				/>
			))}
		</div>
	);
}
