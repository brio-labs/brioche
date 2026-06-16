import { useState, useEffect, useCallback } from 'react';
import type { Skill } from '../ipc';
import { listSkills, getSkillContent } from '../ipc';
import { XIcon, BookIcon, SearchIcon, TagIcon, FolderIcon, ChevronRightIcon } from './Icons';

interface SkillsPanelProps {
    onClose: () => void;
}

export default function SkillsPanel({ onClose }: SkillsPanelProps) {
    const [skills, setSkills] = useState<Skill[]>([]);
    const [selectedSkill, setSelectedSkill] = useState<Skill | null>(null);
    const [skillContent, setSkillContent] = useState<string>('');
    const [search, setSearch] = useState('');
    const [loading, setLoading] = useState(true);
    const [categoryFilter, setCategoryFilter] = useState<string | null>(null);

    useEffect(() => {
        loadSkills();
    }, []);

    const loadSkills = async () => {
        setLoading(true);
        try {
            const data = await listSkills();
            setSkills(data);
        } catch (err) {
            console.error('Failed to load skills:', err);
        } finally {
            setLoading(false);
        }
    };

    const handleSelectSkill = useCallback(async (skill: Skill) => {
        setSelectedSkill(skill);
        try {
            const content = await getSkillContent(skill.name);
            setSkillContent(content);
        } catch (err) {
            setSkillContent(`Error loading skill: ${err}`);
        }
    }, []);

    const categories = Array.from(new Set(skills.map((s) => s.category))).sort();

    const filteredSkills = skills.filter((skill) => {
        const matchesSearch =
            !search.trim() ||
            skill.name.toLowerCase().includes(search.toLowerCase()) ||
            skill.description.toLowerCase().includes(search.toLowerCase()) ||
            skill.tags.some((t) => t.toLowerCase().includes(search.toLowerCase()));
        const matchesCategory = !categoryFilter || skill.category === categoryFilter;
        return matchesSearch && matchesCategory;
    });

    return (
        <div className="overlay" onClick={(e) => e.target === e.currentTarget && onClose()}>
            <div className="skills-panel">
                <div className="skills-panel-header">
                    <h2>
                        <BookIcon />
                        Skills
                    </h2>
                    <button type="button" className="icon-btn" onClick={onClose}>
                        <XIcon />
                    </button>
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
                                className={`category-btn ${!categoryFilter ? 'active' : ''}`}
                                onClick={() => setCategoryFilter(null)}
                            >
                                All
                            </button>
                            {categories.map((cat) => (
                                <button
                                    key={cat}
                                    type="button"
                                    className={`category-btn ${categoryFilter === cat ? 'active' : ''}`}
                                    onClick={() => setCategoryFilter(cat)}
                                >
                                    {cat}
                                </button>
                            ))}
                        </div>

                        <div className="skills-list">
                            {loading ? (
                                <div className="skills-loading">Loading skills...</div>
                            ) : filteredSkills.length === 0 ? (
                                <div className="skills-empty">No skills found</div>
                            ) : (
                                filteredSkills.map((skill) => (
                                    <div
                                        key={skill.name}
                                        className={`skill-item ${selectedSkill?.name === skill.name ? 'active' : ''}`}
                                        onClick={() => handleSelectSkill(skill)}
                                    >
                                        <div className="skill-item-main">
                                            <div className="skill-item-name">{skill.name}</div>
                                            <div className="skill-item-desc">{skill.description}</div>
                                        </div>
                                        <div className="skill-item-meta">
                                            <span className="skill-category">{skill.category}</span>
                                            {skill.version && <span className="skill-version">v{skill.version}</span>}
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
                                            <span className="skill-detail-version">v{selectedSkill.version}</span>
                                        )}
                                        {selectedSkill.author && (
                                            <span className="skill-detail-author">by {selectedSkill.author}</span>
                                        )}
                                        {selectedSkill.license && (
                                            <span className="skill-detail-license">{selectedSkill.license}</span>
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
