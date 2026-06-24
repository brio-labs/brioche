import { useEffect, useState } from "react";
import { getFooterMetrics } from "../ipc";
import type { FooterMetric } from "../ipc";
import { useTauriEvent } from "../hooks/useTauriSync";

export default function Footer() {
	const [metrics, setMetrics] = useState<FooterMetric[]>([]);

	const load = async () => {
		try {
			const data = await getFooterMetrics();
			setMetrics(data);
		} catch (err) {
			console.error("Failed to load footer metrics:", err);
		}
	};

	useEffect(() => {
		void load();
		const interval = setInterval(() => {
			void load();
		}, 1000);
		return () => clearInterval(interval);
	}, []);

	// Reactively refresh footer metrics on new chat messages
	useTauriEvent("chat-message", () => {
		void load();
	});

	return (
		<footer className="flex items-center justify-end gap-4 px-4 bg-bg-0/90 border-t border-border text-[11px] text-text-muted shrink-0 select-none h-7 z-10">
			{metrics.length === 0 ? (
				<div className="flex items-center gap-1">
					<span className="font-medium">Brioche</span>
					<span className="font-mono text-text-secondary">0.1.0</span>
				</div>
			) : (
				metrics.map((m) => (
					<div
						key={m.id}
						className="flex items-center gap-1"
						title={m.tooltip || undefined}
					>
						<span className="font-medium">{m.label}</span>
						<span className="font-mono text-text-secondary">{m.value}</span>
					</div>
				))
			)}
		</footer>
	);
}
