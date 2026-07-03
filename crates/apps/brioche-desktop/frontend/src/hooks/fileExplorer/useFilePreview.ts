import { useCallback, useState } from "react";
import { readFile } from "../../ipc";

interface UseFilePreviewOptions {
	writeExistingFile: (path: string, content: string) => Promise<void>;
}

export function useFilePreview({ writeExistingFile }: UseFilePreviewOptions) {
	const [preview, setPreview] = useState<{ path: string; content: string } | null>(
		null,
	);

	const handlePreview = useCallback(async (path: string) => {
		try {
			const content = await readFile(path);
			setPreview({ path, content });
		} catch (err) {
			console.error("Failed to read file:", err);
		}
	}, []);

	const handleSavePreview = useCallback(async () => {
		if (!preview) return;
		try {
			await writeExistingFile(preview.path, preview.content);
			setPreview(null);
		} catch (err) {
			console.error("Failed to save file:", err);
		}
	}, [preview, writeExistingFile]);

	return { preview, setPreview, handlePreview, handleSavePreview };
}
