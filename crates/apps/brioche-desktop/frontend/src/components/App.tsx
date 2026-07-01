import { useEffect, useRef, useCallback, useState, useMemo } from "react";
import { useChatStore } from "../store";
import { useSessionStore } from "../stores/sessionStore";
import { useSettingsStore } from "../stores/settingsStore";
import { useFileStore } from "../stores/fileStore";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "@tauri-apps/api/core";
import { Minus, Square, Copy, X } from "lucide-react";
import { sendMessage, attachReference, sendImage } from "../ipc";
import { Group, Panel, Separator } from "react-resizable-panels";
import type { PanelImperativeHandle } from "react-resizable-panels";
import Footer from "./Footer";
import Tooltip from "./Tooltip";
import { cn } from "./ui/lib";
import { ClearIcon, SendIcon, PaperclipIcon, ImageIcon } from "./Icons";
import SessionSidebar from "./SessionSidebar";
import FileExplorer from "./FileExplorer";
import ToolsPanel from "./ToolsPanel";
import SettingsPanel from "./SettingsPanel";
import SkillsPanel from "./SkillsPanel";
import MemoryPanel from "./MemoryPanel";
import ProfilesPanel from "./ProfilesPanel";
import ToolCallMessage from "./ToolCallMessage";
import CommandPalette, { buildCommands } from "./CommandPalette";
import MessageSearch from "./MessageSearch";
import { useTauriSync } from "../hooks/useTauriSync";
import {
  SearchIcon,
  BrainIcon,
  BookIcon,
  UserIcon,
  WrenchIcon,
  SettingsIcon,
} from "./Icons";

interface PanelState {
  left: boolean;
  right: boolean;
}

interface OverlayButton {
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

    void win.onResized(() => {
      void win.isMaximized().then((m) => {
        if (!cancelled) setMaximized(m);
      });
    }).then((fn) => {
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

function TitleBar({
  buttons,
  projectName,
}: {
  buttons: OverlayButton[];
  projectName?: string;
}) {
  const maximized = useMaximized();
  const title = projectName ? `Brioche - ${projectName}` : "Brioche";

  const handleMinimize = useCallback(() => {
    if (!isTauri()) return;
    getCurrentWindow()
      .minimize()
      .catch((err: unknown) => console.error("Failed to minimize window:", err));
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
      <div className="flex items-center gap-1">
        {buttons.map(({ label, icon: Icon, active, onClick }) => (
          <Tooltip key={label} label={label}>
            <button
              type="button"
              onClick={onClick}
              className={cn(
                "top-bar-button",
                active && "text-accent",
              )}
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

export default function App() {
  const {
    messages,
    input,
    isLoading,
    addMessage,
    setInput,
    setLoading,
    clearMessages,
  } = useChatStore();
  const { loadSessions, createSession } = useSessionStore();
  const { loadSettings, settings } = useSettingsStore();
  const { loadDirectory } = useFileStore();
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [showSkills, setShowSkills] = useState(false);
  const [showProfiles, setShowProfiles] = useState(false);
  const [showMemory, setShowMemory] = useState(false);
  const [showTools, setShowTools] = useState(false);
  const [showPalette, setShowPalette] = useState(false);
  const [showMessageSearch, setShowMessageSearch] = useState(false);
  const [showChat, setShowChat] = useState(true);
  const [panels, setPanels] = useState<PanelState>({
    left: true,
    right: true,
  });
  const [panelWidths, setPanelWidths] = useState({
    left: 0,
    center: 0,
    right: 0,
  });
  const leftPanelRef = useRef<PanelImperativeHandle>(null);
  const centerPanelRef = useRef<PanelImperativeHandle>(null);
  const rightPanelRef = useRef<PanelImperativeHandle>(null);

  const handleLeftCollapse = useCallback(() => {
    setPanels((p) => ({ ...p, left: false }));
  }, []);

  const handleLeftExpand = useCallback(() => {
    setPanels((p) => ({ ...p, left: true }));
  }, []);

  const handleCenterCollapse = useCallback(() => {
    setShowChat(false);
  }, []);

  const handleCenterExpand = useCallback(() => {
    setShowChat(true);
  }, []);

  const handleLeftResize = useCallback((size: { inPixels: number }) => {
    setPanelWidths((w) => ({ ...w, left: size.inPixels }));
  }, []);

  const handleCenterResize = useCallback((size: { inPixels: number }) => {
    setPanelWidths((w) => ({ ...w, center: size.inPixels }));
  }, []);

  const handleRightResize = useCallback((size: { inPixels: number }) => {
    setPanelWidths((w) => ({ ...w, right: size.inPixels }));
  }, []);

  const handleRightCollapse = useCallback(() => {
    setPanels((p) => ({ ...p, right: false }));
  }, []);

  const handleRightExpand = useCallback(() => {
    setPanels((p) => ({ ...p, right: true }));
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

  const scrollToBottom = useCallback(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, []);

  useEffect(() => {
    scrollToBottom();
  }, [messages, scrollToBottom]);

  useEffect(() => {
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

  useEffect(() => {
    if (workingDir) {
      loadDirectory(workingDir);
    }
  }, [workingDir, loadDirectory]);

  // Synchronize Tauri events with stores reactively
  useTauriSync();

  const handleNewSession = useCallback(async () => {
    const id = await createSession();
    if (id) await loadSessions();
  }, [createSession, loadSessions]);

  const handleClearChat = useCallback(() => {
    clearMessages();
    void sendMessage("/clear");
  }, [clearMessages]);

  const handleExportChat = useCallback(() => {
    const text = messages.map((m) => `${m.role}: ${m.content}`).join("\n\n");
    const blob = new Blob([text], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `brioche-chat-${new Date().toISOString().slice(0, 10)}.txt`;
    a.click();
    URL.revokeObjectURL(url);
  }, [messages]);

  const paletteCommands = useMemo(
    () =>
      buildCommands({
        newSession: handleNewSession,
        clearChat: handleClearChat,
        openSettings: () => setShowSettings(true),
        toggleSessions: toggleLeftPanel,
        toggleFiles: toggleRightPanel,
        exportChat: handleExportChat,
      }),
    [handleNewSession, handleClearChat, toggleLeftPanel, toggleRightPanel, handleExportChat],
  );

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

  const handleSubmit = useCallback(
    async (e?: React.FormEvent) => {
      e?.preventDefault();
      const trimmed = input.trim();
      if (!trimmed || isLoading) return;

      setInput("");
      addMessage("user", trimmed);
      setLoading(true);

      try {
        await sendMessage(trimmed);
      } catch (err) {
        addMessage("error", String(err));
      } finally {
        setLoading(false);
      }
    },
    [input, isLoading, addMessage, setInput, setLoading],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        void handleSubmit();
      }
    },
    [handleSubmit],
  );

  const handleAttach = async () => {
    const path = await open({
      multiple: false,
      directory: false,
    });
    if (!path) return;
    try {
      await attachReference(path);
      addMessage("system", `Attached: ${path}`);
    } catch (err) {
      addMessage("error", String(err));
    }
  };

  const handleImage = async () => {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [
        { name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp"] },
      ],
    });
    if (!path) return;
    try {
      const dataUrl = await sendImage(path);
      addMessage("user", `![${path}](${dataUrl})`);
    } catch (err) {
      addMessage("error", String(err));
    }
  };

  const overlayButtons = [
    {
      label: "Search messages (Ctrl+Shift+F)",
      icon: SearchIcon,
      active: showMessageSearch,
      onClick: () => setShowMessageSearch(true),
    },
    {
      label: "Memory",
      icon: BrainIcon,
      active: showMemory,
      onClick: () => setShowMemory(true),
    },
    {
      label: "Skills",
      icon: BookIcon,
      active: showSkills,
      onClick: () => setShowSkills(true),
    },
    {
      label: "Profiles",
      icon: UserIcon,
      active: showProfiles,
      onClick: () => setShowProfiles(true),
    },
    {
      label: "Tools",
      icon: WrenchIcon,
      active: showTools,
      onClick: () => setShowTools(true),
    },
    {
      label: "Settings",
      icon: SettingsIcon,
      active: showSettings,
      onClick: () => setShowSettings(true),
    },
  ];

  return (
    <div className="app flex flex-col h-screen w-screen overflow-hidden relative text-text-primary">
      <TitleBar buttons={overlayButtons} projectName={projectName} />

      <Group orientation="horizontal" className="flex-1 overflow-hidden">
        <Panel
          panelRef={leftPanelRef}
          defaultSize="20%"
          minSize="15%"
          maxSize="40%"
          collapsible
          collapsedSize="0%"
          onCollapse={handleLeftCollapse}
          onExpand={handleLeftExpand}
          onResize={handleLeftResize}
          className="flex flex-col bg-bg-1/85 backdrop-blur-md border-r border-border overflow-hidden z-10"
        >
          <SessionSidebar />
        </Panel>
        <Separator className="w-1 bg-transparent hover:bg-accent/30 active:bg-accent/50 transition-colors data-[resize-handle-state=drag]:bg-accent/50" />
        <Panel
          panelRef={centerPanelRef}
          defaultSize="60%"
          minSize="30%"
          collapsible
          collapsedSize="0%"
          onCollapse={handleCenterCollapse}
          onExpand={handleCenterExpand}
          onResize={handleCenterResize}
          className="flex flex-col min-w-0 overflow-hidden bg-transparent relative z-10"
        >
          <div className="flex-1 overflow-y-auto px-6 py-4 flex flex-col gap-4 relative">
            {messages.length === 0 && (
              <div className="text-center text-text-muted mt-8 flex flex-col gap-3 items-center">
                <div className="text-base font-semibold text-text-tertiary tracking-wide">
                  Brioche Desktop
                </div>
                <div className="text-sm text-text-muted">
                  Type a message or use /help for commands
                </div>
              </div>
            )}
            {messages.map((msg) =>
              msg.role === "tool_request" || msg.role === "tool_result" ? (
                <div
                  id={`msg-${msg.id}`}
                  key={msg.id}
                  className={`flex flex-col gap-2 relative animate-fadeIn max-w-[85%] ${
                    msg.role === "tool_request" ? "self-end" : "self-start"
                  }`}
                >
                  <ToolCallMessage message={msg} />
                </div>
              ) : (
                <div
                  id={`msg-${msg.id}`}
                  key={msg.id}
                  className={`flex flex-col gap-2 relative animate-fadeIn max-w-[85%] ${
                    msg.role === "user"
                      ? "self-end"
                      : msg.role === "assistant"
                        ? "self-start max-w-[90%]"
                        : "self-center max-w-150 w-full"
                  }`}
                >
                  <div className="flex items-center gap-2 mb-0.5 px-1">
                    <span className="text-xs font-bold uppercase tracking-wider text-text-muted">
                      {msg.role}
                    </span>
                  </div>
                  <div
                    className={`px-4 py-3 rounded-lg leading-relaxed text-sm wrap-break-word relative overflow-hidden ${
                      msg.role === "user"
                        ? "bg-user-bg text-text-primary border border-accent/15 shadow-md"
                        : msg.role === "assistant"
                          ? "bg-assistant-bg text-text-primary border border-border shadow-md"
                          : msg.role === "system"
                            ? "bg-system-bg text-text-secondary border border-border rounded-lg text-xs font-mono"
                            : "bg-error-bg text-error-text border border-error-border rounded-lg text-sm"
                    }`}
                  >
                    <div className="message-content">{msg.content}</div>
                  </div>
                </div>
              ),
            )}
            {isLoading && (
              <div className="flex flex-col gap-2 relative animate-fadeIn max-w-[85%] self-start">
                <div className="flex items-center gap-2 mb-0.5 px-1">
                  <span className="text-xs font-bold uppercase tracking-wider text-text-muted">
                    assistant
                  </span>
                </div>
                <div className="px-4 py-3 rounded-lg leading-relaxed text-sm wrap-break-word relative overflow-hidden bg-assistant-bg text-text-primary border border-border shadow-md">
                  <div className="text-text-muted italic">Thinking...</div>
                </div>
              </div>
            )}
            <div ref={messagesEndRef} />
          </div>

          <form
            className="input-bar flex gap-3 px-4 py-3 bg-bg-1/80 backdrop-blur-md border-t border-border shrink-0 relative"
            onSubmit={handleSubmit}
          >
            <div className="flex items-center gap-2">
              <Tooltip label="Clear history">
                <button
                  type="button"
                  className="btn-icon w-8 h-8 text-fg-secondary hover:text-fg-primary"
                  onClick={handleClearChat}
                  aria-label="Clear history"
                >
                  <ClearIcon className="w-4 h-4" />
                </button>
              </Tooltip>
              <Tooltip label="Attach file/folder">
                <button
                  type="button"
                  className="btn-icon w-8 h-8 text-fg-secondary hover:text-fg-primary"
                  onClick={handleAttach}
                  aria-label="Attach file/folder"
                >
                  <PaperclipIcon className="w-4 h-4" />
                </button>
              </Tooltip>
              <Tooltip label="Send image">
                <button
                  type="button"
                  className="btn-icon w-8 h-8 text-fg-secondary hover:text-fg-primary"
                  onClick={handleImage}
                  aria-label="Send image"
                >
                  <ImageIcon className="w-4 h-4" />
                </button>
              </Tooltip>
            </div>
            <textarea
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Type a message or /help..."
              disabled={isLoading}
              className="flex-1 bg-bg-2 border border-border text-text-primary px-4 py-3 rounded-lg text-sm outline-none resize-none min-h-11 max-h-50 leading-relaxed transition-all duration-200 placeholder:text-text-dim disabled:opacity-50 disabled:cursor-not-allowed focus:border-accent-dim focus:bg-bg-3 focus:ring-2 focus:ring-accent-glow"
              rows={1}
            />
            <button
              type="submit"
              className="px-6 py-3 bg-accent text-white rounded-lg cursor-pointer font-semibold text-sm tracking-wide transition-all duration-200 flex items-center justify-center relative overflow-hidden disabled:opacity-40 disabled:cursor-not-allowed disabled:bg-bg-5 hover:bg-accent-hover hover:shadow-lg hover:shadow-accent-glow/20 hover:-translate-y-0.5 active:translate-y-0"
              disabled={isLoading || !input.trim()}
              aria-label="Send message"
            >
              <SendIcon className="w-4 h-4" />
            </button>
          </form>
        </Panel>
        <Separator className="w-1 bg-transparent hover:bg-accent/30 active:bg-accent/50 transition-colors data-[resize-handle-state=drag]:bg-accent/50" />
        <Panel
          panelRef={rightPanelRef}
          defaultSize="20%"
          minSize="15%"
          maxSize="40%"
          collapsible
          collapsedSize="0%"
          onCollapse={handleRightCollapse}
          onExpand={handleRightExpand}
          onResize={handleRightResize}
          className="flex flex-col bg-bg-1/85 backdrop-blur-md border-l border-border overflow-hidden z-10"
        >
          <FileExplorer />
        </Panel>
      </Group>

      <Footer
        panels={{ left: panels.left, center: showChat, right: panels.right }}
        panelWidths={panelWidths}
        onToggleLeft={toggleLeftPanel}
        onToggleRight={toggleRightPanel}
        onToggleChat={toggleCenterPanel}
      />

      {showSettings && <SettingsPanel onClose={() => setShowSettings(false)} />}
      {showSkills && <SkillsPanel onClose={() => setShowSkills(false)} />}
      {showProfiles && <ProfilesPanel onClose={() => setShowProfiles(false)} />}
      {showMemory && <MemoryPanel onClose={() => setShowMemory(false)} />}
      {showTools && <ToolsPanel onClose={() => setShowTools(false)} />}

      <CommandPalette
        isOpen={showPalette}
        onClose={() => setShowPalette(false)}
        commands={paletteCommands}
      />
      <MessageSearch
        messages={messages.map((m) => ({
          id: m.id,
          role: m.role,
          content: m.content,
          timestamp: Date.now(),
        }))}
        onJumpTo={(id) => {
          const el = document.getElementById(`msg-${id}`);
          el?.scrollIntoView({ behavior: "smooth", block: "center" });
        }}
        isOpen={showMessageSearch}
        onClose={() => setShowMessageSearch(false)}
      />
    </div>
  );
}
