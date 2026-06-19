import { useEffect } from "react";
import { useToolsStore } from "../stores/panelStores";
import PanelOverlay from "./PanelOverlay";
import { WrenchIcon } from "./Icons";

interface ToolsPanelProps {
	onClose?: () => void;
}

export default function ToolsPanel({ onClose = () => {} }: ToolsPanelProps) {
	const {
		tools,
		isLoading,
		error,
		isTauriAvailable,
		loadTools,
		toggleTool,
	} = useToolsStore();

	useEffect(() => {
		loadTools();
	}, [loadTools]);

	const groups = tools.reduce<Record<string, import("../ipc").ToolDescriptor[]>>((acc, tool) => {
		const category = tool.category || "uncategorized";
		if (!acc[category]) acc[category] = [];
		acc[category].push(tool);
		return acc;
	}, {});

	return (
		<PanelOverlay
			title="Tools"
			icon={<WrenchIcon className="w-4 h-4" />}
			onClose={onClose}
			panelClassName="bg-bg-1 border border-border rounded-lg w-[600px] max-w-[95vw] max-h-[85vh] flex flex-col overflow-hidden animate-slideUp shadow-2xl z-[1001]"
		>
			<div className="flex-1 overflow-y-auto p-5 flex flex-col gap-5">
				{error && <div className="bg-error-bg text-[#e8a0a0] border border-error-border px-3.5 py-2.5 rounded-lg text-xs">{error}</div>}
				{!isTauriAvailable && !error && (
					<div className="bg-error-bg text-[#e8a0a0] border border-error-border px-3.5 py-2.5 rounded-lg text-xs">
						Tools preview mode: live tool list requires the Tauri desktop app.
					</div>
				)}
				{tools.length === 0 && !error && (
					<div className="text-center text-text-muted py-12 text-sm select-none">No tools available</div>
				)}
				{Object.entries(groups).map(([category, items]) => (
					<div key={category} className="flex flex-col gap-2.5 [&_h3]:text-[11px] [&_h3]:font-bold [&_h3]:text-text-secondary [&_h3]:uppercase [&_h3]:tracking-wider [&_h3]:border-b [&_h3]:border-border [&_h3]:pb-1.5">
						<h3>{category}</h3>
						<div className="flex flex-col gap-1.5">
							{items.map((tool) => (
								<div key={tool.id} className="p-3 bg-bg-2/30 border border-border rounded-lg flex items-center justify-between gap-4 transition-all hover:border-border-hover">
									<div className="flex flex-col gap-0.5 min-w-0">
										<span className="font-mono text-xs font-semibold text-text-primary">{tool.name}</span>
										<span className="text-[11px] text-text-secondary truncate" title={tool.description}>
											{tool.description}
										</span>
									</div>
									<label className="flex items-center gap-2 cursor-pointer text-xs text-text-secondary select-none shrink-0 [&_input]:rounded [&_input]:bg-bg-2 [&_input]:border-border [&_input]:text-accent [&_input]:focus:ring-accent-glow [&_input]:cursor-pointer">
										<input
											type="checkbox"
											checked={tool.enabled}
											onChange={(e) => toggleTool(tool.id, e.target.checked)}
										/>
										<span>{tool.enabled ? "On" : "Off"}</span>
									</label>
								</div>
							))}
						</div>
					</div>
				))}
			</div>
		</PanelOverlay>
	);
}
