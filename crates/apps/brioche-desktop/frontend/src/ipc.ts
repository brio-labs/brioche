import { invoke } from "@tauri-apps/api/core";

/// Payload returned by the backend for a single chat message.
///
/// Refs: I-Ui-ChatMessage
export interface ChatMessagePayload {
	role: string;
	content: string;
	tool_id?: string;
	tool_name?: string;
	tool_arguments?: string;
	tool_output?: string;
}

/// Metadata describing a Brioche session.
///
/// Refs: I-Ui-Session
export interface SessionInfo {
	id: string;
	active: boolean;
	created_at?: number;
	updated_at?: number;
	workspace?: string;
}

/// A single entry returned by filesystem directory listings.
///
/// Refs: I-Ui-FileSystem
export interface DirEntry {
	name: string;
	is_dir: boolean;
	path: string;
}

/// A stored memory entry.
///
/// Refs: I-Ui-Memory
export interface MemoryEntry {
	key: string;
	value: string;
	category: string;
	created_at: number;
	updated_at: number;
	access_count: number;
	provider_id: string;
}

/// Configuration for an AMP-compatible memory endpoint.
///
/// Refs: I-Ui-MemoryEndpoint
export interface MemoryEndpoint {
	id: string;
	name: string;
	url: string;
	api_key: string | null;
	scope: string | null;
}

/// A chat model profile.
///
/// Refs: I-Ui-Profile
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

/// A reusable skill definition.
///
/// Refs: I-Ui-Skill
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

/// A model available from a configured provider.
///
/// Refs: I-Ui-ModelInfo
export interface ModelInfo {
	id: string;
	name: string;
	provider: string;
}

/// Metadata for a Brioche extension.
///
/// Refs: I-Ui-Extension
export interface ExtensionMetadata {
	id: string;
	name: string;
	version: string;
	default_panel: "left" | "right" | "center" | "bottom" | null;
	enabled: boolean;
}

/// A single option for a select or multi_select settings field.
///
/// Refs: I-Ui-SettingsField
export interface SettingsOption {
	value: string;
	label: string;
}

/// A field inside a settings section.
///
/// Refs: I-Ui-SettingsField
export interface SettingsField {
	key: string;
	label: string;
	field_type:
		| "string"
		| "text"
		| "password"
		| "number"
		| "boolean"
		| "select"
		| "multi_select"
		| "list"
		| "object"
		| "path"
		| "protected_markdown";
	description: string | null;
	placeholder: string | null;
	options: SettingsOption[];
	default_value: unknown;
	protected: boolean;
	keywords: string[];
}

/// A grouped section of settings fields.
///
/// Refs: I-Ui-SettingsSection
export interface SettingsSection {
	id: string;
	module_id: string;
	title: string;
	order: number;
	keywords: string[];
	fields: SettingsField[];
}

/// A metric displayed in the footer.
///
/// Refs: I-Ui-FooterMetric
export interface FooterMetric {
	id: string;
	label: string;
	value: string;
	tooltip: string | null;
	priority: number;
}

/// Metadata describing a tool available to the agent.
///
/// Refs: I-Ui-ToolDescriptor
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

/// Definition for a user-defined tool.
///
/// Refs: I-Ui-UserTool
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

/// Module-scoped settings object keyed by module id.
///
/// Refs: I-Ui-Settings
export type Settings = Record<string, unknown>;

/// Sends a chat message to the backend.
///
/// Refs: I-Ui-Ipc-SendMessage
export async function sendMessage(content: string): Promise<void> {
	return invoke("send_message", { content });
}

/// Retrieves the full chat history for the active session.
///
/// Refs: I-Ui-Ipc-GetMessages
export async function getMessages(): Promise<ChatMessagePayload[]> {
	return invoke("get_messages");
}

/// Clears the chat history for the active session.
///
/// Refs: I-Ui-Ipc-ClearMessages
export async function clearMessages(): Promise<void> {
	return invoke("clear_messages");
}

/// Sort order for sessions within a project folder.
///
/// Refs: I-Ui-SessionSort
export type SessionSort = "date" | "name";

/// Lists all sessions sorted by the given criterion.
///
/// Refs: I-Ui-Ipc-ListSessions
export async function listSessions(
	sort: SessionSort = "date",
): Promise<SessionInfo[]> {
	return invoke("list_sessions", { sort });
}

/// Switches the active session to the given id.
///
/// Refs: I-Ui-Ipc-SwitchSession
export async function switchSession(id: string): Promise<void> {
	return invoke("switch_session", { id });
}

/// Deletes the session with the given id.
///
/// Refs: I-Ui-Ipc-DeleteSession
export async function deleteSession(id: string): Promise<void> {
	return invoke("delete_session", { id });
}

/// Creates a new session and returns its id.
///
/// Refs: I-Ui-Ipc-NewSession
export async function newSession(): Promise<string> {
	return invoke("new_session");
}

/// Loads the full settings object from the backend.
///
/// Refs: I-Ui-Ipc-GetSettings
export async function getSettings(): Promise<Settings> {
	return invoke("get_settings");
}

/// Persists the full settings object in the backend.
///
/// Refs: I-Ui-Ipc-SetSettings
export async function setSettings(settings: Settings): Promise<void> {
	return invoke("set_settings", { settings });
}

/// Lists the contents of a directory on the host filesystem.
///
/// Refs: I-Ui-Ipc-ReadDirectory
export async function readDirectory(path: string): Promise<DirEntry[]> {
	return invoke("read_directory", { path });
}

/// Reads the contents of a file from the host filesystem.
///
/// Refs: I-Ui-Ipc-ReadFile
export async function readFile(path: string): Promise<string> {
	return invoke("read_file", { path });
}

/// Writes content to a file on the host filesystem.
///
/// Refs: I-Ui-Ipc-WriteFile
export async function writeFile(path: string, content: string): Promise<void> {
	return invoke("write_file", { path, content });
}

/// Deletes a file from the host filesystem.
///
/// Refs: I-Ui-Ipc-DeleteFile
export async function deleteFile(path: string): Promise<void> {
	return invoke("delete_file", { path });
}

/// Creates a new empty file on the host filesystem.
///
/// Refs: I-Ui-Ipc-CreateFile
export async function createFile(path: string): Promise<void> {
	return invoke("create_file", { path });
}

/// Creates a new directory on the host filesystem.
///
/// Refs: I-Ui-Ipc-CreateDirectory
export async function createDirectory(path: string): Promise<void> {
	return invoke("create_directory", { path });
}

/// Renames a file or directory on the host filesystem.
///
/// Refs: I-Ui-Ipc-RenamePath
export async function renamePath(source: string, destination: string): Promise<void> {
	return invoke("rename_path", { source, destination });
}

/// Copies a file or directory to a new location on the host filesystem.
///
/// Refs: I-Ui-Ipc-CopyPath
export async function copyPath(source: string, destination: string): Promise<void> {
	return invoke("copy_path", { source, destination });
}

/// Lists memories, optionally filtered by category.
///
/// Refs: I-Ui-Ipc-ListMemories
export async function listMemories(category?: string): Promise<MemoryEntry[]> {
	return invoke("list_memories", { category: category || null });
}

/// Stores a memory entry in the given category.
///
/// Refs: I-Ui-Ipc-SetMemory
export async function setMemory(
	key: string,
	value: string,
	category: string,
): Promise<void> {
	return invoke("set_memory", { key, value, category });
}

/// Deletes the memory entry with the given key.
///
/// Refs: I-Ui-Ipc-DeleteMemory
export async function deleteMemory(key: string): Promise<void> {
	return invoke("delete_memory", { key });
}

/// Searches stored memories for the given query.
///
/// Refs: I-Ui-Ipc-SearchMemories
export async function searchMemories(query: string): Promise<MemoryEntry[]> {
	return invoke("search_memories", { query });
}

/// Lists all configured chat profiles.
///
/// Refs: I-Ui-Ipc-ListProfiles
export async function listProfiles(): Promise<Profile[]> {
	return invoke("list_profiles");
}

/// Fetches a profile by name, or the default profile when omitted.
///
/// Refs: I-Ui-Ipc-GetProfile
export async function getProfile(name?: string): Promise<Profile | null> {
	return invoke("get_profile", { name: name || null });
}

/// Creates a new chat profile.
///
/// Refs: I-Ui-Ipc-CreateProfile
export async function createProfile(
	name: string,
	displayName: string,
	provider: string,
	model: string,
	apiKey: string,
): Promise<void> {
	return invoke("create_profile", {
		name,
		displayName,
		provider,
		model,
		apiKey,
	});
}

/// Switches the active chat profile.
///
/// Refs: I-Ui-Ipc-SwitchProfile
export async function switchProfile(name: string): Promise<void> {
	return invoke("switch_profile", { name });
}

/// Deletes the chat profile with the given name.
///
/// Refs: I-Ui-Ipc-DeleteProfile
export async function deleteProfile(name: string): Promise<void> {
	return invoke("delete_profile", { name });
}

/// Updates an existing chat profile.
///
/// Refs: I-Ui-Ipc-UpdateProfile
export async function updateProfile(profile: Profile): Promise<void> {
	return invoke("update_profile", { profile });
}

/// Lists all available skills.
///
/// Refs: I-Ui-Ipc-ListSkills
export async function listSkills(): Promise<Skill[]> {
	return invoke("list_skills");
}

/// Loads the rendered content of a skill.
///
/// Refs: I-Ui-Ipc-GetSkillContent
export async function getSkillContent(name: string): Promise<string> {
	return invoke("get_skill_content", { name });
}

/// Loads an arbitrary file bundled inside a skill.
///
/// Refs: I-Ui-Ipc-GetSkillFile
export async function getSkillFile(
	name: string,
	filePath: string,
): Promise<string> {
	return invoke("get_skill_file", { name, filePath });
}

/// Enables or disables a skill.
///
/// Refs: I-Ui-Ipc-SetSkillEnabled
export async function setSkillEnabled(
	name: string,
	enabled: boolean,
): Promise<void> {
	return invoke("set_skill_enabled", { name, enabled });
}

/// Creates a new skill.
///
/// Refs: I-Ui-Ipc-CreateSkill
export async function createSkill(
	name: string,
	category: string,
	description: string,
	content: string,
): Promise<void> {
	return invoke("create_skill", { name, category, description, content });
}

/// Deletes a skill.
///
/// Refs: I-Ui-Ipc-DeleteSkill
export async function deleteSkill(name: string): Promise<void> {
	return invoke("delete_skill", { name });
}

/// Fetches available models from the configured provider.
///
/// Refs: I-Ui-Ipc-FetchModels
export async function fetchModels(): Promise<ModelInfo[]> {
	return invoke("fetch_models");
}

/// Lists installed extensions.
///
/// Refs: I-Ui-Ipc-ListExtensions
export async function listExtensions(): Promise<ExtensionMetadata[]> {
	return invoke("list_extensions");
}

/// Lists the settings sections exposed by extensions and built-in modules.
///
/// Refs: I-Ui-Ipc-ListSettingsSections
export async function listSettingsSections(): Promise<SettingsSection[]> {
	return invoke("list_settings_sections");
}

/// Fetches the metrics shown in the footer.
///
/// Refs: I-Ui-Ipc-GetFooterMetrics
export async function getFooterMetrics(): Promise<FooterMetric[]> {
	return invoke("get_footer_metrics");
}

/// Lists all tools available to the agent.
///
/// Refs: I-Ui-Ipc-ListTools
export async function listTools(): Promise<ToolDescriptor[]> {
	return invoke("list_tools");
}

/// Enables or disables a tool.
///
/// Refs: I-Ui-Ipc-SetToolEnabled
export async function setToolEnabled(
	id: string,
	enabled: boolean,
): Promise<void> {
	return invoke("set_tool_enabled", { id, enabled });
}

/// Adds a new user-defined tool.
///
/// Refs: I-Ui-Ipc-AddUserTool
export async function addUserTool(tool: UserToolDefinition): Promise<void> {
	return invoke("add_user_tool", { tool });
}

/// Removes a user-defined tool by id.
///
/// Refs: I-Ui-Ipc-RemoveUserTool
export async function removeUserTool(id: string): Promise<void> {
	return invoke("remove_user_tool", { id });
}

/// Attaches a filesystem reference as context for the current session.
///
/// Refs: I-Ui-Ipc-AttachReference
export async function attachReference(path: string): Promise<void> {
	return invoke("attach_reference", { path });
}

/// Sends an image file and returns its backend reference id.
///
/// Refs: I-Ui-Ipc-SendImage
export async function sendImage(path: string): Promise<string> {
	return invoke("send_image", { path });
}

/// Returns true when the frontend is running inside a Tauri webview.
///
/// Refs: I-Ui-Runtime-Tauri
export function isTauri(): boolean {
	return (
		typeof window !== "undefined" &&
		typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !==
			"undefined"
	);
}
