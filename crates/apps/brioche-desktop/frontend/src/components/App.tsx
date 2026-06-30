import { useEffect, useRef, useCallback, useState, useMemo } from "react";
import { useChatStore } from "../store";
import { useSessionStore } from "../stores/sessionStore";
import { useSettingsStore } from "../stores/settingsStore";
import { useFileStore } from "../stores/fileStore";
import { open } from "@tauri-apps/plugin-dialog";
import { sendMessage, attachReference, sendImage } from "../ipc";
import Footer from "./Footer";
import Tooltip from "./Tooltip";
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

  useEffect(() => {
    const workingDir = (settings.ui as Record<string, unknown> | undefined)
      ?.working_dir as string | undefined;
    if (workingDir) {
      loadDirectory(workingDir);
    }
  }, [settings, loadDirectory]);

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
        toggleSessions: () => setPanels((p) => ({ ...p, left: !p.left })),
        toggleFiles: () => setPanels((p) => ({ ...p, right: !p.right })),
        exportChat: handleExportChat,
      }),
    [handleNewSession, handleClearChat, handleExportChat],
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
      <header className="flex items-center justify-between px-4 h-13 bg-bg-1/70 backdrop-blur-md border-b border-border shrink-0">
        <span className="text-sm font-semibold text-fg-secondary tracking-wider">
          Brioche
        </span>
        <div className="flex items-center gap-3">
          {overlayButtons.map(({ label, icon: Icon, active, onClick }) => (
            <Tooltip key={label} label={label}>
              <button
                type="button"
                onClick={onClick}
                className={`dock-button ${active ? "dock-button-active" : ""}`}
                aria-pressed={active}
                aria-label={label}
              >
                <Icon className="w-4 h-4" />
              </button>
            </Tooltip>
          ))}
        </div>
      </header>

      <div className="flex flex-row flex-1 overflow-hidden">
        <div
          className={`left-sidebar flex flex-col bg-bg-1/85 backdrop-blur-md border-r border-border overflow-hidden transition-all duration-300 ease-out z-10 max-[900px]:absolute max-[900px]:top-0 max-[900px]:bottom-0 max-[900px]:z-20 max-[900px]:left-0 ${panels.left ? "w-70 min-w-70 opacity-100" : "w-0 min-w-0 opacity-0 border-r-0 pointer-events-none"}`}
        >
          <SessionSidebar />
        </div>

        <div
          className={`flex flex-col min-w-0 overflow-hidden bg-transparent relative z-10 transition-all duration-300 ease-out ${showChat ? "flex-1" : "w-0 opacity-0 pointer-events-none border-r-0"}`}
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
        </div>

        <div
          className={`right-sidebar flex flex-col bg-bg-1/85 backdrop-blur-md border-l border-border overflow-hidden transition-all duration-300 ease-out z-10 max-[900px]:absolute max-[900px]:top-0 max-[900px]:bottom-0 max-[900px]:z-20 max-[900px]:right-0 ${panels.right ? "w-70 min-w-70 opacity-100" : "w-0 min-w-0 opacity-0 border-l-0 pointer-events-none"}`}
        >
          <FileExplorer />
        </div>
      </div>

      <Footer
        panels={panels}
        setPanels={setPanels}
        showChat={showChat}
        setShowChat={setShowChat}
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
