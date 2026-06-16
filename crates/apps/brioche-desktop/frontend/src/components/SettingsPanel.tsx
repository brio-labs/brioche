import { useCallback, useEffect, useMemo, useState } from 'react';
import { useSettingsStore } from '../stores/settingsStore';
import { listSettingsSections, setSettings } from '../ipc';
import type { SettingsSection, SettingsField } from '../ipc';
import { XIcon, SearchIcon } from './Icons';

interface SettingsPanelProps {
    onClose: () => void;
}

function getFieldValue(settings: Record<string, unknown>, key: string): unknown {
    const parts = key.split('.');
    let current: unknown = settings;
    for (const part of parts) {
        if (current && typeof current === 'object' && !Array.isArray(current)) {
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
    const [search, setSearch] = useState('');
    const [editingProtected, setEditingProtected] = useState<Set<string>>(new Set());

    useEffect(() => {
        loadSettings();
        listSettingsSections().then(setSections).catch(console.error);
    }, [loadSettings]);

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
                            (f.description || '').toLowerCase().includes(q) ||
                            f.keywords.some((k) => k.toLowerCase().includes(q)),
                    );
                if (matches) {
                    const fields = s.fields.filter(
                        (f) =>
                            f.label.toLowerCase().includes(q) ||
                            (f.description || '').toLowerCase().includes(q) ||
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
            console.error('Failed to save settings:', err);
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
            case 'boolean':
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
            case 'select':
                return (
                    <select
                        value={String(currentValue || '')}
                        onChange={(e) => onChange(e.target.value)}
                    >
                        {field.options.map((opt) => (
                            <option key={opt.value} value={opt.value}>
                                {opt.label}
                            </option>
                        ))}
                    </select>
                );
            case 'multi_select': {
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
            case 'number':
                return (
                    <input
                        type="number"
                        value={Number(currentValue || 0)}
                        onChange={(e) => onChange(Number(e.target.value))}
                        placeholder={field.placeholder || undefined}
                    />
                );
            case 'password':
                return (
                    <input
                        type="password"
                        value={String(currentValue || '')}
                        onChange={(e) => onChange(e.target.value)}
                        placeholder={field.placeholder || undefined}
                    />
                );
            case 'text':
            case 'protected_markdown':
                return (
                    <textarea
                        value={String(currentValue || '')}
                        onChange={(e) => onChange(e.target.value)}
                        rows={field.field_type === 'protected_markdown' ? 8 : 4}
                        disabled={isProtected}
                        placeholder={field.placeholder || undefined}
                    />
                );
            case 'path':
                return (
                    <input
                        type="text"
                        value={String(currentValue || '')}
                        onChange={(e) => onChange(e.target.value)}
                        placeholder={field.placeholder || undefined}
                    />
                );
            default:
                return (
                    <input
                        type="text"
                        value={String(currentValue || '')}
                        onChange={(e) => onChange(e.target.value)}
                        placeholder={field.placeholder || undefined}
                    />
                );
        }
    })();

    return (
        <div className={`setting-group ${field.protected ? 'protected' : ''}`}>
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
                        <button type="button" onClick={onReset}>Reset to default</button>
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
