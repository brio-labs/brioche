//! Brioche Desktop UI primitives barrel export.
//!
//! Centralized re-exports for theme-aware Radix wrappers and Tailwind helpers
//! used by settings panel and future desktop UI surfaces.
//!
//! Refs: I-Shell-Runtime-OnlyIO

export { cn } from "./lib";
export {
	Button,
	Checkbox,
	Input,
	Label,
	Separator,
	Textarea,
	type ButtonProps,
	type InputProps,
	type TextareaProps,
} from "./primitives";
export {
	Select,
	SelectContent,
	SelectGroup,
	SelectItem,
	SelectSeparator,
	SelectTrigger,
	SelectValue,
	MultiSelect,
	type MultiSelectOption,
	type MultiSelectProps,
} from "./select";
export {
	ContextMenu,
	ContextMenuContent,
	ContextMenuGroup,
	ContextMenuItem,
	ContextMenuSeparator,
	ContextMenuTrigger,
	useContextMenu,
	type ContextMenuContentProps,
	type ContextMenuGroupProps,
	type ContextMenuItemProps,
	type ContextMenuProps,
	type ContextMenuSeparatorProps,
	type ContextMenuTriggerProps,
} from "./context-menu";
