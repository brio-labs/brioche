//! Brioche Desktop UI primitives barrel export.
//!
//! Centralized re-exports for theme-aware Radix wrappers and Tailwind helpers
//! used by settings panel and future desktop UI surfaces.
//!
//! Refs: I-Shell-Runtime-OnlyIO

export { cn } from "./lib";
export {
	ActionRow,
	Button,
	Checkbox,
	ContextMenuItem,
	FormFieldStack,
	IconToolbarButton,
	Input,
	Label,
	ProtectedSettingsCard,
	Separator,
	SidePanel,
	Textarea,
	ToolbarButton,
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
