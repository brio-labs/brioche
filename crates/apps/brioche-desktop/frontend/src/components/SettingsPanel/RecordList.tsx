import { useMemo } from "react";
import {
	Button,
	Input,
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "../ui";
import { NullableBooleanSelect } from "./NullableBooleanSelect";
import type { RecordListFieldSchema } from "./settingsUtils";

/// Props for the record list editor used for fallback models and memory endpoints.
///
/// Refs: I-Shell-Runtime-OnlyIO
export interface RecordListProps {
	items: Record<string, unknown>[];
	onChange: (value: unknown) => void;
	schema: RecordListFieldSchema[];
	defaultItem: Record<string, unknown>;
	addLabel: string;
	groups?: number[];
}

/// Renders a list of record rows that can be edited, added, and removed.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function RecordList({
	items,
	onChange,
	schema,
	defaultItem,
	addLabel,
	groups = [2, 2, 99],
}: RecordListProps) {
	const updateItem = (index: number, key: string, value: unknown) => {
		const next = items.map((item, i) =>
			i === index ? { ...item, [key]: value } : item,
		);
		onChange(next);
	};

	const addItem = () => onChange([...items, { ...defaultItem }]);
	const removeItem = (index: number) =>
		onChange(items.filter((_, i) => i !== index));

	const rows = useMemo(() => {
		const result: RecordListFieldSchema[][] = [];
		let offset = 0;
		for (const count of groups) {
			if (offset >= schema.length) break;
			result.push(schema.slice(offset, offset + count));
			offset += count;
		}
		if (offset < schema.length) {
			result.push(schema.slice(offset));
		}
		return result;
	}, [schema, groups]);

	const renderField = (
		item: Record<string, unknown>,
		index: number,
		field: RecordListFieldSchema,
	) => {
		const raw = item[field.key] ?? "";
		const value = field.nullable && raw === "" ? null : raw;
		const onChangeField = (v: string | number | boolean | null) => {
			const next = field.nullable && (v === "" || v === false) ? null : v;
			updateItem(index, field.key, next);
		};

		if (field.type === "select") {
			return (
				<Select
					key={field.key}
					value={String(value ?? "")}
					onValueChange={(v) => onChangeField(v)}
				>
					<SelectTrigger className="flex-1 px-3 py-2 text-xs" />
					<SelectContent>
						{(field.options || []).map((opt) => (
							<SelectItem key={opt.value} value={opt.value}>
								{opt.label}
							</SelectItem>
						))}
					</SelectContent>
				</Select>
			);
		}

		if (field.type === "nullable_boolean") {
			return (
				<NullableBooleanSelect
					key={field.key}
					value={
						typeof value === "boolean" || value === null ? value : null
					}
					placeholder={field.placeholder}
					onChange={(v) => onChangeField(v)}
				/>
			);
		}

		if (field.type === "number") {
			return (
				<Input
					key={field.key}
					type="number"
					value={Number(value || 0)}
					onChange={(e) => {
						const raw = e.target.value;
						if (raw === "") {
							onChangeField("");
							return;
						}
						const n = Number(raw);
						if (Number.isNaN(n) || n < 0) return;
						onChangeField(n > 0 ? n : "");
					}}
					placeholder={field.placeholder}
					className="flex-1 px-3 py-2 text-xs"
				/>
			);
		}

		return (
			<Input
				key={field.key}
				type="text"
				value={String(value ?? "")}
				onChange={(e) => onChangeField(e.target.value)}
				placeholder={field.placeholder}
				className="flex-1 px-3 py-2 text-xs"
			/>
		);
	};

	return (
		<div className="mt-2 flex flex-col gap-3">
			{items.map((item, index) => (
				<div
					key={index}
					className="flex flex-col gap-2 rounded-none border border-border bg-bg-elevated p-3"
				>
					{rows.map((rowFields, rowIndex) => (
						<div key={rowIndex} className="flex items-center gap-2">
							{rowFields.map((field) => renderField(item, index, field))}
							{rowIndex === 0 && (
								<Button
									type="button"
									variant="ghost"
									size="icon"
									onClick={() => removeItem(index)}
									className="shrink-0 text-fg-muted hover:text-error-text"
								>
									×
								</Button>
							)}
						</div>
					))}
				</div>
			))}
			<Button
				type="button"
				variant="secondary"
				size="sm"
				onClick={addItem}
				className="self-start"
			>
				{addLabel}
			</Button>
		</div>
	);
}
