import { useEffect, useState } from "react";
import type { Profile } from "../../ipc";
import { updateProfile } from "../../ipc";
import { Button, Input, Label } from "../ui";
import { CheckIcon, TrashIcon, RefreshIcon } from "../Icons";

interface ProfileDetailsProps {
	profile: Profile;
	isActive: boolean;
	onSwitch: (name: string) => void;
	onDelete: (name: string) => void;
	onUpdate: () => void;
	setError: (message: string | null) => void;
}

/// Renders details for the selected profile and allows editing, switching, or deleting it.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function ProfileDetails({
	profile,
	isActive,
	onSwitch,
	onDelete,
	onUpdate,
	setError,
}: ProfileDetailsProps) {
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
