import { useMemo } from "react";
import { RecordList } from "./RecordList";
import type { RecordListFieldSchema } from "./settingsUtils";

/// Schema for the memory endpoint record list.
const memoryEndpointSchema: RecordListFieldSchema[] = [
	{ key: "id", placeholder: "ID (e.g. memory-amp-1)" },
	{ key: "name", placeholder: "Name" },
	{ key: "url", placeholder: "URL (e.g. http://localhost:9471)" },
	{ key: "api_key", placeholder: "API Key (optional)", nullable: true },
	{ key: "scope", placeholder: "Scope (optional)", nullable: true },
];

/// Renders a memory endpoint record list with sensible default IDs.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function MemoryEndpointList({
	items,
	onChange,
}: {
	items: Record<string, unknown>[];
	onChange: (value: unknown) => void;
}) {
	const nextId = useMemo(() => {
		const used = new Set(
			items.map((item) => item.id).filter((id): id is string => typeof id === "string"),
		);
		let i = 1;
		while (used.has(`memory-amp-${i}`)) {
			i += 1;
		}
		return `memory-amp-${i}`;
	}, [items]);

	return (
		<RecordList
			items={items}
			onChange={onChange}
			schema={memoryEndpointSchema}
			defaultItem={{
				id: nextId,
				name: `Memory ${items.length + 1}`,
				url: "http://127.0.0.1:9471",
				api_key: null,
				scope: null,
			}}
			addLabel="Add memory endpoint"
			groups={[2, 2, 1]}
		/>
	);
}
