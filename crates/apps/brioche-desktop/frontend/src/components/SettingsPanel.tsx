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

const FALLBACK_SECTIONS: SettingsSection[] = [
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
				description: "The system prompt sent at the start of every session.",
				placeholder: null,
				options: [],
				default_value:
					"You are a helpful AI coding assistant with access to filesystem tools.",
				protected: true,
				keywords: ["prompt"],
			},
		],
	},
	{
		id: "context-compressor",
		module_id: "context",
		title: "Context Compressor",
		order: 30,
		keywords: ["context", "compress", "sliding window"],
		fields: [
			{
				key: "context.enabled",
				label: "Enable compressor",
				field_type: "boolean" as const,
				description: "Compress context when it grows too large",
				placeholder: null,
				options: [],
				default_value: true,
				protected: false,
				keywords: [],
			},
			{
				key: "context.trigger_percentage",
				label: "Trigger percentage",
				field_type: "number" as const,
				description:
					"Activate compression when this percentage of the context window is used",
				placeholder: "75",
				options: [],
				default_value: 75,
				protected: false,
				keywords: ["threshold"],
			},
			{
				key: "context.target_percentage",
				label: "Target percentage",
				field_type: "number" as const,
				description: "Target context size after compression",
				placeholder: "50",
				options: [],
				default_value: 50,
				protected: false,
				keywords: [],
			},
			{
				key: "context.preserve_recent",
				label: "Preserve recent",
				field_type: "number" as const,
				description: "Number of recent messages to always keep",
				placeholder: "6",
				options: [],
				default_value: 6,
				protected: false,
				keywords: [],
			},
		],
	},
	{
		id: "memory-providers",
		module_id: "memory",
		title: "Memory Providers",
		order: 40,
		keywords: ["memory", "amp", "endpoint", "honcho", "hindsight", "mem0"],
		fields: [
			{
				key: "memory.active_providers",
				label: "Active providers",
				field_type: "multi_select" as const,
				description:
					"Memory systems consulted during conversations. Built-in memory-local plus any AMP endpoints configured below.",
				placeholder: null,
				options: [{ value: "memory-local", label: "Local memory" }],
				default_value: ["memory-local"],
				protected: false,
				keywords: ["active"],
			},
			{
				key: "memory.endpoints",
				label: "AMP endpoints",
				field_type: "list" as const,
				description:
					"Generic AMP Core-compatible memory endpoints. Any backend that implements /v1/encode, /v1/recall and /v1/forget can be added here without code changes.",
				placeholder: null,
				options: [],
				default_value: [
					{
						id: "memory-amp-1",
						name: "Remote memory",
						url: "http://localhost:9471",
						api_key: null,
						scope: null,
					},
				],
				protected: false,
				keywords: ["amp", "endpoint", "url", "api key"],
			},
		],
	},
];

export default function SettingsPanel({ onClose }: SettingsPanelProps) {
	const { settings, loadSettings, updateSetting } = useSettingsStore();
	const [sections, setSections] = useState<SettingsSection[]>([]);
	const [selectedSectionId, setSelectedSectionId] = useState<string | null>(
		null,
	);
	const [search, setSearch] = useState("");
	const [editingProtected, setEditingProtected] = useState<Set<string>>(
		new Set(),
	);

	useEffect(() => {
		loadSettings();
		listSettingsSections()
			.then((data) => {
				setSections(data.length > 0 ? data : FALLBACK_SECTIONS);
			})
			.catch((err) => {
				console.error("Failed to load settings sections:", err);
				setSections(FALLBACK_SECTIONS);
			});
	}, [loadSettings]);

	const filteredSections = useMemo(() => {
		if (!search.trim()) return sections;
		const q = search.toLowerCase();
		return sections.filter(
			(s) =>
				s.title.toLowerCase().includes(q) ||
				s.keywords.some((k) => k.toLowerCase().includes(q)),
		);
	}, [sections, search]);

	const selectedSection = useMemo(() => {
		if (!selectedSectionId) return null;
		return sections.find((s) => s.id === selectedSectionId) || null;
	}, [selectedSectionId, sections]);

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

				<div className="settings-layout">
					<div className="settings-nav">
						<div className="settings-search">
							<SearchIcon />
							<input
								type="text"
								placeholder="Search settings..."
								value={search}
								onChange={(e) => setSearch(e.target.value)}
							/>
						</div>
						<div className="settings-nav-list">
							{filteredSections.map((section) => (
								<button
									key={section.id}
									type="button"
									className={`settings-nav-item ${
										selectedSectionId === section.id ? "active" : ""
									}`}
									onClick={() => setSelectedSectionId(section.id)}
								>
									<span className="settings-nav-title">{section.title}</span>
								</button>
							))}
						</div>
					</div>

					<div className="settings-content">
						{selectedSection ? (
							<>
								<div className="settings-content-header">
									<h3>{selectedSection.title}</h3>
								</div>
								<div className="settings-fields">
									{selectedSection.fields.map((field) => (
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
							</>
						) : (
							<div className="settings-empty">
								Select a section from the left to view its settings.
							</div>
						)}
					</div>
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
						rows={field.field_type === ("protected_markdown" as const) ? 8 : 4}
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
