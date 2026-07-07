import type { ClassValue } from "clsx";
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/// Merges conditional Tailwind classes into a single string.
///
/// Combines `clsx` for conditional class lists and `tailwind-merge` for
/// deduplicating conflicting utilities. Used throughout the desktop UI for
/// all conditional styling.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function cn(...inputs: ClassValue[]): string {
	return twMerge(clsx(inputs));
}
