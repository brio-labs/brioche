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

export const SelectTrigger = React.forwardRef<
	React.ElementRef<typeof SelectPrimitive.Trigger>,
	React.ComponentPropsWithoutRef<typeof SelectPrimitive.Trigger>
>(({ className, children, ...props }, ref) => (
	<SelectPrimitive.Trigger
		ref={ref}
		className={cn(
			"flex w-full items-center justify-between rounded border border-border bg-bg-2 px-3 py-2 text-[13px] font-mono text-text-primary outline-none transition-all focus:border-accent-dim focus:bg-bg-3 focus:ring-1 focus:ring-accent-glow disabled:cursor-not-allowed disabled:opacity-50 cursor-pointer",
			className,
		)}
		{...props}
	>
		{children}
		<SelectPrimitive.Icon asChild>
			<ChevronDown className="h-4 w-4 opacity-50 shrink-0" />
		</SelectPrimitive.Icon>
	</SelectPrimitive.Trigger>
));
SelectTrigger.displayName = SelectPrimitive.Trigger.displayName;

export const SelectContent = React.forwardRef<
	React.ElementRef<typeof SelectPrimitive.Content>,
	React.ComponentPropsWithoutRef<typeof SelectPrimitive.Content>
>(({ className, children, position = "popper", ...props }, ref) => (
	<SelectPrimitive.Portal>
		<SelectPrimitive.Content
			ref={ref}
			className={cn(
				"relative z-50 max-h-96 min-w-[8rem] overflow-hidden rounded border border-border bg-bg-1 text-text-primary shadow-md animate-fadeIn",
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

export const SelectItem = React.forwardRef<
	React.ElementRef<typeof SelectPrimitive.Item>,
	React.ComponentPropsWithoutRef<typeof SelectPrimitive.Item>
>(({ className, children, ...props }, ref) => (
	<SelectPrimitive.Item
		ref={ref}
		className={cn(
			"relative flex w-full cursor-pointer select-none items-center rounded py-1.5 pl-2 pr-8 text-[13px] text-text-secondary outline-none focus:bg-bg-2 focus:text-text-primary",
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

export interface MultiSelectOption {
	value: string;
	label: string;
}

export interface MultiSelectProps {
	value: string[];
	options: MultiSelectOption[];
	placeholder?: string;
	onChange: (value: string[]) => void;
}

export function MultiSelect({
	value,
	options,
	placeholder = "Select options...",
	onChange,
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
						"flex w-full items-center justify-between rounded border border-border bg-bg-2 px-3 py-2 text-[13px] font-mono outline-none transition-all focus:border-accent-dim focus:bg-bg-3 focus:ring-1 focus:ring-accent-glow cursor-pointer",
						value.length === 0 && "text-text-muted",
						value.length > 0 && "text-text-primary",
					)}
				>
					<span className="truncate">{label}</span>
					<ChevronDown className="h-4 w-4 opacity-50 shrink-0" />
				</button>
			</PopoverPrimitive.Trigger>
			<PopoverPrimitive.Portal>
				<PopoverPrimitive.Content
					align="start"
					sideOffset={4}
					className="z-50 min-w-[var(--radix-popover-trigger-width)] rounded border border-border bg-bg-1 p-1 shadow-md animate-fadeIn"
				>
					<div className="flex flex-col gap-0.5">
						{options.map((option) => (
							<label
								key={option.value}
								className="flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-[13px] text-text-secondary hover:bg-bg-2 hover:text-text-primary"
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
