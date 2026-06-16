import { useCallback, useEffect, useState } from "react";
import { listTools, setToolEnabled } from "../ipc";
import type { ToolDescriptor } from "../ipc";
import { XIcon, WrenchIcon } from "./Icons";

interface ToolsPanelProps {
	onClose?: () => void;
}

export default function ToolsPanel({ onClose = () => {} }: ToolsPanelProps) {
	const [tools, setTools] = useState<ToolDescriptor[]>([]);
	const [error, setError] = useState<string | null>(null);
	const [isTauriAvailable, setIsTauriAvailable] = useState(true);

	useEffect(() => {
		setIsTauriAvailable(
			typeof window !== 'undefined' &&
				typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !==
					'undefined',
		);
	}, []);

	const load = useCallback(async () => {
		setError(null);
		if (!isTauriAvailable) {
			setTools([]);
			return;
		}
		try {
			const data = await listTools();
			setTools(data);
		} catch (err) {
			setError(String(err));
		}
	}, [isTauriAvailable]);

	useEffect(() => {
		load();
	}, [load]);

	const toggle = useCallback(
		async (id: string, enabled: boolean) => {
			if (!isTauriAvailable) {
				setError('Tool toggling requires the Tauri desktop runtime.');
				return;
			}
			try {
				await setToolEnabled(id, enabled);
				await load();
			} catch (err) {
				setError(String(err));
			}
		},
		[load, isTauriAvailable],
	);

	const groups = tools.reduce<Record<string, ToolDescriptor[]>>((acc, tool) => {
		const category = tool.category || "uncategorized";
		if (!acc[category]) acc[category] = [];
		acc[category].push(tool);
		return acc;
	}, {});

	return (
		<div className="tools-overlay" onClick={onClose}>
			<div className="tools-panel" onClick={(e) => e.stopPropagation()}>
				<div className="tools-panel-header">
					<h2>
						<WrenchIcon />
						Tools
					</h2>
					<button
						type="button"
						className="icon-btn"
						onClick={onClose}
						aria-label="Close"
					>
						<XIcon />
					</button>
				</div>
                <div className="tools-panel-body">
                    {error && <div className="tools-error">{error}</div>}
                    {!isTauriAvailable && !error && (
                        <div className="tools-error">
                            Tools preview mode: live tool list requires the Tauri desktop app.
                        </div>
                    )}
                    {tools.length === 0 && !error && (
                        <div className="tools-empty">No tools available</div>
                    )}
					{Object.entries(groups).map(([category, items]) => (
						<div key={category} className="tools-category">
							<h3>{category}</h3>
							<div className="tools-list">
								{items.map((tool) => (
									<div key={tool.id} className="tool-item">
										<div className="tool-info">
											<span className="tool-name">{tool.name}</span>
											<span className="tool-description">
												{tool.description}
											</span>
										</div>
										<label className="tool-toggle">
											<input
												type="checkbox"
												checked={tool.enabled}
												onChange={(e) => toggle(tool.id, e.target.checked)}
											/>
											<span>{tool.enabled ? "On" : "Off"}</span>
										</label>
									</div>
								))}
							</div>
						</div>
					))}
				</div>
			</div>
		</div>
	);
}
