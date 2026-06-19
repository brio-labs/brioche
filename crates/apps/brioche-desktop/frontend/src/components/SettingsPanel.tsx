import { useCallback, useEffect, useMemo, useState } from "react";
import { useSettingsStore } from "../stores/settingsStore";
import { setSettings } from "../ipc";
import type { SettingsSection, SettingsField } from "../ipc";
import PanelOverlay, { SearchBar } from "./PanelOverlay";

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
	const { settings, loadSettings, updateSetting, sections, loadSections } = useSettingsStore();
	const [selectedSectionId, setSelectedSectionId] = useState<string | null>(
		null,
	);
	const [search, setSearch] = useState("");
	const [editingProtected, setEditingProtected] = useState<Set<string>>(
		new Set(),
	);

	useEffect(() => {
		loadSettings();
		loadSections();
	}, [loadSettings, loadSections]);

	const activeSections = useMemo(() => {
		return sections.length > 0 ? sections : FALLBACK_SECTIONS;
	}, [sections]);

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
		<PanelOverlay title="Settings" onClose={onClose} panelClassName="bg-bg-1 border border-border rounded-lg w-[800px] max-w-[95vw] max-h-[85vh] flex flex-col overflow-hidden animate-slideUp shadow-2xl z-[1001]">
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
							<button
								key={section.id}
								type="button"
								className={`w-full text-left px-4 py-2.5 rounded text-[13px] font-semibold transition-all duration-150 cursor-pointer flex items-center ${
									selectedSectionId === section.id
										? "bg-accent/15 border-l-2 border-accent text-text-primary"
										: "text-text-secondary hover:bg-bg-2/50 hover:text-text-primary"
								}`}
								onClick={() => setSelectedSectionId(section.id)}
							>
								<span>{section.title}</span>
							</button>
						))}
					</div>
				</div>

				<div className="flex-1 overflow-y-auto p-6 flex flex-col gap-4">
					{selectedSection ? (
						<>
							<div className="border-b border-border pb-3 mb-2">
								<h3 className="text-base font-semibold text-text-primary">{selectedSection.title}</h3>
							</div>
							<div className="flex flex-col gap-6">
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
						<div className="text-center text-text-muted py-16 text-sm">
							Select a section from the left to view its settings.
						</div>
					)}
				</div>
			</div>

			<div className="flex justify-end gap-3 px-5 py-4 border-t border-border bg-bg-0/30 shrink-0">
				<button type="button" className="px-4 py-2 bg-transparent border border-border hover:border-border-hover text-text-secondary hover:text-text-primary hover:bg-bg-2 rounded text-xs font-medium tracking-wide cursor-pointer transition-colors duration-150" onClick={onClose}>
					Cancel
				</button>
				<button type="button" className="px-4 py-2 bg-accent hover:bg-accent-hover text-white rounded text-xs font-semibold tracking-wide cursor-pointer transition-colors duration-150 relative overflow-hidden shadow-sm shadow-accent-glow/20" onClick={handleSave}>
					Save
				</button>
			</div>
		</PanelOverlay>
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

	const inputClass = "flex-1 bg-bg-2 border border-border text-text-primary text-xs px-2.5 py-1.5 rounded outline-none focus:border-accent-dim/60 font-mono transition-all";
	const selectClass = "flex-1 bg-bg-2 border border-border text-text-primary text-xs px-2.5 py-1.5 rounded outline-none focus:border-accent-dim/60 font-mono transition-all appearance-none cursor-pointer";

	return (
		<div className="flex flex-col gap-3 mt-2">
			{items.map((item, index) => (
				<div key={index} className="p-3 bg-bg-2/30 border border-border rounded-lg flex flex-col gap-2">
					<div className="flex gap-2 items-center">
						<input
							type="text"
							value={String(item.provider || "openrouter")}
							onChange={(e) => updateItem(index, "provider", e.target.value)}
							placeholder="provider"
							className={inputClass}
						/>
						<input
							type="text"
							value={String(item.model || "")}
							onChange={(e) => updateItem(index, "model", e.target.value)}
							placeholder="model"
							className={inputClass}
						/>
						<button 
							type="button" 
							onClick={() => removeItem(index)}
							className="p-1 px-2.5 text-text-muted hover:text-red-400 font-bold transition-all text-sm shrink-0 hover:bg-bg-3 rounded cursor-pointer"
						>
							×
						</button>
					</div>
					<div className="flex gap-2 items-center">
						<input
							type="text"
							value={String(item.api_key || "")}
							onChange={(e) => updateItem(index, "api_key", e.target.value)}
							placeholder="api key (optional)"
							className={inputClass}
						/>
						<input
							type="text"
							value={String(item.base_url || "")}
							onChange={(e) => updateItem(index, "base_url", e.target.value)}
							placeholder="base url (optional)"
							className={inputClass}
						/>
					</div>
					<div className="flex gap-2 items-center">
						<input
							type="number"
							value={Number(item.context_window || 0)}
							onChange={(e) => {
								const n = Number(e.target.value);
								updateItem(index, "context_window", n > 0 ? n : undefined);
							}}
							placeholder="context window"
							className={inputClass}
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
							className={selectClass}
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
							className={inputClass}
						/>
					</div>
				</div>
			))}
			<button type="button" className="px-4 py-2 bg-transparent border border-border hover:border-border-hover text-text-secondary hover:text-text-primary hover:bg-bg-2 rounded text-xs font-medium tracking-wide cursor-pointer transition-colors duration-150 self-start" onClick={addItem}>
				Add fallback model
			</button>
		</div>
	);
}

interface MemoryEndpointListProps {
	items: Record<string, unknown>[];
	onChange: (value: unknown) => void;
}

function MemoryEndpointList({ items, onChange }: MemoryEndpointListProps) {
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
				id: `memory-amp-${items.length + 1}`,
				name: `Memory ${items.length + 1}`,
				url: "http://127.0.0.1:9471",
				api_key: "",
				scope: "",
			},
		]);
	};

	const removeItem = (index: number) => {
		const next = items.filter((_, i) => i !== index);
		onChange(next);
	};

	const inputClass = "flex-1 bg-bg-2 border border-border text-text-primary text-xs px-2.5 py-1.5 rounded outline-none focus:border-accent-dim/60 font-mono transition-all";

	return (
		<div className="flex flex-col gap-3 mt-2">
			{items.map((item, index) => (
				<div key={index} className="p-3 bg-bg-2/30 border border-border rounded-lg flex flex-col gap-2">
					<div className="flex gap-2 items-center">
						<input
							type="text"
							value={String(item.id || "")}
							onChange={(e) => updateItem(index, "id", e.target.value)}
							placeholder="ID (e.g. memory-amp-1)"
							className={inputClass}
						/>
						<input
							type="text"
							value={String(item.name || "")}
							onChange={(e) => updateItem(index, "name", e.target.value)}
							placeholder="Name"
							className={inputClass}
						/>
						<button 
							type="button" 
							onClick={() => removeItem(index)}
							className="p-1 px-2.5 text-text-muted hover:text-red-400 font-bold transition-all text-sm shrink-0 hover:bg-bg-3 rounded cursor-pointer"
						>
							×
						</button>
					</div>
					<div className="flex gap-2 items-center">
						<input
							type="text"
							value={String(item.url || "")}
							onChange={(e) => updateItem(index, "url", e.target.value)}
							placeholder="URL (e.g. http://localhost:9471)"
							className={inputClass}
						/>
					</div>
					<div className="flex gap-2 items-center">
						<input
							type="text"
							value={String(item.api_key || "")}
							onChange={(e) => updateItem(index, "api_key", e.target.value || null)}
							placeholder="API Key (optional)"
							className={inputClass}
						/>
						<input
							type="text"
							value={String(item.scope || "")}
							onChange={(e) => updateItem(index, "scope", e.target.value || null)}
							placeholder="Scope (optional)"
							className={inputClass}
						/>
					</div>
				</div>
			))}
			<button type="button" className="px-4 py-2 bg-transparent border border-border hover:border-border-hover text-text-secondary hover:text-text-primary hover:bg-bg-2 rounded text-xs font-medium tracking-wide cursor-pointer transition-colors duration-150 self-start" onClick={addItem}>
				Add memory endpoint
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

	const inputClass = "bg-bg-2 border border-border text-text-primary px-3 py-2 rounded text-[13px] outline-none font-mono transition-all focus:border-accent-dim focus:bg-bg-3 focus:ring-1 focus:ring-accent-glow";
	const selectClass = "bg-bg-2 border border-border text-text-primary px-3 py-2 rounded text-[13px] outline-none font-mono transition-all focus:border-accent-dim focus:bg-bg-3 focus:ring-1 focus:ring-accent-glow appearance-none cursor-pointer";
	const textareaClass = "bg-bg-2 border border-border text-text-primary px-3 py-2 rounded text-xs font-mono outline-none resize-y transition-all focus:border-accent-dim focus:bg-bg-3 focus:ring-1 focus:ring-accent-glow";

	const input = (() => {
		switch (field.field_type) {
			case "boolean":
				return (
					<label className="flex items-center gap-2 cursor-pointer text-[13px] text-text-secondary select-none">
						<input
							type="checkbox"
							checked={Boolean(currentValue)}
							onChange={(e) => onChange(e.target.checked)}
							className="rounded bg-bg-2 border-border text-accent focus:ring-accent-glow cursor-pointer"
						/>
						<span>{field.label}</span>
					</label>
				);
			case "select" as const:
				return (
					<select
						value={String(currentValue || "")}
						onChange={(e) => onChange(e.target.value)}
						className={selectClass}
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
						className={`${selectClass} h-24`}
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
						className={inputClass}
					/>
				);
			case "password":
				return (
					<input
						type="password"
						value={String(currentValue || "")}
						onChange={(e) => onChange(e.target.value)}
						placeholder={field.placeholder || undefined}
						className={inputClass}
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
						className={textareaClass}
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
						className={inputClass}
					/>
				);
			default:
				return (
					<input
						type="text"
						value={String(currentValue || "")}
						onChange={(e) => onChange(e.target.value)}
						placeholder={field.placeholder || undefined}
						className={inputClass}
					/>
				);
		}
	})();

	return (
		<div className={`flex flex-col gap-2 ${field.protected ? "p-3.5 bg-bg-2/20 border border-border/50 rounded-lg" : ""}`}>
			<label htmlFor={field.key} className="text-[11px] font-bold text-text-secondary uppercase tracking-wider">{field.label}</label>
			{field.protected && (
				<div className="text-amber-500 text-[11px] flex items-center gap-2 mt-0.5">
					{isProtected ? (
						<>
							<span>Editing this field can change model behavior.</span>
							<button
								type="button"
								className="text-accent hover:text-accent-hover font-semibold cursor-pointer underline"
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
						<button 
							type="button" 
							className="text-text-muted hover:text-text-secondary font-semibold cursor-pointer underline"
							onClick={onReset}
						>
							Reset to default
						</button>
					)}
				</div>
			)}
			{input}
			{field.description && (
				<span className="text-[11px] text-text-muted leading-relaxed">{field.description}</span>
			)}
		</div>
	);
}
