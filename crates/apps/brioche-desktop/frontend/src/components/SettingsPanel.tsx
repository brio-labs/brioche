import { useCallback, useState, useEffect } from 'react';
import { useSettingsStore } from '../stores/settingsStore';
import type { Settings } from '../ipc';
import { XIcon } from './Icons';

interface SettingsPanelProps {
    onClose: () => void;
}

export default function SettingsPanel({ onClose }: SettingsPanelProps) {
    const { settings, saveSettings, hasLoaded } = useSettingsStore();
    const [localSettings, setLocalSettings] = useState<Settings>(settings);
    const [isSaving, setIsSaving] = useState(false);

    // Sync local settings when store loads
    useEffect(() => {
        if (hasLoaded) {
            setLocalSettings(settings);
        }
    }, [hasLoaded, settings]);

    const handleChange = useCallback(
        (field: keyof Settings, value: string | boolean) => {
            setLocalSettings((prev) => ({ ...prev, [field]: value }));
        },
        []
    );

    const handleSave = useCallback(async () => {
        setIsSaving(true);
        await saveSettings(localSettings);
        setIsSaving(false);
        onClose();
    }, [localSettings, saveSettings, onClose]);

    return (
        <div className="settings-overlay" onClick={onClose}>
            <div className="settings-panel" onClick={(e) => e.stopPropagation()}>
                <div className="settings-header">
                    <h2>Settings</h2>
                    <button type="button" className="settings-close" onClick={onClose}>
                        <XIcon />
                    </button>
                </div>
                <div className="settings-body">
                    <div className="setting-group">
                        <label htmlFor="api-key">API Key</label>
                        <input
                            id="api-key"
                            type="password"
                            value={localSettings.api_key}
                            onChange={(e) => handleChange('api_key', e.target.value)}
                            placeholder="sk-..."
                        />
                        <span className="setting-hint">OpenAI or OpenRouter API key</span>
                    </div>
                    <div className="setting-group">
                        <label htmlFor="model">Model</label>
                        <input
                            id="model"
                            type="text"
                            value={localSettings.model}
                            onChange={(e) => handleChange('model', e.target.value)}
                            placeholder="gpt-4o-mini"
                        />
                        <span className="setting-hint">e.g. gpt-4o-mini, claude-3.5-sonnet</span>
                    </div>
                    <div className="setting-group">
                        <label htmlFor="base-url">Base URL</label>
                        <input
                            id="base-url"
                            type="text"
                            value={localSettings.base_url}
                            onChange={(e) => handleChange('base_url', e.target.value)}
                            placeholder="https://api.openai.com/v1"
                        />
                        <span className="setting-hint">Use https://openrouter.ai/api/v1 for OpenRouter</span>
                    </div>
                    <div className="setting-group">
                        <label htmlFor="working-dir">Working Directory</label>
                        <input
                            id="working-dir"
                            type="text"
                            value={localSettings.working_dir}
                            onChange={(e) => handleChange('working_dir', e.target.value)}
                            placeholder="/home/user/project"
                        />
                        <span className="setting-hint">Project directory for file operations</span>
                    </div>
                </div>
                <div className="settings-footer">
                    <button type="button" className="btn-secondary" onClick={onClose}>
                        Cancel
                    </button>
                    <button
                        type="button"
                        className="btn-primary"
                        onClick={handleSave}
                        disabled={isSaving}
                    >
                        {isSaving ? 'Saving...' : 'Save'}
                    </button>
                </div>
            </div>
        </div>
    );
}
