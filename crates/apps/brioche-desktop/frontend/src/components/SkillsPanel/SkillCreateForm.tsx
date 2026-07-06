interface SkillCreateFormProps {
	newName: string;
	setNewName: (value: string) => void;
	newCategory: string;
	setNewCategory: (value: string) => void;
	newDescription: string;
	setNewDescription: (value: string) => void;
	newContent: string;
	setNewContent: (value: string) => void;
	canCreate: boolean;
	handleCreate: () => Promise<void>;
	handleCancelCreate: () => void;
}

/// Renders the new-skill creation form.
///
/// Refs: I-Ui-SkillCreateForm
export default function SkillCreateForm({
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
}: SkillCreateFormProps) {
	return (
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
			<div className="flex justify-end gap-2 [&_button]:cursor-pointer [&_button]:rounded [&_button]:px-3.5 [&_button]:py-1.5 [&_button]:text-xs [&_button]:font-medium [&_button:first-child]:border [&_button:first-child]:border-fg-primary/18 [&_button:first-child]:bg-fg-primary/14 [&_button:first-child]:text-fg-primary [&_button:first-child]:hover:bg-fg-primary/22 [&_button:first-child]:disabled:cursor-not-allowed [&_button:first-child]:disabled:opacity-50 [&_button:last-child]:border [&_button:last-child]:border-border [&_button:last-child]:bg-transparent [&_button:last-child]:text-fg-secondary [&_button:last-child]:hover:bg-bg-elevated">
				<button type="button" onClick={handleCreate} disabled={!canCreate}>
					Create
				</button>
				<button type="button" onClick={handleCancelCreate}>
					Cancel
				</button>
			</div>
		</div>
	);
}
