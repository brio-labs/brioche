import { AnimatePresence } from "framer-motion";
import { Group, Panel, Separator } from "react-resizable-panels";
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
import { useAppState } from "../../hooks/app/useAppState";
import { useChatActions } from "../../hooks/app/useChatActions";
import { useCommandPaletteActions } from "../../hooks/app/useCommandPaletteActions";
import ChatPanel from "./ChatPanel";
import TitleBarWrapper from "./TitleBarWrapper";

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

  return (
    <div className="app flex flex-col h-screen w-screen overflow-hidden relative text-text-primary">
      <TitleBarWrapper
        projectName={projectName}
        showMessageSearch={showMessageSearch}
        setShowMessageSearch={setShowMessageSearch}
        showMemory={showMemory}
        setShowMemory={setShowMemory}
        showSkills={showSkills}
        setShowSkills={setShowSkills}
        showProfiles={showProfiles}
        setShowProfiles={setShowProfiles}
        showTools={showTools}
        setShowTools={setShowTools}
        showSettings={showSettings}
        setShowSettings={setShowSettings}
      />

      <Group orientation="horizontal" className="flex-1 overflow-hidden">
        <Panel
          panelRef={leftPanelRef}
          defaultSize="20%"
          minSize="15%"
          maxSize="40%"
          collapsible
          collapsedSize="0%"
          onResize={handleLeftResize}
          className="flex flex-col bg-bg-surface border-r border-border overflow-hidden z-10 panel-transition"
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
          onResize={handleCenterResize}
          className="flex flex-col min-w-0 overflow-hidden bg-transparent relative z-10"
        >
          <ChatPanel
            messages={messages}
            isLoading={isLoading}
            input={input}
            setInput={setInput}
            handleSubmit={handleSubmit}
            handleKeyDown={handleKeyDown}
            handleAttach={handleAttach}
            handleStop={handleStop}
            messagesEndRef={messagesEndRef}
            pendingAttachments={pendingAttachments}
            removeAttachment={removeAttachment}
          />
        </Panel>
        <Separator className="w-1 bg-transparent hover:bg-accent/30 active:bg-accent/50 transition-colors data-[resize-handle-state=drag]:bg-accent/50" />
        <Panel
          panelRef={rightPanelRef}
          defaultSize="20%"
          minSize="15%"
          maxSize="40%"
          collapsible
          collapsedSize="0%"
          onResize={handleRightResize}
          className="flex flex-col bg-bg-surface border-l border-border overflow-hidden z-10 panel-transition"
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
