import type { SettingsField } from "../../ipc";
import { AlertTriangleIcon, EditIcon } from "../Icons";
import {
	Button,
	Checkbox,
	Input,
	Label,
	MultiSelect,
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	Textarea,
	cn,
} from "../ui";
import { MemoryEndpointList } from "./MemoryEndpointList";
import { RecordList } from "./RecordList";
import {
	FALLBACK_MODEL_DEFAULT,
	fallbackModelSchema,
} from "./settingsUtils";

/// Props for the editor that renders a single settings field.
///
/// Refs: I-Shell-Runtime-OnlyIO
export interface FieldEditorProps {
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
export function FieldEditor({
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
