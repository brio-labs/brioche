import { useEffect, useMemo } from "react";
import { useSkillsStore } from "../../stores/panelStores";
import type { Skill } from "../../ipc";
import PanelOverlay, { SearchBar, CategoryFilter } from "../PanelOverlay";
import { BookIcon, PlusIcon } from "../Icons";
import { useSkillCreate } from "../../hooks/skills";
import SkillListItem from "./SkillListItem";
import SkillCreateForm from "./SkillCreateForm";
import SkillDetails from "./SkillDetails";

/// Props for the skills management panel.
interface SkillsPanelProps {
	onClose: () => void;
}

/// Renders the skills management panel with search, category filtering, and creation.
///
/// Refs: I-Ui-SkillsPanel
export default function SkillsPanel({ onClose }: SkillsPanelProps) {
	const {
		skills,
		selectedSkill,
		skillContent,
		searchQuery,
		categoryFilter,
		isLoading,
		error,
		showCreate,
		isTauriAvailable,
		loadSkills,
		selectSkill,
		toggleSkillEnabled,
		deleteExistingSkill,
		setSearchQuery,
		setCategoryFilter,
		setShowCreate,
		setError,
	} = useSkillsStore();

	const {
		newName,
		setNewName,
		newCategory,
		setNewCategory,
		newDescription,
		setNewDescription,
		newContent,
		setNewContent,
		canCreate,
		handleCreate,
		handleCancelCreate,
	} = useSkillCreate();

	useEffect(() => {
		loadSkills();
	}, [loadSkills]);

	const handleDelete = async (skill: Skill) => {
		if (!isTauriAvailable) {
			setError("Deleting skills requires the Tauri desktop runtime.");
			return;
		}
		if (!confirm(`Delete skill "${skill.name}"?`)) return;
		await deleteExistingSkill(skill);
	};

	const categories = useMemo(() => {
		return Array.from(new Set(skills.map((s) => s.category))).sort();
	}, [skills]);

	const filteredSkills = useMemo(() => {
		return skills.filter((skill) => {
			const matchesSearch =
				!searchQuery.trim() ||
				skill.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
				skill.description.toLowerCase().includes(searchQuery.toLowerCase()) ||
				skill.tags.some((t) =>
					t.toLowerCase().includes(searchQuery.toLowerCase()),
				);
			const matchesCategory =
				!categoryFilter || skill.category === categoryFilter;
			return matchesSearch && matchesCategory;
		});
	}, [skills, searchQuery, categoryFilter]);

	return (
		<PanelOverlay
			title="Skills"
			icon={<BookIcon className="h-4 w-4" />}
			onClose={onClose}
			size="lg"
			padded={false}
			headerActions={
				<button
					type="button"
					className="mr-1.5 flex cursor-pointer items-center justify-center rounded-md bg-transparent p-1.5 text-fg-muted transition-all duration-150 hover:bg-bg-highlight hover:text-fg-secondary"
					onClick={() => setShowCreate(true)}
					title="New skill"
					aria-label="New skill"
				>
					<PlusIcon className="h-4 w-4" />
				</button>
			}
		>
			<div className="flex min-h-0 flex-1 flex-row overflow-hidden">
				<div className="flex w-70 min-w-70 flex-col border-r border-border bg-bg-base/20">
					<SearchBar
						placeholder="Search skills..."
						value={searchQuery}
						onChange={setSearchQuery}
						containerClassName="shrink-0 border-b border-border rounded-none bg-bg-base/30 px-5 py-4"
					/>

					<CategoryFilter
						categories={categories}
						activeCategory={categoryFilter}
						onSelect={setCategoryFilter}
						containerClassName="shrink-0 border-b border-border bg-bg-base/20 px-5 py-4"
					/>

					{error && (
						<div className="notice-error mx-4 my-2 shrink-0">
							{error}
						</div>
					)}
					{!isTauriAvailable && !error && (
						<div className="notice-error mx-4 my-2 shrink-0">
							Skills preview mode: scanning requires the Tauri desktop
							app.
						</div>
					)}
					<div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4">
						{isLoading ? (
							<div className="empty-state">Loading skills...</div>
						) : filteredSkills.length === 0 ? (
							<div className="empty-state">No skills found</div>
						) : (
							filteredSkills.map((skill) => (
								<SkillListItem
									key={skill.name}
									skill={skill}
									isSelected={selectedSkill?.name === skill.name}
									onSelect={selectSkill}
									onToggleEnabled={toggleSkillEnabled}
									onDelete={handleDelete}
								/>
							))
						)}
					</div>
				</div>

				<div className="flex flex-1 flex-col gap-4 overflow-y-auto bg-bg-base/10 p-5">
					{showCreate ? (
						<SkillCreateForm
							newName={newName}
							setNewName={setNewName}
							newCategory={newCategory}
							setNewCategory={setNewCategory}
							newDescription={newDescription}
							setNewDescription={setNewDescription}
							newContent={newContent}
							setNewContent={setNewContent}
							canCreate={canCreate}
							handleCreate={handleCreate}
							handleCancelCreate={handleCancelCreate}
						/>
					) : selectedSkill ? (
						<SkillDetails
							selectedSkill={selectedSkill}
							skillContent={skillContent}
						/>
					) : (
						<div className="flex flex-col items-center justify-center gap-3 py-24 text-center text-fg-muted select-none">
							<BookIcon className="h-12 w-12 stroke-[1.2] text-fg-dim" />
							<p className="text-sm">
								Select a skill to view its documentation
							</p>
						</div>
					)}
				</div>
			</div>
		</PanelOverlay>
	);
}
