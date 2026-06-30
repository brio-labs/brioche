import type React from "react";

interface TooltipProps {
	children: React.ReactElement;
	label: string;
}

export default function Tooltip({ children, label }: TooltipProps) {
	return (
		<div className="group relative inline-flex items-center justify-center">
			{children}
			<span
				className="pointer-events-none absolute bottom-full left-1/2 z-50 mb-2 -translate-x-1/2 whitespace-nowrap rounded-md border border-border bg-bg-elevated px-2 py-1 text-sm font-medium text-fg-secondary opacity-0 shadow-lg transition-opacity duration-150 group-hover:opacity-100 group-focus-within:opacity-100"
				role="tooltip"
			>
				{label}
			</span>
		</div>
	);
}
