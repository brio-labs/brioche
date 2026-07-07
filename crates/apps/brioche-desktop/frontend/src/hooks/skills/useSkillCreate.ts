import { useState, useCallback } from "react";
import { useSkillsStore } from "../../stores/skillsStore";

/// Hook that owns the local state and handlers for the skill creation form.
///
/// Refs: I-Ui-SkillCreate
export function useSkillCreate() {
	const { createNewSkill, setShowCreate } = useSkillsStore();

	const [newName, setNewName] = useState("");
	const [newCategory, setNewCategory] = useState("general");
	const [newDescription, setNewDescription] = useState("");
	const [newContent, setNewContent] = useState("");

	const handleCreate = useCallback(async () => {
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
	}, [
		newName,
		newCategory,
		newDescription,
		newContent,
		createNewSkill,
		setShowCreate,
	]);

	const handleCancelCreate = useCallback(() => {
		setNewName("");
		setNewCategory("general");
		setNewDescription("");
		setNewContent("");
		setShowCreate(false);
	}, [setShowCreate]);

	const canCreate = Boolean(newName.trim() && newCategory.trim());

	return {
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
	};
}
