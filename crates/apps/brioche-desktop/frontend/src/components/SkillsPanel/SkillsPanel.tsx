import { useEffect, useMemo } from "react";
import { useSkillsStore } from "../../stores/skillsStore";
import type { Skill } from "../../ipc";
import { BookOpen, Plus } from "lucide-react";
import PanelOverlay, { SearchBar, CategoryFilter } from "../PanelOverlay";
import { useSkillCreate } from "../../hooks/skills";
import { EmptyState } from "../ui";
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
			icon={<BookOpen className="h-4 w-4" />}
			onClose={onClose}
			size="lg"
			padded={false}
			headerActions={
				<button
					type="button"
					className="mr-2 flex cursor-pointer items-center justify-center rounded-md bg-transparent p-2 text-fg-muted transition-all duration-150 hover:bg-bg-highlight hover:text-fg-secondary"
					onClick={() => setShowCreate(true)}
					title="New skill"
					aria-label="New skill"
				>
					<Plus className="h-4 w-4" />
				</button>
			}
		>
			<div className="flex min-h-0 flex-1 flex-row overflow-hidden">
				<div className="flex w-70 min-w-70 flex-col border-r border-border bg-bg-base">
					<SearchBar
						placeholder="Search skills..."
						value={searchQuery}
						onChange={setSearchQuery}
						containerClassName="shrink-0 border-b border-border rounded-none bg-bg-base px-4 py-4"
					/>

					<CategoryFilter
						categories={categories}
						activeCategory={categoryFilter}
						onSelect={setCategoryFilter}
						containerClassName="shrink-0 border-b border-border bg-bg-base px-4 py-4"
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
							<EmptyState title="Loading skills..." description="Scanning local workspace definitions." />
						) : filteredSkills.length === 0 ? (
							<EmptyState icon={BookOpen} title="No skills found" description="Create a skill to extend functionality." />
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

				<div className="flex flex-1 flex-col gap-4 overflow-y-auto bg-bg-base p-5">
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
						<EmptyState icon={BookOpen} title="Select a skill" description="Choose a skill from the sidebar to view details." />
					)}
				</div>
			</div>
		</PanelOverlay>
	);
}
