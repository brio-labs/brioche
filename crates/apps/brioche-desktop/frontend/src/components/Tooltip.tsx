import type { ReactElement, ReactNode } from "react";
import * as RadixTooltip from "@radix-ui/react-tooltip";
import { cn } from "./ui/lib";

/**
 * Props for the Tooltip component.
 *
 * Refs: I-Ui-OverlayCohesion
 */
interface TooltipProps {
	children: ReactElement;
	label: ReactNode;
	side?: RadixTooltip.TooltipContentProps["side"];
	align?: RadixTooltip.TooltipContentProps["align"];
}

/**
 * Renders an accessible, collision-aware tooltip on hover or focus of its child.
 *
 * The tooltip is rendered in a portal and automatically flips to the opposite
 * side when it would overflow the viewport, so it is never clipped by parent
 * `overflow: hidden` containers.
 *
 * Refs: I-Ui-OverlayCohesion
 */
export default function Tooltip({
	children,
	label,
	side = "top",
	align = "center",
}: TooltipProps) {
	return (
		<RadixTooltip.Root delayDuration={150}>
			<RadixTooltip.Trigger asChild>{children}</RadixTooltip.Trigger>
			<RadixTooltip.Portal>
				<RadixTooltip.Content
					side={side}
					align={align}
					sideOffset={8}
					collisionPadding={8}
					avoidCollisions
					className={cn(
						"z-[9999] rounded-md border border-border bg-bg-elevated px-2 py-1 shadow-lg",
						"text-xs text-fg-secondary whitespace-nowrap",
						"data-[state=delayed-open]:animate-fadeIn data-[state=instant-open]:animate-fadeIn",
					)}
					role="tooltip"
				>
					{label}
					<RadixTooltip.Arrow className="fill-bg-elevated" />
				</RadixTooltip.Content>
			</RadixTooltip.Portal>
		</RadixTooltip.Root>
	);
}
