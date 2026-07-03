import { Tag, Folder } from "lucide-react";
import type { Skill } from "../../ipc";

interface SkillDetailsProps {
	selectedSkill: Skill;
	skillContent: string;
}

/// Renders the selected skill view with metadata and content.
///
/// Refs: I-Ui-SkillDetails
export default function SkillDetails({
	selectedSkill,
	skillContent,
}: SkillDetailsProps) {
	return (
		<>
			<div className="mb-2 flex flex-col gap-2 border-b border-border pb-4">
				<h3 className="text-lg font-semibold text-fg-primary">
					{selectedSkill.name}
				</h3>
				<div className="flex flex-wrap select-none items-center gap-3 text-xs text-fg-muted">
					<span className="flex items-center gap-1 font-medium">
						<Folder className="h-3.5 w-3.5" />
						{selectedSkill.category}
					</span>
					{selectedSkill.version && (
						<span className="flex items-center gap-1 border-l border-border pl-3 font-medium">
							v{selectedSkill.version}
						</span>
					)}
					{selectedSkill.author && (
						<span className="flex items-center gap-1 border-l border-border pl-3 font-medium">
							by {selectedSkill.author}
						</span>
					)}
					{selectedSkill.license && (
						<span className="flex items-center gap-1 border-l border-border pl-3 font-medium">
							{selectedSkill.license}
						</span>
					)}
				</div>
				{selectedSkill.tags.length > 0 && (
					<div className="mt-1 flex flex-wrap gap-2">
						{selectedSkill.tags.map((tag) => (
							<span
								key={tag}
								className="inline-flex items-center gap-1 rounded-sm border border-border bg-bg-elevated px-2 py-0.5 font-mono text-xs font-medium text-fg-secondary"
							>
								<Tag className="h-3.5 w-3.5" />
								{tag}
							</span>
						))}
					</div>
				)}
			</div>
			<div className="flex-1 overflow-auto rounded-none border border-border bg-bg-base p-4">
				<pre className="font-mono text-xs leading-relaxed whitespace-pre-wrap text-fg-secondary">
					{skillContent}
				</pre>
			</div>
		</>
	);
}
