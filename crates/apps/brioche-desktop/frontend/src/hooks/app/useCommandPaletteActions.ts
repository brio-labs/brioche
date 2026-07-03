import { useMemo } from "react";
import { buildCommands } from "../../components/CommandPalette";

interface UseCommandPaletteActionsOptions {
  handleNewSession: () => Promise<void> | void;
  handleClearChat: () => void;
  setShowSettings: (value: boolean) => void;
  toggleSessions: () => void;
  toggleFiles: () => void;
  handleExportChat: () => void;
}

export function useCommandPaletteActions({
  handleNewSession,
  handleClearChat,
  setShowSettings,
  toggleSessions,
  toggleFiles,
  handleExportChat,
}: UseCommandPaletteActionsOptions) {
  return useMemo(
    () =>
      buildCommands({
        newSession: handleNewSession,
        clearChat: handleClearChat,
        openSettings: () => setShowSettings(true),
        toggleSessions,
        toggleFiles,
        exportChat: handleExportChat,
      }),
    [
      handleNewSession,
      handleClearChat,
      setShowSettings,
      toggleSessions,
      toggleFiles,
      handleExportChat,
    ],
  );
}
