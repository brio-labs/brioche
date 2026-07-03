import { Input, Textarea, Button } from "../ui";

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
		<div className="flex flex-col gap-2 rounded-none border border-border bg-bg-elevated p-4">
			<Input
				type="text"
				placeholder="Key (e.g., user_name)"
				value={newKey}
				onChange={(e) => setNewKey(e.target.value)}
				className="rounded-md"
			/>
			<Textarea
				placeholder="Value"
				value={newValue}
				onChange={(e) => setNewValue(e.target.value)}
				rows={3}
				className="rounded-md"
			/>
			<select
				value={newCategory}
				onChange={(e) => setNewCategory(e.target.value)}
				className="w-full cursor-pointer appearance-none rounded-md border border-border bg-bg-elevated px-3 py-2 text-xs text-fg-primary outline-none focus:border-accent-dim/60"
			>
				{categories.map((cat) => (
					<option key={cat} value={cat}>
						{cat.charAt(0).toUpperCase() + cat.slice(1)}
					</option>
				))}
			</select>
			<div className="flex justify-end gap-2 mt-1">
				<Button type="button" onClick={handleAdd} disabled={!canSave}>
					Save
				</Button>
				<Button type="button" variant="secondary" onClick={handleCancel}>
					Cancel
				</Button>
			</div>
		</div>
	);
}
