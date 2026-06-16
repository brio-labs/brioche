import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface ChatMessagePayload {
    role: string;
    content: string;
}

export interface SessionInfo {
    id: string;
    active: boolean;
    created_at?: number;
    workspace?: string;
}

export interface DirEntry {
    name: string;
    is_dir: boolean;
    path: string;
}

export interface MemoryEntry {
    key: string;
    value: string;
    category: string;
    created_at: number;
    updated_at: number;
    access_count: number;
    provider_id: string;
}

export interface Profile {
    name: string;
    display_name: string;
    description: string | null;
    provider: string;
    model: string;
    api_key: string;
    system_prompt: string | null;
    temperature: number | null;
    max_tokens: number | null;
    created_at: number;
    is_default: boolean;
}

export interface Skill {
    name: string;
    description: string;
    version: string;
    author: string;
    license: string;
    platforms: string[];
    category: string;
    path: string;
    tags: string[];
    related_skills: string[];
    content: string;
    enabled: boolean;
}

export interface ModelInfo {
    id: string;
    name: string;
    provider: string;
}

export interface ExtensionMetadata {
    id: string;
    name: string;
    version: string;
    default_panel: 'left' | 'right' | 'center' | 'bottom' | null;
    enabled: boolean;
}

export interface SettingsOption {
    value: string;
    label: string;
}

export interface SettingsField {
    key: string;
    label: string;
    field_type:
        | 'string'
        | 'text'
        | 'password'
        | 'number'
        | 'boolean'
        | 'select'
        | 'multi_select'
        | 'list'
        | 'object'
        | 'path'
        | 'protected_markdown';
    description: string | null;
    placeholder: string | null;
    options: SettingsOption[];
    default_value: unknown;
    protected: boolean;
    keywords: string[];
}

export interface SettingsSection {
    id: string;
    module_id: string;
    title: string;
    order: number;
    keywords: string[];
    fields: SettingsField[];
}

export interface FooterMetric {
    id: string;
    label: string;
    value: string;
    tooltip: string | null;
    priority: number;
}

export interface ToolDescriptor {
    id: string;
    name: string;
    description: string;
    parameters: unknown;
    category: string;
    tags: string[];
    enabled: boolean;
    source: string;
}

export interface UserToolDefinition {
    id: string;
    name: string;
    description: string;
    parameters: unknown;
    category: string;
    tags: string[];
    executor:
        | { command: string; working_dir: string | null }
        | { url: string; headers: Record<string, string> }
        | { path: string };
}

// Module-scoped settings object.
export type Settings = Record<string, unknown>;

export async function sendMessage(content: string): Promise<void> {
    return invoke('send_message', { content });
}

export async function getMessages(): Promise<ChatMessagePayload[]> {
    return invoke('get_messages');
}

export async function clearMessages(): Promise<void> {
    return invoke('clear_messages');
}

export async function listSessions(): Promise<SessionInfo[]> {
    return invoke('list_sessions');
}

export async function switchSession(id: string): Promise<void> {
    return invoke('switch_session', { id });
}

export async function deleteSession(id: string): Promise<void> {
    return invoke('delete_session', { id });
}

export async function newSession(): Promise<string> {
    return invoke('new_session');
}

export async function getSettings(): Promise<Settings> {
    return invoke('get_settings');
}

export async function setSettings(settings: Settings): Promise<void> {
    return invoke('set_settings', { settings });
}

export async function readDirectory(path: string): Promise<DirEntry[]> {
    return invoke('read_directory', { path });
}

// Memory IPC
export async function listMemories(category?: string): Promise<MemoryEntry[]> {
    return invoke('list_memories', { category: category || null });
}

export async function setMemory(key: string, value: string, category: string): Promise<void> {
    return invoke('set_memory', { key, value, category });
}

export async function deleteMemory(key: string): Promise<void> {
    return invoke('delete_memory', { key });
}

export async function searchMemories(query: string): Promise<MemoryEntry[]> {
    return invoke('search_memories', { query });
}

// Profile IPC
export async function listProfiles(): Promise<Profile[]> {
    return invoke('list_profiles');
}

export async function getProfile(name?: string): Promise<Profile | null> {
    return invoke('get_profile', { name: name || null });
}

export async function createProfile(
    name: string,
    displayName: string,
    provider: string,
    model: string,
    apiKey: string,
): Promise<void> {
    return invoke('create_profile', { name, displayName, provider, model, apiKey });
}

export async function switchProfile(name: string): Promise<void> {
    return invoke('switch_profile', { name });
}

export async function deleteProfile(name: string): Promise<void> {
    return invoke('delete_profile', { name });
}

export async function updateProfile(profile: Profile): Promise<void> {
    return invoke('update_profile', { profile });
}

// Skills IPC
export async function listSkills(): Promise<Skill[]> {
    return invoke('list_skills');
}

export async function getSkillContent(name: string): Promise<string> {
    return invoke('get_skill_content', { name });
}

export async function getSkillFile(name: string, filePath: string): Promise<string> {
    return invoke('get_skill_file', { name, filePath });
}

export async function setSkillEnabled(name: string, enabled: boolean): Promise<void> {
    return invoke('set_skill_enabled', { name, enabled });
}

// Model fetching
export async function fetchModels(): Promise<ModelInfo[]> {
    return invoke('fetch_models');
}

// Extension IPC
export async function listExtensions(): Promise<ExtensionMetadata[]> {
    return invoke('list_extensions');
}

export async function listSettingsSections(): Promise<SettingsSection[]> {
    return invoke('list_settings_sections');
}

export async function getFooterMetrics(): Promise<FooterMetric[]> {
    return invoke('get_footer_metrics');
}

// Tool IPC
export async function listTools(): Promise<ToolDescriptor[]> {
    return invoke('list_tools');
}

export async function setToolEnabled(id: string, enabled: boolean): Promise<void> {
    return invoke('set_tool_enabled', { id, enabled });
}

export async function addUserTool(tool: UserToolDefinition): Promise<void> {
    return invoke('add_user_tool', { tool });
}

export async function removeUserTool(id: string): Promise<void> {
    return invoke('remove_user_tool', { id });
}

// Attachment IPC
export async function attachReference(path: string): Promise<void> {
    return invoke('attach_reference', { path });
}

export async function sendImage(path: string): Promise<string> {
    return invoke('send_image', { path });
}

// Event listeners
export function onChatMessage(callback: (msg: ChatMessagePayload) => void): Promise<() => void> {
    return listen<ChatMessagePayload>('chat-message', (event) => {
        callback(event.payload);
    });
}

export function onAppExit(callback: () => void): Promise<() => void> {
    return listen('app-exit', () => {
        callback();
    });
}

export function onSessionChanged(callback: (id: string) => void): Promise<() => void> {
    return listen<string>('session-changed', (event) => {
        callback(event.payload);
    });
}

export function onSessionsUpdated(callback: () => void): Promise<() => void> {
    return listen('sessions-updated', () => {
        callback();
    });
}
