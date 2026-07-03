import { useCallback, useEffect, useRef, useState } from "react";

export interface MessageSearchMessage {
	id: string;
	role: string;
	content: string;
	timestamp: number;
}

export interface MessageSearchResult {
	id: string;
	role: string;
	preview: string;
}

export interface UseMessageSearchOptions {
	messages: MessageSearchMessage[];
	onJumpTo: (messageId: string) => void;
	isOpen: boolean;
	onClose: () => void;
}

/**
 * Hook that owns the search query, filtering, and keyboard navigation state
 * for the message search overlay.
 *
 * Refs: I-Ui-OverlayCohesion
 */
export function useMessageSearch({
	messages,
	onJumpTo,
	isOpen,
	onClose,
}: UseMessageSearchOptions) {
	const [query, setQuery] = useState("");
	const [results, setResults] = useState<MessageSearchResult[]>([]);
	const [selectedIndex, setSelectedIndex] = useState(0);
	const inputRef = useRef<HTMLInputElement>(null);

	useEffect(() => {
		if (isOpen) {
			setQuery("");
			setResults([]);
			setSelectedIndex(0);
			setTimeout(() => inputRef.current?.focus(), 50);
		}
	}, [isOpen]);

	useEffect(() => {
		if (!query.trim()) {
			setResults([]);
			return;
		}
		const q = query.toLowerCase();
		const found = messages
			.filter((m) => m.content.toLowerCase().includes(q))
			.map((m) => ({
				id: m.id,
				role: m.role,
				preview: getPreview(m.content, q),
			}));
		setResults(found);
		setSelectedIndex(0);
	}, [query, messages]);

	const handleKeyDown = useCallback(
		(e: React.KeyboardEvent) => {
			if (e.key === "Escape") {
				onClose();
				return;
			}
			if (e.key === "Enter" && results[selectedIndex]) {
				onJumpTo(results[selectedIndex].id);
				onClose();
				return;
			}
			if (e.key === "ArrowDown") {
				e.preventDefault();
				setSelectedIndex((i) => (i + 1) % results.length);
				return;
			}
			if (e.key === "ArrowUp") {
				e.preventDefault();
				setSelectedIndex((i) => (i - 1 + results.length) % results.length);
				return;
			}
		},
		[results, selectedIndex, onJumpTo, onClose],
	);

	return {
		query,
		setQuery,
		results,
		selectedIndex,
		setSelectedIndex,
		inputRef,
		handleKeyDown,
	};
}

/**
 * Extracts a short preview around the first query match, or a fallback snippet.
 *
 * Refs: I-Ui-OverlayCohesion
 */
function getPreview(content: string, query: string): string {
	const lowerContent = content.toLowerCase();
	const idx = lowerContent.indexOf(query.toLowerCase());
	if (idx === -1) return content.slice(0, 100);
	const start = Math.max(0, idx - 40);
	const end = Math.min(content.length, idx + query.length + 40);
	let preview = content.slice(start, end);
	if (start > 0) preview = "..." + preview;
	if (end < content.length) preview = preview + "...";
	return preview;
}
