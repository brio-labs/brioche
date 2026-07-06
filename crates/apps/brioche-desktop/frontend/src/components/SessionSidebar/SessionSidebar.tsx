import { useCallback } from "react";
import { MessageSquareDashed, Plus, ListFilter, Check } from "lucide-react";
import { useSessionStore } from "../../stores/sessionStore";
import { useSessionGrouping } from "../../hooks/sessionSidebar/useSessionGrouping";
import { SessionGroup } from "./SessionGroup";
import { EmptyState, SectionHeader, SectionHeaderTitle } from "../ui";
import { cn } from "../ui/lib";
import * as PopoverPrimitive from "@radix-ui/react-popover";

/// Renders the session sidebar, listing sessions with grouping, sorting, and creation controls.
///
/// Refs: I-Ui-Sidebar
export default function SessionSidebar() {
  const {
    sessions,
    sortMode,
    setSortMode,
    switchToSession,
    deleteSession,
    createSession,
  } = useSessionStore();

  const handleNewSession = useCallback(async () => {
    await createSession();
  }, [createSession]);

  const groupedSessions = useSessionGrouping(sessions, sortMode);

  return (
    <div className="flex h-full w-full flex-col bg-transparent text-fg-primary">
      <SectionHeader>
        <div className="flex items-center gap-2">
          <SectionHeaderTitle>Sessions</SectionHeaderTitle>
        </div>
      </SectionHeader>

      <div className="p-3 pb-2">
        <button
          type="button"
          onClick={handleNewSession}
          className="flex w-full cursor-pointer items-center justify-center gap-2 rounded-full border border-fg-primary/18 bg-fg-primary/14 px-3 py-2 text-xs font-semibold text-fg-primary shadow-sm transition-colors hover:bg-fg-primary/22 active:bg-fg-primary/28"
        >
          <Plus className="h-4 w-4" />
          New conversation
        </button>
      </div>

      <div className="flex items-center justify-between border-b border-border px-4 py-2">
        <span className="text-xs font-semibold text-fg-secondary">
          Projects
        </span>
        <PopoverPrimitive.Root>
          <PopoverPrimitive.Trigger asChild>
            <button
              type="button"
              className="btn-icon h-7 w-7 text-fg-secondary hover:text-fg-primary"
              title="Sort conversations"
            >
              <ListFilter className="h-4 w-4" />
            </button>
          </PopoverPrimitive.Trigger>
          <PopoverPrimitive.Portal>
            <PopoverPrimitive.Content
              align="end"
              sideOffset={4}
              className="z-[3000] min-w-40 rounded-[8px] border border-border bg-bg-surface p-1.5 shadow-md animate-fadeIn"
            >
              <div className="flex flex-col gap-0.5">
                <span className="px-2 py-1 text-[10px] font-semibold text-fg-muted uppercase tracking-wider">
                  Sort By
                </span>
                <button
                  type="button"
                  onClick={() => setSortMode("date")}
                  className={cn(
                    "flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-xs text-fg-secondary hover:bg-bg-elevated hover:text-fg-primary",
                    sortMode === "date" &&
                      "bg-bg-highlight text-fg-primary font-medium",
                  )}
                >
                  <span className="w-4 flex items-center justify-center">
                    {sortMode === "date" && (
                      <Check className="h-3.5 w-3.5 text-accent" />
                    )}
                  </span>
                  <span>Date</span>
                </button>
                <button
                  type="button"
                  onClick={() => setSortMode("name")}
                  className={cn(
                    "flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-xs text-fg-secondary hover:bg-bg-elevated hover:text-fg-primary",
                    sortMode === "name" &&
                      "bg-bg-highlight text-fg-primary font-medium",
                  )}
                >
                  <span className="w-4 flex items-center justify-center">
                    {sortMode === "name" && (
                      <Check className="h-3.5 w-3.5 text-accent" />
                    )}
                  </span>
                  <span>Name</span>
                </button>
              </div>
            </PopoverPrimitive.Content>
          </PopoverPrimitive.Portal>
        </PopoverPrimitive.Root>
      </div>

      <div className="flex flex-1 flex-col space-y-3 overflow-y-auto py-2">
        {sessions.length > 0 ? (
          Array.from(groupedSessions.entries()).map(([group, items]) => (
            <SessionGroup
              key={group}
              title={group}
              sessions={items}
              switchToSession={switchToSession}
              deleteSession={deleteSession}
            />
          ))
        ) : (
          <EmptyState
            icon={MessageSquareDashed}
            title="No sessions"
            description="Create a session to start a new conversation."
          />
        )}
      </div>
    </div>
  );
}
