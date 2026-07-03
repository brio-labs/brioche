import { Search } from "lucide-react";
import { cn } from "../ui/lib";

/**
 * Props for the reusable search bar.
 *
 * Refs: I-Ui-OverlayCohesion
 */
interface SearchBarProps {
	value: string;
	onChange: (value: string) => void;
	placeholder?: string;
	onSearch?: () => void;
	containerClassName?: string;
}

/**
 * Reusable search bar input component.
 *
 * Refs: I-Ui-OverlayCohesion
 */
export function SearchBar({
	value,
	onChange,
	placeholder = "Search...",
	onSearch,
	containerClassName = "",
}: SearchBarProps) {
	return (
		<div
			className={cn(
				"flex items-center gap-2",
				"px-3 py-2",
				"rounded-md border border-border bg-bg-elevated",
				"transition-all",
				"focus-within:border-accent-dim/60 focus-within:ring-1 focus-within:ring-accent-dim/30",
				containerClassName,
			)}
		>
			<Search className="w-4 h-4 shrink-0 text-fg-muted" />
			<input
				type="text"
				placeholder={placeholder}
				value={value}
				onChange={(e) => onChange(e.target.value)}
				onKeyDown={(e) => e.key === "Enter" && onSearch?.()}
				className="flex-1 bg-transparent border-none py-1 font-sans text-sm text-fg-primary outline-none placeholder:text-fg-dim"
			/>
			{onSearch && (
				<button
					type="button"
					onClick={onSearch}
					className="rounded-md bg-accent px-3 py-1 text-xs font-semibold text-accent-text transition-colors hover:bg-accent-hover active:bg-accent-dim focus-visible:ring-1 focus-visible:ring-accent-glow"
				>
					Search
				</button>
			)}
		</div>
	);
}
