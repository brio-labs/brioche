import { useTauriEvent } from "../hooks/useTauriSync";
import Tooltip from "./Tooltip";
import { MessageIcon, ChatBubbleIcon, FolderIcon } from "./Icons";
import { cn } from "./ui/lib";

interface PanelState {
	left: boolean;
	center: boolean;
	right: boolean;
}

interface FooterProps {
	panels: PanelState;
	panelWidths?: {
		left: number;
		center: number;
		right: number;
	};
	onToggleLeft: () => void;
	onToggleRight: () => void;
	onToggleChat: () => void;
}

const BUTTON_WIDTH = 28;
const SEPARATOR_WIDTH = 1;

export default function Footer({
	panels,
	panelWidths,
	onToggleLeft,
	onToggleRight,
	onToggleChat,
}: FooterProps) {
	// Kept for future reactive footer state; chat-message listener is a no-op for now.
	useTauriEvent("chat-message", () => {});

	const left = panelWidths?.left ?? 0;
	const center = panelWidths?.center ?? 0;
	const right = panelWidths?.right ?? 0;

	const leftIconRight = Math.max(left, BUTTON_WIDTH);
	const centerIconRight = Math.max(left + SEPARATOR_WIDTH + center, leftIconRight + BUTTON_WIDTH + SEPARATOR_WIDTH);
	const rightIconRight = left + SEPARATOR_WIDTH + center + SEPARATOR_WIDTH + right;

	return (
		<footer className="relative flex h-10 bg-bg-base/90 border-t border-border text-fg-muted shrink-0 select-none z-10">
			<Tooltip label="Sessions">
				<button
					type="button"
					onClick={onToggleLeft}
					className={cn(
						"dock-button absolute",
						panels.left && "dock-button-active",
					)}
					style={{ left: Math.max(leftIconRight - BUTTON_WIDTH, 0) }}
					aria-pressed={panels.left}
					aria-label="Sessions"
				>
					<MessageIcon className="w-4 h-4" />
				</button>
			</Tooltip>

			<Tooltip label="Conversation">
				<button
					type="button"
					onClick={onToggleChat}
					className={cn(
						"dock-button absolute",
						panels.center && "dock-button-active",
					)}
					style={{ left: Math.max(centerIconRight - BUTTON_WIDTH, leftIconRight + SEPARATOR_WIDTH) }}
					aria-pressed={panels.center}
					aria-label="Conversation"
				>
					<ChatBubbleIcon className="w-4 h-4" />
				</button>
			</Tooltip>

			<Tooltip label="Explorer">
				<button
					type="button"
					onClick={onToggleRight}
					className={cn(
						"dock-button absolute right-0",
						panels.right && "dock-button-active",
					)}
					aria-pressed={panels.right}
					aria-label="Explorer"
				>
					<FolderIcon className="w-4 h-4" />
				</button>
			</Tooltip>
		</footer>
	);
}
