import { Button, cn } from "../ui";
import type { SettingsSection } from "../../ipc";
import { SearchBar } from "../PanelOverlay";

interface SettingsSectionListProps {
  sections: SettingsSection[];
  selectedSectionId: string | null;
  onSelectSection: (sectionId: string) => void;
  search: string;
  onSearchChange: (search: string) => void;
  searchPlaceholder?: string;
}

/// Renders the left sidebar list of settings sections with a search filter.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function SettingsSectionList({
  sections,
  selectedSectionId,
  onSelectSection,
  search,
  onSearchChange,
  searchPlaceholder = "Search settings...",
}: SettingsSectionListProps) {
  return (
    <div className="flex flex-col w-60 min-w-60 border-r border-border bg-bg-base/20">
      <SearchBar
        placeholder={searchPlaceholder}
        value={search}
        onChange={onSearchChange}
        containerClassName="border-b border-border rounded-none bg-bg-base/30 px-5 py-4"
      />
      <div className="flex flex-1 flex-col gap-1 overflow-y-auto p-4">
        {sections.map((section) => (
          <Button
            key={section.id}
            type="button"
            variant="ghost"
            onClick={() => onSelectSection(section.id)}
            className={cn(
              "w-full justify-start px-4 py-2.5 text-sm font-semibold transition-all duration-150",
              selectedSectionId === section.id
                ? "border-l-2 border-accent bg-accent/15 text-fg-primary"
                : "text-fg-secondary hover:bg-bg-elevated/50 hover:text-fg-primary",
            )}
          >
            {section.title}
          </Button>
        ))}
      </div>
    </div>
  );
}
