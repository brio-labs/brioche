import { useCallback, useEffect, useMemo, useState } from "react";
import { useSettingsStore, FALLBACK_SECTIONS } from "../stores/settingsStore";
import { setSettings, isTauri } from "../ipc";
import type { SettingsSection, SettingsField } from "../ipc";
import PanelOverlay, { SearchBar } from "./PanelOverlay";
import { AlertTriangleIcon, EditIcon } from "./Icons";
import {
	Button,
	Input,
	Textarea,
	Label,
	Checkbox,
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
	MultiSelect,
	cn,
} from "./ui";

/// Props for the settings management panel.
///
/// Refs: I-Shell-Runtime-OnlyIO
interface SettingsPanelProps {
	onClose: () => void;
}

/// Reads a dotted path from the settings record.
///
/// Refs: I-Shell-Runtime-OnlyIO
function getFieldValue(
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
interface RecordListFieldSchema {
	key: string;
	placeholder: string;
	type?: "text" | "select" | "number" | "nullable_boolean";
	options?: { value: string; label: string }[];
	nullable?: boolean;
}

/// Props for the record list editor used for fallback models and memory endpoints.
///
/// Refs: I-Shell-Runtime-OnlyIO
interface RecordListProps {
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
function RecordList({
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
					<SelectTrigger className="flex-1 px-2.5 py-1.5 text-xs" />
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
					className="flex-1 px-2.5 py-1.5 text-xs"
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
				className="flex-1 px-2.5 py-1.5 text-xs"
			/>
		);
	};

	return (
		<div className="mt-2 flex flex-col gap-3">
			{items.map((item, index) => (
				<div
					key={index}
					className="flex flex-col gap-2 rounded-lg border border-border bg-bg-elevated/30 p-3"
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

/// Schema for the fallback model record list.
const fallbackModelSchema: RecordListFieldSchema[] = [
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
const FALLBACK_MODEL_DEFAULT: Record<string, unknown> = {
	provider: "openrouter",
	model: "",
	api_key: "",
	base_url: "",
	context_window: undefined,
	reasoning_enabled: null,
	reasoning_effort: "medium",
};

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
function MemoryEndpointList({
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

/// Props for a select that can represent true, false, or null.
///
/// Refs: I-Shell-Runtime-OnlyIO
interface NullableBooleanSelectProps {
	value: boolean | null;
	placeholder: string;
	onChange: (value: boolean | null) => void;
}

/// Renders a select dropdown with true, false, and unset (null) options.
///
/// Refs: I-Shell-Runtime-OnlyIO
function NullableBooleanSelect({
	value,
	placeholder,
	onChange,
}: NullableBooleanSelectProps) {
	return (
		<Select
			value={value === null ? "unset" : String(value)}
			onValueChange={(v) => onChange(v === "unset" ? null : v === "true")}
		>
			<SelectTrigger className="flex-1 px-2.5 py-1.5 text-xs" />
			<SelectContent>
				<SelectItem value="unset">{placeholder}</SelectItem>
				<SelectItem value="true">enabled</SelectItem>
				<SelectItem value="false">disabled</SelectItem>
			</SelectContent>
		</Select>
	);
}

/// Renders the settings panel with a searchable section list and field editor.
///
/// Refs: I-Shell-Runtime-OnlyIO
export default function SettingsPanel({ onClose }: SettingsPanelProps) {
	const { settings, loadSettings, updateSetting, sections, loadSections } =
		useSettingsStore();
	const [selectedSectionId, setSelectedSectionId] = useState<string | null>(
		null,
	);
	const [search, setSearch] = useState("");
	const [saveError, setSaveError] = useState<string | null>(null);
	const [editingProtected, setEditingProtected] = useState<Set<string>>(
		new Set(),
	);
	const isTauriAvailable = isTauri();

	useEffect(() => {
		if (!isTauriAvailable) return;
		loadSettings();
		loadSections();
	}, [loadSettings, loadSections, isTauriAvailable]);

	const endpoints =
		(Array.isArray(getFieldValue(settings, "memory.endpoints"))
			? (getFieldValue(settings, "memory.endpoints") as Record<string, unknown>[])
			: []) ?? [];

	const activeSections = useMemo(() => {
		const base = sections.length > 0 ? sections : FALLBACK_SECTIONS;
		return base.map((section) => {
			if (section.id !== "memory-providers") return section;
			return {
				...section,
				fields: section.fields.map((field) => {
					if (field.key !== "memory.active_providers") return field;
					const endpointOptions = endpoints
						.map((ep) => ep.id)
						.filter((id): id is string => typeof id === "string")
						.map((id) => ({ value: id, label: id }));
					return {
						...field,
						options: [
							{ value: "memory-local", label: "Local memory" },
							...endpointOptions,
						],
					};
				}),
			};
		});
	}, [sections, endpoints]);

	const filteredSections = useMemo(() => {
		if (!search.trim()) return activeSections;
		const q = search.toLowerCase();
		return activeSections.filter(
			(s) =>
				s.title.toLowerCase().includes(q) ||
				s.keywords.some((k) => k.toLowerCase().includes(q)),
		);
	}, [activeSections, search]);

	const selectedSection = useMemo(() => {
		if (!selectedSectionId) return null;
		return activeSections.find((s) => s.id === selectedSectionId) || null;
	}, [selectedSectionId, activeSections]);

	const handleSave = useCallback(async () => {
		setSaveError(null);
		try {
			await setSettings(settings);
			onClose();
		} catch (err) {
			const message = err instanceof Error ? err.message : String(err);
			setSaveError(message);
		}
	}, [settings, onClose]);

	const handleFieldChange = useCallback(
		(key: string, value: unknown) => {
			if (saveError) setSaveError(null);
			updateSetting(key, value);
		},
		[updateSetting, saveError],
	);

	const handleReset = useCallback(
		(field: SettingsField) => {
			updateSetting(field.key, field.default_value);
		},
		[updateSetting],
	);

	return (
		<PanelOverlay
			title="Settings"
			onClose={onClose}
			size="md"
			padded={false}
		>
			<div className="flex flex-1 min-h-0 flex-row overflow-hidden">
				<div className="flex flex-col w-60 min-w-60 border-r border-border bg-bg-base/20">
					<SearchBar
						placeholder="Search settings..."
						value={search}
						onChange={setSearch}
						containerClassName="border-b border-border rounded-none bg-bg-base/30 px-5 py-4"
					/>
					<div className="flex flex-1 flex-col gap-1 overflow-y-auto p-4">
						{filteredSections.map((section) => (
							<Button
								key={section.id}
								type="button"
								variant="ghost"
								onClick={() => setSelectedSectionId(section.id)}
								className={cn(
									"w-full justify-start px-4 py-2.5 text-sm font-semibold transition-all duration-150",
									selectedSectionId === section.id
										? "border-l-2 border-accent bg-accent/15 text-fg-primary"
										: "text-fg-secondary hover:bg-bg-elevated/50 hover:text-fg-primary",
								)}
							>
								{section.title}
							</Button>
						))}
					</div>
				</div>

				<div className="flex flex-1 flex-col gap-4 overflow-y-auto p-6">
					{!isTauriAvailable && (
						<div className="rounded-lg border border-error-border bg-error-bg p-4 text-xs text-error-text">
							Settings preview mode: saving requires the Tauri desktop app.
						</div>
					)}
					{selectedSection ? (
						<>
							<div className="mb-2 border-b border-border pb-3">
								<h3 className="text-base font-semibold text-fg-primary">
									{selectedSection.title}
								</h3>
							</div>
							<div className="flex flex-col gap-6">
								{selectedSection.fields.map((field) => (
									<FieldEditor
										key={field.key}
										field={field}
										value={getFieldValue(settings, field.key)}
										editingProtected={editingProtected}
										setEditingProtected={setEditingProtected}
										onChange={(value) => handleFieldChange(field.key, value)}
										onReset={() => handleReset(field)}
									/>
								))}
							</div>
						</>
					) : (
						<div className="flex flex-1 flex-col items-center justify-center py-16 text-sm text-fg-muted">
							Select a section from the left to view its settings.
						</div>
					)}
					{saveError && (
						<div className="mt-auto pt-5">
							<div className="whitespace-pre-wrap rounded-lg border border-error-border bg-error-bg p-4 text-sm text-error-text">
								{saveError}
							</div>
						</div>
					)}
				</div>
			</div>

			<div className="flex shrink-0 justify-end gap-3 border-t border-border bg-bg-base/30 px-6 py-5">
				<Button type="button" variant="secondary" onClick={onClose}>
					Cancel
				</Button>
				<Button type="button" onClick={handleSave} disabled={!isTauriAvailable}>
					Save
				</Button>
			</div>
		</PanelOverlay>
	);
}

/// Props for the editor that renders a single settings field.
///
/// Refs: I-Shell-Runtime-OnlyIO
interface FieldEditorProps {
	field: SettingsField;
	value: unknown;
	editingProtected: Set<string>;
	setEditingProtected: React.Dispatch<React.SetStateAction<Set<string>>>;
	onChange: (value: unknown) => void;
	onReset: () => void;
}

/// Renders the appropriate editor for a settings field based on its type.
///
/// Refs: I-Shell-Runtime-OnlyIO
function FieldEditor({
	field,
	value,
	editingProtected,
	setEditingProtected,
	onChange,
	onReset,
}: FieldEditorProps) {
	const isProtected = field.protected && !editingProtected.has(field.key);
	const currentValue = value !== undefined ? value : field.default_value;

	const input = (() => {
		switch (field.field_type) {
			case "boolean":
				return (
					<div className="flex items-center gap-2">
						<Checkbox
							id={field.key}
							checked={Boolean(currentValue)}
							onCheckedChange={(checked) => onChange(Boolean(checked))}
						/>
						<Label htmlFor={field.key} className="text-sm normal-case">
							{field.label}
						</Label>
					</div>
				);
			case "select":
				return (
					<Select
						value={String(currentValue || "")}
						onValueChange={onChange}
					>
						<SelectTrigger />
						<SelectContent>
							{field.options.map((opt) => (
								<SelectItem key={opt.value} value={opt.value}>
									{opt.label}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				);
			case "multi_select": {
				const selected = Array.isArray(currentValue)
					? currentValue.map(String)
					: [];
				return (
					<MultiSelect
						value={selected}
						options={field.options}
						onChange={onChange}
					/>
				);
			}
			case "number":
				return (
					<Input
						type="number"
						value={Number(currentValue || 0)}
						placeholder={field.placeholder ?? undefined}
						onChange={(e) => onChange(Number(e.target.value))}
					/>
				);
			case "password":
				return (
					<Input
						type="password"
						value={String(currentValue || "")}
						placeholder={field.placeholder ?? undefined}
						onChange={(e) => onChange(e.target.value)}
						disabled={isProtected}
					/>
				);
			case "text":
			case "protected_markdown":
				return (
					<Textarea
						value={String(currentValue || "")}
						placeholder={field.placeholder ?? undefined}
						onChange={(e) => onChange(e.target.value)}
						disabled={isProtected}
						rows={field.field_type === "protected_markdown" ? 8 : 4}
					/>
				);
			case "list": {
				const items = Array.isArray(currentValue) ? currentValue : [];
				if (field.key === "memory.endpoints") {
					return (
						<MemoryEndpointList
							items={items as Record<string, unknown>[]}
							onChange={onChange}
						/>
					);
				}
				return (
					<RecordList
						items={items as Record<string, unknown>[]}
						onChange={onChange}
						schema={fallbackModelSchema}
						defaultItem={FALLBACK_MODEL_DEFAULT}
						addLabel="Add fallback model"
						groups={[2, 2, 3]}
					/>
				);
			}
			case "path":
			default:
				return (
					<Input
						type="text"
						value={String(currentValue || "")}
						placeholder={field.placeholder ?? undefined}
						onChange={(e) => onChange(e.target.value)}
					/>
				);
		}
	})();

	return (
		<div
			className={cn(
				"flex flex-col gap-2",
				field.protected &&
					"rounded-lg border border-border/50 bg-bg-elevated/20 p-3.5",
			)}
		>
			{field.field_type !== "boolean" && (
				<Label htmlFor={field.key}>{field.label}</Label>
			)}
			{field.protected && (
				<div className="mt-0.5 flex items-start gap-2 text-xs text-warning-text">
					<AlertTriangleIcon className="mt-0.5 h-3.5 w-3.5 shrink-0" />
					<div className="flex flex-wrap items-center gap-x-2 gap-y-1">
						<span>
							{isProtected
								? "This field is protected. Editing it may change model behavior."
								: "This field is unlocked. Changes may change model behavior."}
						</span>
						{isProtected ? (
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={() =>
									setEditingProtected((prev) => new Set(prev).add(field.key))
								}
								className="h-auto px-1 py-0.5 text-accent underline hover:text-accent-hover"
							>
								<EditIcon className="mr-1 h-3 w-3" />
								Unlock to edit
							</Button>
						) : (
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={onReset}
								className="h-auto px-1 py-0.5 text-fg-muted underline hover:text-fg-secondary"
							>
								Reset to default
							</Button>
						)}
					</div>
				</div>
			)}
			{input}
			{field.description && (
				<span className="text-xs leading-relaxed text-fg-muted">
					{field.description}
				</span>
			)}
		</div>
	);
}
