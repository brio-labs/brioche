import { MessageSearchResultItem } from "./MessageSearchResultItem";

export interface MessageSearchResult {
	id: string;
	role: string;
	preview: string;
}

interface MessageSearchResultsProps {
	results: MessageSearchResult[];
	query: string;
	selectedIndex: number;
	onSelect: (id: string) => void;
	onHover: (index: number) => void;
}

/**
 * Renders the scrollable list of message search results.
 *
 * Refs: I-Ui-OverlayCohesion
 */
export function MessageSearchResults({
	results,
	query,
	selectedIndex,
	onSelect,
	onHover,
}: MessageSearchResultsProps) {
	return (
		<div className="flex-1 min-h-0 overflow-y-auto p-4">
			{results.length === 0 && query.trim() && (
				<div className="p-8 text-center text-sm text-fg-muted">
					No messages found
				</div>
			)}
			{results.map((result, idx) => (
				<MessageSearchResultItem
					key={result.id}
					result={result}
					isSelected={idx === selectedIndex}
					onSelect={() => onSelect(result.id)}
					onHover={() => onHover(idx)}
				/>
			))}
			{results.length > 0 && (
				<div className="mt-2 border-t border-border px-4 py-2 text-xs text-fg-muted">
					{results.length} result{results.length !== 1 ? "s" : ""}
				</div>
			)}
		</div>
	);
}
