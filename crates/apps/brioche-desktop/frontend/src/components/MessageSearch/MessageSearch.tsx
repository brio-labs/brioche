import { ModalOverlay, ModalSearchHeader } from "../PanelOverlay";
import { cn } from "../ui/lib";
import { useMessageSearch } from "../../hooks/messageSearch";

export interface MessageSearchProps {
	messages: Array<{
		id: string;
		role: string;
		content: string;
		timestamp: number;
	}>;
	onJumpTo: (messageId: string) => void;
	isOpen: boolean;
	onClose: () => void;
}

interface MessageSearchResult {
	id: string;
	role: string;
	preview: string;
}

function MessageSearchResultItem({
	result,
	isSelected,
	onSelect,
	onHover,
}: {
	result: MessageSearchResult;
	isSelected: boolean;
	onSelect: () => void;
	onHover: () => void;
}) {
	return (
		<div
			className={cn(
				"flex cursor-pointer items-start gap-3 rounded-sm px-4 py-4 text-sm text-fg-secondary transition-all hover:bg-accent/10 hover:text-fg-primary",
				isSelected &&
					"border-l-2 border-accent bg-accent/10 text-fg-primary",
			)}
			onClick={onSelect}
			onMouseEnter={onHover}
		>
			<div className="w-12 shrink-0 pt-0.5">
				<span className="text-xs font-bold uppercase tracking-wider text-fg-muted">
					{result.role}
				</span>
			</div>
			<div className="flex-1 leading-snug">{result.preview}</div>
		</div>
	);
}

function MessageSearchResults({
	results,
	query,
	selectedIndex,
	onSelect,
	onHover,
}: {
	results: MessageSearchResult[];
	query: string;
	selectedIndex: number;
	onSelect: (id: string) => void;
	onHover: (index: number) => void;
}) {
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

export default function MessageSearch({
	messages,
	onJumpTo,
	isOpen,
	onClose,
}: MessageSearchProps) {
	const {
		query,
		setQuery,
		results,
		selectedIndex,
		setSelectedIndex,
		inputRef,
		handleKeyDown,
	} = useMessageSearch({ messages, onJumpTo, isOpen, onClose });

	return (
		<ModalOverlay isOpen={isOpen} onClose={onClose}>
			<ModalSearchHeader
				query={query}
				onChange={setQuery}
				onKeyDown={handleKeyDown}
				inputRef={inputRef}
				placeholder="Search messages..."
				onClear={() => setQuery("")}
			/>
			<MessageSearchResults
				results={results}
				query={query}
				selectedIndex={selectedIndex}
				onSelect={(id) => {
					onJumpTo(id);
					onClose();
				}}
				onHover={setSelectedIndex}
			/>
		</ModalOverlay>
	);
}
