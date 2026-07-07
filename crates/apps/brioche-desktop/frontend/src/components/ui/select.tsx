//! Brioche Desktop select components.
//!
//! Provides a single-select wrapper around Radix UI Select and a custom
//! multi-select popover built on Radix Popover. Both share the Brioche
//! Tailwind token set and are used by the settings panel.
//!
//! Refs: I-Shell-Runtime-OnlyIO

import * as React from "react";
import * as PopoverPrimitive from "@radix-ui/react-popover";
import * as SelectPrimitive from "@radix-ui/react-select";
import { Check, ChevronDown } from "lucide-react";
import { cn } from "./lib";
import { Checkbox } from "./primitives";

const Select = SelectPrimitive.Root;
const SelectGroup = SelectPrimitive.Group;
const SelectValue = SelectPrimitive.Value;

/// Trigger for the single-select dropdown. Renders a button with a visible
/// focus state and a chevron indicator.
///
/// Refs: I-Shell-Runtime-OnlyIO
export const SelectTrigger = React.forwardRef<
	React.ElementRef<typeof SelectPrimitive.Trigger>,
	React.ComponentPropsWithoutRef<typeof SelectPrimitive.Trigger>
>(({ className, children, ...props }, ref) => (
	<SelectPrimitive.Trigger
		ref={ref}
		type="button"
		className={cn(
			"flex w-full items-center justify-between cursor-pointer",
			"rounded-md border border-border bg-bg-elevated",
			"px-3 py-2 text-sm font-mono text-fg-primary",
			"outline-none focus:border-accent-dim focus:bg-bg-highlight focus:ring-1 focus:ring-accent-glow",
			"disabled:cursor-not-allowed disabled:opacity-50",
			"hover:border-border-hover hover:bg-bg-highlight",
			"transition-all",
			className,
		)}
		{...props}
	>
		{children}
		<SelectPrimitive.Icon asChild>
			<ChevronDown className="h-4 w-4 shrink-0 opacity-50" />
		</SelectPrimitive.Icon>
	</SelectPrimitive.Trigger>
));
SelectTrigger.displayName = SelectPrimitive.Trigger.displayName;

/// Dropdown content container for the single-select dropdown.
///
/// Refs: I-Shell-Runtime-OnlyIO
export const SelectContent = React.forwardRef<
	React.ElementRef<typeof SelectPrimitive.Content>,
	React.ComponentPropsWithoutRef<typeof SelectPrimitive.Content>
>(({ className, children, position = "popper", ...props }, ref) => (
	<SelectPrimitive.Portal>
		<SelectPrimitive.Content
			ref={ref}
			className={cn(
				"relative z-[3000] overflow-hidden rounded-[8px] border border-border bg-bg-surface shadow-md",
				"max-h-96 min-w-32",
				"text-fg-primary",
				"animate-fadeIn",
				position === "popper" &&
					"data-[side=bottom]:translate-y-1 data-[side=left]:-translate-x-1 data-[side=right]:translate-x-1 data-[side=top]:-translate-y-1",
				className,
			)}
			position={position}
			{...props}
		>
			<SelectPrimitive.Viewport
				className={cn(
					"p-1",
					position === "popper" &&
						"h-[var(--radix-select-trigger-height)] w-full min-w-[var(--radix-select-trigger-width)]",
				)}
			>
				{children}
			</SelectPrimitive.Viewport>
		</SelectPrimitive.Content>
	</SelectPrimitive.Portal>
));
SelectContent.displayName = SelectPrimitive.Content.displayName;

/// Selectable item inside a single-select dropdown.
///
/// Refs: I-Shell-Runtime-OnlyIO
export const SelectItem = React.forwardRef<
	React.ElementRef<typeof SelectPrimitive.Item>,
	React.ComponentPropsWithoutRef<typeof SelectPrimitive.Item>
>(({ className, children, ...props }, ref) => (
	<SelectPrimitive.Item
		ref={ref}
		className={cn(
			"relative flex w-full cursor-pointer select-none items-center rounded-md",
			"py-2 pl-2 pr-8 text-sm text-fg-secondary",
			"outline-none focus:bg-bg-elevated focus:text-fg-primary",
			"hover:bg-bg-highlight hover:text-fg-primary",
			className,
		)}
		{...props}
	>
		<span className="absolute right-2 flex h-3.5 w-3.5 items-center justify-center">
			<SelectPrimitive.ItemIndicator>
				<Check className="h-4 w-4" />
			</SelectPrimitive.ItemIndicator>
		</span>
		<SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
	</SelectPrimitive.Item>
));
SelectItem.displayName = SelectPrimitive.Item.displayName;

/// Visual separator between groups inside a single-select dropdown.
///
/// Refs: I-Shell-Runtime-OnlyIO
export const SelectSeparator = React.forwardRef<
	React.ElementRef<typeof SelectPrimitive.Separator>,
	React.ComponentPropsWithoutRef<typeof SelectPrimitive.Separator>
>(({ className, ...props }, ref) => (
	<SelectPrimitive.Separator
		ref={ref}
		className={cn("-mx-1 my-1 h-px bg-border", className)}
		{...props}
	/>
));
SelectSeparator.displayName = SelectPrimitive.Separator.displayName;

export { Select, SelectGroup, SelectValue };

/// Option entry used by the MultiSelect popover.
///
/// Refs: I-Shell-Runtime-OnlyIO
export interface MultiSelectOption {
	value: string;
	label: string;
}

/// Props for the MultiSelect popover.
///
/// Refs: I-Shell-Runtime-OnlyIO
export interface MultiSelectProps {
	value: string[];
	options: MultiSelectOption[];
	placeholder?: string;
	onChange: (value: string[]) => void;
	className?: string;
}

/// Popover-based multi-select control. Displays selected option labels in the
/// trigger and renders a checkbox list inside the popover.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function MultiSelect({
	value,
	options,
	placeholder = "Select options...",
	onChange,
	className,
}: MultiSelectProps) {
	const toggle = (optionValue: string) => {
		const next = value.includes(optionValue)
			? value.filter((v) => v !== optionValue)
			: [...value, optionValue];
		onChange(next);
	};

	const label =
		value.length === 0
			? placeholder
			: options
					.filter((o) => value.includes(o.value))
					.map((o) => o.label)
					.join(", ");

	return (
		<PopoverPrimitive.Root>
			<PopoverPrimitive.Trigger asChild>
				<button
					type="button"
					className={cn(
						"flex w-full items-center justify-between cursor-pointer",
						"rounded-md border border-border bg-bg-elevated",
						"px-3 py-2 text-sm font-mono",
						"outline-none focus:border-accent-dim focus:bg-bg-highlight focus:ring-1 focus:ring-accent-glow",
						"hover:border-border-hover hover:bg-bg-highlight",
						"transition-all",
						value.length === 0 && "text-fg-muted",
						value.length > 0 && "text-fg-primary",
						className,
					)}
				>
					<span className="truncate">{label}</span>
					<ChevronDown className="h-4 w-4 shrink-0 opacity-50" />
				</button>
			</PopoverPrimitive.Trigger>
			<PopoverPrimitive.Portal>
				<PopoverPrimitive.Content
					align="start"
					sideOffset={4}
					className={cn(
						"z-[3000] min-w-[var(--radix-popover-trigger-width)] rounded-[8px] border border-border bg-bg-surface p-1 shadow-md",
						"animate-fadeIn",
					)}
				>
					<div className="flex flex-col gap-0.5">
						{options.map((option) => (
							<label
								key={option.value}
								className={cn(
									"flex cursor-pointer items-center gap-2 rounded-md",
									"px-2 py-2 text-sm text-fg-secondary",
									"hover:bg-bg-elevated hover:text-fg-primary",
								)}
							>
								<Checkbox
									checked={value.includes(option.value)}
									onCheckedChange={() => toggle(option.value)}
								/>
								<span>{option.label}</span>
							</label>
						))}
					</div>
				</PopoverPrimitive.Content>
			</PopoverPrimitive.Portal>
		</PopoverPrimitive.Root>
	);
}
