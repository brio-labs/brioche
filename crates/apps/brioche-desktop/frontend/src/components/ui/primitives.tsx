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

/// Props for the Button primitive.
///
/// Supports multiple visual variants, sizes, and optional child composition via
/// Radix Slot. Refs: I-Shell-Runtime-OnlyIO
export interface ButtonProps
	extends React.ButtonHTMLAttributes<HTMLButtonElement> {
	variant?: "default" | "secondary" | "ghost" | "destructive";
	size?: "default" | "sm" | "icon";
	asChild?: boolean;
}

/// Theme-aware button with default `type="button"` and visible interaction
/// states.
///
/// Refs: I-Shell-Runtime-OnlyIO
export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
	(
		{ className, variant = "default", size = "default", asChild = false, type, ...props },
		ref,
	) => {
		const Comp = asChild ? Slot : "button";
		return (
			<Comp
				ref={ref}
				type={asChild ? undefined : (type ?? "button")}
				className={cn(
					"inline-flex items-center justify-center rounded-md font-medium",
					"px-3 py-2 text-xs tracking-wide",
					"focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent-glow focus-visible:ring-offset-2 focus-visible:ring-offset-bg-surface",
					"disabled:pointer-events-none disabled:opacity-50 cursor-pointer",
					"transition-all",
					variant === "default" &&
						"bg-accent text-accent-text hover:bg-accent-hover active:bg-accent-dim shadow-sm shadow-accent-glow/20",
					variant === "secondary" &&
						"bg-transparent border border-border text-fg-secondary hover:text-fg-primary hover:bg-bg-elevated hover:border-border-hover active:bg-bg-highlight active:border-border-hover",
					variant === "ghost" &&
						"bg-transparent text-fg-secondary hover:text-fg-primary hover:bg-bg-elevated active:bg-bg-highlight",
					variant === "destructive" &&
						"bg-transparent border border-error-border text-error-text hover:bg-error-bg active:bg-error-bg/80",
					size === "default" && "px-3 py-2",
					size === "sm" && "px-2 py-1",
					size === "icon" && "h-8 w-8 p-0",
					className,
				)}
				{...props}
			/>
		);
	},
);
Button.displayName = "Button";

/// Props for the Input primitive.
///
/// Refs: I-Shell-Runtime-OnlyIO
export interface InputProps
	extends React.InputHTMLAttributes<HTMLInputElement> {}

/// Theme-aware text input that delegates to the shared `input-field` utility.
///
/// Refs: I-Shell-Runtime-OnlyIO
export const Input = React.forwardRef<HTMLInputElement, InputProps>(
	({ className, type, ...props }, ref) => (
		<input
			type={type}
			ref={ref}
			className={cn("input-field", className)}
			{...props}
		/>
	),
);
Input.displayName = "Input";

/// Props for the Textarea primitive.
///
/// Refs: I-Shell-Runtime-OnlyIO
export interface TextareaProps
	extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {}

/// Theme-aware textarea that delegates to the shared `textarea-field` utility.
///
/// Refs: I-Shell-Runtime-OnlyIO
export const Textarea = React.forwardRef<HTMLTextAreaElement, TextareaProps>(
	({ className, ...props }, ref) => (
		<textarea
			ref={ref}
			className={cn("textarea-field", className)}
			{...props}
		/>
	),
);
Textarea.displayName = "Textarea";

/// Theme-aware label for form controls.
///
/// Refs: I-Shell-Runtime-OnlyIO
export const Label = React.forwardRef<
	React.ElementRef<typeof LabelPrimitive.Root>,
	React.ComponentPropsWithoutRef<typeof LabelPrimitive.Root>
>(({ className, ...props }, ref) => (
	<LabelPrimitive.Root
		ref={ref}
		className={cn(
			"text-xs font-bold uppercase tracking-wider text-fg-secondary",
			className,
		)}
		{...props}
	/>
));
Label.displayName = LabelPrimitive.Root.displayName;

/// Theme-aware checkbox with visible checked, focus, and disabled states.
///
/// Refs: I-Shell-Runtime-OnlyIO
export const Checkbox = React.forwardRef<
	React.ElementRef<typeof CheckboxPrimitive.Root>,
	React.ComponentPropsWithoutRef<typeof CheckboxPrimitive.Root>
>(({ className, ...props }, ref) => (
	<CheckboxPrimitive.Root
		ref={ref}
		className={cn(
			"peer h-4 w-4 shrink-0 rounded-sm cursor-pointer",
			"border border-border bg-bg-elevated",
			"ring-offset-bg-surface focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent-glow",
			"disabled:cursor-not-allowed disabled:opacity-50",
			"data-[state=checked]:bg-accent data-[state=checked]:border-accent data-[state=checked]:text-accent-text",
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

/// Theme-aware horizontal or vertical separator.
///
/// Refs: I-Shell-Runtime-OnlyIO
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
				orientation === "horizontal" ? "h-px w-full" : "h-full w-px",
				className,
			)}
			{...props}
		/>
	),
);
Separator.displayName = SeparatorPrimitive.Root.displayName;
