import { useState, useCallback, useEffect, useRef } from "react";
import { ModalOverlay, ModalSearchHeader } from "./PanelOverlay";
import { cn } from "./ui/lib";

/**
 * Props for the message search overlay.
 *
 * Refs: I-Ui-OverlayCohesion
 */
interface MessageSearchProps {
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
	const [query, setQuery] = useState("");
	const [results, setResults] = useState<
		Array<{ id: string; preview: string; role: string }>
	>([]);
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
			<div className="flex-1 min-h-0 overflow-y-auto p-4">
				{results.length === 0 && query.trim() && (
					<div className="p-8 text-center text-sm text-fg-muted">
						No messages found
					</div>
				)}
				{results.map((result, idx) => (
					<div
						key={result.id}
						className={cn(
							"flex cursor-pointer items-start gap-3 rounded-sm px-4 py-4 text-sm text-fg-secondary transition-all hover:bg-accent/10 hover:text-fg-primary",
							idx === selectedIndex &&
								"border-l-2 border-accent bg-accent/10 text-fg-primary",
						)}
						onClick={() => {
							onJumpTo(result.id);
							onClose();
						}}
						onMouseEnter={() => setSelectedIndex(idx)}
					>
						<div className="w-12 shrink-0 pt-0.5">
							<span className="text-xs font-bold uppercase tracking-wider text-fg-muted">
								{result.role}
							</span>
						</div>
						<div className="flex-1 leading-snug">{result.preview}</div>
					</div>
				))}
				{results.length > 0 && (
					<div className="mt-2 border-t border-border px-4 py-2 text-xs text-fg-muted">
						{results.length} result{results.length !== 1 ? "s" : ""}
					</div>
				)}
			</div>
		</ModalOverlay>
	);
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
