import { useEffect, useState, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "@tauri-apps/api/core";
import { Minus, Square, Copy, X } from "lucide-react";
import Tooltip from "./Tooltip";
import { cn } from "./ui/lib";

export interface OverlayButton {
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  active: boolean;
  onClick: () => void;
}

function useMaximized() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;

    let cancelled = false;
    let unlisten: (() => void) | undefined;

    const win = getCurrentWindow();
    void win.isMaximized().then((m) => {
      if (!cancelled) setMaximized(m);
    });

    void win
      .onResized(() => {
        void win.isMaximized().then((m) => {
          if (!cancelled) setMaximized(m);
        });
      })
      .then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      });

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  return maximized;
}

export interface TitleBarProps {
  buttons: OverlayButton[];
  projectName?: string;
}

export function TitleBar({ buttons, projectName }: TitleBarProps) {
  const maximized = useMaximized();
  const title = projectName ? `Brioche - ${projectName}` : "Brioche";

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
    <header className="title-bar">
      <div className="flex items-center px-3">
        <span className="text-sm font-semibold text-fg-secondary tracking-wider">
          {title}
        </span>
      </div>
      <div className="flex-1 cursor-default" data-tauri-drag-region />
      <div className="flex items-center">
        {buttons.map(({ label, icon: Icon, active, onClick }) => (
          <Tooltip key={label} label={label}>
            <button
              type="button"
              onClick={onClick}
              className={cn("top-bar-button", active && "text-accent")}
              aria-pressed={active}
              aria-label={label}
            >
              <Icon className="w-4 h-4" />
            </button>
          </Tooltip>
        ))}
        <div className="w-px h-5 bg-fg-muted/30 mx-2" aria-hidden="true" />
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
    </header>
  );
}
