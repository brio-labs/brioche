import { useTauriEvent } from "../hooks/useTauriSync";
import Tooltip from "./Tooltip";
import { MessageIcon, ChatBubbleIcon, FolderIcon } from "./Icons";

interface PanelState {
	left: boolean;
	right: boolean;
}

interface FooterProps {
	panels: PanelState;
	setPanels: React.Dispatch<React.SetStateAction<PanelState>>;
	showChat: boolean;
	setShowChat: (value: boolean) => void;
}

export default function Footer({
	panels,
	setPanels,
	showChat,
	setShowChat,
}: FooterProps) {
	// Kept for future reactive footer state; chat-message listener is a no-op for now.
	useTauriEvent("chat-message", () => {});

	const toggleLeft = () => setPanels((p) => ({ ...p, left: !p.left }));
	const toggleRight = () => setPanels((p) => ({ ...p, right: !p.right }));
	const toggleChat = () => setShowChat(!showChat);

	return (
		<footer className="flex items-center justify-between px-4 h-10 bg-bg-0/90 border-t border-border text-text-muted shrink-0 select-none z-10">
			<Tooltip label="Sessions">
				<button
					type="button"
					onClick={toggleLeft}
					className={`dock-button ${panels.left ? "dock-button-active" : ""}`}
					aria-pressed={panels.left}
					aria-label="Sessions"
				>
					<MessageIcon className="w-4 h-4" />
				</button>
			</Tooltip>

			<Tooltip label="Conversation">
				<button
					type="button"
					onClick={toggleChat}
					className={`dock-button ${showChat ? "dock-button-active" : ""}`}
					aria-pressed={showChat}
					aria-label="Conversation"
				>
					<ChatBubbleIcon className="w-4 h-4" />
				</button>
			</Tooltip>

			<Tooltip label="Explorer">
				<button
					type="button"
					onClick={toggleRight}
					className={`dock-button ${panels.right ? "dock-button-active" : ""}`}
					aria-pressed={panels.right}
					aria-label="Explorer"
				>
					<FolderIcon className="w-4 h-4" />
				</button>
			</Tooltip>
		</footer>
	);
}
