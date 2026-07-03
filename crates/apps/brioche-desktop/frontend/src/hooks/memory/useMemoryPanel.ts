import { useState, useCallback } from "react";
import { useMemoryStore } from "../../stores/panelStores";

/// Hook that owns the local form state and handlers for the memory creation panel.
///
/// Refs: I-Ui-MemoryPanel
export function useMemoryPanel() {
	const { addNewMemory, setIsAdding } = useMemoryStore();

	const [newKey, setNewKey] = useState("");
	const [newValue, setNewValue] = useState("");
	const [newCategory, setNewCategory] = useState("preference");

	const handleAdd = useCallback(async () => {
		if (!newKey.trim() || !newValue.trim()) return;
		const success = await addNewMemory(
			newKey.trim(),
			newValue.trim(),
			newCategory,
		);
		if (success) {
			setNewKey("");
			setNewValue("");
			setNewCategory("preference");
			setIsAdding(false);
		}
	}, [newKey, newValue, newCategory, addNewMemory, setIsAdding]);

	const handleCancel = useCallback(() => {
		setNewKey("");
		setNewValue("");
		setNewCategory("preference");
		setIsAdding(false);
	}, [setIsAdding]);

	const canSave = Boolean(newKey.trim() && newValue.trim());

	return {
		newKey,
		newValue,
		newCategory,
		setNewKey,
		setNewValue,
		setNewCategory,
		canSave,
		handleAdd,
		handleCancel,
	};
}
