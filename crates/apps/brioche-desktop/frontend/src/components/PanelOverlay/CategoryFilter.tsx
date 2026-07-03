import { cn } from "../ui/lib";

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
