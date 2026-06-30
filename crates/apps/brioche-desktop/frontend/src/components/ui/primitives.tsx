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
					"inline-flex items-center justify-center rounded font-medium transition-all focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent-glow focus-visible:ring-offset-2 focus-visible:ring-offset-bg-surface disabled:pointer-events-none disabled:opacity-50 cursor-pointer",
					variant === "default" &&
						"bg-accent text-accent-text hover:bg-accent-hover active:bg-accent-dim shadow-sm shadow-accent-glow/20",
					variant === "secondary" &&
						"bg-transparent border border-border text-fg-secondary hover:text-fg-primary hover:bg-bg-elevated hover:border-border-hover active:bg-bg-highlight active:border-border-hover",
					variant === "ghost" &&
						"bg-transparent text-fg-secondary hover:text-fg-primary hover:bg-bg-elevated active:bg-bg-highlight",
					variant === "destructive" &&
						"bg-transparent text-red-400 hover:text-red-300 hover:bg-red-400/10 active:bg-red-400/20 border border-red-400/30",
					size === "default" && "px-3 py-2 text-xs tracking-wide",
					size === "sm" && "px-2 py-1 text-xs",
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
			className={cn("input-field", className)}
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
			className={cn("textarea-field", className)}
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
			"text-xs font-bold uppercase tracking-wider text-fg-secondary",
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
			"peer h-4 w-4 shrink-0 rounded border border-border bg-bg-elevated ring-offset-bg-surface focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent-glow disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-accent data-[state=checked]:border-accent data-[state=checked]:text-accent-text cursor-pointer",
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
				orientation === "horizontal" ? "h-px w-full" : "h-full w-px",
				className,
			)}
			{...props}
		/>
	),
);
Separator.displayName = SeparatorPrimitive.Root.displayName;
