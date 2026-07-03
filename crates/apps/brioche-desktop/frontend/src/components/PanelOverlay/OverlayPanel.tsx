import type { ReactNode } from "react";
import { X } from "lucide-react";
import { cn } from "../ui/lib";

/**
 * Props for the overlay panel layout.
 *
 * Refs: I-Ui-OverlayCohesion
 */
interface OverlayPanelProps {
	title: string;
	icon?: ReactNode;
	onClose: () => void;
	children: ReactNode;
	headerActions?: ReactNode;
	size?: "sm" | "md" | "lg" | "xl";
	padded?: boolean;
}

const sizeClasses: Record<NonNullable<OverlayPanelProps["size"]>, string> = {
	sm: "w-150",
	md: "w-200",
	lg: "w-[850px]",
	xl: "w-250",
};

/**
 * Reusable modal/overlay layout that centralizes backdrop clicks, animations,
 * header structures, and close-button concerns.
 *
 * Refs: I-Ui-OverlayCohesion
 */
export default function OverlayPanel({
	title,
	icon,
	onClose,
	children,
	headerActions,
	size = "md",
	padded = true,
}: OverlayPanelProps) {
	return (
		<div className="panel-backdrop" onClick={onClose}>
			<div
				className={cn(
					"panel",
					sizeClasses[size],
					"max-w-[95vw] max-h-[85vh] z-1001",
				)}
				onClick={(e) => e.stopPropagation()}
			>
				<div className="panel-header">
					<h2 className="flex items-center text-sm font-semibold text-fg-primary">
						{icon && (
							<span className="mr-2.5 flex items-center text-accent">
								{icon}
							</span>
						)}
						<span>{title}</span>
					</h2>
					<div className="flex items-center gap-2">
						{headerActions}
						<button
							type="button"
							className="btn-icon w-7 h-7"
							onClick={onClose}
							aria-label="Close panel"
						>
							<X className="w-4 h-4" />
						</button>
					</div>
				</div>
				{padded ? (
					<div className="flex flex-col flex-1 min-h-0 gap-4 overflow-y-auto p-6">
						{children}
					</div>
				) : (
					children
				)}
			</div>
		</div>
	);
}
