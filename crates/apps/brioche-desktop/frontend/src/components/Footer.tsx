import { useTauriEvent } from "../hooks/useTauriSync";
import Tooltip from "./Tooltip";
import { MessageSquare, MessageCircle, Folder } from "lucide-react";
import { cn } from "./ui/lib";

interface PanelState {
  left: boolean;
  center: boolean;
  right: boolean;
}

interface FooterProps {
  panels: PanelState;
  onToggleLeft: () => void;
  onToggleRight: () => void;
}

function FooterSeparator() {
  return (
    <div
      className="w-px h-full bg-fg-muted/50 self-center mx-1 shrink-0"
      aria-hidden="true"
    />
  );
}

export default function Footer({
  panels,
  onToggleLeft,
  onToggleRight,
}: FooterProps) {
  // Kept for future reactive footer state; chat-message listener is a no-op for now.
  useTauriEvent("chat-message", () => {});

  return (
    <footer className="flex items-center h-10 bg-bg-base border-t border-border text-fg-muted shrink-0 select-none z-10">
      <Tooltip label="Sessions">
        <button
          type="button"
          onClick={onToggleLeft}
          className={cn("dock-button", panels.left && "dock-button-active")}
          aria-pressed={panels.left}
          aria-label="Sessions"
        >
          <MessageSquare className="w-4 h-4" />
        </button>
      </Tooltip>
      <FooterSeparator />

      <div className="flex-1 flex items-center justify-end">
        <Tooltip label="Explorer">
          <button
            type="button"
            onClick={onToggleRight}
            className={cn("dock-button", panels.right && "dock-button-active")}
            aria-pressed={panels.right}
            aria-label="Explorer"
          >
            <Folder className="w-4 h-4" />
          </button>
        </Tooltip>
      </div>
    </footer>
  );
}
