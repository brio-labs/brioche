import { useCallback } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import Tooltip from "../Tooltip";
import { cn } from "../ui/lib";
import { useMaximized } from "../../hooks/titleBar/useMaximized";

export interface OverlayButtonItem {
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  active: boolean;
  onClick: () => void;
}

export interface TitleBarProps {
  buttons: OverlayButtonItem[];
  projectName?: string;
}

function OverlayButton({
  label,
  icon: Icon,
  active,
  onClick,
}: OverlayButtonItem) {
  return (
    <Tooltip key={label} label={label}>
      <button
        type="button"
        onClick={onClick}
        className={cn("top-bar-button", active && "text-accent")}
        aria-pressed={active}
        aria-label={label}
      >
        <Icon className="w-5 h-5" />
      </button>
    </Tooltip>
  );
}

function WindowControls() {
  const maximized = useMaximized();

  const handleMinimize = useCallback(() => {
    if (!isTauri()) return;
    getCurrentWindow()
      .minimize()
      .catch((err: unknown) =>
        console.error("Failed to minimize window:", err),
      );
  }, []);

  const handleMaximize = useCallback(async () => {
    if (!isTauri()) return;
    try {
      const win = getCurrentWindow();
      const m = await win.isMaximized();
      if (m) {
        await win.unmaximize();
      } else {
        await win.maximize();
      }
    } catch (err: unknown) {
      console.error("Failed to maximize/restore window:", err);
    }
  }, []);

  const handleClose = useCallback(() => {
    if (!isTauri()) return;
    getCurrentWindow()
      .close()
      .catch((err: unknown) => console.error("Failed to close window:", err));
  }, []);

  return (
    <div className="flex items-center">
      <button
        type="button"
        className="top-bar-button"
        onClick={handleMinimize}
        aria-label="Minimize"
      >
        <Minus className="w-4 h-4" />
      </button>
      <button
        type="button"
        className="top-bar-button"
        onClick={handleMaximize}
        aria-label={maximized ? "Restore" : "Maximize"}
      >
        {maximized ? (
          <Copy className="w-4 h-4" />
        ) : (
          <Square className="w-4 h-4" />
        )}
      </button>
      <button
        type="button"
        className="top-bar-button"
        data-close
        onClick={handleClose}
        aria-label="Close"
      >
        <X className="w-4 h-4" />
      </button>
    </div>
  );
}

export function TitleBar({ buttons, projectName }: TitleBarProps) {
  const title = projectName ? `Brioche - ${projectName}` : "Brioche";

  return (
    <header className="title-bar">
      <div className="flex items-center px-3">
        <span className="font-mono text-xs text-fg-secondary">
          {title}
        </span>
      </div>
      <div className="flex-1 cursor-default" data-tauri-drag-region />
      <div className="flex items-center">
        {buttons.map((button) => (
          <OverlayButton key={button.label} {...button} />
        ))}
        <div className="w-px h-5 bg-fg-muted/30 mx-2" aria-hidden="true" />
        <WindowControls />
      </div>
    </header>
  );
}
