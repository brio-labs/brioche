import { useState, useEffect, useMemo } from "react";
import { useSkillsStore } from "../stores/panelStores";
import type { Skill } from "../ipc";
import PanelOverlay, { SearchBar, CategoryFilter } from "./PanelOverlay";
import { cn } from "./ui/lib";
import {
	BookIcon,
	TagIcon,
	FolderIcon,
	PlusIcon,
	TrashIcon,
} from "./Icons";

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
		createNewSkill,
		deleteExistingSkill,
		setSearchQuery,
		setCategoryFilter,
		setShowCreate,
		setError,
	} = useSkillsStore();

	const [newName, setNewName] = useState("");
	const [newCategory, setNewCategory] = useState("general");
	const [newDescription, setNewDescription] = useState("");
	const [newContent, setNewContent] = useState("");

	useEffect(() => {
		loadSkills();
	}, [loadSkills]);

	const handleCreate = async () => {
		if (!newName.trim()) return;
		const success = await createNewSkill(
			newName.trim(),
			newCategory.trim() || "general",
			newDescription.trim(),
			newContent.trim(),
		);
		if (success) {
			setNewName("");
			setNewCategory("general");
			setNewDescription("");
			setNewContent("");
			setShowCreate(false);
		}
	};

	const handleDelete = async (skill: Skill) => {
		if (!isTauriAvailable) {
			setError("Deleting skills requires the Tauri desktop runtime.");
			return;
		}
		if (!confirm(`Delete skill "${skill.name}"?`)) return;
		await deleteExistingSkill(skill);
	};

	const handleCancelCreate = () => {
		setNewName("");
		setNewCategory("general");
		setNewDescription("");
		setNewContent("");
		setShowCreate(false);
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

	const canCreate = Boolean(newName.trim() && newCategory.trim());

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
								<div
									key={skill.name}
									className={cn(
										"flex cursor-pointer flex-col gap-1.5 rounded-lg border p-3 transition-all duration-200",
										selectedSkill?.name === skill.name
											? "border-accent-dim/40 bg-accent/10 shadow-sm"
											: "border-transparent bg-transparent hover:border-border/60 hover:bg-bg-elevated/30",
									)}
								>
									<div
										className="flex flex-1 flex-col gap-1"
										onClick={() => selectSkill(skill)}
									>
										<div className="flex items-center justify-between gap-1 text-xs font-semibold text-fg-primary">
											{skill.name}
											<span
												className={cn(
													"rounded px-1.5 py-0.5 text-xs font-bold uppercase select-none",
													skill.enabled
														? "bg-success-bg border border-success-border text-success-text"
														: "bg-bg-subtle border border-border text-fg-muted",
												)}
											>
												{skill.enabled ? "on" : "off"}
											</span>
										</div>
										<div className="line-clamp-2 text-xs text-fg-secondary">
											{skill.description}
										</div>
									</div>
									<div className="mt-1 flex select-none items-center gap-2 text-xs text-fg-muted">
										<span className="rounded border border-border bg-bg-subtle px-1.5 py-0.5 font-mono text-xs font-medium text-fg-tertiary">
											{skill.category}
										</span>
										{skill.version && (
											<span className="rounded border border-border bg-bg-subtle px-1.5 py-0.5 font-mono text-xs font-medium text-fg-tertiary">
												v{skill.version}
											</span>
										)}
										<button
											type="button"
											className="ml-auto cursor-pointer rounded border border-border bg-bg-highlight px-2.5 py-1 text-xs font-medium text-fg-secondary transition-all hover:border-accent-dim/45 hover:bg-bg-subtle hover:text-fg-primary"
											onClick={() => toggleSkillEnabled(skill)}
											title={skill.enabled ? "Disable" : "Enable"}
										>
											{skill.enabled ? "Disable" : "Enable"}
										</button>
										<button
											type="button"
											className="flex shrink-0 cursor-pointer items-center justify-center rounded border border-transparent p-1.5 text-fg-muted transition-all hover:border-border hover:bg-bg-highlight hover:text-error-text"
											onClick={() => handleDelete(skill)}
											title="Delete"
											aria-label={`Delete skill ${skill.name}`}
										>
											<TrashIcon className="h-3.5 w-3.5" />
										</button>
									</div>
								</div>
							))
						)}
					</div>
				</div>

				<div className="flex flex-1 flex-col gap-4 overflow-y-auto bg-bg-base/10 p-5">
					{showCreate ? (
						<div className="flex max-w-xl flex-col gap-3 p-2 [&_h3]:mb-1 [&_h3]:border-b [&_h3]:border-border [&_h3]:pb-2 [&_h3]:text-base [&_h3]:font-semibold [&_h3]:text-fg-primary [&_input]:rounded [&_input]:border [&_input]:border-border [&_input]:bg-bg-elevated [&_input]:px-2.5 [&_input]:py-1.5 [&_input]:text-xs [&_input]:text-fg-primary [&_input]:outline-none [&_input]:focus:border-accent-dim/60 [&_textarea]:rounded [&_textarea]:border [&_textarea]:border-border [&_textarea]:bg-bg-elevated [&_textarea]:px-2.5 [&_textarea]:py-1.5 [&_textarea]:font-mono [&_textarea]:text-xs [&_textarea]:text-fg-primary [&_textarea]:outline-none [&_textarea]:focus:border-accent-dim/60">
							<h3>New skill</h3>
							<input
								type="text"
								placeholder="Skill name"
								value={newName}
								onChange={(e) => setNewName(e.target.value)}
							/>
							<input
								type="text"
								placeholder="Category"
								value={newCategory}
								onChange={(e) => setNewCategory(e.target.value)}
							/>
							<input
								type="text"
								placeholder="Short description"
								value={newDescription}
								onChange={(e) => setNewDescription(e.target.value)}
							/>
							<textarea
								placeholder="Markdown content"
								rows={12}
								value={newContent}
								onChange={(e) => setNewContent(e.target.value)}
							/>
							<div className="flex justify-end gap-2 [&_button]:cursor-pointer [&_button]:rounded [&_button]:px-3.5 [&_button]:py-1.5 [&_button]:text-xs [&_button]:font-medium [&_button:first-child]:bg-accent [&_button:first-child]:text-white [&_button:first-child]:hover:bg-accent-hover [&_button:first-child]:disabled:cursor-not-allowed [&_button:first-child]:disabled:opacity-50 [&_button:last-child]:border [&_button:last-child]:border-border [&_button:last-child]:bg-transparent [&_button:last-child]:text-fg-secondary [&_button:last-child]:hover:bg-bg-elevated">
								<button
									type="button"
									onClick={handleCreate}
									disabled={!canCreate}
								>
									Create
								</button>
								<button
									type="button"
									onClick={handleCancelCreate}
								>
									Cancel
								</button>
							</div>
						</div>
					) : selectedSkill ? (
						<>
							<div className="mb-2 flex flex-col gap-2 border-b border-border pb-4">
								<h3 className="text-lg font-semibold text-fg-primary">
									{selectedSkill.name}
								</h3>
								<div className="flex flex-wrap select-none items-center gap-3 text-xs text-fg-muted">
									<span className="flex items-center gap-1 font-medium">
										<FolderIcon className="h-3.5 w-3.5" />
										{selectedSkill.category}
									</span>
									{selectedSkill.version && (
										<span className="flex items-center gap-1 border-l border-border pl-3 font-medium">
											v{selectedSkill.version}
										</span>
									)}
									{selectedSkill.author && (
										<span className="flex items-center gap-1 border-l border-border pl-3 font-medium">
											by {selectedSkill.author}
										</span>
									)}
									{selectedSkill.license && (
										<span className="flex items-center gap-1 border-l border-border pl-3 font-medium">
											{selectedSkill.license}
										</span>
									)}
								</div>
								{selectedSkill.tags.length > 0 && (
									<div className="mt-1 flex flex-wrap gap-1.5">
										{selectedSkill.tags.map((tag) => (
											<span
												key={tag}
												className="inline-flex items-center gap-1 rounded border border-accent/15 bg-accent/5 px-2 py-0.5 font-mono text-xs font-medium text-accent-hover"
											>
												<TagIcon className="h-3 w-3" />
												{tag}
											</span>
										))}
									</div>
								)}
							</div>
							<div className="flex-1 overflow-auto rounded-lg border border-border bg-bg-base p-4">
								<pre className="font-mono text-xs leading-relaxed whitespace-pre-wrap text-fg-secondary">
									{skillContent}
								</pre>
							</div>
						</>
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
