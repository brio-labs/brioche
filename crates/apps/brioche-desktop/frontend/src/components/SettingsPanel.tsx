import { getPathValue } from "../stores/settingsPath";
import {
	useCallback,
	useEffect,
	useMemo,
	useState,
	type Dispatch,
	type SetStateAction,
} from "react";
import { useSettingsStore } from "../stores/settingsStore";
import { setSettings } from "../ipc";
import type {
	SettingsSection,
	SettingsField,
	SettingsListSchema,
	SettingsListField,
	SettingsListRenderer,
} from "../ipc";
import PanelOverlay, { SearchBar } from "./PanelOverlay";
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
	MultiSelect,
	ProtectedSettingsCard,
} from "./ui";

interface SettingsPanelProps {
	onClose: () => void;
}

interface RecordListFieldSchema {
	key: string;
	type: "text" | "select" | "number" | "nullable_boolean";
	placeholder: string;
	options: { value: string; label: string }[];
	nullable: boolean;
	defaultValue: unknown;
}

interface RecordListProps {
	items: Record<string, unknown>[];
	onChange: (value: unknown) => void;
	schema: RecordListFieldSchema[];
	defaultItem: Record<string, unknown>;
	addLabel: string;
	groups?: number[];
}

function normalizeRecordField(field: SettingsListField): RecordListFieldSchema {
	return {
		key: field.key,
		type: field.field_type,
		placeholder: field.placeholder || "",
		options: field.options,
		nullable: field.nullable,
		defaultValue: field.default_value,
	};
}

function toRecordListFields(schema: SettingsListSchema | null | undefined) {
	const itemSchema = schema?.item_schema ?? [];
	return itemSchema.map(normalizeRecordField);
}

function listItemDefaults(schema: RecordListFieldSchema[]): Record<string, unknown> {
	return schema.reduce<Record<string, unknown>>((acc, field) => {
		const fallback: unknown = field.nullable ? null : field.type === "number" ? 0 : "";
		acc[field.key] = field.defaultValue ?? fallback;
		return acc;
	}, {});
}

function memoryEndpointNextId(items: Record<string, unknown>[]) {
	const used = new Set(
		items
			.map((item) => item.id)
			.filter((id): id is string => typeof id === "string"),
	);
	for (let index = 1; ; index += 1) {
		const candidate = `memory-amp-${index}`;
		if (!used.has(candidate)) return candidate;
	}
}

function getListRenderer(field: SettingsField): SettingsListRenderer | "record" {
	const schema = field.list_schema;
	return schema?.renderer ?? "record";
}

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
			const next = field.nullable && v === "" ? null : v;
			updateItem(index, field.key, next);
		};

		if (field.type === "select") {
			return (
				<Select
					key={field.key}
					value={String(value ?? "")}
					onValueChange={(v) => onChangeField(v)}
				>
					<SelectTrigger className="flex-1 text-xs px-2.5 py-1.5" />
					<SelectContent>
						{field.options.map((opt) => (
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
					className="flex-1 text-xs px-2.5 py-1.5"
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
				className="flex-1 text-xs px-2.5 py-1.5"
			/>
		);
	};

	return (
		<div className="flex flex-col gap-3 mt-2">
			{items.map((item, index) => (
				<div
					key={index}
					className="p-3 bg-bg-2/30 border border-border rounded-lg flex flex-col gap-2"
				>
					{rows.map((rowFields, rowIndex) => (
						<div
							key={rowIndex}
							className="flex gap-2 items-center"
						>
							{rowFields.map((field) =>
								renderField(item, index, field),
							)}
							{rowIndex === 0 && (
								<Button
									type="button"
									variant="ghost"
									size="icon"
									onClick={() => removeItem(index)}
									className="text-text-muted hover:text-red-400 shrink-0"
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

interface StringListProps {
	items: string[];
	onChange: (value: unknown) => void;
	addLabel: string;
}

function StringList({ items, onChange, addLabel }: StringListProps) {
	const addItem = () => onChange([...items, ""]);
	const removeItem = (index: number) =>
		onChange(items.filter((_, i) => i !== index));
	const updateItem = (index: number, value: string) => {
		const next = items.map((item, i) => (i === index ? value : item));
		onChange(next);
	};

	return (
		<div className="flex flex-col gap-3 mt-2">
			{items.map((item, index) => (
				<div
					key={index}
					className="p-3 bg-bg-2/30 border border-border rounded-lg flex items-center gap-2"
				>
					<Input
						type="text"
						value={item}
						onChange={(e) => updateItem(index, e.target.value)}
						className="flex-1 text-xs px-2.5 py-1.5"
					/>
					<Button
						type="button"
						variant="ghost"
						size="icon"
						onClick={() => removeItem(index)}
						className="text-text-muted hover:text-red-400 shrink-0"
					>
						×
					</Button>
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

function MemoryEndpointList({
	items,
	onChange,
	listSchema,
}: {
	items: Record<string, unknown>[];
	onChange: (value: unknown) => void;
	listSchema: SettingsListSchema;
}) {
	const fields = useMemo(() => toRecordListFields(listSchema), [listSchema]);
	const defaultItem = useMemo(() => {
		const defaults = listItemDefaults(fields);
		const nextId = memoryEndpointNextId(items);
		if (typeof defaults.id === "string" || typeof defaults.id === "number") {
			defaults.id = nextId;
		}
		defaults.name = `Memory ${items.length + 1}`;
		return defaults;
	}, [fields, items]);

	return (
		<RecordList
			items={items}
			onChange={onChange}
			schema={fields}
			defaultItem={defaultItem}
			addLabel={listSchema.add_label ?? "Add memory endpoint"}
			groups={listSchema.groups ?? [2, 2, 1]}
		/>
	);
}

interface NullableBooleanSelectProps {
	value: boolean | null;
	placeholder: string;
	onChange: (value: boolean | null) => void;
}

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
			<SelectTrigger className="flex-1 text-xs px-2.5 py-1.5" />
			<SelectContent>
				<SelectItem value="unset">{placeholder}</SelectItem>
				<SelectItem value="true">enabled</SelectItem>
				<SelectItem value="false">disabled</SelectItem>
			</SelectContent>
		</Select>
	);
}


export default function SettingsPanel({ onClose }: SettingsPanelProps) {
	const { settings, loadSettings, updateSetting, sections, loadSections } =
		useSettingsStore();
	const [selectedSectionId, setSelectedSectionId] = useState<string | null>(
		null,
	);
	const [search, setSearch] = useState("");
	const [saveError, setSaveError] = useState<string | null>(null);

	const [editingProtected, setEditingProtected] = useState<Set<string>>(new Set());


	useEffect(() => {
		loadSettings();
		loadSections();
	}, [loadSettings, loadSections]);

	const endpoints =
		(Array.isArray(getPathValue(settings, "memory.endpoints"))
			? (getPathValue(settings, "memory.endpoints") as Record<string, unknown>[])
			: []) ?? [];

	const activeSections = useMemo(() => {
		return sections.map((section) => {
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
			panelClassName="bg-bg-1 border border-border rounded-lg w-[800px] max-w-[95vw] max-h-[85vh] flex flex-col overflow-hidden animate-slideUp shadow-2xl z-[1001]"
		>
			<div className="flex flex-row flex-1 overflow-hidden min-h-0">
				<div className="w-[240px] min-w-[240px] border-r border-border flex flex-col bg-bg-0/20">
					<SearchBar
						placeholder="Search settings..."
						value={search}
						onChange={setSearch}
						containerClassName="border-b border-border rounded-none px-4 py-3 bg-bg-0/30"
					/>
					<div className="flex-1 overflow-y-auto p-2 flex flex-col gap-0.5">
						{filteredSections.map((section) => (
							<Button
								key={section.id}
								type="button"
								variant="ghost"
								onClick={() => setSelectedSectionId(section.id)}
								className={`w-full justify-start px-4 py-2.5 text-[13px] font-semibold transition-all duration-150 ${
									selectedSectionId === section.id
										? "bg-accent/15 border-l-2 border-accent text-text-primary"
										: "text-text-secondary hover:bg-bg-2/50 hover:text-text-primary"
								}`}
							>
								{section.title}
							</Button>
						))}
					</div>
				</div>

				<div className="flex-1 overflow-y-auto p-6 flex flex-col gap-4">
					{selectedSection ? (
						<>
							<div className="border-b border-border pb-3 mb-2">
								<h3 className="text-base font-semibold text-text-primary">
									{selectedSection.title}
								</h3>
							</div>
							<div className="flex flex-col gap-6">
								{selectedSection.fields.map((field) => (
									<FieldEditor
										key={field.key}
										field={field}
										value={getPathValue(settings, field.key)}
										editingProtected={editingProtected}
										setEditingProtected={setEditingProtected}
										onChange={(value) => handleFieldChange(field.key, value)}
										onReset={() => handleReset(field)}
									/>
								))}
							</div>
						</>
					) : (
						<div className="text-center text-text-muted py-16 text-sm">
							Select a section from the left to view its settings.
						</div>
					)}
				{saveError && (
					<div className="px-5 pt-4">
						<div className="rounded-lg border border-error-border bg-error-bg px-4 py-3 text-[13px] text-[#e8a0a0] whitespace-pre-wrap">
							{saveError}
						</div>
					</div>
				)}
				</div>
			</div>

			<div className="flex justify-end gap-3 px-5 py-4 border-t border-border bg-bg-0/30 shrink-0">
				<Button type="button" variant="secondary" onClick={onClose}>
					Cancel
				</Button>
				<Button type="button" onClick={handleSave}>
					Save
				</Button>
			</div>
		</PanelOverlay>
	);
}

interface FieldEditorProps {
	field: SettingsField;
	value: unknown;
	editingProtected: Set<string>;
	setEditingProtected: Dispatch<SetStateAction<Set<string>>>;
	onChange: (value: unknown) => void;
	onReset: () => void;
}

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
						<Label htmlFor={field.key} className="normal-case text-[13px]">
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
				const schema = toRecordListFields(field.list_schema);
				const listRenderer = getListRenderer(field);
				const addLabel = field.list_schema?.add_label ?? "Add item";
				if (schema.length === 0 || listRenderer === "string") {
					const stringValues = items.map((item) =>
						typeof item === "string" ? item : "",
					);
					return (
						<StringList
							items={stringValues}
							onChange={onChange}
							addLabel={addLabel}
						/>
					);
				}
				if (listRenderer === "memory_endpoints") {
					return (
						<MemoryEndpointList
							items={items as Record<string, unknown>[]}
							onChange={onChange}
							listSchema={field.list_schema as SettingsListSchema}
						/>
					);
				}
				return (
					<RecordList
						items={items as Record<string, unknown>[]}
						onChange={onChange}
						schema={schema}
						defaultItem={listItemDefaults(schema)}
						addLabel={addLabel}
						groups={field.list_schema?.groups}
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
		<ProtectedSettingsCard protected={field.protected}>
			{field.field_type !== "boolean" && (
				<Label htmlFor={field.key}>{field.label}</Label>
			)}
			{field.protected && (
				<div className="text-amber-500 text-[11px] flex items-center gap-2 mt-0.5">
					{isProtected ? (
						<>
							<span>Editing this field can change model behavior.</span>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={() =>
									setEditingProtected((prev) => new Set(prev).add(field.key))
								}
								className="h-auto px-0 py-0 text-accent hover:text-accent-hover underline"
							>
								Edit
							</Button>
						</>
					) : (
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={onReset}
							className="h-auto px-0 py-0 text-text-muted hover:text-text-secondary underline"
						>
							Reset to default
						</Button>
					)}
				</div>
			)}
			{input}
			{field.description && (
				<span className="text-[11px] text-text-muted leading-relaxed">
					{field.description}
				</span>
			)}
		</ProtectedSettingsCard>
	);
}
