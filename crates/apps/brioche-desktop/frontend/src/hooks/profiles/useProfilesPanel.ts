import { useCallback, useEffect, useMemo, useState } from "react";
import type { Profile } from "../../ipc";
import {
	listProfiles,
	getProfile,
	switchProfile,
	deleteProfile,
	isTauri,
} from "../../ipc";

/// Hook that manages profiles panel state and operations.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function useProfilesPanel() {
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

	return {
		profiles,
		activeName,
		selectedName,
		searchQuery,
		isLoading,
		error,
		showCreate,
		setSearchQuery,
		setSelectedName,
		setShowCreate,
		setError,
		loadProfiles,
		handleSwitch,
		handleDelete,
		handleProfileCreated,
		isTauriAvailable,
		selectedProfile,
		filteredProfiles,
	};
}
