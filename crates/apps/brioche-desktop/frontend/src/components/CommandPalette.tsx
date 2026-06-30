import { useState, useCallback, useEffect, useRef, useMemo } from "react";
import {
  SearchIcon,
  CommandIcon,
  MessageIcon,
  SettingsIcon,
  TerminalIcon,
  PlusIcon,
  ExportIcon,
  XIcon,
} from "./Icons";

interface Command {
  id: string;
  label: string;
  shortcut?: string;
  group: string;
  icon: React.ReactNode;
  action: () => void;
}

interface CommandPaletteProps {
  isOpen: boolean;
  onClose: () => void;
  commands: Command[];
}

export default function CommandPalette({
  isOpen,
  onClose,
  commands,
}: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const filtered = useMemo(() => {
    if (!query.trim()) return commands;
    const q = query.toLowerCase();
    return commands.filter(
      (c) =>
        c.label.toLowerCase().includes(q) ||
        c.group.toLowerCase().includes(q) ||
        (c.shortcut && c.shortcut.toLowerCase().includes(q)),
    );
  }, [query, commands]);

  const grouped = useMemo(() => {
    const groups: Record<string, Command[]> = {};
    filtered.forEach((cmd) => {
      if (!groups[cmd.group]) groups[cmd.group] = [];
      groups[cmd.group].push(cmd);
    });
    return groups;
  }, [filtered]);

  useEffect(() => {
    if (isOpen) {
      setQuery("");
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [isOpen]);

  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
        return;
      }
      if (e.key === "Enter") {
        const flat = Object.values(grouped).flat();
        if (flat[selectedIndex]) {
          flat[selectedIndex].action();
          onClose();
        }
        return;
      }
      if (e.key === "ArrowDown") {
        e.preventDefault();
        const total = Object.values(grouped).flat().length;
        setSelectedIndex((i) => (i + 1) % total);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        const total = Object.values(grouped).flat().length;
        setSelectedIndex((i) => (i - 1 + total) % total);
        return;
      }
    },
    [grouped, selectedIndex, onClose],
  );

  if (!isOpen) return null;

  let flatIndex = 0;

  return (
    <div
      className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-start justify-center z-2000 animate-fadeIn pt-[15vh]"
      onClick={onClose}
    >
      <div
        className="bg-bg-1 border border-border rounded-lg w-140 max-w-[90vw] max-h-[60vh] flex flex-col overflow-hidden animate-slideDown shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-5 border-b border-border flex items-center gap-3">
          <SearchIcon className="w-4 h-4 text-text-muted shrink-0" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Type a command or search..."
            className="w-full bg-transparent border-none text-text-primary text-base outline-none placeholder:text-text-muted font-sans py-1"
          />
        </div>
        <div className="flex-1 overflow-y-auto p-4">
          {Object.entries(grouped).map(([groupName, items]) => (
            <div key={groupName} className="mb-3">
              <div className="text-xs font-bold uppercase tracking-wider text-text-muted px-3 py-2 select-none">
                {groupName}
              </div>
              {items.map((cmd) => {
                const isSelected = flatIndex === selectedIndex;
                const idx = flatIndex++;
                return (
                  <div
                    key={cmd.id}
                    className={`flex items-center gap-3 px-4 py-3 rounded-sm cursor-pointer transition-all text-text-secondary text-sm hover:bg-accent/10 hover:text-text-primary ${
                      isSelected
                        ? "bg-accent/10 text-text-primary border-l-2 border-accent"
                        : ""
                    }`}
                    onClick={() => {
                      cmd.action();
                      onClose();
                    }}
                    onMouseEnter={() => setSelectedIndex(idx)}
                  >
                    <div className="w-5 h-5 flex items-center justify-center text-text-muted shrink-0 [&_svg]:w-3.5 [&_svg]:h-3.5">
                      {cmd.icon}
                    </div>
                    <div className="flex-1">{cmd.label}</div>
                    {cmd.shortcut && (
                      <div className="text-xs text-text-muted font-mono bg-bg-3 px-1.5 py-0.5 rounded">
                        {cmd.shortcut}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          ))}
          {filtered.length === 0 && (
            <div className="p-8 text-center text-text-muted text-sm">
              No commands found
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// Helper to build commands
export function buildCommands(handlers: {
  newSession: () => void;
  clearChat: () => void;
  openSettings: () => void;
  toggleSessions: () => void;
  toggleFiles: () => void;
  exportChat: () => void;
}): Command[] {
  return [
    {
      id: "new-session",
      label: "New Session",
      shortcut: "⌘N",
      group: "Session",
      icon: <PlusIcon />,
      action: handlers.newSession,
    },
    {
      id: "clear-chat",
      label: "Clear Chat",
      shortcut: "⌘K C",
      group: "Session",
      icon: <XIcon />,
      action: handlers.clearChat,
    },
    {
      id: "export-chat",
      label: "Export Chat",
      shortcut: "⌘E",
      group: "Session",
      icon: <ExportIcon />,
      action: handlers.exportChat,
    },
    {
      id: "toggle-sessions",
      label: "Toggle Sessions Panel",
      shortcut: "⌘B",
      group: "View",
      icon: <MessageIcon />,
      action: handlers.toggleSessions,
    },
    {
      id: "toggle-files",
      label: "Toggle Files Panel",
      shortcut: "⌘J",
      group: "View",
      icon: <TerminalIcon />,
      action: handlers.toggleFiles,
    },
    {
      id: "open-settings",
      label: "Settings",
      shortcut: "⌘,",
      group: "Settings",
      icon: <SettingsIcon />,
      action: handlers.openSettings,
    },
  ];
}
