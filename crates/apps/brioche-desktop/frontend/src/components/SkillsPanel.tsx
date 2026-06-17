import { useState, useEffect, useCallback } from "react";
import type { Skill } from "../ipc";
import {
	listSkills,
	getSkillContent,
	setSkillEnabled,
	createSkill,
	deleteSkill,
} from "../ipc";
import {
	XIcon,
	BookIcon,
	SearchIcon,
	TagIcon,
	FolderIcon,
	PlusIcon,
	TrashIcon,
} from "./Icons";

interface SkillsPanelProps {
	onClose: () => void;
}

export default function SkillsPanel({ onClose }: SkillsPanelProps) {
	const [skills, setSkills] = useState<Skill[]>([]);
	const [selectedSkill, setSelectedSkill] = useState<Skill | null>(null);
	const [skillContent, setSkillContent] = useState<string>("");
	const [search, setSearch] = useState("");
	const [loading, setLoading] = useState(true);
	const [categoryFilter, setCategoryFilter] = useState<string | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [isTauriAvailable, setIsTauriAvailable] = useState(true);
	const [showCreate, setShowCreate] = useState(false);
	const [newName, setNewName] = useState("");
	const [newCategory, setNewCategory] = useState("general");
	const [newDescription, setNewDescription] = useState("");
	const [newContent, setNewContent] = useState("");

	useEffect(() => {
		setIsTauriAvailable(
			typeof window !== "undefined" &&
				typeof (window as unknown as { __TAURI_INTERNALS__?: unknown })
					.__TAURI_INTERNALS__ !== "undefined",
		);
	}, []);

	useEffect(() => {
		loadSkills();
	}, [isTauriAvailable]);

	const loadSkills = async () => {
		setLoading(true);
		setError(null);
		if (!isTauriAvailable) {
			setSkills([]);
			setLoading(false);
			return;
		}
		try {
			const data = await listSkills();
			setSkills(data);
		} catch (err) {
			console.error("Failed to load skills:", err);
			setError(String(err));
		} finally {
			setLoading(false);
		}
	};

	const handleSelectSkill = useCallback(
		async (skill: Skill) => {
			setSelectedSkill(skill);
			if (!isTauriAvailable) {
				setSkillContent("Skill preview requires the Tauri desktop runtime.");
				return;
			}
			try {
				const content = await getSkillContent(skill.name);
				setSkillContent(content);
			} catch (err) {
				setSkillContent(`Error loading skill: ${err}`);
			}
		},
		[isTauriAvailable],
	);

	const handleToggleEnabled = async (skill: Skill) => {
		if (!isTauriAvailable) {
			setError("Enabling/disabling skills requires the Tauri desktop runtime.");
			return;
		}
		try {
			setError(null);
			await setSkillEnabled(skill.name, !skill.enabled);
			await loadSkills();
			if (selectedSkill?.name === skill.name) {
				setSelectedSkill((prev) =>
					prev ? { ...prev, enabled: !prev.enabled } : prev,
				);
			}
		} catch (err) {
			setError(String(err));
		}
	};

	const handleCreate = async () => {
		if (!newName.trim()) return;
		if (!isTauriAvailable) {
			setError("Creating skills requires the Tauri desktop runtime.");
			return;
		}
		try {
			setError(null);
			await createSkill(
				newName.trim(),
				newCategory.trim() || "general",
				newDescription.trim(),
				newContent.trim(),
			);
			setNewName("");
			setNewCategory("general");
			setNewDescription("");
			setNewContent("");
			setShowCreate(false);
			await loadSkills();
		} catch (err) {
			setError(String(err));
		}
	};

	const handleDelete = async (skill: Skill) => {
		if (!isTauriAvailable) {
			setError("Deleting skills requires the Tauri desktop runtime.");
			return;
		}
		if (!confirm(`Delete skill "${skill.name}"?`)) return;
		try {
			setError(null);
			await deleteSkill(skill.name);
			if (selectedSkill?.name === skill.name) {
				setSelectedSkill(null);
				setSkillContent("");
			}
			await loadSkills();
		} catch (err) {
			setError(String(err));
		}
	};

	const categories = Array.from(new Set(skills.map((s) => s.category))).sort();

	const filteredSkills = skills.filter((skill) => {
		const matchesSearch =
			!search.trim() ||
			skill.name.toLowerCase().includes(search.toLowerCase()) ||
			skill.description.toLowerCase().includes(search.toLowerCase()) ||
			skill.tags.some((t) => t.toLowerCase().includes(search.toLowerCase()));
		const matchesCategory =
			!categoryFilter || skill.category === categoryFilter;
		return matchesSearch && matchesCategory;
	});

	return (
		<div
			className="overlay"
			onClick={(e) => e.target === e.currentTarget && onClose()}
		>
			<div className="skills-panel">
				<div className="skills-panel-header">
					<h2>
						<BookIcon />
						Skills
					</h2>
					<div className="skills-header-actions">
						<button
							type="button"
							className="icon-btn"
							onClick={() => setShowCreate(true)}
							title="New skill"
						>
							<PlusIcon />
						</button>
						<button type="button" className="icon-btn" onClick={onClose}>
							<XIcon />
						</button>
					</div>
				</div>

				<div className="skills-panel-body">
					<div className="skills-sidebar">
						<div className="skills-search">
							<SearchIcon />
							<input
								type="text"
								value={search}
								onChange={(e) => setSearch(e.target.value)}
								placeholder="Search skills..."
							/>
						</div>

						<div className="skills-categories">
							<button
								type="button"
								className={`category-btn ${!categoryFilter ? "active" : ""}`}
								onClick={() => setCategoryFilter(null)}
							>
								All
							</button>
							{categories.map((cat) => (
								<button
									key={cat}
									type="button"
									className={`category-btn ${categoryFilter === cat ? "active" : ""}`}
									onClick={() => setCategoryFilter(cat)}
								>
									{cat}
								</button>
							))}
						</div>

						{error && <div className="skills-error">{error}</div>}
						{!isTauriAvailable && !error && (
							<div className="skills-error">
								Skills preview mode: scanning requires the Tauri desktop
								app.
							</div>
						)}
						<div className="skills-list">
							{loading ? (
								<div className="skills-loading">Loading skills...</div>
							) : filteredSkills.length === 0 ? (
								<div className="skills-empty">No skills found</div>
							) : (
								filteredSkills.map((skill) => (
									<div
										key={skill.name}
										className={`skill-item ${selectedSkill?.name === skill.name ? "active" : ""}`}
									>
										<div
											className="skill-item-main"
											onClick={() => handleSelectSkill(skill)}
										>
											<div className="skill-item-name">
												{skill.name}
												<span
													className={`skill-enabled-badge ${skill.enabled ? "on" : "off"}`}
												>
													{skill.enabled ? "on" : "off"}
												</span>
											</div>
											<div className="skill-item-desc">
												{skill.description}
											</div>
										</div>
										<div className="skill-item-meta">
											<span className="skill-category">
												{skill.category}
											</span>
											{skill.version && (
												<span className="skill-version">
													v{skill.version}
												</span>
											)}
											<button
												type="button"
												className="skill-toggle-btn"
												onClick={() => handleToggleEnabled(skill)}
												title={skill.enabled ? "Disable" : "Enable"}
											>
												{skill.enabled ? "Disable" : "Enable"}
											</button>
											<button
												type="button"
												className="skill-delete-btn"
												onClick={() => handleDelete(skill)}
												title="Delete"
											>
												<TrashIcon />
											</button>
										</div>
									</div>
								))
							)}
						</div>
					</div>

					<div className="skills-content">
						{selectedSkill ? (
							<>
								<div className="skill-detail-header">
									<h3>{selectedSkill.name}</h3>
									<div className="skill-detail-meta">
										<span className="skill-detail-category">
											<FolderIcon />
											{selectedSkill.category}
										</span>
										{selectedSkill.version && (
											<span className="skill-detail-version">
												v{selectedSkill.version}
											</span>
										)}
										{selectedSkill.author && (
											<span className="skill-detail-author">
												by {selectedSkill.author}
											</span>
										)}
										{selectedSkill.license && (
											<span className="skill-detail-license">
												{selectedSkill.license}
											</span>
										)}
									</div>
									{selectedSkill.tags.length > 0 && (
										<div className="skill-detail-tags">
											{selectedSkill.tags.map((tag) => (
												<span key={tag} className="skill-tag">
													<TagIcon />
													{tag}
												</span>
											))}
										</div>
									)}
								</div>
								<div className="skill-detail-body">
									<pre className="skill-content">{skillContent}</pre>
								</div>
							</>
						) : showCreate ? (
							<div className="skills-create-form">
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
								<div className="skills-create-actions">
									<button
										type="button"
										className="btn-primary"
										onClick={handleCreate}
									>
										Create
									</button>
									<button
										type="button"
										className="btn-secondary"
										onClick={() => setShowCreate(false)}
									>
										Cancel
									</button>
								</div>
							</div>
						) : (
							<div className="skills-empty-state">
								<BookIcon />
								<p>Select a skill to view its documentation</p>
							</div>
						)}
					</div>
				</div>
			</div>
		</div>
	);
}
