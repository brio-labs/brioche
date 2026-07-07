import type { SettingsSection } from '../ipc';
import { THEMES } from './themeStore';

/// Theme section is frontend-owned because the palette must apply before any
/// backend settings are available.
export const APPEARANCE_SETTINGS_SECTION: SettingsSection = {
    id: 'appearance',
    module_id: 'ui',
    title: 'Appearance',
    order: 5,
    keywords: ['theme', 'color', 'palette'],
    fields: [
        {
            key: 'ui.theme',
            label: 'Color theme',
            field_type: 'select' as const,
            description:
                'Switch the desktop color palette. Applies immediately and is saved locally.',
            placeholder: null,
            options: THEMES.map((theme) => ({
                value: theme.id,
                label: theme.label,
            })),
            default_value: 'brio',
            protected: false,
            keywords: ['theme', 'appearance'],
        },
    ],
};

/// Default settings sections shown when the backend returns no sections.
///
/// Refs: I-Ui-SettingsFallback
export const FALLBACK_SECTIONS: SettingsSection[] = [
    APPEARANCE_SETTINGS_SECTION,
    {
        id: 'chat-model',
        module_id: 'chat',
        title: 'Model',
        order: 10,
        keywords: ['model', 'provider', 'api key'],
        fields: [
            {
                key: 'chat.provider',
                label: 'Provider',
                field_type: 'select' as const,
                description: 'LLM provider backend',
                placeholder: null,
                options: [
                    { value: 'openai', label: 'OpenAI' },
                    { value: 'openrouter', label: 'OpenRouter' },
                    { value: 'anthropic', label: 'Anthropic' },
                ],
                default_value: 'openrouter',
                protected: false,
                keywords: [],
            },
            {
                key: 'chat.model',
                label: 'Model',
                field_type: 'string' as const,
                description: 'Primary model identifier',
                placeholder: 'qwen/qwen3.7-plus',
                options: [],
                default_value: 'qwen/qwen3.7-plus',
                protected: false,
                keywords: [],
            },
        ],
    },
    {
        id: 'chat-identity',
        module_id: 'chat',
        title: 'Model Identity',
        order: 20,
        keywords: ['personality', 'system prompt'],
        fields: [
            {
                key: 'chat.personality',
                label: 'Personality',
                field_type: 'select' as const,
                description: 'Default conversational style',
                placeholder: null,
                options: [
                    { value: 'helpful', label: 'Helpful' },
                    { value: 'teacher', label: 'Teacher' },
                    { value: 'creative', label: 'Creative' },
                    { value: 'concise', label: 'Concise' },
                ],
                default_value: 'helpful',
                protected: false,
                keywords: [],
            },
            {
                key: 'chat.system_prompt',
                label: 'System prompt',
                field_type: 'protected_markdown' as const,
                description: 'The system prompt sent at the start of every session.',
                placeholder: null,
                options: [],
                default_value:
                    'You are a helpful AI coding assistant with access to filesystem tools.',
                protected: true,
                keywords: ['prompt'],
            },
        ],
    },
    {
        id: 'context-compressor',
        module_id: 'context',
        title: 'Context Compressor',
        order: 30,
        keywords: ['context', 'compress', 'sliding window'],
        fields: [
            {
                key: 'context.enabled',
                label: 'Enable compressor',
                field_type: 'boolean' as const,
                description: 'Compress context when it grows too large',
                placeholder: null,
                options: [],
                default_value: true,
                protected: false,
                keywords: [],
            },
            {
                key: 'context.trigger_percentage',
                label: 'Trigger percentage',
                field_type: 'number' as const,
                description:
                    'Activate compression when this percentage of the context window is used',
                placeholder: '75',
                options: [],
                default_value: 75,
                protected: false,
                keywords: ['threshold'],
            },
            {
                key: 'context.target_percentage',
                label: 'Target percentage',
                field_type: 'number' as const,
                description: 'Target context size after compression',
                placeholder: '50',
                options: [],
                default_value: 50,
                protected: false,
                keywords: [],
            },
            {
                key: 'context.preserve_recent',
                label: 'Preserve recent',
                field_type: 'number' as const,
                description: 'Number of recent messages to always keep',
                placeholder: '6',
                options: [],
                default_value: 6,
                protected: false,
                keywords: [],
            },
        ],
    },
    {
        id: 'memory-providers',
        module_id: 'memory',
        title: 'Memory Providers',
        order: 40,
        keywords: ['memory', 'amp', 'endpoint', 'honcho', 'hindsight', 'mem0'],
        fields: [
            {
                key: 'memory.active_providers',
                label: 'Active providers',
                field_type: 'multi_select' as const,
                description:
                    'Memory systems consulted during conversations. Built-in memory-local plus any AMP endpoints configured below.',
                placeholder: null,
                options: [{ value: 'memory-local', label: 'Local memory' }],
                default_value: ['memory-local'],
                protected: false,
                keywords: ['active'],
            },
            {
                key: 'memory.endpoints',
                label: 'AMP endpoints',
                field_type: 'list' as const,
                description:
                    'Generic AMP Core-compatible memory endpoints. Any backend that implements /v1/encode, /v1/recall and /v1/forget can be added here without code changes.',
                placeholder: null,
                options: [],
                default_value: [
                    {
                        id: 'memory-amp-1',
                        name: 'Remote memory',
                        url: 'http://localhost:9471',
                        api_key: null,
                        scope: null,
                    },
                ],
                protected: false,
                keywords: ['amp', 'endpoint', 'url', 'api key'],
            },
        ],
    },
];

