import { useState, useEffect, useMemo } from "react";
import { useSkillsStore } from "../stores/panelStores";
import type { Skill } from "../ipc";
import PanelOverlay, { SearchBar, CategoryFilter } from "./PanelOverlay";
import {
	BookIcon,
	TagIcon,
	FolderIcon,
	PlusIcon,
	TrashIcon,
} from "./Icons";

interface SkillsPanelProps {
	onClose: () => void;
}

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

	// Local temporary state for the "New Skill" creation form
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
				skill.tags.some((t) => t.toLowerCase().includes(searchQuery.toLowerCase()));
			const matchesCategory =
				!categoryFilter || skill.category === categoryFilter;
			return matchesSearch && matchesCategory;
		});
	}, [skills, searchQuery, categoryFilter]);

	const canCreate = Boolean(newName.trim() && newCategory.trim());

	return (
		<PanelOverlay
			title="Skills"
			icon={<BookIcon className="w-4 h-4" />}
			onClose={onClose}
			panelClassName="bg-bg-1 border border-border rounded-lg w-[850px] max-w-[95vw] max-h-[85vh] flex flex-col overflow-hidden animate-slideUp shadow-2xl z-[1001]"
			headerActions={
				<button
					type="button"
					className="p-1.5 bg-transparent text-text-muted hover:text-text-secondary hover:bg-bg-3 rounded-md transition-all duration-150 cursor-pointer flex items-center justify-center mr-1.5"
					onClick={() => setShowCreate(true)}
					title="New skill"
					aria-label="New skill"
				>
					<PlusIcon className="w-4 h-4" />
				</button>
			}
		>
			<div className="flex flex-row flex-1 overflow-hidden min-h-0">
				<div className="w-[280px] min-w-[280px] border-r border-border flex flex-col bg-bg-0/20">
					<SearchBar
						placeholder="Search skills..."
						value={searchQuery}
						onChange={setSearchQuery}
						containerClassName="shrink-0 border-b border-border rounded-none px-4 py-3 bg-bg-0/30"
					/>

					<CategoryFilter
						categories={categories}
						activeCategory={categoryFilter}
						onSelect={setCategoryFilter}
						containerClassName="shrink-0 px-4 py-3 border-b border-border bg-bg-0/20"
						buttonClassName="category-btn"
					/>

					{error && <div className="bg-error-bg text-[#e8a0a0] border border-error-border px-3.5 py-2.5 rounded-lg text-xs mx-4 my-3">{error}</div>}
					{!isTauriAvailable && !error && (
						<div className="bg-error-bg text-[#e8a0a0] border border-error-border px-3.5 py-2.5 rounded-lg text-xs mx-4 my-3">
							Skills preview mode: scanning requires the Tauri desktop app.
						</div>
					)}
					<div className="flex-1 overflow-y-auto min-h-0 p-3 flex flex-col gap-2">
						{isLoading ? (
							<div className="text-center text-text-muted py-12 text-sm">Loading skills...</div>
						) : filteredSkills.length === 0 ? (
							<div className="text-center text-text-muted py-12 text-sm">No skills found</div>
						) : (
							filteredSkills.map((skill) => (
								<div
									key={skill.name}
									className={`p-3 rounded-lg cursor-pointer transition-all duration-200 border flex flex-col gap-1.5 ${
										selectedSkill?.name === skill.name
											? "bg-accent/10 border-accent-dim/40 shadow-sm"
											: "bg-transparent border-transparent hover:bg-bg-2/30 hover:border-border/60"
									}`}
								>
									<div
										className="flex-1 flex flex-col gap-1"
										onClick={() => selectSkill(skill)}
									>
										<div className="text-xs font-semibold text-text-primary flex items-center justify-between gap-1">
											{skill.name}
											<span
												className={`px-1.5 py-0.5 rounded text-[8px] font-bold uppercase select-none ${
													skill.enabled
														? "bg-green-800/20 border border-green-700/30 text-green-400"
														: "bg-bg-4 border border-border text-text-muted"
												}`}
											>
												{skill.enabled ? "on" : "off"}
											</span>
										</div>
										<div className="text-[11px] text-text-secondary line-clamp-2">{skill.description}</div>
									</div>
									<div className="flex items-center gap-2 mt-1 select-none text-[10px] text-text-muted">
										<span className="px-1.5 py-0.5 rounded text-[9px] font-medium bg-bg-4 border border-border text-text-tertiary font-mono">{skill.category}</span>
										{skill.version && (
											<span className="px-1.5 py-0.5 rounded text-[9px] font-medium bg-bg-4 border border-border text-text-tertiary font-mono">v{skill.version}</span>
										)}
										<button
											type="button"
											className="px-2.5 py-1 bg-bg-3 border border-border hover:border-accent-dim/45 hover:bg-bg-4 text-text-secondary hover:text-text-primary rounded text-[10px] font-medium cursor-pointer transition-all ml-auto"
											onClick={() => toggleSkillEnabled(skill)}
											title={skill.enabled ? "Disable" : "Enable"}
										>
											{skill.enabled ? "Disable" : "Enable"}
										</button>
										<button
											type="button"
											className="p-1.5 text-text-muted hover:text-red-400 hover:bg-bg-3 border border-transparent hover:border-border rounded transition-all cursor-pointer flex items-center justify-center shrink-0"
											onClick={() => handleDelete(skill)}
											title="Delete"
											aria-label={`Delete skill ${skill.name}`}
										>
											<TrashIcon className="w-3.5 h-3.5" />
										</button>
									</div>
								</div>
							))
						)}
					</div>
				</div>

				<div className="flex-1 overflow-y-auto p-5 flex flex-col gap-4 bg-bg-0/10">
					{showCreate ? (
						<div className="flex flex-col gap-3 p-2 max-w-xl [&_h3]:text-base [&_h3]:font-semibold [&_h3]:text-text-primary [&_h3]:border-b [&_h3]:border-border [&_h3]:pb-2 [&_h3]:mb-1 [&_input]:bg-bg-2 [&_input]:border [&_input]:border-border [&_input]:text-text-primary [&_input]:text-xs [&_input]:px-2.5 [&_input]:py-1.5 [&_input]:rounded [&_input]:outline-none [&_input]:focus:border-accent-dim/60 [&_textarea]:bg-bg-2 [&_textarea]:border [&_textarea]:border-border [&_textarea]:text-text-primary [&_textarea]:text-xs [&_textarea]:px-2.5 [&_textarea]:py-1.5 [&_textarea]:rounded [&_textarea]:outline-none [&_textarea]:focus:border-accent-dim/60 [&_textarea]:font-mono">
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
							<div className="flex justify-end gap-2 [&_button]:px-3.5 [&_button]:py-1.5 [&_button]:text-xs [&_button]:font-medium [&_button]:rounded [&_button]:cursor-pointer [&_button:first-child]:bg-accent [&_button:first-child]:hover:bg-accent-hover [&_button:first-child]:text-white [&_button:first-child]:disabled:opacity-50 [&_button:first-child]:disabled:cursor-not-allowed [&_button:last-child]:bg-transparent [&_button:last-child]:border [&_button:last-child]:border-border [&_button:last-child]:text-text-secondary [&_button:last-child]:hover:bg-bg-2">
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
							<div className="border-b border-border pb-4 mb-2 flex flex-col gap-2">
								<h3 className="text-lg font-semibold text-text-primary">{selectedSkill.name}</h3>
								<div className="flex flex-wrap items-center gap-3 select-none text-[11px] text-text-muted">
									<span className="flex items-center gap-1 font-medium">
										<FolderIcon className="w-3.5 h-3.5" />
										{selectedSkill.category}
									</span>
									{selectedSkill.version && (
										<span className="flex items-center gap-1 font-medium border-l border-border pl-3">
											v{selectedSkill.version}
										</span>
									)}
									{selectedSkill.author && (
										<span className="flex items-center gap-1 font-medium border-l border-border pl-3">
											by {selectedSkill.author}
										</span>
									)}
									{selectedSkill.license && (
										<span className="flex items-center gap-1 font-medium border-l border-border pl-3">
											{selectedSkill.license}
										</span>
									)}
								</div>
								{selectedSkill.tags.length > 0 && (
									<div className="flex flex-wrap gap-1.5 mt-1">
										{selectedSkill.tags.map((tag) => (
											<span key={tag} className="inline-flex items-center gap-1 px-2 py-0.5 bg-accent/5 border border-accent/15 rounded text-[10px] text-accent-hover font-medium font-mono">
												<TagIcon className="w-3 h-3" />
												{tag}
											</span>
										))}
									</div>
								)}
							</div>
							<div className="flex-1 bg-bg-0 border border-border rounded-lg p-4 overflow-auto">
								<pre className="font-mono text-xs text-text-secondary whitespace-pre-wrap leading-relaxed">{skillContent}</pre>
							</div>
						</>
					) : (
						<div className="flex flex-col items-center justify-center gap-3 py-24 text-center select-none text-text-muted">
							<BookIcon className="w-12 h-12 text-text-dim stroke-[1.2]" />
							<p className="text-sm">Select a skill to view its documentation</p>
						</div>
					)}
				</div>
			</div>
		</PanelOverlay>
	);
}
