/// Reads a dotted path from the settings record.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function getFieldValue(
	settings: Record<string, unknown>,
	key: string,
): unknown {
	const parts = key.split(".");
	let current: unknown = settings;
	for (const part of parts) {
		if (current && typeof current === "object" && !Array.isArray(current)) {
			current = (current as Record<string, unknown>)[part];
		} else {
			return undefined;
		}
	}
	return current;
}

/// Schema for a single field inside a record list row.
///
/// Refs: I-Shell-Runtime-OnlyIO
export interface RecordListFieldSchema {
	key: string;
	placeholder: string;
	type?: "text" | "select" | "number" | "nullable_boolean";
	options?: { value: string; label: string }[];
	nullable?: boolean;
}

/// Schema for the fallback model record list.
export const fallbackModelSchema: RecordListFieldSchema[] = [
	{ key: "provider", placeholder: "provider" },
	{ key: "model", placeholder: "model" },
	{ key: "api_key", placeholder: "api key (optional)", nullable: true },
	{ key: "base_url", placeholder: "base url (optional)", nullable: true },
	{
		key: "context_window",
		placeholder: "context window",
		type: "number",
		nullable: true,
	},
	{
		key: "reasoning_enabled",
		placeholder: "default reasoning",
		type: "nullable_boolean",
	},
	{ key: "reasoning_effort", placeholder: "reasoning effort", nullable: true },
];

/// Default values for a new fallback model entry.
export const FALLBACK_MODEL_DEFAULT: Record<string, unknown> = {
	provider: "openrouter",
	model: "",
	api_key: "",
	base_url: "",
	context_window: undefined,
	reasoning_enabled: null,
	reasoning_effort: "medium",
};
