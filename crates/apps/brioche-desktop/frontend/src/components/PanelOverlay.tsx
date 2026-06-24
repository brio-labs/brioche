import React from "react";
import { XIcon, SearchIcon } from "./Icons";

interface PanelOverlayProps {
	title: string;
	icon?: React.ReactNode;
	onClose: () => void;
	children: React.ReactNode;
	headerActions?: React.ReactNode;
	panelClassName?: string;
}

/**
 * Reusable modal/overlay layout that centralizes backdrop clicks, animations,
 * header structures, and close-button concerns.
 *
 * Refs: I-Ui-OverlayCohesion
 */
export default function PanelOverlay({
	title,
	icon,
	onClose,
	children,
	headerActions,
	panelClassName = "bg-bg-1 border border-border rounded-lg w-[800px] max-w-[95vw] max-h-[85vh] flex flex-col overflow-hidden animate-slideUp shadow-2xl z-[1001]",
}: PanelOverlayProps) {
	return (
		<div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-[1000] animate-fadeIn" onClick={onClose}>
			<div className={panelClassName} onClick={(e) => e.stopPropagation()}>
				<div className="flex items-center justify-between px-5 py-4 border-b border-border bg-bg-0/50">
					<h2 className="flex items-center text-sm font-semibold text-text-primary">
						{icon && <span className="mr-2.5 flex items-center text-accent">{icon}</span>}
						<span>{title}</span>
					</h2>
					<div className="flex items-center gap-2">
						{headerActions}
						<button type="button" className="p-1.5 bg-transparent text-text-muted hover:text-text-secondary hover:bg-bg-3 rounded-md transition-all duration-150 cursor-pointer flex items-center justify-center" onClick={onClose} aria-label="Close panel">
							<XIcon className="w-4 h-4" />
						</button>
					</div>
				</div>
				{children}
			</div>
		</div>
	);
}

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
		<div className={`flex items-center gap-2 px-3 py-2 border border-border bg-bg-2/30 rounded-md focus-within:border-accent-dim/60 focus-within:ring-1 focus-within:ring-accent-dim/30 transition-all ${containerClassName}`}>
			{!containerClassName.includes("memory") && <SearchIcon className="w-4 h-4 text-text-muted shrink-0" />}
			<input
				type="text"
				placeholder={placeholder}
				value={value}
				onChange={(e) => onChange(e.target.value)}
				onKeyDown={(e) => e.key === "Enter" && onSearch?.()}
				className="flex-1 bg-transparent border-none text-text-primary text-[13px] outline-none placeholder:text-text-dim font-sans"
			/>
			{onSearch && (
				<button 
					onClick={onSearch}
					className="px-3 py-1 bg-accent hover:bg-accent-hover text-white text-xs font-semibold rounded cursor-pointer transition-colors"
				>
					Search
				</button>
			)}
		</div>
	);
}

interface CategoryFilterProps {
	categories: string[];
	activeCategory: string | null;
	onSelect: (category: string | null) => void;
	containerClassName: string;
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
		<div className={`flex flex-wrap gap-2 ${containerClassName}`}>
			<button
				type="button"
				className={`px-3 py-1 rounded text-xs font-medium cursor-pointer transition-all ${
					!activeCategory
						? "bg-accent/20 text-text-primary border border-accent/30"
						: "bg-bg-2/50 text-text-muted hover:text-text-secondary border border-border/50"
				} ${buttonClassName}`}
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
						className={`px-3 py-1 rounded text-xs font-medium cursor-pointer transition-all ${
							isActive
								? "bg-accent/20 text-text-primary border border-accent/30"
								: "bg-bg-2/50 text-text-muted hover:text-text-secondary border border-border/50"
						} ${buttonClassName}`}
						onClick={() => onSelect(cat)}
					>
						{cat.charAt(0).toUpperCase() + cat.slice(1)}
					</button>
				);
			})}
		</div>
	);
}
