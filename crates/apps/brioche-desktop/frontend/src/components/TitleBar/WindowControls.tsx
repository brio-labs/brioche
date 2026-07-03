import { useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "@tauri-apps/api/core";
import { Minus, Square, Copy, X } from "lucide-react";
import { useMaximized } from "../../hooks/titleBar/useMaximized";

export function WindowControls() {
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
