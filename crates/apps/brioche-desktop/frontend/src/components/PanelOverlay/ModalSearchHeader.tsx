import type { RefObject } from "react";
import { X, Search } from "lucide-react";

/**
 * Props for the modal search input header.
 *
 * Refs: I-Ui-OverlayCohesion
 */
interface ModalSearchHeaderProps {
	query: string;
	onChange: (value: string) => void;
	onKeyDown: (e: React.KeyboardEvent) => void;
	inputRef: RefObject<HTMLInputElement | null>;
	placeholder?: string;
	onClear?: () => void;
}

/**
 * Search input header used inside modal overlays.
 *
 * Refs: I-Ui-OverlayCohesion
 */
export function ModalSearchHeader({
	query,
	onChange,
	onKeyDown,
	inputRef,
	placeholder = "Search...",
	onClear,
}: ModalSearchHeaderProps) {
	return (
		<div className="flex items-center gap-3 p-5 border-b border-border">
			<Search className="w-4 h-4 shrink-0 text-fg-muted" />
			<input
				ref={inputRef}
				type="text"
				value={query}
				onChange={(e) => onChange(e.target.value)}
				onKeyDown={onKeyDown}
				placeholder={placeholder}
				className="flex-1 bg-transparent border-none py-1 font-sans text-base text-fg-primary outline-none placeholder:text-fg-muted"
			/>
			{onClear && query && (
				<button
					type="button"
					className="btn-icon w-6 h-6 shrink-0"
					onClick={onClear}
					title="Clear search"
				>
					<X className="w-3.5 h-3.5" />
				</button>
			)}
		</div>
	);
}
