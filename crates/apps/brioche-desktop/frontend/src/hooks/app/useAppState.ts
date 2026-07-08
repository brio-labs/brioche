import { useEffect, useRef, useCallback, useState, useMemo } from "react";
import { useSessionStore } from "../../stores/sessionStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useFileStore } from "../../stores/fileStore";
import { isTauri } from "../../ipc";
import { useTauriSync } from "../../hooks/useTauriSync";
import type { PanelImperativeHandle } from "react-resizable-panels";

interface PanelState {
  left: boolean;
  right: boolean;
}

export interface AppState {
  panels: PanelState;
  panelWidths: { left: number; center: number; right: number };
  showChat: boolean;
  showSettings: boolean;
  showSkills: boolean;

  showMemory: boolean;
  showTools: boolean;
  showPalette: boolean;
  showMessageSearch: boolean;
  leftPanelRef: React.RefObject<PanelImperativeHandle | null>;
  centerPanelRef: React.RefObject<PanelImperativeHandle | null>;
  rightPanelRef: React.RefObject<PanelImperativeHandle | null>;
  projectName: string | undefined;
  handleNewSession: () => Promise<void>;
  handleLeftResize: (size: { inPixels: number }) => void;
  handleCenterResize: (size: { inPixels: number }) => void;
  handleRightResize: (size: { inPixels: number }) => void;
  toggleLeftPanel: () => void;
  toggleCenterPanel: () => void;
  toggleRightPanel: () => void;
  setShowSettings: (value: boolean) => void;
  setShowSkills: (value: boolean) => void;

  setShowMemory: (value: boolean) => void;
  setShowTools: (value: boolean) => void;
  setShowPalette: (value: boolean) => void;
  setShowMessageSearch: (value: boolean) => void;
}

export function useAppState(): AppState {
  const { loadSessions, createSession } = useSessionStore();
  const { loadSettings, settings } = useSettingsStore();
  const { loadDirectory } = useFileStore();

  const [showSettings, setShowSettings] = useState(false);
  const [showSkills, setShowSkills] = useState(false);

  const [showMemory, setShowMemory] = useState(false);
  const [showTools, setShowTools] = useState(false);
  const [showPalette, setShowPalette] = useState(false);
  const [showMessageSearch, setShowMessageSearch] = useState(false);
  const [showChat, setShowChat] = useState(true);
  const [panels, setPanels] = useState<PanelState>({ left: true, right: true });
  const [panelWidths, setPanelWidths] = useState({
    left: 0,
    center: 0,
    right: 0,
  });

  const leftPanelRef = useRef<PanelImperativeHandle>(null);
  const centerPanelRef = useRef<PanelImperativeHandle>(null);
  const rightPanelRef = useRef<PanelImperativeHandle>(null);

  const handleLeftResize = useCallback((size: { inPixels: number }) => {
    setPanelWidths((w) => ({ ...w, left: size.inPixels }));
    setPanels((p) => ({ ...p, left: size.inPixels > 0 }));
  }, []);

  const handleCenterResize = useCallback((size: { inPixels: number }) => {
    setPanelWidths((w) => ({ ...w, center: size.inPixels }));
    setShowChat(size.inPixels > 0);
  }, []);

  const handleRightResize = useCallback((size: { inPixels: number }) => {
    setPanelWidths((w) => ({ ...w, right: size.inPixels }));
    setPanels((p) => ({ ...p, right: size.inPixels > 0 }));
  }, []);

  const toggleLeftPanel = useCallback(() => {
    const ref = leftPanelRef.current;
    if (!ref) return;
    if (panels.left) {
      ref.collapse();
    } else {
      ref.expand();
    }
  }, [panels.left]);

  const toggleCenterPanel = useCallback(() => {
    const ref = centerPanelRef.current;
    if (!ref) return;
    if (showChat) {
      ref.collapse();
    } else {
      ref.expand();
    }
  }, [showChat]);

  const toggleRightPanel = useCallback(() => {
    const ref = rightPanelRef.current;
    if (!ref) return;
    if (panels.right) {
      ref.collapse();
    } else {
      ref.expand();
    }
  }, [panels.right]);

  // Load sessions and settings once on mount when running inside Tauri.
  useEffect(() => {
    if (!isTauri()) return;
    loadSessions();
    loadSettings();
  }, [loadSessions, loadSettings]);

  const workingDir = (settings.ui as Record<string, unknown> | undefined)
    ?.working_dir as string | undefined;
  const projectName = useMemo(() => {
    if (!workingDir) return undefined;
    const parts = workingDir.split(/[/\\]/).filter(Boolean);
    return parts[parts.length - 1];
  }, [workingDir]);

  // Keep the file explorer in sync with the configured working directory.
  useEffect(() => {
    if (!isTauri() || !workingDir) return;
    loadDirectory(workingDir);
  }, [workingDir, loadDirectory]);

  // Synchronize Tauri events with stores reactively.
  useTauriSync();

  const handleNewSession = useCallback(async () => {
    const id = await createSession();
    if (id) await loadSessions();
  }, [createSession, loadSessions]);

  // Global keyboard shortcuts for overlays and search.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setShowPalette(false);
        setShowMessageSearch(false);
        return;
      }
      const meta = e.ctrlKey || e.metaKey;
      if (meta && e.key.toLowerCase() === "k" && !e.shiftKey) {
        e.preventDefault();
        setShowPalette((open) => {
          if (!open) setShowMessageSearch(false);
          return !open;
        });
      }
      if (meta && e.shiftKey && e.key.toLowerCase() === "f") {
        e.preventDefault();
        setShowPalette(false);
        setShowMessageSearch(true);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return {
    panels,
    panelWidths,
    showChat,
    showSettings,
    showSkills,

    showMemory,
    showTools,
    showPalette,
    showMessageSearch,
    leftPanelRef,
    centerPanelRef,
    rightPanelRef,
    projectName,
    handleNewSession,
    handleLeftResize,
    handleCenterResize,
    handleRightResize,
    toggleLeftPanel,
    toggleCenterPanel,
    toggleRightPanel,
    setShowSettings,
    setShowSkills,

    setShowMemory,
    setShowTools,
    setShowPalette,
    setShowMessageSearch,
  };
}
