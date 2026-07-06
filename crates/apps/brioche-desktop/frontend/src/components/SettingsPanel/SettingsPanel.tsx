import { FieldEditor } from "./FieldEditor";
import PanelOverlay from "../PanelOverlay";
import { SettingsSectionList } from "./SettingsSectionList";
import { getFieldValue } from "./settingsUtils";
import { useSettingsPanel } from "../../hooks/settings/useSettingsPanel";
import { Button } from "../ui";

/// Props for the settings management panel.
///
/// Refs: I-Shell-Runtime-OnlyIO
interface SettingsPanelProps {
	onClose: () => void;
}

/// Renders the settings panel with a searchable section list and field editor.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function SettingsPanel({ onClose }: SettingsPanelProps) {
	const {
		search,
		setSearch,
		selectedSectionId,
		setSelectedSectionId,
		saveError,
		editingProtected,
		setEditingProtected,
		handleSave,
		handleFieldChange,
		handleReset,
		filteredSections,
		selectedSection,
		isTauriAvailable,
		settings,
	} = useSettingsPanel(onClose);

	return (
		<PanelOverlay
			title="Settings"
			onClose={onClose}
			size="md"
			padded={false}
		>
			<div className="flex flex-1 min-h-0 flex-row overflow-hidden">
				<SettingsSectionList
					sections={filteredSections}
					selectedSectionId={selectedSectionId}
					onSelectSection={setSelectedSectionId}
					search={search}
					onSearchChange={setSearch}
				/>

				<div className="flex flex-1 flex-col gap-4 overflow-y-auto p-6">
					{!isTauriAvailable && (
						<div className="rounded-sm border border-error-border bg-error-bg p-4 text-xs text-error-text">
							Settings preview mode: saving requires the Tauri desktop app.
						</div>
					)}
					{selectedSection ? (
						<>
							<div className="mb-2 border-b border-border pb-3">
								<h3 className="text-base font-semibold text-fg-primary">
									{selectedSection.title}
								</h3>
							</div>
							<div className="flex flex-col gap-6">
								{selectedSection.fields.map((field) => (
									<FieldEditor
										key={field.key}
										field={field}
										value={getFieldValue(settings, field.key)}
										editingProtected={editingProtected}
										setEditingProtected={setEditingProtected}
										onChange={(value) => handleFieldChange(field.key, value)}
										onReset={() => handleReset(field)}
									/>
								))}
							</div>
						</>
					) : (
						<div className="flex flex-1 flex-col items-center justify-center py-16 text-sm text-fg-muted">
							Select a section from the left to view its settings.
						</div>
					)}
					{saveError && (
						<div className="mt-auto pt-5">
							<div className="whitespace-pre-wrap rounded-sm border border-error-border bg-error-bg p-4 text-sm text-error-text">
								{saveError}
							</div>
						</div>
					)}
				</div>
			</div>

			<div className="flex shrink-0 justify-end gap-3 border-t border-border bg-bg-base px-6 py-4">
				<Button type="button" variant="secondary" onClick={onClose}>
					Cancel
				</Button>
				<Button type="button" onClick={handleSave} disabled={!isTauriAvailable}>
					Save
				</Button>
			</div>
		</PanelOverlay>
	);
}

export default SettingsPanel;
