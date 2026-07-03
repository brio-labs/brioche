import type { Skill } from "../../ipc";
import { cn } from "../ui/lib";
import { TrashIcon } from "../Icons";

interface SkillListItemProps {
	skill: Skill;
	isSelected: boolean;
	onSelect: (skill: Skill) => void;
	onToggleEnabled: (skill: Skill) => void;
	onDelete: (skill: Skill) => void;
}

/// Renders a selectable skill card in the list.
///
/// Refs: I-Ui-SkillListItem
export default function SkillListItem({
	skill,
	isSelected,
	onSelect,
	onToggleEnabled,
	onDelete,
}: SkillListItemProps) {
	return (
		<div
			className={cn(
				"flex cursor-pointer flex-col gap-1.5 rounded-lg border p-3 transition-all duration-200",
				isSelected
					? "border-accent-dim/40 bg-accent/10 shadow-sm"
					: "border-transparent bg-transparent hover:border-border/60 hover:bg-bg-elevated/30",
			)}
		>
			<div
				className="flex flex-1 flex-col gap-1"
				onClick={() => onSelect(skill)}
			>
				<div className="flex items-center justify-between gap-1 text-xs font-semibold text-fg-primary">
					{skill.name}
					<span
						className={cn(
							"rounded px-1.5 py-0.5 text-xs font-bold uppercase select-none",
							skill.enabled
								? "bg-success-bg border border-success-border text-success-text"
								: "bg-bg-subtle border border-border text-fg-muted",
						)}
					>
						{skill.enabled ? "on" : "off"}
					</span>
				</div>
				<div className="line-clamp-2 text-xs text-fg-secondary">
					{skill.description}
				</div>
			</div>
			<div className="mt-1 flex select-none items-center gap-2 text-xs text-fg-muted">
				<span className="rounded border border-border bg-bg-subtle px-1.5 py-0.5 font-mono text-xs font-medium text-fg-tertiary">
					{skill.category}
				</span>
				{skill.version && (
					<span className="rounded border border-border bg-bg-subtle px-1.5 py-0.5 font-mono text-xs font-medium text-fg-tertiary">
						v{skill.version}
					</span>
				)}
				<button
					type="button"
					className="ml-auto cursor-pointer rounded border border-border bg-bg-highlight px-2.5 py-1 text-xs font-medium text-fg-secondary transition-all hover:border-accent-dim/45 hover:bg-bg-subtle hover:text-fg-primary"
					onClick={() => onToggleEnabled(skill)}
					title={skill.enabled ? "Disable" : "Enable"}
				>
					{skill.enabled ? "Disable" : "Enable"}
				</button>
				<button
					type="button"
					className="flex shrink-0 cursor-pointer items-center justify-center rounded border border-transparent p-1.5 text-fg-muted transition-all hover:border-border hover:bg-bg-highlight hover:text-error-text"
					onClick={() => onDelete(skill)}
					title="Delete"
					aria-label={`Delete skill ${skill.name}`}
				>
					<TrashIcon className="h-3.5 w-3.5" />
				</button>
			</div>
		</div>
	);
}
