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
} from "./ui";

interface ProfilesPanelProps {
	onClose: () => void;
}

const PROVIDERS = [
	{ value: "openai", label: "OpenAI" },
	{ value: "openrouter", label: "OpenRouter" },
];

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
			onClick={() => onSelect(profile.name)}
			className={`p-3 mx-1 rounded-lg cursor-pointer transition-all duration-200 border flex flex-col gap-1.5 ${
				isSelected
					? "bg-accent/10 border-accent-dim/40 shadow-sm"
					: "bg-transparent border-transparent hover:bg-bg-2/30 hover:border-border/60"
			}`}
		>
			<div className="flex items-center justify-between gap-2">
				<span className="text-xs font-semibold text-text-primary truncate">
					{profile.display_name || profile.name}
				</span>
				{isActive && (
					<span className="px-1.5 py-0.5 rounded text-[8px] font-bold uppercase select-none bg-green-800/20 border border-green-700/30 text-green-400 shrink-0">
						Active
					</span>
				)}
			</div>
			<div className="text-[11px] text-text-secondary truncate">
				{profile.provider} / {profile.model}
			</div>
		</div>
	);
}

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
		<div className="flex-1 overflow-y-auto p-5 flex flex-col gap-4">
			<h3 className="text-sm font-semibold text-text-primary">Create profile</h3>
			<div className="grid grid-cols-1 md:grid-cols-2 gap-4">
				<div className="flex flex-col gap-1.5">
					<Label className="text-[11px] font-medium text-text-secondary">Name</Label>
					<Input
						value={name}
						onChange={(e) => setName(e.target.value)}
						placeholder="e.g. work"
					/>
				</div>
				<div className="flex flex-col gap-1.5">
					<Label className="text-[11px] font-medium text-text-secondary">
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
				<Label className="text-[11px] font-medium text-text-secondary">Provider</Label>
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
				<Label className="text-[11px] font-medium text-text-secondary">Model</Label>
				<Input
					value={model}
					onChange={(e) => setModel(e.target.value)}
					placeholder="e.g. gpt-4o-mini"
				/>
			</div>
			<div className="flex flex-col gap-1.5">
				<Label className="text-[11px] font-medium text-text-secondary">API key</Label>
				<Input
					type="password"
					value={apiKey}
					onChange={(e) => setApiKey(e.target.value)}
					placeholder="sk-..."
				/>
			</div>
			<div className="flex justify-end gap-2 mt-2">
				<Button variant="secondary" onClick={onCancel} disabled={isSaving}>
					Cancel
				</Button>
				<Button onClick={handleSubmit} disabled={isSaving}>
					Create
				</Button>
			</div>
		</div>
	);
}

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
		<div className="flex-1 overflow-y-auto p-5 flex flex-col gap-4">
			<div className="flex items-start justify-between gap-4">
				<div className="flex flex-col gap-0.5">
					<h3 className="text-sm font-semibold text-text-primary">
						{profile.display_name || profile.name}
					</h3>
					<div className="text-[11px] text-text-secondary">
						{profile.provider} / {profile.model}
					</div>
				</div>
				<div className="flex items-center gap-2 shrink-0">
					{!isActive && (
						<div className="flex items-center gap-2">
							<Button
								variant="secondary"
								size="sm"
								onClick={() => onSwitch(profile.name)}
							>
								<CheckIcon className="w-3 h-3 mr-1" />
								Switch
							</Button>
							<Button
								variant="destructive"
								size="sm"
								onClick={() => onDelete(profile.name)}
							>
								<TrashIcon className="w-3 h-3 mr-1" />
								Delete
							</Button>
						</div>
					)}
					{isActive && (
						<span className="px-2 py-1 rounded text-[10px] font-bold uppercase select-none bg-green-800/20 border border-green-700/30 text-green-400">
							Active
						</span>
					)}
				</div>
			</div>

			<div className="flex flex-col gap-1.5">
				<Label className="text-[11px] font-medium text-text-secondary">
					Display name
				</Label>
				<Input
					value={displayName}
					onChange={(e) => setDisplayName(e.target.value)}
					placeholder={profile.name}
				/>
			</div>
			<div className="grid grid-cols-1 md:grid-cols-2 gap-4">
				<div className="flex flex-col gap-1.5">
					<Label className="text-[11px] font-medium text-text-secondary">
						Provider
					</Label>
					<Input value={profile.provider} disabled />
				</div>
				<div className="flex flex-col gap-1.5">
					<Label className="text-[11px] font-medium text-text-secondary">Model</Label>
					<Input
						value={model}
						onChange={(e) => setModel(e.target.value)}
						placeholder="e.g. gpt-4o-mini"
					/>
				</div>
			</div>
			<div className="flex flex-col gap-1.5">
				<Label className="text-[11px] font-medium text-text-secondary">API key</Label>
				<Input
					type="password"
					value={apiKey}
					onChange={(e) => setApiKey(e.target.value)}
					placeholder="sk-..."
				/>
			</div>
			<div className="flex justify-end gap-2 mt-2">
				<Button
					variant="secondary"
					size="sm"
					onClick={handleReset}
					disabled={!hasChanges || isSaving}
				>
					<RefreshIcon className="w-3 h-3 mr-1" />
					Reset
				</Button>
				<Button size="sm" onClick={handleSave} disabled={!hasChanges || isSaving}>
					Save
				</Button>
			</div>
		</div>
	);
}

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

	const isTauriAvailable = isTauri();

	return (
		<PanelOverlay
			title="Profiles"
			icon={<UserIcon className="w-4 h-4" />}
			onClose={onClose}
			panelClassName="bg-bg-1 border border-border rounded-lg w-[850px] max-w-[95vw] max-h-[85vh] flex flex-col overflow-hidden animate-slideUp shadow-2xl z-[1001]"
			headerActions={
				<button
					type="button"
					className="p-1.5 bg-transparent text-text-muted hover:text-text-secondary hover:bg-bg-3 rounded-md transition-all duration-150 cursor-pointer flex items-center justify-center mr-1.5"
					onClick={() => setShowCreate(true)}
					title="New profile"
				>
					<PlusIcon className="w-4 h-4" />
				</button>
			}
		>
			<div className="flex flex-row flex-1 overflow-hidden min-h-0">
				<div className="w-[280px] min-w-[280px] border-r border-border flex flex-col bg-bg-0/20">
					<SearchBar
						placeholder="Search profiles..."
						value={searchQuery}
						onChange={setSearchQuery}
						containerClassName="shrink-0 border-b border-border rounded-none px-3 py-2 bg-bg-0/30"
					/>
					{error && (
						<div className="bg-error-bg text-[#e8a0a0] border border-error-border px-3.5 py-2.5 rounded-lg text-xs m-2">
							{error}
						</div>
					)}
					{!isTauriAvailable && !error && (
						<div className="bg-error-bg text-[#e8a0a0] border border-error-border px-3.5 py-2.5 rounded-lg text-xs m-2">
							Profiles preview mode: profile management requires the Tauri
							desktop app.
						</div>
					)}
					<div className="flex-1 overflow-y-auto p-2 flex flex-col gap-1.5">
						{isLoading ? (
							<div className="text-center text-text-muted py-12 text-sm">
								Loading profiles...
							</div>
						) : filteredProfiles.length === 0 ? (
							<div className="text-center text-text-muted py-12 text-sm">
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

				<div className="flex-1 flex flex-col min-w-0 overflow-hidden">
					{showCreate ? (
						<CreateProfileForm
							onCancel={() => setShowCreate(false)}
							onCreated={loadProfiles}
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
						<div className="flex-1 flex items-center justify-center text-text-muted text-sm">
							Select a profile to view details
						</div>
					)}
				</div>
			</div>
		</PanelOverlay>
	);
}
