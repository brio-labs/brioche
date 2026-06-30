import { useCallback, useEffect, useMemo, useState } from "react";
import type { Profile } from "../ipc";
import {
	listProfiles,
	getProfile,
	createProfile,
	switchProfile,
	deleteProfile,
	updateProfile,
	isTauri,
} from "../ipc";
import PanelOverlay, { SearchBar } from "./PanelOverlay";
import {
	UserIcon,
	PlusIcon,
	TrashIcon,
	CheckIcon,
	RefreshIcon,
} from "./Icons";
import {
	Button,
	Input,
	Label,
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
	cn,
} from "./ui";

/// Props for the profile management panel.
///
/// Refs: I-Shell-Runtime-OnlyIO
interface ProfilesPanelProps {
	onClose: () => void;
}

/// Available LLM providers when creating a new profile.
const PROVIDERS = [
	{ value: "openai", label: "OpenAI" },
	{ value: "openrouter", label: "OpenRouter" },
];

/// Renders a single profile entry in the selectable list.
///
/// Refs: I-Shell-Runtime-OnlyIO
function ProfileListItem({
	profile,
	isActive,
	isSelected,
	onSelect,
}: {
	profile: Profile;
	isActive: boolean;
	isSelected: boolean;
	onSelect: (name: string) => void;
}) {
	return (
		<div
			tabIndex={0}
			role="button"
			onClick={() => onSelect(profile.name)}
			onKeyDown={(e) => {
				if (e.key === "Enter" || e.key === " ") {
					e.preventDefault();
					onSelect(profile.name);
				}
			}}
			className={cn(
				"flex cursor-pointer flex-col gap-1.5 rounded-lg border p-3 transition-all duration-200 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent-glow",
				isSelected
					? "border-accent-dim/40 bg-accent/10 shadow-sm"
					: "border-transparent bg-transparent hover:border-border/60 hover:bg-bg-elevated/30",
			)}
		>
			<div className="flex items-center justify-between gap-2">
				<span className="truncate text-xs font-semibold text-fg-primary">
					{profile.display_name || profile.name}
				</span>
				{isActive && (
					<span className="shrink-0 rounded border border-success-border bg-success-bg px-1.5 py-0.5 text-xs font-bold uppercase text-success-text select-none">
						Active
					</span>
				)}
			</div>
			<div className="truncate text-xs text-fg-secondary">
				{profile.provider} / {profile.model}
			</div>
		</div>
	);
}

/// Form for creating a new profile.
///
/// Refs: I-Shell-Runtime-OnlyIO
function CreateProfileForm({
	onCancel,
	onCreated,
	setError,
}: {
	onCancel: () => void;
	onCreated: () => void;
	setError: (message: string | null) => void;
}) {
	const [name, setName] = useState("");
	const [displayName, setDisplayName] = useState("");
	const [provider, setProvider] = useState("openai");
	const [model, setModel] = useState("");
	const [apiKey, setApiKey] = useState("");
	const [isSaving, setIsSaving] = useState(false);

	const canSubmit = Boolean(name.trim() && model.trim() && !isSaving);

	const handleSubmit = async () => {
		const trimmedName = name.trim();
		if (!trimmedName) {
			setError("Profile name is required.");
			return;
		}
		const trimmedModel = model.trim();
		if (!trimmedModel) {
			setError("Model is required.");
			return;
		}
		setIsSaving(true);
		setError(null);
		try {
			await createProfile(
				trimmedName,
				displayName.trim() || trimmedName,
				provider,
				trimmedModel,
				apiKey.trim(),
			);
			onCreated();
		} catch (err) {
			setError(String(err));
		} finally {
			setIsSaving(false);
		}
	};

	return (
		<div className="flex flex-1 flex-col gap-4 overflow-y-auto p-5">
			<h3 className="text-sm font-semibold text-fg-primary">Create profile</h3>
			<div className="grid grid-cols-1 gap-4 md:grid-cols-2">
				<div className="flex flex-col gap-1.5">
					<Label className="text-xs font-medium text-fg-secondary">Name</Label>
					<Input
						value={name}
						onChange={(e) => setName(e.target.value)}
						placeholder="e.g. work"
					/>
				</div>
				<div className="flex flex-col gap-1.5">
					<Label className="text-xs font-medium text-fg-secondary">
						Display name
					</Label>
					<Input
						value={displayName}
						onChange={(e) => setDisplayName(e.target.value)}
						placeholder="e.g. Work account"
					/>
				</div>
			</div>
			<div className="flex flex-col gap-1.5">
				<Label className="text-xs font-medium text-fg-secondary">Provider</Label>
				<Select value={provider} onValueChange={setProvider}>
					<SelectTrigger>
						<SelectValue placeholder="Select provider" />
					</SelectTrigger>
					<SelectContent>
						{PROVIDERS.map((p) => (
							<SelectItem key={p.value} value={p.value}>
								{p.label}
							</SelectItem>
						))}
					</SelectContent>
				</Select>
			</div>
			<div className="flex flex-col gap-1.5">
				<Label className="text-xs font-medium text-fg-secondary">Model</Label>
				<Input
					value={model}
					onChange={(e) => setModel(e.target.value)}
					placeholder="e.g. gpt-4o-mini"
				/>
			</div>
			<div className="flex flex-col gap-1.5">
				<Label className="text-xs font-medium text-fg-secondary">API key</Label>
				<Input
					type="password"
					value={apiKey}
					onChange={(e) => setApiKey(e.target.value)}
					placeholder="sk-..."
				/>
			</div>
			<div className="mt-2 flex justify-end gap-2">
				<Button
					type="button"
					variant="secondary"
					onClick={onCancel}
					disabled={isSaving}
				>
					Cancel
				</Button>
				<Button type="button" onClick={handleSubmit} disabled={!canSubmit}>
					Create
				</Button>
			</div>
		</div>
	);
}

/// Renders details for the selected profile and allows editing, switching, or deleting it.
///
/// Refs: I-Shell-Runtime-OnlyIO
function ProfileDetails({
	profile,
	isActive,
	onSwitch,
	onDelete,
	onUpdate,
	setError,
}: {
	profile: Profile;
	isActive: boolean;
	onSwitch: (name: string) => void;
	onDelete: (name: string) => void;
	onUpdate: () => void;
	setError: (message: string | null) => void;
}) {
	const [displayName, setDisplayName] = useState(profile.display_name);
	const [model, setModel] = useState(profile.model);
	const [apiKey, setApiKey] = useState(profile.api_key);
	const [isSaving, setIsSaving] = useState(false);

	useEffect(() => {
		setDisplayName(profile.display_name);
		setModel(profile.model);
		setApiKey(profile.api_key);
	}, [profile.name]);

	const hasChanges =
		profile.display_name !== displayName ||
		profile.model !== model ||
		profile.api_key !== apiKey;

	const handleSave = async () => {
		const trimmedModel = model.trim();
		if (!trimmedModel) {
			setError("Model is required.");
			return;
		}
		setIsSaving(true);
		setError(null);
		try {
			await updateProfile({
				...profile,
				display_name: displayName.trim() || profile.name,
				model: trimmedModel,
				api_key: apiKey.trim(),
			});
			onUpdate();
		} catch (err) {
			setError(String(err));
		} finally {
			setIsSaving(false);
		}
	};

	const handleReset = () => {
		setDisplayName(profile.display_name);
		setModel(profile.model);
		setApiKey(profile.api_key);
	};

	return (
		<div className="flex flex-1 flex-col gap-4 overflow-y-auto p-5">
			<div className="flex items-start justify-between gap-4">
				<div className="flex flex-col gap-0.5">
					<h3 className="text-sm font-semibold text-fg-primary">
						{profile.display_name || profile.name}
					</h3>
					<div className="text-xs text-fg-secondary">
						{profile.provider} / {profile.model}
					</div>
				</div>
				<div className="flex shrink-0 items-center gap-2">
					{!isActive && (
						<div className="flex items-center gap-2">
							<Button
								type="button"
								variant="secondary"
								size="sm"
								onClick={() => onSwitch(profile.name)}
							>
								<CheckIcon className="mr-1 h-3 w-3" />
								Switch
							</Button>
							<Button
								type="button"
								variant="destructive"
								size="sm"
								onClick={() => onDelete(profile.name)}
							>
								<TrashIcon className="mr-1 h-3 w-3" />
								Delete
							</Button>
						</div>
					)}
					{isActive && (
						<span className="rounded border border-success-border bg-success-bg px-2 py-1 text-xs font-bold uppercase text-success-text select-none">
							Active
						</span>
					)}
				</div>
			</div>

			<div className="flex flex-col gap-1.5">
				<Label className="text-xs font-medium text-fg-secondary">
					Display name
				</Label>
				<Input
					value={displayName}
					onChange={(e) => setDisplayName(e.target.value)}
					placeholder={profile.name}
				/>
			</div>

			<div className="grid grid-cols-1 gap-4 md:grid-cols-2">
				<div className="flex flex-col gap-1.5">
					<Label className="text-xs font-medium text-fg-secondary">
						Provider
					</Label>
					<Input value={profile.provider} disabled />
				</div>
				<div className="flex flex-col gap-1.5">
					<Label className="text-xs font-medium text-fg-secondary">Model</Label>
					<Input
						value={model}
						onChange={(e) => setModel(e.target.value)}
						placeholder="e.g. gpt-4o-mini"
					/>
				</div>
			</div>
			<div className="flex flex-col gap-1.5">
				<Label className="text-xs font-medium text-fg-secondary">API key</Label>
				<Input
					type="password"
					value={apiKey}
					onChange={(e) => setApiKey(e.target.value)}
					placeholder="sk-..."
				/>
			</div>
			<div className="mt-2 flex justify-end gap-2">
				<Button
					type="button"
					variant="secondary"
					size="sm"
					onClick={handleReset}
					disabled={!hasChanges || isSaving}
				>
					<RefreshIcon className="mr-1 h-3 w-3" />
					Reset
				</Button>
				<Button
					type="button"
					size="sm"
					onClick={handleSave}
					disabled={!hasChanges || isSaving}
				>
					Save
				</Button>
			</div>
		</div>
	);
}

/// Renders the profiles panel with a searchable list and an editor pane.
///
/// Refs: I-Shell-Runtime-OnlyIO
export default function ProfilesPanel({ onClose }: ProfilesPanelProps) {
	const [profiles, setProfiles] = useState<Profile[]>([]);
	const [activeName, setActiveName] = useState<string | null>(null);
	const [selectedName, setSelectedName] = useState<string | null>(null);
	const [searchQuery, setSearchQuery] = useState("");
	const [isLoading, setIsLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [showCreate, setShowCreate] = useState(false);

	const loadProfiles = useCallback(async () => {
		setIsLoading(true);
		setError(null);
		try {
			const [all, active] = await Promise.all([listProfiles(), getProfile()]);
			setProfiles(all);
			setActiveName(active?.name ?? null);
			setSelectedName((prev) =>
				prev && all.some((p) => p.name === prev) ? prev : active?.name ?? null,
			);
		} catch (err) {
			setError(String(err));
		} finally {
			setIsLoading(false);
		}
	}, []);

	useEffect(() => {
		void loadProfiles();
	}, [loadProfiles]);

	const filteredProfiles = useMemo(() => {
		const query = searchQuery.trim().toLowerCase();
		if (!query) return profiles;
		return profiles.filter(
			(p) =>
				p.name.toLowerCase().includes(query) ||
				p.display_name.toLowerCase().includes(query) ||
				p.provider.toLowerCase().includes(query) ||
				p.model.toLowerCase().includes(query),
		);
	}, [profiles, searchQuery]);

	const selectedProfile = useMemo(
		() => profiles.find((p) => p.name === selectedName) ?? null,
		[profiles, selectedName],
	);

	const handleSwitch = useCallback(
		async (name: string) => {
			setError(null);
			try {
				await switchProfile(name);
				await loadProfiles();
			} catch (err) {
				setError(String(err));
			}
		},
		[loadProfiles],
	);

	const handleDelete = useCallback(
		async (name: string) => {
			if (name === activeName) {
				setError("Cannot delete the active profile.");
				return;
			}
			if (!confirm(`Delete profile "${name}"?`)) return;
			setError(null);
			try {
				await deleteProfile(name);
				await loadProfiles();
			} catch (err) {
				setError(String(err));
			}
		},
		[activeName, loadProfiles],
	);

	const handleProfileCreated = useCallback(() => {
		void loadProfiles();
		setShowCreate(false);
	}, [loadProfiles]);

	const isTauriAvailable = isTauri();

	return (
		<PanelOverlay
			title="Profiles"
			icon={<UserIcon className="h-4 w-4" />}
			onClose={onClose}
			size="lg"
			padded={false}
			headerActions={
				<button
					type="button"
					className="mr-1.5 flex cursor-pointer items-center justify-center rounded-md bg-transparent p-1.5 text-fg-muted transition-all duration-150 hover:bg-bg-highlight hover:text-fg-secondary focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent-glow"
					onClick={() => setShowCreate(true)}
					title="New profile"
					aria-label="New profile"
				>
					<PlusIcon className="h-4 w-4" />
				</button>
			}
		>
			<div className="flex flex-1 min-h-0 flex-row overflow-hidden">
				<div className="flex flex-col w-70 min-w-70 border-r border-border bg-bg-base/20">
					<SearchBar
						placeholder="Search profiles..."
						value={searchQuery}
						onChange={setSearchQuery}
						containerClassName="shrink-0 px-5 py-4 border-b border-border rounded-none bg-bg-base/30"
					/>
					{error && (
						<div className="shrink-0 mx-4 my-2 rounded-lg border border-error-border bg-error-bg px-3.5 py-2.5 text-xs text-error-text">
							{error}
						</div>
					)}
					{!isTauriAvailable && !error && (
						<div className="shrink-0 mx-4 my-2 rounded-lg border border-error-border bg-error-bg px-3.5 py-2.5 text-xs text-error-text">
							Profiles preview mode: profile management requires the Tauri
							desktop app.
						</div>
					)}
					<div className="flex flex-1 min-h-0 flex-col gap-3 overflow-y-auto p-4">
						{isLoading ? (
							<div className="py-12 text-center text-sm text-fg-muted">
								Loading profiles...
							</div>
						) : filteredProfiles.length === 0 ? (
							<div className="py-12 text-center text-sm text-fg-muted">
								No profiles found
							</div>
						) : (
							filteredProfiles.map((profile) => (
								<ProfileListItem
									key={profile.name}
									profile={profile}
									isActive={profile.name === activeName}
									isSelected={profile.name === selectedName}
									onSelect={setSelectedName}
								/>
							))
						)}
					</div>
				</div>

				<div className="flex flex-1 min-w-0 flex-col overflow-hidden">
					{showCreate ? (
						<CreateProfileForm
							onCancel={() => setShowCreate(false)}
							onCreated={handleProfileCreated}
							setError={setError}
						/>
					) : selectedProfile ? (
						<ProfileDetails
							profile={selectedProfile}
							isActive={selectedProfile.name === activeName}
							onSwitch={handleSwitch}
							onDelete={handleDelete}
							onUpdate={loadProfiles}
							setError={setError}
						/>
					) : (
						<div className="flex flex-1 flex-col items-center justify-center p-5 text-center text-sm text-fg-muted">
							Select a profile to view details
						</div>
					)}
				</div>
			</div>
		</PanelOverlay>
	);
}
