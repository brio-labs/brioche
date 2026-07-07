import { useEffect } from "react";
import { useToolsStore, isUserTool } from "../../stores/toolsStore";
import type { ToolDescriptor } from "../../ipc";
import PanelOverlay from "../PanelOverlay";
import { Wrench, AlertTriangle, Terminal } from "lucide-react";
import { EmptyState } from "../ui";

interface ToolsPanelProps {
	onClose?: () => void;
}

function ToolListItem({
	tool,
	onToggle,
}: {
	tool: ToolDescriptor;
	onToggle: (enabled: boolean) => void;
}) {
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

export default function ToolsPanel({ onClose = () => {} }: ToolsPanelProps) {
	const {
		tools,
		isLoading,
		error,
		isTauriAvailable,
		userToolsEnabled,
		loadTools,
		toggleTool,
	} = useToolsStore();

	useEffect(() => {
		loadTools();
	}, [loadTools]);

	const groups = tools.reduce<Record<string, ToolDescriptor[]>>(
		(acc, tool) => {
			const category = tool.category || "uncategorized";
			if (!acc[category]) acc[category] = [];
			acc[category].push(tool);
			return acc;
		},
		{},
	);
	const hasUserTools = tools.some((tool) => isUserTool(tool));

	return (
		<PanelOverlay
			title="Tools"
			icon={<Wrench className="h-4 w-4" />}
			onClose={onClose}
			size="sm"
		>
			{error && (
				<div className="rounded-sm border border-error-border bg-error-bg p-4 text-xs text-error-text">
					{error}
				</div>
			)}
			{!isTauriAvailable && !error && (
				<div className="rounded-sm border border-error-border bg-error-bg p-4 text-xs text-error-text">
					Tools preview mode: live tool list requires the Tauri desktop app.
				</div>
			)}
			{tools.length === 0 && !error && (
				<EmptyState
					icon={Wrench}
					title="No tools available"
					description="Install or enable tools to extend capability."
				/>
			)}
			{hasUserTools && !userToolsEnabled && (
				<div className="flex items-start gap-2 rounded-sm border border-warning-border bg-warning-bg p-4 text-xs text-warning-text">
					<AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
					<span>
						User-defined tools are disabled for security. Enable them in
						Settings &gt; Tools.
					</span>
				</div>
			)}
			{hasUserTools && userToolsEnabled && (
				<div className="flex items-start gap-2 rounded-sm border border-border bg-bg-highlight p-4 text-xs text-fg-secondary">
					<Terminal className="mt-0.5 h-4 w-4 shrink-0" />
					<span>User-defined tools can execute arbitrary commands.</span>
				</div>
			)}
			{Object.entries(groups).map(([category, items]) => (
				<div
					key={category}
					className="flex flex-col gap-2 [&_h3]:border-b [&_h3]:border-border [&_h3]:pb-2 [&_h3]:text-xs [&_h3]:font-semibold [&_h3]:text-fg-primary"
				>
					<h3>{category}</h3>
					<div className="flex flex-col gap-2">
						{items.map((tool) => (
							<ToolListItem
								key={tool.id}
								tool={tool}
								onToggle={(enabled) => toggleTool(tool.id, enabled)}
							/>
						))}
					</div>
				</div>
			))}
			{isLoading && tools.length === 0 && (
				<div className="py-12 text-center text-sm text-fg-muted">
					Loading tools...
				</div>
			)}
		</PanelOverlay>
	);
}
