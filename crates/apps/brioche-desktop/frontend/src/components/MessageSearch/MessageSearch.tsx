import { ModalOverlay } from "../PanelOverlay";
import { MessageSearchInput } from "./MessageSearchInput";
import { MessageSearchResults } from "./MessageSearchResults";
import { useMessageSearch } from "../../hooks/messageSearch";

/**
 * Props for the message search overlay.
 *
 * Refs: I-Ui-OverlayCohesion
 */
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

/**
 * Renders a search overlay for navigating messages by content.
 *
 * Refs: I-Ui-OverlayCohesion
 */
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
			<MessageSearchInput
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
