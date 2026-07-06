import { SectionHeader, SectionHeaderTitle } from "../ui";

export function SessionHeader() {
  return (
    <SectionHeader>
      <div className="flex items-center gap-2">
        <SectionHeaderTitle>Sessions</SectionHeaderTitle>
      </div>
    </SectionHeader>
  );
}
