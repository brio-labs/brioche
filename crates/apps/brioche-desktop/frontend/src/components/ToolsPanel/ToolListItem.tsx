import type { ToolDescriptor } from "../../ipc";

interface ToolListItemProps {
	tool: ToolDescriptor;
	onToggle: (enabled: boolean) => void;
}

/// Renders a single tool entry with an enablement toggle.
///
/// Refs: I-Shell-Runtime-OnlyIO
export default function ToolListItem({ tool, onToggle }: ToolListItemProps) {
	return (
		<div className="flex items-center justify-between gap-4 rounded-none border border-border bg-bg-elevated p-3 transition-all hover:border-border-hover">
			<div className="flex min-w-0 flex-col gap-0.5">
				<span className="font-mono text-xs font-semibold text-fg-primary">
					{tool.name}
				</span>
				<span
					className="truncate text-xs text-fg-secondary"
					title={tool.description}
				>
					{tool.description}
				</span>
			</div>
			<label className="flex shrink-0 items-center gap-2 py-1 text-xs text-fg-secondary select-none cursor-pointer [&_input]:rounded-sm [&_input]:border-border [&_input]:bg-bg-elevated [&_input]:text-accent [&_input]:cursor-pointer [&_input]:focus:ring-accent-glow">
				<input
					type="checkbox"
					checked={tool.enabled}
					onChange={(e) => onToggle(e.target.checked)}
				/>
				<span>{tool.enabled ? "On" : "Off"}</span>
			</label>
		</div>
	);
}
