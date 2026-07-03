import { useEffect } from "react";
import { useToolsStore, isUserTool } from "../../stores/panelStores";
import type { ToolDescriptor } from "../../ipc";
import PanelOverlay from "../PanelOverlay";
import { Wrench, AlertTriangle, Terminal } from "lucide-react";
import ToolListItem from "./ToolListItem";
import { EmptyState } from "../ui";

/// Props for the tools management panel.
///
/// Refs: I-Shell-Runtime-OnlyIO
interface ToolsPanelProps {
	onClose?: () => void;
}

/// Renders the tools panel with category grouping and enablement toggles.
///
/// Loads the available tool descriptors and lets users toggle which tools are
/// active. Displays warnings when user-defined tools are disabled or enabled.
///
/// Refs: I-Shell-Runtime-OnlyIO
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
