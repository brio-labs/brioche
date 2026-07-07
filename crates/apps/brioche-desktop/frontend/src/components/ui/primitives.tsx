//! Brioche Desktop primitive UI components.
//!
//! This module provides thin, theme-aware wrappers around Radix UI primitives
//! and standard HTML elements used throughout the desktop settings panel and
//! future UI surfaces. All components share the Brioche Tailwind color tokens
//! and focus/disabled states.
//!
//! Refs: I-Shell-Runtime-OnlyIO

import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import * as CheckboxPrimitive from "@radix-ui/react-checkbox";
import * as LabelPrimitive from "@radix-ui/react-label";
import * as SeparatorPrimitive from "@radix-ui/react-separator";
import { Check } from "lucide-react";
import { cn } from "./lib";

export interface ButtonProps
	extends React.ButtonHTMLAttributes<HTMLButtonElement> {
	variant?: "default" | "secondary" | "ghost" | "destructive";
	size?: "default" | "sm" | "icon";
	asChild?: boolean;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
	(
		{ className, variant = "default", size = "default", asChild = false, ...props },
		ref,
	) => {
		const Comp = asChild ? Slot : "button";
		return (
			<Comp
				ref={ref}
				className={cn(
					"inline-flex items-center justify-center rounded font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent-glow disabled:pointer-events-none disabled:opacity-50 cursor-pointer",
					variant === "default" &&
						"bg-accent text-white hover:bg-accent-hover shadow-sm shadow-accent-glow/20",
					variant === "secondary" &&
						"bg-transparent border border-border text-text-secondary hover:text-text-primary hover:bg-bg-2 hover:border-border-hover",
					variant === "ghost" &&
						"bg-transparent text-text-secondary hover:text-text-primary hover:bg-bg-2",
					variant === "destructive" &&
						"bg-transparent text-red-400 hover:text-red-300 hover:bg-red-400/10 border border-red-400/30",
					size === "default" && "px-4 py-2 text-xs tracking-wide",
					size === "sm" && "px-2.5 py-1 text-[11px]",
					size === "icon" && "h-8 w-8 p-0",
					className,
				)}
				{...props}
			/>
		);
	},
);
Button.displayName = "Button";

export interface InputProps
	extends React.InputHTMLAttributes<HTMLInputElement> {}

export const Input = React.forwardRef<HTMLInputElement, InputProps>(
	({ className, type, ...props }, ref) => (
		<input
			type={type}
			ref={ref}
			className={cn(
				"flex w-full rounded border border-border bg-bg-2 px-3 py-2 text-[13px] font-mono text-text-primary outline-none transition-all placeholder:text-text-muted focus:border-accent-dim focus:bg-bg-3 focus:ring-1 focus:ring-accent-glow disabled:cursor-not-allowed disabled:opacity-50",
				className,
			)}
			{...props}
		/>
	),
);
Input.displayName = "Input";

export interface TextareaProps
	extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {}

export const Textarea = React.forwardRef<HTMLTextAreaElement, TextareaProps>(
	({ className, ...props }, ref) => (
		<textarea
			ref={ref}
			className={cn(
				"flex min-h-[80px] w-full rounded border border-border bg-bg-2 px-3 py-2 text-xs font-mono text-text-primary outline-none transition-all placeholder:text-text-muted focus:border-accent-dim focus:bg-bg-3 focus:ring-1 focus:ring-accent-glow disabled:cursor-not-allowed disabled:opacity-50 resize-y",
				className,
			)}
			{...props}
		/>
	),
);
Textarea.displayName = "Textarea";

export const Label = React.forwardRef<
	React.ElementRef<typeof LabelPrimitive.Root>,
	React.ComponentPropsWithoutRef<typeof LabelPrimitive.Root>
>(({ className, ...props }, ref) => (
	<LabelPrimitive.Root
		ref={ref}
		className={cn(
			"text-[11px] font-bold uppercase tracking-wider text-text-secondary",
			className,
		)}
		{...props}
	/>
));
Label.displayName = LabelPrimitive.Root.displayName;

export const Checkbox = React.forwardRef<
	React.ElementRef<typeof CheckboxPrimitive.Root>,
	React.ComponentPropsWithoutRef<typeof CheckboxPrimitive.Root>
>(({ className, ...props }, ref) => (
	<CheckboxPrimitive.Root
		ref={ref}
		className={cn(
			"peer h-4 w-4 shrink-0 rounded border border-border bg-bg-2 ring-offset-bg-1 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent-glow disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-accent data-[state=checked]:border-accent data-[state=checked]:text-white cursor-pointer",
			className,
		)}
		{...props}
	>
		<CheckboxPrimitive.Indicator
			className={cn("flex items-center justify-center text-current")}
		>
			<Check className="h-3 w-3" />
		</CheckboxPrimitive.Indicator>
	</CheckboxPrimitive.Root>
));
Checkbox.displayName = CheckboxPrimitive.Root.displayName;

export const Separator = React.forwardRef<
	React.ElementRef<typeof SeparatorPrimitive.Root>,
	React.ComponentPropsWithoutRef<typeof SeparatorPrimitive.Root>
>(
	(
		{ className, orientation = "horizontal", decorative = true, ...props },
		ref,
	) => (
		<SeparatorPrimitive.Root
			ref={ref}
			decorative={decorative}
			orientation={orientation}
			className={cn(
				"shrink-0 bg-border",
				orientation === "horizontal" ? "h-[1px] w-full" : "h-full w-[1px]",
				className,
			)}
			{...props}
		/>
	),
);
Separator.displayName = SeparatorPrimitive.Root.displayName;

export interface SidePanelProps extends React.HTMLAttributes<HTMLDivElement> {
	open: boolean;
	side: "left" | "right";
}

export function SidePanel({
	open,
	side,
	className,
	children,
	...props
}: SidePanelProps) {
	return (
		<div
			className={cn(
				"flex flex-col bg-bg-1/85 backdrop-blur-md overflow-hidden transition-all duration-300 ease-out z-[1] max-[900px]:absolute max-[900px]:top-0 max-[900px]:bottom-0 max-[900px]:z-10",
				side === "left" && "border-r border-border max-[900px]:left-0",
				side === "right" && "border-l border-border max-[900px]:right-0",
				open
					? "w-[280px] min-w-[280px] opacity-100"
					: "w-0 min-w-0 opacity-0 pointer-events-none",
				!open && side === "left" && "border-r-0",
				!open && side === "right" && "border-l-0",
				className,
			)}
			{...props}
		>
			{children}
		</div>
	);
}

export interface ToolbarButtonProps extends ButtonProps {
	active?: boolean;
}

export function ToolbarButton({
	active = false,
	className,
	children,
	...props
}: ToolbarButtonProps) {
	return (
		<Button
			variant="ghost"
			className={cn(
				"flex items-center gap-2 px-3 py-2 bg-transparent text-text-muted hover:text-text-secondary rounded text-[11px] font-medium tracking-wider transition-all duration-200",
				active && "bg-bg-3 text-text-secondary",
				className,
			)}
			{...props}
		>
			{children}
		</Button>
	);
}

export function IconToolbarButton({
	className,
	children,
	...props
}: ButtonProps) {
	return (
		<Button
			variant="ghost"
			size="icon"
			className={cn(
				"rounded bg-transparent text-text-muted hover:text-text-secondary hover:bg-bg-3 active:bg-bg-4 transition-all duration-200",
				className,
			)}
			{...props}
		>
			{children}
		</Button>
	);
}

export function FormFieldStack({
	className,
	children,
	...props
}: React.HTMLAttributes<HTMLDivElement>) {
	return (
		<div
			className={cn(
				"flex flex-col gap-2.5 p-3.5 bg-bg-2/30 border border-border rounded-lg",
				"[&_input]:bg-bg-2 [&_input]:border [&_input]:border-border [&_input]:text-text-primary [&_input]:text-xs [&_input]:px-2.5 [&_input]:py-1.5 [&_input]:rounded [&_input]:outline-none [&_input]:focus:border-accent-dim/60",
				"[&_textarea]:bg-bg-2 [&_textarea]:border [&_textarea]:border-border [&_textarea]:text-text-primary [&_textarea]:text-xs [&_textarea]:px-2.5 [&_textarea]:py-1.5 [&_textarea]:rounded [&_textarea]:outline-none [&_textarea]:focus:border-accent-dim/60",
				"[&_select]:bg-bg-2 [&_select]:border [&_select]:border-border [&_select]:text-text-primary [&_select]:text-xs [&_select]:px-2.5 [&_select]:py-1.5 [&_select]:rounded [&_select]:outline-none",
				className,
			)}
			{...props}
		>
			{children}
		</div>
	);
}

export function ActionRow({
	className,
	children,
	...props
}: React.HTMLAttributes<HTMLDivElement>) {
	return (
		<div
			className={cn(
				"flex justify-end gap-2 [&_button]:px-3 [&_button]:py-1.5 [&_button]:text-xs [&_button]:font-medium [&_button]:rounded [&_button]:cursor-pointer [&_button:first-child]:bg-accent [&_button:first-child]:hover:bg-accent-hover [&_button:first-child]:text-white [&_button:last-child]:bg-transparent [&_button:last-child]:border [&_button:last-child]:border-border [&_button:last-child]:text-text-secondary [&_button:last-child]:hover:bg-bg-2",
				className,
			)}
			{...props}
		>
			{children}
		</div>
	);
}

export interface ContextMenuItemProps extends React.HTMLAttributes<HTMLDivElement> {
	variant?: "default" | "danger";
}

export function ContextMenuItem({
	variant = "default",
	className,
	children,
	...props
}: ContextMenuItemProps) {
	return (
		<div
			className={cn(
				"px-4 py-2 text-[13px] cursor-pointer flex items-center gap-[var(--space-2)] transition-colors duration-150",
				variant === "default" &&
					"text-[var(--text-primary)] hover:bg-[var(--accent-dim)] hover:text-white",
				variant === "danger" &&
					"text-[#ff5555] hover:bg-[var(--error-bg)] hover:text-[#ff8888]",
				className,
			)}
			{...props}
		>
			{children}
		</div>
	);
}

export function ProtectedSettingsCard({
	protected: isProtected,
	className,
	children,
	...props
}: React.HTMLAttributes<HTMLDivElement> & { protected?: boolean }) {
	return (
		<div
			className={cn(
				"flex flex-col gap-2",
				isProtected && "p-3.5 bg-bg-2/20 border border-border/50 rounded-lg",
				className,
			)}
			{...props}
		>
			{children}
		</div>
	);
}
