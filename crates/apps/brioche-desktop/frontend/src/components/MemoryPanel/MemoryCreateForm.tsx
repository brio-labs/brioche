interface MemoryCreateFormProps {
	categories: string[];
	newKey: string;
	newValue: string;
	newCategory: string;
	canSave: boolean;
	setNewKey: (value: string) => void;
	setNewValue: (value: string) => void;
	setNewCategory: (value: string) => void;
	handleAdd: () => void;
	handleCancel: () => void;
}

/// Renders the add-memory form.
///
/// Refs: I-Ui-MemoryPanel
export default function MemoryCreateForm({
	categories,
	newKey,
	newValue,
	newCategory,
	canSave,
	setNewKey,
	setNewValue,
	setNewCategory,
	handleAdd,
	handleCancel,
}: MemoryCreateFormProps) {
	return (
		<div className="flex flex-col gap-2.5 rounded-lg border border-border bg-bg-elevated/30 p-3.5 [&_input]:rounded [&_input]:border [&_input]:border-border [&_input]:bg-bg-elevated [&_input]:px-2.5 [&_input]:py-1.5 [&_input]:text-xs [&_input]:text-fg-primary [&_input]:outline-none [&_input]:focus:border-accent-dim/60 [&_textarea]:resize-none [&_textarea]:rounded [&_textarea]:border [&_textarea]:border-border [&_textarea]:bg-bg-elevated [&_textarea]:px-2.5 [&_textarea]:py-1.5 [&_textarea]:text-xs [&_textarea]:text-fg-primary [&_textarea]:outline-none [&_textarea]:focus:border-accent-dim/60 [&_select]:cursor-pointer [&_select]:appearance-none [&_select]:rounded [&_select]:border [&_select]:border-border [&_select]:bg-bg-elevated [&_select]:px-2.5 [&_select]:py-1.5 [&_select]:text-xs [&_select]:text-fg-primary [&_select]:outline-none [&_select]:focus:border-accent-dim/60">
			<input
				type="text"
				placeholder="Key (e.g., user_name)"
				value={newKey}
				onChange={(e) => setNewKey(e.target.value)}
			/>
			<textarea
				placeholder="Value"
				value={newValue}
				onChange={(e) => setNewValue(e.target.value)}
				rows={3}
			/>
			<select
				value={newCategory}
				onChange={(e) => setNewCategory(e.target.value)}
			>
				{categories.map((cat) => (
					<option key={cat} value={cat}>
						{cat.charAt(0).toUpperCase() + cat.slice(1)}
					</option>
				))}
			</select>
			<div className="flex justify-end gap-2 [&_button]:cursor-pointer [&_button]:rounded [&_button]:px-3 [&_button]:py-1.5 [&_button]:text-xs [&_button]:font-medium [&_button:first-child]:bg-accent [&_button:first-child]:text-white [&_button:first-child]:hover:bg-accent-hover [&_button:first-child]:disabled:cursor-not-allowed [&_button:first-child]:disabled:opacity-50 [&_button:last-child]:border [&_button:last-child]:border-border [&_button:last-child]:bg-transparent [&_button:last-child]:text-fg-secondary [&_button:last-child]:hover:bg-bg-elevated">
				<button type="button" onClick={handleAdd} disabled={!canSave}>
					Save
				</button>
				<button type="button" onClick={handleCancel}>
					Cancel
				</button>
			</div>
		</div>
	);
}
