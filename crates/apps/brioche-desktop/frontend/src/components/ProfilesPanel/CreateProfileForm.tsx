import { useState } from "react";
import { createProfile } from "../../ipc";
import { Button, Input, Label, Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui";

interface CreateProfileFormProps {
	onCancel: () => void;
	onCreated: () => void;
	setError: (message: string | null) => void;
}

/// Available LLM providers when creating a new profile.
const PROVIDERS = [
	{ value: "openai", label: "OpenAI" },
	{ value: "openrouter", label: "OpenRouter" },
];

/// Form for creating a new profile.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function CreateProfileForm({
	onCancel,
	onCreated,
	setError,
}: CreateProfileFormProps) {
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
