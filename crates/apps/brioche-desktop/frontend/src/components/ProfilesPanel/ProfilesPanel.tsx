import { useProfilesPanel } from "../../hooks/profiles/useProfilesPanel";
import PanelOverlay, { SearchBar } from "../PanelOverlay";
import { User, Plus } from "lucide-react";
import { ProfileListItem } from "./ProfileListItem";
import { EmptyState } from "../ui";
import { CreateProfileForm } from "./CreateProfileForm";
import { ProfileDetails } from "./ProfileDetails";

/// Props for the profile management panel.
///
/// Refs: I-Shell-Runtime-OnlyIO
interface ProfilesPanelProps {
	onClose: () => void;
}

/// Renders the profiles panel with a searchable list and an editor pane.
///
/// Refs: I-Shell-Runtime-OnlyIO
export default function ProfilesPanel({ onClose }: ProfilesPanelProps) {
	const {
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
	} = useProfilesPanel();

	return (
		<PanelOverlay
			title="Profiles"
			icon={<User className="h-4 w-4" />}
			onClose={onClose}
			size="lg"
			padded={false}
			headerActions={
				<button
					type="button"
					className="mr-2 flex cursor-pointer items-center justify-center rounded-md bg-transparent p-2 text-fg-muted transition-all duration-150 hover:bg-bg-highlight hover:text-fg-secondary focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent-glow"
					onClick={() => setShowCreate(true)}
					title="New profile"
					aria-label="New profile"
				>
					<Plus className="h-4 w-4" />
				</button>
			}
		>
			<div className="flex flex-1 min-h-0 flex-row overflow-hidden">
				<div className="flex flex-col w-70 min-w-70 border-r border-border bg-bg-base">
					<SearchBar
						placeholder="Search profiles..."
						value={searchQuery}
						onChange={setSearchQuery}
						containerClassName="shrink-0 px-4 py-4 border-b border-border rounded-none bg-bg-base"
					/>
					{error && (
						<div className="shrink-0 mx-4 my-2 rounded-sm border border-error-border bg-error-bg px-4 py-3 text-xs text-error-text">
							{error}
						</div>
					)}
					{!isTauriAvailable && !error && (
						<div className="shrink-0 mx-4 my-2 rounded-sm border border-error-border bg-error-bg px-4 py-3 text-xs text-error-text">
							Profiles preview mode: profile management requires the Tauri
							desktop app.
						</div>
					)}
					<div className="flex flex-1 min-h-0 flex-col gap-3 overflow-y-auto p-4">
						{isLoading ? (
							<EmptyState title="Loading profiles..." description="Fetching profile definitions." />
						) : filteredProfiles.length === 0 ? (
							<EmptyState icon={User} title="No profiles found" description="Create a profile to configure access." />
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
						<EmptyState icon={User} title="Select a profile" description="Choose a profile from the sidebar to view details." />
					)}
				</div>
			</div>
		</PanelOverlay>
	);
}
