import type { ReactNode, RefObject } from "react";
import { XIcon, SearchIcon } from "./Icons";
import { cn } from "./ui/lib";

/**
 * Props for the overlay panel layout.
 *
 * Refs: I-Ui-OverlayCohesion
 */
interface OverlayPanelProps {
	title: string;
	icon?: ReactNode;
	onClose: () => void;
	children: ReactNode;
	headerActions?: ReactNode;
	size?: "sm" | "md" | "lg" | "xl";
	padded?: boolean;
}

const sizeClasses: Record<NonNullable<OverlayPanelProps["size"]>, string> = {
	sm: "w-150",
	md: "w-200",
	lg: "w-[850px]",
	xl: "w-250",
};

/**
 * Reusable modal/overlay layout that centralizes backdrop clicks, animations,
 * header structures, and close-button concerns.
 *
 * Refs: I-Ui-OverlayCohesion
 */
export default function OverlayPanel({
	title,
	icon,
	onClose,
	children,
	headerActions,
	size = "md",
	padded = true,
}: OverlayPanelProps) {
	return (
		<div className="panel-backdrop" onClick={onClose}>
			<div
				className={cn(
					"panel",
					sizeClasses[size],
					"max-w-[95vw] max-h-[85vh] z-1001",
				)}
				onClick={(e) => e.stopPropagation()}
			>
				<div className="panel-header">
					<h2 className="flex items-center text-sm font-semibold text-fg-primary">
						{icon && (
							<span className="mr-2.5 flex items-center text-accent">
								{icon}
							</span>
						)}
						<span>{title}</span>
					</h2>
					<div className="flex items-center gap-2">
						{headerActions}
						<button
							type="button"
							className="btn-icon w-7 h-7"
							onClick={onClose}
							aria-label="Close panel"
						>
							<XIcon className="w-4 h-4" />
						</button>
					</div>
				</div>
				{padded ? (
					<div className="flex flex-col flex-1 min-h-0 gap-4 overflow-y-auto p-6">
						{children}
					</div>
				) : (
					children
				)}
			</div>
		</div>
	);
}

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
				"rounded-md border border-border bg-bg-elevated/30",
				"transition-all",
				"focus-within:border-accent-dim/60 focus-within:ring-1 focus-within:ring-accent-dim/30",
				containerClassName,
			)}
		>
			<SearchIcon className="w-4 h-4 shrink-0 text-fg-muted" />
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

/**
 * Props for the category filter pills.
 *
 * Refs: I-Ui-OverlayCohesion
 */
interface CategoryFilterProps {
	categories: string[];
	activeCategory: string | null;
	onSelect: (category: string | null) => void;
	containerClassName?: string;
	buttonClassName?: string;
}

/**
 * Reusable horizontal pills/tab filter list component.
 *
 * Refs: I-Ui-OverlayCohesion
 */
export function CategoryFilter({
	categories,
	activeCategory,
	onSelect,
	containerClassName = "",
	buttonClassName = "",
}: CategoryFilterProps) {
	return (
		<div className={cn("flex flex-wrap gap-2", containerClassName)}>
			<button
				type="button"
				className={cn(
					"rounded-md border px-3 py-1 text-xs font-medium transition-all",
					!activeCategory
						? "border-accent/30 bg-accent/20 text-fg-primary"
						: "border-border/50 bg-bg-elevated/50 text-fg-muted hover:text-fg-secondary",
					buttonClassName,
				)}
				onClick={() => onSelect(null)}
			>
				All
			</button>
			{categories.map((cat) => {
				const isActive = activeCategory === cat;
				return (
					<button
						key={cat}
						type="button"
						className={cn(
							"rounded-md border px-3 py-1 text-xs font-medium transition-all",
							isActive
								? "border-accent/30 bg-accent/20 text-fg-primary"
								: "border-border/50 bg-bg-elevated/50 text-fg-muted hover:text-fg-secondary",
							buttonClassName,
						)}
						onClick={() => onSelect(cat)}
					>
						{cat.charAt(0).toUpperCase() + cat.slice(1)}
					</button>
				);
			})}
		</div>
	);
}

/**
 * Props for the floating modal overlay.
 *
 * Refs: I-Ui-OverlayCohesion
 */
interface ModalOverlayProps {
	isOpen: boolean;
	onClose: () => void;
	children: ReactNode;
}

/**
 * Backdrop and centered modal container for floating overlays.
 *
 * Refs: I-Ui-OverlayCohesion
 */
export function ModalOverlay({
	isOpen,
	onClose,
	children,
}: ModalOverlayProps) {
	if (!isOpen) return null;
	return (
		<div
			className={cn(
				"fixed inset-0 z-2000 flex items-start justify-center",
				"bg-black/60 backdrop-blur-sm pt-[15vh]",
				"animate-fadeIn",
			)}
			onClick={onClose}
		>
			<div
				className={cn(
					"flex flex-col w-140 max-w-[90vw] max-h-[60vh] overflow-hidden rounded-lg border border-border bg-bg-surface",
					"shadow-2xl animate-slideDown",
				)}
				onClick={(e) => e.stopPropagation()}
			>
				{children}
			</div>
		</div>
	);
}

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
			<SearchIcon className="w-4 h-4 shrink-0 text-fg-muted" />
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
					<XIcon className="w-3.5 h-3.5" />
				</button>
			)}
		</div>
	);
}
