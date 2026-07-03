//! Brioche Desktop context menu component.
//!
//! A custom, theme-aware context menu built with React and the existing `cn`
//! utility. Supports right-click positioning, keyboard dismissal, and
//! accessible item rows.
//!
//! Refs: I-Shell-Runtime-OnlyIO

import * as React from "react";
import { createPortal } from "react-dom";
import { Slot } from "@radix-ui/react-slot";
import { cn } from "./lib";

/// State and controls shared by every ContextMenu sub-component.
///
/// Refs: I-Shell-Runtime-OnlyIO
export interface ContextMenuContextValue {
	isOpen: boolean;
	position: { x: number; y: number } | null;
	open: (x: number, y: number) => void;
	close: () => void;
}

const ContextMenuContext = React.createContext<ContextMenuContextValue | null>(
	null,
);

function useContextMenuContext(): ContextMenuContextValue {
	const context = React.useContext(ContextMenuContext);
	if (!context) {
		throw new Error(
			"ContextMenu sub-components must be rendered inside a ContextMenu.",
		);
	}
	return context;
}

/// Hook for consuming the surrounding ContextMenu state. Returns the current
/// open state, cursor position, and open/close helpers.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function useContextMenu(): ContextMenuContextValue {
	return useContextMenuContext();
}

/// Props for the root ContextMenu wrapper.
///
/// Refs: I-Shell-Runtime-OnlyIO
export interface ContextMenuProps {
	children: React.ReactNode;
	isOpen?: boolean;
	defaultOpen?: boolean;
	onClose?: () => void;
}

/// Root wrapper that holds context-menu state and exposes it to descendants.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function ContextMenu({
	children,
	isOpen: controlledOpen,
	defaultOpen = false,
	onClose,
}: ContextMenuProps) {
	const [internalOpen, setInternalOpen] = React.useState(defaultOpen);
	const [position, setPosition] = React.useState<{ x: number; y: number } | null>(
		null,
	);
	const isControlled = controlledOpen !== undefined;
	const isOpen = isControlled ? controlledOpen : internalOpen;

	const open = React.useCallback(
		(x: number, y: number) => {
			setPosition({ x, y });
			if (!isControlled) setInternalOpen(true);
		},
		[isControlled],
	);

	const close = React.useCallback(() => {
		if (!isControlled) setInternalOpen(false);
		onClose?.();
	}, [isControlled, onClose]);

	const value = React.useMemo<ContextMenuContextValue>(
		() => ({ isOpen, position, open, close }),
		[isOpen, position, open, close],
	);

	return (
		<ContextMenuContext.Provider value={value}>
			{children}
		</ContextMenuContext.Provider>
	);
}

/// Props for the element that receives the right-click handler.
///
/// Refs: I-Shell-Runtime-OnlyIO
export interface ContextMenuTriggerProps
	extends React.HTMLAttributes<HTMLDivElement> {
	asChild?: boolean;
}

/// Right-click target that opens the context menu at the cursor location.
///
/// Refs: I-Shell-Runtime-OnlyIO
export const ContextMenuTrigger = React.forwardRef<
	HTMLDivElement,
	ContextMenuTriggerProps
>(
	(
		{ asChild = false, className, children, onContextMenu, ...props },
		ref,
	) => {
		const { isOpen, open } = useContextMenuContext();
		const Comp = asChild ? Slot : "div";

		return (
			<Comp
				ref={ref}
				className={cn(className)}
				aria-haspopup="menu"
				aria-expanded={isOpen}
				onContextMenu={(event: React.MouseEvent<HTMLDivElement>) => {
					event.preventDefault();
					open(event.clientX, event.clientY);
					onContextMenu?.(event);
				}}
				{...props}
			>
				{children}
			</Comp>
		);
	},
);
ContextMenuTrigger.displayName = "ContextMenuTrigger";

/// Props for the floating context-menu surface.
///
/// Refs: I-Shell-Runtime-OnlyIO
export interface ContextMenuContentProps
	extends React.HTMLAttributes<HTMLDivElement> {}

/// Floating menu surface positioned at the click coordinates and constrained
/// to the viewport.
///
/// Refs: I-Shell-Runtime-OnlyIO
export const ContextMenuContent = React.forwardRef<
	HTMLDivElement,
	ContextMenuContentProps
>(({ className, children, ...props }, forwardedRef) => {
	const { isOpen, position, close } = useContextMenuContext();
	const contentRef = React.useRef<HTMLDivElement>(null);
	const [adjustedPosition, setAdjustedPosition] = React.useState<{
		x: number;
		y: number;
	} | null>(null);

	React.useImperativeHandle(forwardedRef, () => contentRef.current!);

	React.useLayoutEffect(() => {
		if (!position || !contentRef.current) {
			setAdjustedPosition(position);
			return;
		}

		const rect = contentRef.current.getBoundingClientRect();
		const padding = 8;
		let x = position.x;
		let y = position.y;

		if (x + rect.width > window.innerWidth - padding) {
			x = window.innerWidth - rect.width - padding;
		}
		if (y + rect.height > window.innerHeight - padding) {
			y = window.innerHeight - rect.height - padding;
		}
		x = Math.max(padding, x);
		y = Math.max(padding, y);

		setAdjustedPosition({ x, y });
	}, [position, isOpen]);

	React.useEffect(() => {
		if (!isOpen) return;

		const handlePointerDown = (event: PointerEvent) => {
			if (
				contentRef.current &&
				!contentRef.current.contains(event.target as Node)
			) {
				close();
			}
		};

		const handleKeyDown = (event: KeyboardEvent) => {
			if (event.key === "Escape") close();
		};

		document.addEventListener("pointerdown", handlePointerDown);
		document.addEventListener("keydown", handleKeyDown);

		return () => {
			document.removeEventListener("pointerdown", handlePointerDown);
			document.removeEventListener("keydown", handleKeyDown);
		};
	}, [isOpen, close]);

	if (!isOpen || !adjustedPosition) return null;

	return createPortal(
		<div
			ref={contentRef}
			role="menu"
			className={cn(
				"fixed z-[9999] min-w-[160px]",
				"rounded-md border border-border bg-bg-surface shadow-lg backdrop-blur-sm",
				"p-1",
				"animate-fadeIn",
				"outline-none",
				className,
			)}
			style={{ left: adjustedPosition.x, top: adjustedPosition.y }}
			{...props}
		>
			{children}
		</div>,
		document.body,
	);
});
ContextMenuContent.displayName = "ContextMenuContent";

/// Props for a single context-menu row.
///
/// Refs: I-Shell-Runtime-OnlyIO
export interface ContextMenuItemProps
	extends React.HTMLAttributes<HTMLDivElement> {
	inset?: boolean;
	destructive?: boolean;
	disabled?: boolean;
}

/// Clickable menu row. Closes the menu when selected unless the click is
/// disabled.
///
/// Refs: I-Shell-Runtime-OnlyIO
export const ContextMenuItem = React.forwardRef<
	HTMLDivElement,
	ContextMenuItemProps
>(
	(
		{
			className,
			inset = false,
			destructive = false,
			disabled = false,
			children,
			onClick,
			onKeyDown,
			...props
		},
		ref,
	) => {
		const { close } = useContextMenuContext();

		return (
			<div
				ref={ref}
				role="menuitem"
				tabIndex={disabled ? -1 : 0}
				aria-disabled={disabled}
				className={cn(
					"px-3 py-2 text-sm text-fg-secondary cursor-pointer transition-colors",
					"rounded-sm",
					"hover:bg-bg-elevated hover:text-fg-primary",
					"focus-visible:outline-none focus-visible:bg-bg-elevated focus-visible:text-fg-primary",
					inset && "pl-8",
					destructive &&
						"text-error-text hover:bg-error-bg focus-visible:bg-error-bg",
					disabled && "pointer-events-none opacity-50 cursor-default",
					className,
				)}
				onClick={(event) => {
					if (disabled) return;
					onClick?.(event);
					close();
				}}
				onKeyDown={(event) => {
					onKeyDown?.(event);
					if (disabled) return;
					if (event.key === "Enter" || event.key === " ") {
						event.preventDefault();
						onClick?.(event as unknown as React.MouseEvent<HTMLDivElement>);
						close();
					}
				}}
				{...props}
			>
				{children}
			</div>
		);
	},
);
ContextMenuItem.displayName = "ContextMenuItem";

/// Props for the visual divider between menu groups.
///
/// Refs: I-Shell-Runtime-OnlyIO
export interface ContextMenuSeparatorProps
	extends React.HTMLAttributes<HTMLDivElement> {}

/// Horizontal divider between context-menu sections.
///
/// Refs: I-Shell-Runtime-OnlyIO
export const ContextMenuSeparator = React.forwardRef<
	HTMLDivElement,
	ContextMenuSeparatorProps
>(({ className, ...props }, ref) => (
	<div
		ref={ref}
		role="separator"
		className={cn("h-px bg-border my-1", className)}
		{...props}
	/>
));
ContextMenuSeparator.displayName = "ContextMenuSeparator";

/// Props for a labelled grouping of menu items.
///
/// Refs: I-Shell-Runtime-OnlyIO
export interface ContextMenuGroupProps {
	children: React.ReactNode;
	label?: string;
	className?: string;
}

/// Group of related items with an optional uppercase label.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function ContextMenuGroup({
	children,
	label,
	className,
}: ContextMenuGroupProps) {
	return (
		<div
			className={cn("py-1", className)}
			role="group"
			aria-label={label}
		>
			{label && (
				<div className="px-3 py-1.5 text-xs font-semibold text-fg-muted uppercase tracking-wider">
					{label}
				</div>
			)}
			{children}
		</div>
	);
}
