import { AnimatePresence } from "framer-motion";
import { Group, Panel, Separator } from "react-resizable-panels";
import { Search, Brain, BookOpen, User, Wrench, Settings } from "lucide-react";
import Footer from "../Footer";
import SessionSidebar from "../SessionSidebar";
import FileExplorer from "../FileExplorer";
import ToolsPanel from "../ToolsPanel";
import SettingsPanel from "../SettingsPanel";
import SkillsPanel from "../SkillsPanel";
import MemoryPanel from "../MemoryPanel";
import ProfilesPanel from "../ProfilesPanel";
import CommandPalette from "../CommandPalette";
import MessageSearch from "../MessageSearch";
import { TitleBar } from "../TitleBar";
import { useAppState } from "../../hooks/app/useAppState";
import { useChatActions } from "../../hooks/app/useChatActions";
import { useCommandPaletteActions } from "../../hooks/app/useCommandPaletteActions";
import MessageList from "./MessageList";
import ChatInput from "./ChatInput";

export default function App() {
  const {
    panels,
    showChat,
    showSettings,
    showSkills,
    showProfiles,
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
    toggleRightPanel,
    setShowSettings,
    setShowSkills,
    setShowProfiles,
    setShowMemory,
    setShowTools,
    setShowPalette,
    setShowMessageSearch,
  } = useAppState();

  const {
    messages,
    input,
    isLoading,
    setInput,
    messagesEndRef,
    handleSubmit,
    handleKeyDown,
    handleAttach,
    handleStop,
    handleClearChat,
    handleExportChat,
    pendingAttachments,
    removeAttachment,
  } = useChatActions();

  const paletteCommands = useCommandPaletteActions({
    handleNewSession,
    handleClearChat,
    setShowSettings,
    toggleSessions: toggleLeftPanel,
    toggleFiles: toggleRightPanel,
    handleExportChat,
  });

  const overlayButtons = [
    {
      label: "Search messages (Ctrl+Shift+F)",
      icon: Search,
      active: showMessageSearch,
      onClick: () => setShowMessageSearch(true),
    },
    {
      label: "Memory",
      icon: Brain,
      active: showMemory,
      onClick: () => setShowMemory(true),
    },
    {
      label: "Skills",
      icon: BookOpen,
      active: showSkills,
      onClick: () => setShowSkills(true),
    },
    {
      label: "Profiles",
      icon: User,
      active: showProfiles,
      onClick: () => setShowProfiles(true),
    },
    {
      label: "Tools",
      icon: Wrench,
      active: showTools,
      onClick: () => setShowTools(true),
    },
    {
      label: "Settings",
      icon: Settings,
      active: showSettings,
      onClick: () => setShowSettings(true),
    },
  ];

  return (
    <div className="app flex flex-col h-screen w-screen overflow-hidden relative text-fg-primary">
      <TitleBar buttons={overlayButtons} projectName={projectName} />

      <Group orientation="horizontal" className="flex-1 overflow-hidden">
        <Panel
          panelRef={leftPanelRef}
          defaultSize="20%"
          minSize="15%"
          maxSize="40%"
          collapsible
          collapsedSize="0%"
          onResize={handleLeftResize}
          className="flex flex-col bg-bg-surface/20 backdrop-blur-lg border-r border-border/45 overflow-hidden z-10 panel-transition"
        >
          <SessionSidebar />
        </Panel>
        <Separator className="w-px bg-border/45 hover:bg-accent/40 active:bg-accent/60 transition-colors data-[resize-handle-state=drag]:bg-accent/60" />
        <Panel
          panelRef={centerPanelRef}
          defaultSize="60%"
          minSize="30%"
          collapsible
          collapsedSize="0%"
          onResize={handleCenterResize}
          className="flex flex-col min-w-0 overflow-hidden bg-bg-base/10 backdrop-blur-[5px] relative z-10"
        >
          <MessageList
            messages={messages}
            isLoading={isLoading}
            messagesEndRef={messagesEndRef}
          />
          <ChatInput
            input={input}
            setInput={setInput}
            isLoading={isLoading}
            handleSubmit={handleSubmit}
            handleKeyDown={handleKeyDown}
            handleAttach={handleAttach}
            handleStop={handleStop}
            pendingAttachments={pendingAttachments}
            removeAttachment={removeAttachment}
          />
        </Panel>
        <Separator className="w-px bg-border/45 hover:bg-accent/40 active:bg-accent/60 transition-colors data-[resize-handle-state=drag]:bg-accent/60" />
        <Panel
          panelRef={rightPanelRef}
          defaultSize="20%"
          minSize="15%"
          maxSize="40%"
          collapsible
          collapsedSize="0%"
          onResize={handleRightResize}
          className="flex flex-col bg-bg-surface/20 backdrop-blur-lg border-l border-border/45 overflow-hidden z-10 panel-transition"
        >
          <FileExplorer />
        </Panel>
      </Group>

      <Footer
        panels={{ left: panels.left, center: showChat, right: panels.right }}
        onToggleLeft={toggleLeftPanel}
        onToggleRight={toggleRightPanel}
      />

      <AnimatePresence>
        {showSettings && (
          <SettingsPanel
            key="settings"
            onClose={() => setShowSettings(false)}
          />
        )}
        {showSkills && (
          <SkillsPanel key="skills" onClose={() => setShowSkills(false)} />
        )}
        {showProfiles && (
          <ProfilesPanel
            key="profiles"
            onClose={() => setShowProfiles(false)}
          />
        )}
        {showMemory && (
          <MemoryPanel key="memory" onClose={() => setShowMemory(false)} />
        )}
        {showTools && (
          <ToolsPanel key="tools" onClose={() => setShowTools(false)} />
        )}
      </AnimatePresence>

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
