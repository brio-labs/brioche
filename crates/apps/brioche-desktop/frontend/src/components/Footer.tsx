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

const MIN_SLOT_WIDTH = 48;

function FooterSlot({
  width,
  children,
}: {
  width: number;
  children: React.ReactNode;
}) {
  return (
    <div
      className="flex items-center justify-end shrink min-w-[48px]"
      style={{ flexBasis: Math.max(width, MIN_SLOT_WIDTH) }}
    >
      {children}
    </div>
  );
}

function FooterSeparator() {
  return (
    <div
      className="w-px h-5 bg-fg-muted/30 self-center mx-1 shrink-0"
      aria-hidden="true"
    />
  );
}

export default function Footer({
  panels,
  panelWidths,
  onToggleLeft,
  onToggleRight,
  onToggleChat,
}: FooterProps) {
  // Kept for future reactive footer state; chat-message listener is a no-op for now.
  useTauriEvent("chat-message", () => {});

  const left = Math.max(panelWidths?.left ?? 0, 0);
  const center = Math.max(panelWidths?.center ?? 0, 0);

  return (
    <footer className="flex h-10 bg-bg-base/90 border-t border-border text-fg-muted shrink-0 select-none z-10">
      <FooterSlot width={left}>
        <Tooltip label="Sessions">
          <button
            type="button"
            onClick={onToggleLeft}
            className={cn("dock-button", panels.left && "dock-button-active")}
            aria-pressed={panels.left}
            aria-label="Sessions"
          >
            <MessageIcon className="w-4 h-4" />
          </button>
        </Tooltip>
      </FooterSlot>

      <FooterSeparator />

      <FooterSlot width={center}>
        <Tooltip label="Conversation">
          <button
            type="button"
            onClick={onToggleChat}
            className={cn("dock-button", panels.center && "dock-button-active")}
            aria-pressed={panels.center}
            aria-label="Conversation"
          >
            <ChatBubbleIcon className="w-4 h-4" />
          </button>
        </Tooltip>
      </FooterSlot>

      <FooterSeparator />

      <div className="flex-1 flex items-center justify-end min-w-[48px]">
        <Tooltip label="Explorer">
          <button
            type="button"
            onClick={onToggleRight}
            className={cn("dock-button", panels.right && "dock-button-active")}
            aria-pressed={panels.right}
            aria-label="Explorer"
          >
            <FolderIcon className="w-4 h-4" />
          </button>
        </Tooltip>
      </div>
    </footer>
  );
}
