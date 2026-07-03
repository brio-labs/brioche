import type { Profile } from "../../ipc";
import { cn } from "../ui";

interface ProfileListItemProps {
	profile: Profile;
	isActive: boolean;
	isSelected: boolean;
	onSelect: (name: string) => void;
}

/// Renders a single profile entry in the selectable list.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function ProfileListItem({
	profile,
	isActive,
	isSelected,
	onSelect,
}: ProfileListItemProps) {
	return (
		<div
			tabIndex={0}
			role="button"
			onClick={() => onSelect(profile.name)}
			onKeyDown={(e) => {
				if (e.key === "Enter" || e.key === " ") {
					e.preventDefault();
					onSelect(profile.name);
				}
			}}
			className={cn(
				"flex cursor-pointer flex-col gap-1.5 rounded-lg border p-3 transition-all duration-200 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent-glow",
				isSelected
					? "border-accent-dim/40 bg-accent/10 shadow-sm"
					: "border-transparent bg-transparent hover:border-border/60 hover:bg-bg-elevated/30",
			)}
		>
			<div className="flex items-center justify-between gap-2">
				<span className="truncate text-xs font-semibold text-fg-primary">
					{profile.display_name || profile.name}
				</span>
				{isActive && (
					<span className="shrink-0 rounded border border-success-border bg-success-bg px-1.5 py-0.5 text-xs font-bold uppercase text-success-text select-none">
						Active
					</span>
				)}
			</div>
			<div className="truncate text-xs text-fg-secondary">
				{profile.provider} / {profile.model}
			</div>
		</div>
	);
}
