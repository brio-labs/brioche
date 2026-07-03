import type { ReactNode } from "react";
import { cn } from "../ui/lib";

/**
 * Props for the floating modal overlay.
 *
 * Refs: I-Ui-OverlayCohesion
 */
interface ModalOverlayProps {
	isOpen: boolean;
	onClose: () => void;
	children: ReactNode;
}

/**
 * Backdrop and centered modal container for floating overlays.
 *
 * Refs: I-Ui-OverlayCohesion
 */
export function ModalOverlay({
	isOpen,
	onClose,
	children,
}: ModalOverlayProps) {
	if (!isOpen) return null;
	return (
		<div
			className={cn(
				"fixed inset-0 z-2000 flex items-start justify-center",
				"bg-black/60 backdrop-blur-sm pt-[15vh]",
				"animate-fadeIn",
			)}
			onClick={onClose}
		>
			<div
				className={cn(
					"flex flex-col w-140 max-w-[90vw] max-h-[60vh] overflow-hidden rounded-lg border border-border bg-bg-surface",
					"shadow-2xl animate-slideDown",
				)}
				onClick={(e) => e.stopPropagation()}
			>
				{children}
			</div>
		</div>
	);
}
