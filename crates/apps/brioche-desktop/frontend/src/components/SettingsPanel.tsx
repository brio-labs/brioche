import { useCallback, useEffect, useMemo, useState } from "react";
import { useSettingsStore } from "../stores/settingsStore";
import { listSettingsSections, setSettings } from "../ipc";
import type { SettingsSection, SettingsField } from "../ipc";
import { XIcon, SearchIcon } from "./Icons";

interface SettingsPanelProps {
	onClose: () => void;
}

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

export default function SettingsPanel({ onClose }: SettingsPanelProps) {
	const { settings, loadSettings, updateSetting } = useSettingsStore();
	const [sections, setSections] = useState<SettingsSection[]>([]);
	const [search, setSearch] = useState("");
	const [editingProtected, setEditingProtected] = useState<Set<string>>(
		new Set(),
	);

	useEffect(() => {
		loadSettings();
		listSettingsSections()
			.then((data) => {
				setSections(data);
			})
			.catch((err) => {
				console.error("Failed to load settings sections:", err);
			});
	}, [loadSettings]);

	// When Tauri IPC is unavailable (e.g. browser preview), show sections with
	// fallback defaults so the panel is testable without the backend.
	useEffect(() => {
		if (sections.length === 0) {
			const timer = setTimeout(() => {
				setSections((current) =>
					current.length > 0
						? current
						: [
								{
									id: "chat-model",
									module_id: "chat",
									title: "Model",
									order: 10,
									keywords: ["model", "provider", "api key"],
									fields: [
										{
											key: "chat.provider",
											label: "Provider",
											field_type: "select" as const,
											description: "LLM provider backend",
											placeholder: null,
											options: [
												{ value: "openai", label: "OpenAI" },
												{ value: "openrouter", label: "OpenRouter" },
												{ value: "anthropic", label: "Anthropic" },
											],
											default_value: "openrouter",
											protected: false,
											keywords: [],
										},
										{
											key: "chat.model",
											label: "Model",
											field_type: "string" as const,
											description: "Primary model identifier",
											placeholder: "qwen/qwen3.7-plus",
											options: [],
											default_value: "qwen/qwen3.7-plus",
											protected: false,
											keywords: [],
										},
									],
								},
								{
									id: "chat-identity",
									module_id: "chat",
									title: "Model Identity",
									order: 20,
									keywords: ["personality", "system prompt"],
									fields: [
										{
											key: "chat.personality",
											label: "Personality",
											field_type: "select" as const,
											description: "Default conversational style",
											placeholder: null,
											options: [
												{ value: "helpful", label: "Helpful" },
												{ value: "teacher", label: "Teacher" },
												{ value: "creative", label: "Creative" },
												{ value: "concise", label: "Concise" },
											],
											default_value: "helpful",
											protected: false,
											keywords: [],
										},
										{
											key: "chat.system_prompt",
											label: "System prompt",
											field_type: "protected_markdown" as const,
											description:
												"The system prompt sent at the start of every session.",
											placeholder: null,
											options: [],
											default_value:
												"You are a helpful AI coding assistant with access to filesystem tools.",
											protected: true,
											keywords: ["prompt"],
										},
									],
								},
							],
				);
			}, 300);
			return () => clearTimeout(timer);
		}
	}, [sections.length]);

	const filteredSections = useMemo(() => {
		if (!search.trim()) return sections;
		const q = search.toLowerCase();
		return sections
			.map((s) => {
				const matches =
					s.title.toLowerCase().includes(q) ||
					s.keywords.some((k) => k.toLowerCase().includes(q)) ||
					s.fields.some(
						(f) =>
							f.label.toLowerCase().includes(q) ||
							(f.description || "").toLowerCase().includes(q) ||
							f.keywords.some((k) => k.toLowerCase().includes(q)),
					);
				if (matches) {
					const fields = s.fields.filter(
						(f) =>
							f.label.toLowerCase().includes(q) ||
							(f.description || "").toLowerCase().includes(q) ||
							f.keywords.some((k) => k.toLowerCase().includes(q)) ||
							s.title.toLowerCase().includes(q) ||
							s.keywords.some((k) => k.toLowerCase().includes(q)),
					);
					return { ...s, fields };
				}
				return null;
			})
			.filter(Boolean) as SettingsSection[];
	}, [sections, search]);

	const handleSave = useCallback(async () => {
		try {
			await setSettings(settings);
			onClose();
		} catch (err) {
			console.error("Failed to save settings:", err);
		}
	}, [settings, onClose]);

	const handleReset = useCallback(
		(field: SettingsField) => {
			updateSetting(field.key, field.default_value);
		},
		[updateSetting],
	);

	return (
		<div className="settings-overlay" onClick={onClose}>
			<div className="settings-panel" onClick={(e) => e.stopPropagation()}>
				<div className="settings-header">
					<h2>Settings</h2>
					<button type="button" className="settings-close" onClick={onClose}>
						<XIcon />
					</button>
				</div>

				<div className="settings-search">
					<SearchIcon />
					<input
						type="text"
						placeholder="Search settings..."
						value={search}
						onChange={(e) => setSearch(e.target.value)}
					/>
				</div>

				<div className="settings-body">
					{filteredSections.map((section) => (
						<div key={section.id} className="settings-section">
							<h3>{section.title}</h3>
							<div className="settings-fields">
								{section.fields.map((field) => (
									<FieldEditor
										key={field.key}
										field={field}
										value={getFieldValue(settings, field.key)}
										editingProtected={editingProtected}
										setEditingProtected={setEditingProtected}
										onChange={(value) => updateSetting(field.key, value)}
										onReset={() => handleReset(field)}
									/>
								))}
							</div>
						</div>
					))}
				</div>

				<div className="settings-footer">
					<button type="button" className="btn-secondary" onClick={onClose}>
						Cancel
					</button>
					<button type="button" className="btn-primary" onClick={handleSave}>
						Save
					</button>
				</div>
			</div>
		</div>
	);
}

interface FieldEditorProps {
	field: SettingsField;
	value: unknown;
	editingProtected: Set<string>;
	setEditingProtected: React.Dispatch<React.SetStateAction<Set<string>>>;
	onChange: (value: unknown) => void;
	onReset: () => void;
}

interface FallbackModelListProps {
	items: Record<string, unknown>[];
	onChange: (value: unknown) => void;
}

function FallbackModelList({ items, onChange }: FallbackModelListProps) {
	const updateItem = (index: number, key: string, value: unknown) => {
		const next = items.map((item, i) =>
			i === index ? { ...item, [key]: value } : item,
		);
		onChange(next);
	};

	const addItem = () => {
		onChange([
			...items,
			{
				provider: "openrouter",
				model: "",
				api_key: "",
				base_url: "",
				context_window: undefined,
				reasoning_enabled: undefined,
				reasoning_effort: "medium",
			},
		]);
	};

	const removeItem = (index: number) => {
		const next = items.filter((_, i) => i !== index);
		onChange(next);
	};

	return (
		<div className="fallback-model-list">
			{items.map((item, index) => (
				<div key={index} className="fallback-model-item">
					<div className="fallback-model-row">
						<input
							type="text"
							value={String(item.provider || "openrouter")}
							onChange={(e) => updateItem(index, "provider", e.target.value)}
							placeholder="provider"
						/>
						<input
							type="text"
							value={String(item.model || "")}
							onChange={(e) => updateItem(index, "model", e.target.value)}
							placeholder="model"
						/>
						<button type="button" onClick={() => removeItem(index)}>
							×
						</button>
					</div>
					<div className="fallback-model-row">
						<input
							type="text"
							value={String(item.api_key || "")}
							onChange={(e) => updateItem(index, "api_key", e.target.value)}
							placeholder="api key (optional)"
						/>
						<input
							type="text"
							value={String(item.base_url || "")}
							onChange={(e) => updateItem(index, "base_url", e.target.value)}
							placeholder="base url (optional)"
						/>
					</div>
					<div className="fallback-model-row">
						<input
							type="number"
							value={Number(item.context_window || 0)}
							onChange={(e) => {
								const n = Number(e.target.value);
								updateItem(index, "context_window", n > 0 ? n : undefined);
							}}
							placeholder="context window"
						/>
						<select
							value={
								item.reasoning_enabled === true
									? "true"
									: item.reasoning_enabled === false
										? "false"
										: ""
							}
							onChange={(e) => {
								const val = e.target.value;
								updateItem(
									index,
									"reasoning_enabled",
									val === "" ? undefined : val === "true",
								);
							}}
						>
							<option value="">default reasoning</option>
							<option value="true">enabled</option>
							<option value="false">disabled</option>
						</select>
						<input
							type="text"
							value={String(item.reasoning_effort || "medium")}
							onChange={(e) =>
								updateItem(index, "reasoning_effort", e.target.value)
							}
							placeholder="reasoning effort"
						/>
					</div>
				</div>
			))}
			<button type="button" className="btn-secondary" onClick={addItem}>
				Add fallback model
			</button>
		</div>
	);
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
					<label className="setting-toggle">
						<input
							type="checkbox"
							checked={Boolean(currentValue)}
							onChange={(e) => onChange(e.target.checked)}
						/>
						<span>{field.label}</span>
					</label>
				);
			case "select" as const:
				return (
					<select
						value={String(currentValue || "")}
						onChange={(e) => onChange(e.target.value)}
					>
						{field.options.map((opt) => (
							<option key={opt.value} value={opt.value}>
								{opt.label}
							</option>
						))}
					</select>
				);
			case "multi_select": {
				const selected = Array.isArray(currentValue)
					? currentValue.map(String)
					: [];
				return (
					<select
						multiple
						value={selected}
						onChange={(e) => {
							const values = Array.from(e.target.selectedOptions).map(
								(o) => o.value,
							);
							onChange(values);
						}}
					>
						{field.options.map((opt) => (
							<option key={opt.value} value={opt.value}>
								{opt.label}
							</option>
						))}
					</select>
				);
			}
			case "number":
				return (
					<input
						type="number"
						value={Number(currentValue || 0)}
						onChange={(e) => onChange(Number(e.target.value))}
						placeholder={field.placeholder || undefined}
					/>
				);
			case "password":
				return (
					<input
						type="password"
						value={String(currentValue || "")}
						onChange={(e) => onChange(e.target.value)}
						placeholder={field.placeholder || undefined}
					/>
				);
			case "text":
			case "protected_markdown" as const:
				return (
					<textarea
						value={String(currentValue || "")}
						onChange={(e) => onChange(e.target.value)}
						rows={field.field_type === "protected_markdown" as const ? 8 : 4}
						disabled={isProtected}
						placeholder={field.placeholder || undefined}
					/>
				);
			case "list": {
				const items = Array.isArray(currentValue) ? currentValue : [];
				return (
					<FallbackModelList
						items={items as Record<string, unknown>[]}
						onChange={onChange}
					/>
				);
			}
			case "path":
				return (
					<input
						type="text"
						value={String(currentValue || "")}
						onChange={(e) => onChange(e.target.value)}
						placeholder={field.placeholder || undefined}
					/>
				);
			default:
				return (
					<input
						type="text"
						value={String(currentValue || "")}
						onChange={(e) => onChange(e.target.value)}
						placeholder={field.placeholder || undefined}
					/>
				);
		}
	})();

	return (
		<div className={`setting-group ${field.protected ? "protected" : ""}`}>
			<label htmlFor={field.key}>{field.label}</label>
			{field.protected && (
				<div className="protected-warning">
					{isProtected ? (
						<>
							<span>Editing this field can change model behavior.</span>
							<button
								type="button"
								onClick={() =>
									setEditingProtected((prev) => {
										const next = new Set(prev);
										next.add(field.key);
										return next;
									})
								}
							>
								Edit
							</button>
						</>
					) : (
						<button type="button" onClick={onReset}>
							Reset to default
						</button>
					)}
				</div>
			)}
			{input}
			{field.description && (
				<span className="setting-hint">{field.description}</span>
			)}
		</div>
	);
}
